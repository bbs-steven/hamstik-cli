// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! `hamstik squeakql` — validate SqueakQL expressions.

use hamstik_api_client::SqueakQlValidateRequest;

use crate::app::Session;
use crate::args::{SqueakQlArgs, SqueakQlCommand};
use crate::error::CliError;

use super::emit_json;

/// Runs a SqueakQL command.
pub async fn run(session: &mut Session<'_>, args: &SqueakQlArgs) -> Result<(), CliError> {
    match &args.command {
        SqueakQlCommand::Validate { query } => validate(session, query).await,
    }
}

async fn validate(session: &mut Session<'_>, query: &str) -> Result<(), CliError> {
    if query.trim().is_empty() {
        return Err(CliError::usage("SqueakQL query must not be empty"));
    }
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;
    let api = session.api(&selection)?;
    let response = api
        .validate_squeakql(
            &org,
            &SqueakQlValidateRequest {
                query: query.to_string(),
            },
        )
        .await
        .map_err(CliError::from_client)?;

    if session.json() {
        return emit_json(session, &response.raw);
    }
    if response.value.valid {
        session
            .out
            .line(&format!(
                "Valid SqueakQL (language version {})",
                response.value.language_version
            ))
            .map_err(CliError::general)
    } else {
        session
            .out
            .line(&format!(
                "Invalid SqueakQL ({} error{})",
                response.value.errors.len(),
                if response.value.errors.len() == 1 {
                    ""
                } else {
                    "s"
                }
            ))
            .map_err(CliError::general)?;
        for diagnostic in &response.value.errors {
            session
                .out
                .line(&format!(
                    "  {}:{} {}: {}",
                    diagnostic.line,
                    diagnostic.column,
                    diagnostic.code.as_str(),
                    diagnostic.message
                ))
                .map_err(CliError::general)?;
            if let Some(suggestion) = &diagnostic.suggestion {
                session
                    .out
                    .line(&format!("    suggestion: {suggestion}"))
                    .map_err(CliError::general)?;
            }
        }
        Ok(())
    }
}
