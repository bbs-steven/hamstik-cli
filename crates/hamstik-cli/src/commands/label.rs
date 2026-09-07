// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! `hamstik label` (list / create).

use hamstik_api_client::{
    CreateLabelRequest, ListOptions, PageItems, ProjectLabel, follow_all, generate_key,
    validate_key,
};

use crate::app::Session;
use crate::args::{LabelArgs, LabelCommand};
use crate::error::CliError;

use super::{emit_table, emit_view};

/// Runs the `label` subcommands.
pub async fn run(session: &mut Session<'_>, args: &LabelArgs) -> Result<(), CliError> {
    match &args.command {
        LabelCommand::List {
            project,
            pagination,
        } => list(session, project.as_deref(), pagination).await,
        LabelCommand::Create {
            name,
            color,
            project,
            idempotency_key,
        } => {
            create(
                session,
                name,
                color.as_deref(),
                project.as_deref(),
                idempotency_key.as_deref(),
            )
            .await
        }
    }
}

/// Resolves the effective project key: explicit flag wins over context.
fn require_project(session: &Session<'_>, explicit: Option<&str>) -> Result<String, CliError> {
    if let Some(project) = explicit {
        return Ok(project.to_string());
    }
    let selection = session.selection()?;
    session.require_project(&selection)
}

async fn list(
    session: &mut Session<'_>,
    project_flag: Option<&str>,
    pagination: &crate::args::PaginationArgs,
) -> Result<(), CliError> {
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;
    let project = require_project(session, project_flag)?;
    let api = session.api(&selection)?;

    if pagination.all {
        let limit = pagination.limit;
        let fetch_api = api.clone();
        let org = org.clone();
        let project = project.clone();
        let page = follow_all(move |cursor| {
            let fetch_api = fetch_api.clone();
            let org = org.clone();
            let project = project.clone();
            async move {
                let response = fetch_api
                    .list_labels(&org, &project, ListOptions { limit, cursor })
                    .await?;
                Ok(PageItems::new(
                    response.value.items,
                    &response.raw,
                    response.value.page,
                ))
            }
        })
        .await
        .map_err(CliError::from_client)?;
        let rows: Vec<Vec<String>> = page.items.iter().map(label_row).collect();
        let json_value = serde_json::json!({ "items": page.raw_items, "page": page.page });
        emit_table(session, &json_value, &["ID", "NAME", "COLOR"], &rows)
    } else {
        let response = api
            .list_labels(
                &org,
                &project,
                ListOptions {
                    limit: pagination.limit,
                    cursor: pagination.cursor.clone(),
                },
            )
            .await
            .map_err(CliError::from_client)?;
        let rows: Vec<Vec<String>> = response.value.items.iter().map(label_row).collect();
        emit_table(session, &response.raw, &["ID", "NAME", "COLOR"], &rows)
    }
}

fn label_row(label: &ProjectLabel) -> Vec<String> {
    vec![label.id.clone(), label.name.clone(), label.color.clone()]
}

async fn create(
    session: &mut Session<'_>,
    name: &Option<String>,
    color: Option<&str>,
    project_flag: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<(), CliError> {
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;
    let project = require_project(session, project_flag)?;

    let name = match name {
        Some(name) => name.clone(),
        None if session.can_prompt() => session
            .prompt
            .read_line("Label name: ")
            .map_err(|err| CliError::general(format!("prompt failed: {err}")))?,
        None => return Err(CliError::usage("missing required option --name")),
    };
    if name.trim().is_empty() {
        return Err(CliError::usage("name must not be empty"));
    }

    let body = CreateLabelRequest {
        name,
        color: color.map(str::to_string),
    };
    let idempotency = match idempotency_key {
        Some(key) => {
            validate_key(key).map_err(|err| CliError::usage(err.to_string()))?;
            key.to_string()
        }
        None => generate_key(),
    };

    let api = session.api(&selection)?;
    let response = api
        .create_label(&org, &project, &body, &idempotency)
        .await
        .map_err(CliError::from_client)?;
    if response.idempotency_replayed {
        session
            .out
            .warn("note: request replayed (idempotent duplicate)");
    }
    let label = response.value.clone();
    emit_view(session, &response.raw, &label.id.clone(), |session| {
        session
            .out
            .line(&format!("{}  {}  {}", label.id, label.name, label.color))
            .map_err(CliError::general)
    })
}
