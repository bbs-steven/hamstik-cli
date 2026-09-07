// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! `hamstik doctor` — verify configuration, credentials, and connectivity.

use secrecy::SecretString;
use serde_json::json;

use crate::app::Session;
use crate::credentials;
use crate::error::CliError;
use crate::exit;

use super::emit_json;

/// A single diagnostic line.
///
/// `critical` marks checks that gate the command's exit code. Informational
/// checks (organization/project/etc.) are reported but never fail `doctor`.
struct Check {
    name: &'static str,
    ok: bool,
    critical: bool,
    detail: String,
}

impl Check {
    fn info(name: &'static str, ok: bool, detail: String) -> Self {
        Self {
            name,
            ok,
            critical: false,
            detail,
        }
    }
    fn critical(name: &'static str, ok: bool, detail: String) -> Self {
        Self {
            name,
            ok,
            critical: true,
            detail,
        }
    }
}

pub async fn run(session: &mut Session<'_>) -> Result<(), CliError> {
    let mut checks: Vec<Check> = Vec::new();
    let mut code = exit::SUCCESS;

    // A config or context file that cannot be parsed must not abort the
    // diagnostics: `doctor` is exactly the command people reach for then, and
    // the credential store check below does not depend on config at all.
    let config_path = session.config.path();
    let selection = match session.selection() {
        Ok(selection) => {
            checks.push(Check::info(
                "configuration file",
                config_path.exists(),
                config_path.display().to_string(),
            ));
            selection
        }
        Err(err) => {
            code = err.exit_code();
            checks.push(Check::critical(
                "configuration file",
                false,
                err.message.clone(),
            ));
            check_store(session, &mut checks, &mut code);
            return render(session, checks, code);
        }
    };

    checks.push(Check::info(
        "host",
        true,
        format!(
            "{} (from {})",
            selection.host,
            selection.host_source.label()
        ),
    ));
    checks.push(Check::info(
        "profile",
        selection.profile.is_some(),
        selection
            .profile
            .clone()
            .unwrap_or_else(|| "none selected".to_string()),
    ));
    checks.push(Check::info(
        "organization",
        selection.organization.value.is_some(),
        selection
            .organization
            .value
            .clone()
            .unwrap_or_else(|| "not set".to_string()),
    ));
    checks.push(Check::info(
        "project",
        selection.project.value.is_some(),
        selection
            .project
            .value
            .clone()
            .unwrap_or_else(|| "not set".to_string()),
    ));

    check_store(session, &mut checks, &mut code);

    match resolve_token(session, &selection, &mut checks) {
        Some(secret) => match session.build_client(selection.host.clone(), secret) {
            Ok(api) => match api.whoami().await {
                Ok(me) => checks.push(Check::critical("authentication", true, me.value.email)),
                Err(err) => {
                    let cli = CliError::from_client(err);
                    code = cli.exit_code();
                    checks.push(Check::critical("authentication", false, cli.message));
                }
            },
            Err(err) => {
                code = err.exit_code();
                checks.push(Check::critical("client", false, err.message));
            }
        },
        None => {
            // A failing credential store read already pushed its own critical
            // check, so don't pile a second "no token" failure on top of it.
            if !checks.iter().any(|c| c.name == "credential store access") {
                code = exit::AUTHENTICATION;
                checks.push(Check::critical(
                    "authentication",
                    false,
                    "no token available".to_string(),
                ));
            }
        }
    }

    render(session, checks, code)
}

/// Reports whether a persistent credential store is reachable.
///
/// A keyring build without a backend, or a locked/unavailable keyring service,
/// accepts writes that never survive the process, so it is reported before
/// anything else can blame the token for it (SPEC §25, §26).
fn check_store(session: &Session<'_>, checks: &mut Vec<Check>, code: &mut i32) {
    // With HAMSTIK_TOKEN set the store is never consulted, so its state cannot
    // be responsible for what happened.
    if session.env.var("HAMSTIK_TOKEN").is_some() {
        return;
    }
    if credentials::store_is_persistent() {
        checks.push(Check::info(
            "credential store",
            true,
            "persistent store available".to_string(),
        ));
    } else {
        *code = if *code == exit::SUCCESS {
            exit::CONFIGURATION
        } else {
            *code
        };
        checks.push(Check::critical(
            "credential store",
            false,
            credentials::NO_STORE_HINT.to_string(),
        ));
    }
}

fn render(session: &mut Session<'_>, checks: Vec<Check>, code: i32) -> Result<(), CliError> {
    let overall_ok = code == exit::SUCCESS;

    if session.json() {
        let items: Vec<_> = checks
            .iter()
            .map(|c| {
                json!({
                    "name": c.name,
                    "ok": c.ok,
                    "critical": c.critical,
                    "detail": c.detail,
                })
            })
            .collect();
        emit_json(session, &json!({ "checks": items, "ok": overall_ok }))?;
    } else if !session.out.is_quiet() {
        for check in &checks {
            let marker = match (check.ok, check.critical) {
                (true, _) => "ok",
                (false, true) => "FAIL",
                (false, false) => "info",
            };
            session
                .out
                .line(&format!("[{marker:4}] {:<18} {}", check.name, check.detail))
                .map_err(CliError::general)?;
        }
        session
            .out
            .line(if overall_ok { "ready." } else { "not ready." })
            .map_err(CliError::general)?;
    }

    session.exit_code = code;
    Ok(())
}

/// Attempts to locate a usable token, recording a failing check on store errors.
fn resolve_token(
    session: &Session<'_>,
    selection: &crate::app::Selection,
    checks: &mut Vec<Check>,
) -> Option<SecretString> {
    if let Some(token) = session.env.var("HAMSTIK_TOKEN") {
        return Some(SecretString::new(token.into()));
    }
    let profile = selection.profile_meta.as_ref()?;
    let account = credentials::account_key(selection.host.as_str(), &profile.user_id);
    match session.store.get(&account) {
        Ok(secret) => secret,
        Err(err) => {
            checks.push(Check::critical(
                "credential store access",
                false,
                err.to_string(),
            ));
            None
        }
    }
}
