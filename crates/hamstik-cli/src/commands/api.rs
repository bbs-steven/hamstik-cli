// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! `hamstik api` — Public API metadata.

use crate::app::Session;
use crate::args::{ApiArgs, ApiCommand};
use crate::error::CliError;

use super::emit_json;

/// Runs a Public API metadata command.
pub async fn run(session: &mut Session<'_>, args: &ApiArgs) -> Result<(), CliError> {
    match args.command {
        ApiCommand::Openapi => openapi(session).await,
    }
}

async fn openapi(session: &mut Session<'_>) -> Result<(), CliError> {
    let selection = session.selection()?;
    let api = session.public_api(&selection)?;
    let response = api.get_open_api().await.map_err(CliError::from_client)?;
    // OpenAPI is itself JSON data, so it is printed as JSON in every output
    // mode. No bearer credential is required or sent for this operation.
    emit_json(session, &response.raw)
}
