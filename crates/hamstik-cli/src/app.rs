// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Application wiring: dependency seams, session assembly, and the run loop.
//!
//! Everything the commands need (environment, credential store, config store,
//! API factory, prompts, output writers, working directory) is injected through
//! [`Services`] so the whole CLI can be exercised in tests with fakes. The real
//! process entrypoint in `main.rs` assembles the production services.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use secrecy::SecretString;

use hamstik_api_client::{ClientConfig, HamstikApi, HamstikClient, Host};

use crate::args::{Cli, GlobalOptions};
use crate::commands;
use crate::config::{ConfigStore, Profile};
use crate::context::{self, ResolvedField, Source};
use crate::credentials::{self, CredentialError, CredentialStore};
use crate::environment::Environment;
use crate::error::CliError;
use crate::exit::SUCCESS;
use crate::input::Prompt;
use crate::output::{Mode, Output};

/// Builds API clients behind a seam so tests can inject fakes.
pub trait ApiFactory: Send + Sync {
    fn build(&self, request: &ClientRequest) -> Result<Arc<dyn HamstikApi>, CliError>;
}

/// Parameters for constructing one API client.
pub struct ClientRequest<'a> {
    pub host: Host,
    pub token: SecretString,
    pub no_retry: bool,
    pub ca_bundle: Option<&'a Path>,
    pub user_agent: String,
}

/// Production factory producing real [`HamstikClient`] instances.
pub struct ProductionApiFactory;

impl ApiFactory for ProductionApiFactory {
    fn build(&self, request: &ClientRequest) -> Result<Arc<dyn HamstikApi>, CliError> {
        let mut config = ClientConfig {
            user_agent: request.user_agent.clone(),
            ..ClientConfig::default()
        };
        if request.no_retry {
            config.retry = hamstik_api_client::RetryPolicy::none();
        }
        if let Some(path) = request.ca_bundle {
            let pem = std::fs::read_to_string(path)
                .map_err(|err| CliError::config(format!("cannot read CA bundle: {err}")))?;
            config.ca_pem.push(pem);
        }
        let client = HamstikClient::new(request.host.clone(), request.token.clone(), config)
            .map_err(CliError::from_client)?;
        Ok(Arc::new(client))
    }
}

/// External dependencies injected into [`run`].
pub struct Services<'a> {
    pub env: &'a dyn Environment,
    pub store: &'a dyn CredentialStore,
    pub config: ConfigStore,
    pub factory: &'a dyn ApiFactory,
    pub prompt: &'a mut dyn Prompt,
    pub cwd: PathBuf,
    pub stdout: Box<dyn Write>,
    pub stderr: Box<dyn Write>,
}

/// The resolved selection for a single command invocation.
pub struct Selection {
    pub host: Host,
    pub host_source: Source,
    pub profile: Option<String>,
    pub profile_meta: Option<Profile>,
    pub organization: ResolvedField,
    pub project: ResolvedField,
    pub context_path: Option<PathBuf>,
    pub ephemeral_token: bool,
}

/// A command execution context bundling services with resolved state.
pub struct Session<'a> {
    pub out: Output,
    pub global: GlobalOptions,
    pub env: &'a dyn Environment,
    pub store: &'a dyn CredentialStore,
    pub config: ConfigStore,
    pub factory: &'a dyn ApiFactory,
    pub prompt: &'a mut dyn Prompt,
    pub cwd: PathBuf,
    /// Exit code used when a command succeeds but wants a non-zero status
    /// (e.g. `doctor` reporting a failed check).
    pub exit_code: i32,
}

impl Session<'_> {
    /// Loads config, selects the profile, resolves context, and parses the host.
    pub fn selection(&self) -> Result<Selection, CliError> {
        let config = self.config.load()?;
        let profile = context::select_profile(&config, self.global.profile.as_deref(), self.env)?;
        let profile_meta = profile
            .as_ref()
            .and_then(|name| config.profiles.get(name))
            .cloned();

        let context_path = context::discover(&self.cwd);
        let context_file = match &context_path {
            Some(path) => Some(context::load(path)?),
            None => None,
        };

        let resolution = context::resolve(
            (&self.global.host, &self.global.org, &self.global.project),
            self.env,
            context_file.as_ref(),
            profile_meta.as_ref(),
        );

        let host = Host::parse(&resolution.host)
            .map_err(|err| CliError::config(format!("invalid host: {err}")))?;

        Ok(Selection {
            host,
            host_source: resolution.host_source,
            profile,
            profile_meta,
            organization: resolution.organization,
            project: resolution.project,
            context_path,
            ephemeral_token: self.env.var("HAMSTIK_TOKEN").is_some(),
        })
    }

    /// Returns the resolved organization slug or a usage error.
    pub fn require_org(&self, selection: &Selection) -> Result<String, CliError> {
        selection.organization.value.clone().ok_or_else(|| {
            CliError::usage(
                "no organization selected; pass --org, set HAMSTIK_ORG, or run `hamstik org use <slug>`",
            )
        })
    }

    /// Returns the resolved project key or a usage error.
    pub fn require_project(&self, selection: &Selection) -> Result<String, CliError> {
        selection.project.value.clone().ok_or_else(|| {
            CliError::usage(
                "no project selected; pass --project, set HAMSTIK_PROJECT, or run `hamstik project use <key>`",
            )
        })
    }

    /// Resolves the authentication token (SPEC §25, §34).
    pub fn token_for(&self, selection: &Selection) -> Result<SecretString, CliError> {
        if let Some(token) = self.env.var("HAMSTIK_TOKEN") {
            return Ok(SecretString::new(token.into()));
        }
        let profile = selection.profile_meta.as_ref().ok_or_else(|| {
            CliError::usage("not authenticated; run `hamstik auth login` or set HAMSTIK_TOKEN")
        })?;
        let account = credentials::account_key(selection.host.as_str(), &profile.user_id);
        self.store
            .get(&account)
            .map_err(map_credential_error)?
            .ok_or_else(|| {
                CliError::usage(format!(
                    "no stored credential for profile {:?}; run `hamstik auth login`",
                    selection.profile.as_deref().unwrap_or_default()
                ))
            })
    }

    /// Builds an API client for the resolved selection.
    pub fn api(&self, selection: &Selection) -> Result<Arc<dyn HamstikApi>, CliError> {
        let token = self.token_for(selection)?;
        self.build_client(selection.host.clone(), token)
    }

    /// Builds an API client with an explicit token (used by `auth login`).
    pub fn build_client(
        &self,
        host: Host,
        token: SecretString,
    ) -> Result<Arc<dyn HamstikApi>, CliError> {
        let env_ca_bundle = self
            .env
            .var("HAMSTIK_CA_BUNDLE")
            .map(std::path::PathBuf::from);
        let ca_bundle = self
            .global
            .ca_bundle
            .as_deref()
            .or(env_ca_bundle.as_deref());
        let request = ClientRequest {
            host,
            token,
            no_retry: self.global.no_retry,
            ca_bundle,
            user_agent: user_agent(),
        };
        self.factory.build(&request)
    }

    /// Returns true when interactive prompting is permitted.
    #[must_use]
    pub fn can_prompt(&self) -> bool {
        !self.global.no_input && self.env.stdin_is_terminal()
    }

    /// Whether the JSON success envelope should echo the raw API body.
    #[must_use]
    pub fn json(&self) -> bool {
        self.out.is_json()
    }
}

fn map_credential_error(err: CredentialError) -> CliError {
    CliError::credential(format!("credential store unavailable: {err}"))
}

#[must_use]
pub fn user_agent() -> String {
    format!("hamstik-cli/{}", env!("CARGO_PKG_VERSION"))
}

/// Runs one parsed command to completion, returning the process exit code.
pub async fn run(cli: Cli, services: Services<'_>) -> i32 {
    let Services {
        env,
        store,
        config,
        factory,
        prompt,
        cwd,
        stdout,
        stderr,
    } = services;

    let global = cli.global.clone();
    let mode = if global.json {
        Mode::Json
    } else if global.quiet {
        Mode::Quiet
    } else {
        Mode::Human
    };
    let mut out = Output::new(mode, global.verbose, stdout, stderr);

    if global.json && global.quiet {
        let err = CliError::usage("cannot combine --json and --quiet");
        let _ = out.error(&err);
        return err.exit_code();
    }
    if global.quiet && global.verbose {
        let err = CliError::usage("cannot combine --quiet and --verbose");
        let _ = out.error(&err);
        return err.exit_code();
    }

    let mut session = Session {
        out,
        global,
        env,
        store,
        config,
        factory,
        prompt,
        cwd,
        exit_code: SUCCESS,
    };

    match commands::dispatch(&mut session, &cli.command).await {
        Ok(()) => session.exit_code,
        Err(err) => {
            let _ = session.out.error(&err);
            err.exit_code()
        }
    }
}
