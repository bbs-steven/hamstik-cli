// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Project context (`.hamstik.toml`) and cross-source precedence resolution.
//!
//! Resolution is pure (SPEC §34) so it can be unit-tested without touching the
//! filesystem; discovery/loading/writing are thin IO wrappers around it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{ConfigFile, Profile};
use crate::environment::Environment;
use crate::error::CliError;

/// Current context schema version.
pub const CONTEXT_VERSION: u32 = 1;

/// Maximum accepted `.hamstik.toml` size (64 KiB).
pub const MAX_CONTEXT_BYTES: u64 = 64 * 1024;

/// The name of the per-directory context file.
pub const CONTEXT_FILENAME: &str = ".hamstik.toml";

/// Where a resolved value came from (for `context show --explain`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Cli,
    Env,
    ContextFile,
    Profile,
    Default,
}

impl Source {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Source::Cli => "--flag",
            Source::Env => "environment",
            Source::ContextFile => ".hamstik.toml",
            Source::Profile => "global config",
            Source::Default => "default",
        }
    }

    /// Stable machine-readable identifier (JSON output).
    #[must_use]
    pub fn as_key(self) -> &'static str {
        match self {
            Source::Cli => "cli",
            Source::Env => "environment",
            Source::ContextFile => "context_file",
            Source::Profile => "profile",
            Source::Default => "default",
        }
    }
}

/// A resolved value together with its winning source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedField {
    pub value: Option<String>,
    pub source: Source,
}

/// The `.hamstik.toml` document.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContextFile {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// The full resolved selection for a command invocation.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub host: String,
    pub host_source: Source,
    pub profile: Option<String>,
    pub organization: ResolvedField,
    pub project: ResolvedField,
}

/// Selects the active profile name (SPEC §28).
pub fn select_profile(
    config: &ConfigFile,
    cli_profile: Option<&str>,
    env: &dyn Environment,
) -> Result<Option<String>, CliError> {
    let requested = cli_profile
        .map(str::to_string)
        .or_else(|| env.var("HAMSTIK_PROFILE"))
        .or_else(|| config.active_profile.clone());
    match requested {
        Some(name) => {
            crate::config::validate_profile_name(&name)?;
            if !config.profiles.contains_key(&name) {
                return Err(CliError::config(format!("no such profile: {name}")));
            }
            Ok(Some(name))
        }
        None => Ok(None),
    }
}

/// Resolves host/organization/project by precedence (SPEC §34).
#[must_use]
pub fn resolve(
    cli: (&Option<String>, &Option<String>, &Option<String>),
    env: &dyn Environment,
    context: Option<&ContextFile>,
    profile: Option<&Profile>,
) -> Resolution {
    let (cli_host, cli_org, cli_project) = cli;
    let mut resolution = resolve_inner(
        cli_host.as_deref(),
        cli_org.as_deref(),
        cli_project.as_deref(),
        env,
        context,
        profile,
    );
    // The profile name is attached by the caller once it is known.
    resolution.profile = None;
    resolution
}

fn resolve_inner(
    cli_host: Option<&str>,
    cli_org: Option<&str>,
    cli_project: Option<&str>,
    env: &dyn Environment,
    context: Option<&ContextFile>,
    profile: Option<&Profile>,
) -> Resolution {
    let host = pick_string(
        cli_host.map(|v| (v, Source::Cli)),
        env.var("HAMSTIK_HOST").map(|v| (v, Source::Env)),
        context
            .and_then(|c| c.host.as_deref())
            .map(|v| (v, Source::ContextFile)),
        profile.map(|p| (p.host.as_str(), Source::Profile)),
    );
    let host_value = host
        .value
        .clone()
        .unwrap_or_else(|| hamstik_api_client::host::DEFAULT_HOST.to_string());
    let host_source = if host.value.is_some() {
        host.source
    } else {
        Source::Default
    };

    let organization = pick_string(
        cli_org.map(|v| (v, Source::Cli)),
        env.var("HAMSTIK_ORG").map(|v| (v, Source::Env)),
        context
            .and_then(|c| c.organization.as_deref())
            .map(|v| (v, Source::ContextFile)),
        profile
            .and_then(|p| p.default_organization.as_deref())
            .map(|v| (v, Source::Profile)),
    );
    let project = pick_string(
        cli_project.map(|v| (v, Source::Cli)),
        env.var("HAMSTIK_PROJECT").map(|v| (v, Source::Env)),
        context
            .and_then(|c| c.project.as_deref())
            .map(|v| (v, Source::ContextFile)),
        profile
            .and_then(|p| p.default_project.as_deref())
            .map(|v| (v, Source::Profile)),
    );

    Resolution {
        host: host_value,
        host_source,
        profile: None,
        organization,
        project,
    }
}

fn pick_string(
    cli: Option<(&str, Source)>,
    env: Option<(String, Source)>,
    context: Option<(&str, Source)>,
    profile: Option<(&str, Source)>,
) -> ResolvedField {
    if let Some((value, source)) = cli {
        return ResolvedField {
            value: Some(value.to_string()),
            source,
        };
    }
    if let Some((value, source)) = env {
        return ResolvedField {
            value: Some(value),
            source,
        };
    }
    if let Some((value, source)) = context {
        return ResolvedField {
            value: Some(value.to_string()),
            source,
        };
    }
    if let Some((value, source)) = profile {
        return ResolvedField {
            value: Some(value.to_string()),
            source,
        };
    }
    ResolvedField {
        value: None,
        source: Source::Default,
    }
}

/// Searches upward from `start` for the nearest `.hamstik.toml`.
#[must_use]
pub fn discover(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join(CONTEXT_FILENAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

/// Loads and validates a context file.
pub fn load(path: &Path) -> Result<ContextFile, CliError> {
    let metadata = fs::metadata(path)
        .map_err(|err| CliError::config(format!("cannot read context: {err}")))?;
    if metadata.len() > MAX_CONTEXT_BYTES {
        return Err(CliError::config(
            "context file is too large (exceeds 64 KiB)",
        ));
    }
    let contents = fs::read_to_string(path)
        .map_err(|err| CliError::config(format!("cannot read context: {err}")))?;
    toml::from_str(&contents)
        .map_err(|err| CliError::config(format!("invalid context file: {err}")))
}

/// Writes a context file to disk.
pub fn save(path: &Path, context: &ContextFile) -> Result<(), CliError> {
    if context.version == 0 {
        return Err(CliError::config("internal error: context version not set"));
    }
    let serialized = toml::to_string_pretty(context)
        .map_err(|err| CliError::config(format!("cannot serialize context: {err}")))?;
    fs::write(path, serialized)
        .map_err(|err| CliError::config(format!("cannot write context file: {err}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::MapEnvironment;

    fn profile() -> Profile {
        Profile {
            host: "https://profile.host".to_string(),
            user_id: "u".to_string(),
            email: "u@profile.host".to_string(),
            default_organization: Some("profile-org".to_string()),
            default_project: Some("PRO".to_string()),
        }
    }

    #[test]
    fn cli_overrides_everything() {
        let env = MapEnvironment::new()
            .with_var("HAMSTIK_HOST", "https://env.host")
            .with_var("HAMSTIK_ORG", "env-org")
            .with_var("HAMSTIK_PROJECT", "ENVP");
        let context = ContextFile {
            version: 1,
            host: Some("https://ctx.host".into()),
            organization: Some("ctx-org".into()),
            project: Some("CTX".into()),
        };
        let resolution = resolve(
            (
                &Some("https://cli.host".to_string()),
                &Some("cli-org".to_string()),
                &Some("CLI".to_string()),
            ),
            &env,
            Some(&context),
            Some(&profile()),
        );
        assert_eq!(resolution.host, "https://cli.host");
        assert_eq!(resolution.host_source, Source::Cli);
        assert_eq!(resolution.organization.value.as_deref(), Some("cli-org"));
        assert_eq!(resolution.project.value.as_deref(), Some("CLI"));
    }

    #[test]
    fn falls_back_through_env_context_profile() {
        let env = MapEnvironment::new();
        let context = ContextFile {
            version: 1,
            host: None,
            organization: Some("ctx-org".into()),
            project: None,
        };
        let resolution = resolve(
            (&None, &None, &None),
            &env,
            Some(&context),
            Some(&profile()),
        );
        assert_eq!(resolution.host, "https://profile.host");
        assert_eq!(resolution.host_source, Source::Profile);
        assert_eq!(resolution.organization.value.as_deref(), Some("ctx-org"));
        assert_eq!(resolution.organization.source, Source::ContextFile);
        assert_eq!(resolution.project.value.as_deref(), Some("PRO"));
        assert_eq!(resolution.project.source, Source::Profile);
    }

    #[test]
    fn default_host_when_nothing_set() {
        let resolution = resolve((&None, &None, &None), &MapEnvironment::new(), None, None);
        assert_eq!(resolution.host, "https://hamstik.com");
        assert_eq!(resolution.host_source, Source::Default);
        assert_eq!(resolution.organization.value, None);
    }

    #[test]
    fn env_wins_over_context() {
        let env = MapEnvironment::new().with_var("HAMSTIK_ORG", "env-org");
        let context = ContextFile {
            version: 1,
            host: None,
            organization: Some("ctx-org".into()),
            project: None,
        };
        let resolution = resolve((&None, &None, &None), &env, Some(&context), None);
        assert_eq!(resolution.organization.value.as_deref(), Some("env-org"));
        assert_eq!(resolution.organization.source, Source::Env);
    }

    #[test]
    fn discovers_nearest_context() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let nested = root.join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join(CONTEXT_FILENAME), "version = 1\n").unwrap();
        fs::write(root.join(CONTEXT_FILENAME), "version = 1\n").unwrap();
        let found = discover(&nested).unwrap();
        assert_eq!(found, nested.join(CONTEXT_FILENAME));
    }

    #[test]
    fn context_roundtrip_and_rejects_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONTEXT_FILENAME);
        let context = ContextFile {
            version: 1,
            host: Some("https://h".into()),
            organization: Some("org".into()),
            project: None,
        };
        save(&path, &context).unwrap();
        assert_eq!(load(&path).unwrap(), context);
        fs::write(&path, "version = 1\nbogus = 1\n").unwrap();
        assert!(load(&path).is_err());
    }
}
