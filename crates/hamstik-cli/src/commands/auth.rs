// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! `hamstik auth` (login / status / list / switch / logout).

use std::io::Read;

use secrecy::SecretString;
use serde_json::{json, value::Value};

use crate::app::Session;
use crate::args::{AuthArgs, AuthCommand};
use crate::config::{self, Profile};
use crate::credentials;
use crate::error::CliError;

use super::{emit_json, emit_table};

pub async fn run(session: &mut Session<'_>, args: &AuthArgs) -> Result<(), CliError> {
    match &args.command {
        AuthCommand::Login { with_token } => login(session, *with_token).await,
        AuthCommand::Status => status(session).await,
        AuthCommand::List => list(session),
        AuthCommand::Switch { profile } => switch(session, profile),
        AuthCommand::Logout => logout(session),
    }
}

async fn login(session: &mut Session<'_>, with_token: bool) -> Result<(), CliError> {
    let selection = session.selection()?;

    let raw = if with_token {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|err| CliError::general(format!("cannot read token: {err}")))?;
        buffer
    } else if let Some(token) = session.env.var("HAMSTIK_TOKEN") {
        token
    } else if session.can_prompt() {
        session
            .prompt
            .read_secret("Personal Access Token: ")
            .map_err(|err| CliError::general(format!("prompt failed: {err}")))?
    } else {
        return Err(CliError::usage(
            "no token provided; use --with-token, set HAMSTIK_TOKEN, or run interactively",
        ));
    };

    if raw.trim().is_empty() {
        return Err(CliError::usage("token is empty"));
    }
    let secret = SecretString::new(raw.trim().to_string().into());

    let api = session.build_client(selection.host.clone(), secret.clone())?;
    let me = api.whoami().await.map_err(CliError::from_client)?;

    let mut config = session.config.load()?;
    let base = config::profile_auto_name(selection.host.as_str(), &me.value.email);
    let name = config::unique_profile_name(&config, &base, &me.value.id, selection.host.as_str());
    let account = credentials::account_key(selection.host.as_str(), &me.value.id);

    session
        .store
        .set(&account, &secret)
        .map_err(|err| CliError::credential(format!("cannot store credential: {err}")))?;

    let profile = Profile {
        host: selection.host.as_str().to_string(),
        user_id: me.value.id.clone(),
        email: me.value.email.clone(),
        default_organization: me
            .value
            .default_organization
            .as_ref()
            .map(|org| org.slug.clone()),
        default_project: None,
    };
    config.profiles.insert(name.clone(), profile);
    config.active_profile = Some(name.clone());
    session.config.save(&config)?;

    if session.json() {
        emit_json(
            session,
            &json!({
                "authenticated": true,
                "profile": name,
                "host": selection.host.as_str(),
                "user": { "id": me.value.id, "email": me.value.email },
                "scopes": me.value.authentication.scopes,
            }),
        )
    } else {
        session
            .out
            .line(&format!(
                "Authenticated as {} ({})",
                me.value.email, selection.host
            ))
            .map_err(CliError::general)?;
        session
            .out
            .line(&format!("Profile: {name}"))
            .map_err(CliError::general)?;
        Ok(())
    }
}

async fn status(session: &mut Session<'_>) -> Result<(), CliError> {
    let selection = session.selection()?;
    let secret = resolve_status_token(session, &selection)?;

    let api = session.build_client(selection.host.clone(), secret)?;
    let me = api.whoami().await.map_err(CliError::from_client)?;

    if session.json() {
        emit_json(
            session,
            &json!({
                "authenticated": true,
                "host": selection.host.as_str(),
                "profile": selection.profile,
                "user": { "id": me.value.id, "email": me.value.email, "name": me.value.name },
                "credential": {
                    "name": me.value.authentication.credential_name,
                    "expiresAt": me.value.authentication.expires_at,
                },
                "scopes": me.value.authentication.scopes,
            }),
        )
    } else {
        session
            .out
            .line(&format!("Authenticated as {}", me.value.email))
            .map_err(CliError::general)?;
        session
            .out
            .line(&format!("  host:      {}", selection.host))
            .map_err(CliError::general)?;
        if let Some(profile) = &selection.profile {
            session
                .out
                .line(&format!("  profile:   {profile}"))
                .map_err(CliError::general)?;
        }
        if !me.value.authentication.scopes.is_empty() {
            session
                .out
                .line(&format!(
                    "  scopes:    {}",
                    me.value.authentication.scopes.join(", ")
                ))
                .map_err(CliError::general)?;
        }
        Ok(())
    }
}

/// Resolves the token for status without surfacing a usage error when absent.
fn resolve_status_token(
    session: &Session<'_>,
    selection: &crate::app::Selection,
) -> Result<SecretString, CliError> {
    if let Some(token) = session.env.var("HAMSTIK_TOKEN") {
        return Ok(SecretString::new(token.into()));
    }
    let profile = selection
        .profile_meta
        .as_ref()
        .ok_or_else(|| CliError::auth("not authenticated; run `hamstik auth login`"))?;
    let account = credentials::account_key(selection.host.as_str(), &profile.user_id);
    session
        .store
        .get(&account)
        .map_err(|err| CliError::credential(format!("credential store unavailable: {err}")))?
        .ok_or_else(|| CliError::auth("no stored credential; run `hamstik auth login`"))
}

fn list(session: &mut Session<'_>) -> Result<(), CliError> {
    let config = session.config.load()?;
    let active = session
        .global
        .profile
        .clone()
        .or_else(|| session.env.var("HAMSTIK_PROFILE"))
        .or_else(|| config.active_profile.clone());

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut items: Vec<Value> = Vec::new();
    for (name, profile) in &config.profiles {
        let is_active = active.as_deref() == Some(name.as_str());
        rows.push(vec![
            if is_active {
                "*".to_string()
            } else {
                String::new()
            },
            name.clone(),
            profile.host.clone(),
            profile.email.clone(),
            profile.default_organization.clone().unwrap_or_default(),
        ]);
        items.push(json!({
            "name": name,
            "host": profile.host,
            "email": profile.email,
            "defaultOrganization": profile.default_organization,
            "active": is_active,
        }));
    }

    if session.json() {
        emit_json(session, &json!({ "profiles": items }))
    } else {
        emit_table(
            session,
            &json!(null),
            &["", "NAME", "HOST", "EMAIL", "ORG"],
            &rows,
        )
    }
}

fn switch(session: &mut Session<'_>, name: &str) -> Result<(), CliError> {
    crate::config::validate_profile_name(name)?;
    let mut config = session.config.load()?;
    if !config.profiles.contains_key(name) {
        return Err(CliError::config(format!("no such profile: {name}")));
    }
    config.active_profile = Some(name.to_string());
    session.config.save(&config)?;
    session
        .out
        .line(&format!("Active profile: {name}"))
        .map_err(CliError::general)
}

fn logout(session: &mut Session<'_>) -> Result<(), CliError> {
    if session.env.var("HAMSTIK_TOKEN").is_some() {
        return Err(CliError::usage(
            "HAMSTIK_TOKEN is set; logout only removes stored credentials (unset the variable to stop using it)",
        ));
    }
    let selection = session.selection()?;
    let profile = selection
        .profile_meta
        .as_ref()
        .ok_or_else(|| CliError::usage("no active profile to log out"))?;
    let account = credentials::account_key(&profile.host, &profile.user_id);
    session
        .store
        .delete(&account)
        .map_err(|err| CliError::credential(format!("cannot delete credential: {err}")))?;
    session
        .out
        .line(&format!(
            "Removed stored credential for profile {:?}",
            selection.profile.as_deref().unwrap_or_default()
        ))
        .map_err(CliError::general)
}
