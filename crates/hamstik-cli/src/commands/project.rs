// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! `hamstik project` (list / view / use).

use serde_json::json;

use hamstik_api_client::{ListOptions, Project};

use crate::app::Session;
use crate::args::{PaginationArgs, ProjectArgs, ProjectCommand};
use crate::error::CliError;

use super::org::render_lines;
use super::{emit_json, emit_table, emit_view};

/// Runs the `project` subcommands.
pub async fn run(session: &mut Session<'_>, args: &ProjectArgs) -> Result<(), CliError> {
    match &args.command {
        ProjectCommand::List(pagination) => list(session, pagination).await,
        ProjectCommand::View { key } => view(session, key).await,
        ProjectCommand::Use { key } => use_project(session, key),
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

fn use_project(session: &mut Session<'_>, key: &str) -> Result<(), CliError> {
    let selection = session.selection()?;
    let profile_name = selection
        .profile
        .clone()
        .ok_or_else(|| CliError::usage("no active profile; run `hamstik auth login` first"))?;
    let mut config = session.config.load()?;
    let profile = config
        .profiles
        .get_mut(&profile_name)
        .ok_or_else(|| CliError::config(format!("no such profile: {profile_name}")))?;
    profile.default_project = Some(key.to_string());
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
