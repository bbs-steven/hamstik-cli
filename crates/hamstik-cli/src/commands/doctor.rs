// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! `hamstik doctor` — verify configuration, credentials, connectivity, and
//! terminal rendering.

use secrecy::SecretString;
use serde_json::json;

use crate::app::Session;
use crate::credentials;
use crate::error::CliError;
use crate::exit;
use crate::terminal::{SGR_GREEN, SGR_RED, SGR_YELLOW, color_probe, emoji_probe, paint};

use super::emit_json;

/// The emoji sample rendered so users can visually confirm their terminal
/// shows the mascot instead of tofu boxes. U+1F439 is the hamster face.
const EMOJI_SAMPLE: &str = "🐹 🐹 🐹";

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

/// Runs the connectivity/configuration diagnostics.
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

    // Terminal rendering checks are informational: a dumb or piped terminal is
    // a working configuration, just a monochrome/plain-text one.
    checks.push(color_check(session));
    checks.push(emoji_check(session));

    render(session, checks, code)
}

/// Probes ANSI color support for this invocation's stdout.
fn color_check(session: &Session<'_>) -> Check {
    let probe = color_probe(
        session.env,
        session.global.no_color,
        session.env.stdout_is_terminal(),
    );
    Check::info("terminal color", probe.ok, probe.detail)
}

/// Probes emoji support for this invocation's stdout.
fn emoji_check(session: &Session<'_>) -> Check {
    let probe = emoji_probe(session.env, session.env.stdout_is_terminal());
    Check::info("terminal emoji", probe.ok, probe.detail)
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
        // Colors apply to the doctor output itself only when the terminal
        // probe says they are safe; `--no-color` and piped stdout stay plain.
        let color = color_probe(
            session.env,
            session.global.no_color,
            session.env.stdout_is_terminal(),
        )
        .ok;
        for check in &checks {
            let (marker, sgr) = match (check.ok, check.critical) {
                (true, _) => ("ok", SGR_GREEN),
                (false, true) => ("FAIL", SGR_RED),
                (false, false) => ("info", SGR_YELLOW),
            };
            let marker = paint(color, sgr, marker);
            session
                .out
                .line(&format!(
                    "[{marker:<4}] {:<18} {}",
                    check.name, check.detail
                ))
                .map_err(CliError::general)?;
        }
        session
            .out
            .line(if overall_ok { "ready." } else { "not ready." })
            .map_err(CliError::general)?;
        // Visual rendering samples (human mode only; never in --json):
        // if the emoji row shows boxes or the color row shows escape codes,
        // this terminal will mangle decorated CLI output.
        let emoji_ok = checks.iter().any(|c| c.name == "terminal emoji" && c.ok);
        session
            .out
            .line(&format!(
                "emoji sample: {EMOJI_SAMPLE}{}",
                if emoji_ok {
                    ""
                } else {
                    " (expected above; boxes mean your terminal lacks emoji)"
                }
            ))
            .map_err(CliError::general)?;
        if color {
            let swatches = format!(
                "{}green{} {}red{} {}yellow{}",
                SGR_GREEN, "\x1b[0m", SGR_RED, "\x1b[0m", SGR_YELLOW, "\x1b[0m",
            );
            session
                .out
                .line(&format!("color sample: {swatches}"))
                .map_err(CliError::general)?;
        }
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
        return match crate::input::token_to_secret(&token) {
            Ok(secret) => Some(secret),
            Err(message) => {
                checks.push(Check::critical(
                    "authentication",
                    false,
                    format!("HAMSTIK_TOKEN is unusable: {message}"),
                ));
                None
            }
        };
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
