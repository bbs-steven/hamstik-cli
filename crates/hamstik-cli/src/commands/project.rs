// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! `hamstik project` (list / view / create / use).

use serde_json::json;

use hamstik_api_client::{CreateProjectRequest, ListOptions, Project, generate_key, validate_key};

use crate::app::Session;
use crate::args::{PaginationArgs, ProjectArgs, ProjectCommand};
use crate::error::CliError;
use crate::input::resolve_text;

use super::org::render_lines;
use super::{emit_json, emit_table, emit_view};

/// Runs the `project` subcommands.
pub async fn run(session: &mut Session<'_>, args: &ProjectArgs) -> Result<(), CliError> {
    match &args.command {
        ProjectCommand::List(pagination) => list(session, pagination).await,
        ProjectCommand::View { key } => view(session, key).await,
        ProjectCommand::Create(create_args) => create(session, create_args).await,
        ProjectCommand::Use { key } => use_project(session, key).await,
    }
}

async fn list(session: &mut Session<'_>, pagination: &PaginationArgs) -> Result<(), CliError> {
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;
    let api = session.api(&selection)?;
    let response = api
        .list_projects(
            &org,
            ListOptions {
                limit: pagination.limit,
                cursor: pagination.cursor.clone(),
            },
        )
        .await
        .map_err(CliError::from_client)?;
    let rows: Vec<Vec<String>> = response
        .value
        .items
        .iter()
        .map(|project| {
            vec![
                project.key.clone(),
                project.name.clone(),
                project.color.clone(),
            ]
        })
        .collect();
    emit_table(session, &response.raw, &["KEY", "NAME", "COLOR"], &rows)
}

async fn view(session: &mut Session<'_>, key: &str) -> Result<(), CliError> {
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;
    let api = session.api(&selection)?;
    let response = api
        .get_project(&org, key)
        .await
        .map_err(CliError::from_client)?;
    let project: Project = response.value.clone();
    emit_view(session, &response.raw, key, |session| {
        let lines = [
            ("name", project.name.clone()),
            ("key", project.key.clone()),
            ("color", project.color.clone()),
            (
                "description",
                project.description.clone().unwrap_or_default(),
            ),
        ];
        render_lines(session, &lines)
    })
}

async fn create(
    session: &mut Session<'_>,
    args: &crate::args::ProjectCreateArgs,
) -> Result<(), CliError> {
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;

    let name = match &args.name {
        Some(name) => name.clone(),
        None if session.can_prompt() => session
            .prompt
            .read_line("Name: ")
            .map_err(|err| CliError::general(format!("prompt failed: {err}")))?,
        None => return Err(CliError::usage("missing required option --name")),
    };
    if name.trim().is_empty() {
        return Err(CliError::usage("name must not be empty"));
    }
    let description = resolve_text(
        args.description.clone(),
        args.description_file.as_deref(),
        &mut std::io::stdin(),
    )
    .map_err(|err| CliError::general(format!("cannot read text: {err}")))?;
    if let Some(key) = &args.key
        && key.trim().is_empty()
    {
        return Err(CliError::usage("key must not be empty"));
    }

    let body = CreateProjectRequest {
        name,
        key: args.key.clone(),
        description,
        color: args.color.clone(),
    };
    let idempotency = match &args.idempotency_key {
        Some(key) => {
            validate_key(key).map_err(|err| CliError::usage(err.to_string()))?;
            key.clone()
        }
        None => generate_key(),
    };

    let api = session.api(&selection)?;
    let response = api
        .create_project(&org, &body, &idempotency)
        .await
        .map_err(CliError::from_client)?;
    if response.idempotency_replayed {
        session
            .out
            .warn("note: request replayed (idempotent duplicate)");
    }
    let project = response.value.clone();
    emit_view(session, &response.raw, &project.key.clone(), |session| {
        let lines = [
            ("name", project.name.clone()),
            ("key", project.key.clone()),
            ("color", project.color.clone()),
            (
                "description",
                project.description.clone().unwrap_or_default(),
            ),
        ];
        render_lines(session, &lines)
    })
}

async fn use_project(session: &mut Session<'_>, key: &str) -> Result<(), CliError> {
    let selection = session.selection()?;
    // SPEC §35: validate the project through the Public API — inside the
    // resolved organization — before persisting it, so a typo never lands in
    // the config and fails later with an opaque not-found.
    let org = session.require_org(&selection)?;
    let api = session.api(&selection)?;
    let response = api
        .get_project(&org, key)
        .await
        .map_err(CliError::from_client)?;
    let project: Project = response.value;

    let profile_name = selection
        .profile
        .clone()
        .ok_or_else(|| CliError::usage("no active profile; run `hamstik auth login` first"))?;
    let mut config = session.config.load()?;
    let profile = config
        .profiles
        .get_mut(&profile_name)
        .ok_or_else(|| CliError::config(format!("no such profile: {profile_name}")))?;
    profile.default_project = Some(project.key);
    session.config.save(&config)?;

    if session.json() {
        emit_json(
            session,
            &json!({ "profile": profile_name, "defaultProject": key }),
        )
    } else {
        session
            .out
            .line(&format!(
                "Default project for profile {profile_name}: {key}"
            ))
            .map_err(CliError::general)
    }
}
