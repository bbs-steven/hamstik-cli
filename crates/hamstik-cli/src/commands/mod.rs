// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Command dispatch and shared output helpers.

pub mod auth;
pub mod completion;
pub mod context_cmd;
pub mod doctor;
pub mod label;
pub mod org;
pub mod project;
pub mod sprint;
pub mod user;
pub mod work;

use serde_json::Value;

use crate::app::Session;
use crate::args::Command;
use crate::error::CliError;

/// Runs the selected subcommand against the session.
pub async fn dispatch(session: &mut Session<'_>, command: &Command) -> Result<(), CliError> {
    match command {
        Command::Auth(args) => auth::run(session, args).await,
        Command::Context(args) => context_cmd::run(session, args).await,
        Command::Org(args) => org::run(session, args).await,
        Command::Project(args) => project::run(session, args).await,
        Command::Sprint(args) => sprint::run(session, args).await,
        Command::Label(args) => label::run(session, args).await,
        Command::Work(args) => work::run(session, args).await,
        Command::User(args) => user::run(session, args).await,
        Command::Doctor => doctor::run(session).await,
        Command::Completion(args) => completion::run(session, args),
        Command::Version => version(session),
    }
}

fn version(session: &mut Session<'_>) -> Result<(), CliError> {
    let version = env!("CARGO_PKG_VERSION");
    if session.json() {
        emit_json(session, &serde_json::json!({ "version": version }))
    } else {
        session
            .out
            .line(&crate::banner::banner())
            .map_err(CliError::general)
    }
}

pub(crate) fn emit_json(session: &mut Session<'_>, value: &Value) -> Result<(), CliError> {
    session.out.json(value).map_err(CliError::general)
}

/// Renders a list: JSON body verbatim, or a table (first column in quiet mode).
pub(crate) fn emit_table(
    session: &mut Session<'_>,
    json_value: &Value,
    headers: &[&str],
    rows: &[Vec<String>],
) -> Result<(), CliError> {
    if session.json() {
        emit_json(session, json_value)?;
    } else if session.out.is_quiet() {
        for row in rows {
            if let Some(cell) = row.first() {
                session.out.line(cell).map_err(CliError::general)?;
            }
        }
    } else {
        session
            .out
            .table(headers, rows)
            .map_err(CliError::general)?;
    }
    Ok(())
}

/// Renders a single resource: JSON body verbatim, quiet identifier, or detail.
pub(crate) fn emit_view<F>(
    session: &mut Session<'_>,
    json_value: &Value,
    quiet_id: &str,
    detail: F,
) -> Result<(), CliError>
where
    F: FnOnce(&mut Session<'_>) -> Result<(), CliError>,
{
    if session.json() {
        emit_json(session, json_value)?;
    } else if session.out.is_quiet() {
        session.out.line(quiet_id).map_err(CliError::general)?;
    } else {
        detail(session)?;
    }
    Ok(())
}
