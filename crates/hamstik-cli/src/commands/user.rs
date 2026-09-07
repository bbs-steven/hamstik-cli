// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! `hamstik user` (view / work / activity / avatar) — authenticated profile
//! reads over `profile:read`.

use serde_json::{Value, json};

use hamstik_api_client::{ActivityOptions, ListWorkItemsQuery, PageItems, follow_all};

use crate::app::Session;
use crate::args::{UserArgs, UserCommand};
use crate::error::CliError;

use super::org::render_lines;
use super::{emit_json, emit_table, emit_view};

/// Runs the `user` subcommands.
pub async fn run(session: &mut Session<'_>, args: &UserArgs) -> Result<(), CliError> {
    match &args.command {
        UserCommand::View { public_id } => view(session, public_id).await,
        UserCommand::Work {
            public_id,
            involvement,
            org,
            project,
            status,
            scope,
            priority,
            search,
            pagination,
        } => {
            work(
                session,
                public_id,
                involvement,
                org,
                project,
                status,
                *scope,
                priority,
                search.as_deref(),
                pagination,
            )
            .await
        }
        UserCommand::Activity {
            public_id,
            since,
            pagination,
        } => activity(session, public_id, since.as_deref(), pagination).await,
        UserCommand::Avatar { public_id, output } => {
            avatar(session, public_id, output.as_deref()).await
        }
    }
}

async fn view(session: &mut Session<'_>, public_id: &str) -> Result<(), CliError> {
    let selection = session.selection()?;
    let api = session.api(&selection)?;
    let response = api
        .get_user_profile(public_id)
        .await
        .map_err(CliError::from_client)?;
    let profile = response.value.clone();
    emit_view(session, &response.raw, public_id, |session| {
        let mut lines = vec![
            ("public id", profile.public_id.clone()),
            ("name", profile.name.clone()),
            ("joined", profile.joined_at.clone()),
            ("you", profile.is_current_user.to_string()),
            (
                "avatar",
                profile
                    .avatar_url
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ];
        for shared in &profile.shared_organizations {
            let username = shared.username.clone().unwrap_or_else(|| "-".to_string());
            lines.push((
                Box::leak(format!("user @ {}", shared.organization.slug).into_boxed_str()),
                username,
            ));
        }
        lines.push(("projects", profile.stats.projects.to_string()));
        lines.push(("assigned", profile.stats.work_items_assigned.to_string()));
        lines.push(("created", profile.stats.work_items_created.to_string()));
        lines.push(("completed", profile.stats.work_items_completed.to_string()));
        lines.push(("comments", profile.stats.comments.to_string()));
        render_lines(session, &lines)
    })
}

#[allow(clippy::too_many_arguments)]
async fn work(
    session: &mut Session<'_>,
    public_id: &str,
    involvement: &[crate::args::InvolvementArg],
    org: &[String],
    project: &[String],
    status: &[crate::args::StatusArg],
    scope: Option<crate::args::ScopeArg>,
    priority: &[crate::args::PriorityArg],
    search: Option<&str>,
    pagination: &crate::args::PaginationArgs,
) -> Result<(), CliError> {
    let selection = session.selection()?;
    let api = session.api(&selection)?;
    let base = ListWorkItemsQuery {
        limit: pagination.limit,
        cursor: pagination.cursor.clone(),
        involvement: involvement.iter().map(|i| i.as_str().to_string()).collect(),
        organizations: org.to_vec(),
        projects: project.to_vec(),
        status: status.iter().map(|s| s.as_str().to_string()).collect(),
        scope: scope.map(|s| s.as_str().to_string()),
        priority: priority.iter().map(|p| p.as_str().to_string()).collect(),
        q: search.map(str::to_string),
        // Profile Work fixes the target as the assignee; the caller cannot
        // choose one.
        ..Default::default()
    };
    let json_value: Value = if pagination.all {
        let fetch_api = api.clone();
        let public_id = public_id.to_string();
        let page = follow_all(move |cursor| {
            let fetch_api = fetch_api.clone();
            let public_id = public_id.clone();
            let mut query = base.clone();
            query.cursor = cursor;
            async move {
                let response = fetch_api.list_user_profile_work(&public_id, query).await?;
                Ok(PageItems::new(
                    response.value.items,
                    &response.raw,
                    response.value.page,
                ))
            }
        })
        .await
        .map_err(CliError::from_client)?;
        json!({ "items": page.raw_items, "page": page.page })
    } else {
        let response = api
            .list_user_profile_work(public_id, base)
            .await
            .map_err(CliError::from_client)?;
        response.raw
    };
    let rows: Vec<Vec<String>> = json_value
        .get("items")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(work_row_from_raw).collect())
        .unwrap_or_default();
    emit_table(
        session,
        &json_value,
        &["KEY", "PROJECT", "TITLE", "STATUS", "ASSIGNEE", "REPORTER"],
        &rows,
    )
}

fn work_row_from_raw(raw: &Value) -> Vec<String> {
    let project_key = raw
        .get("project")
        .and_then(|p| p.get("key"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let user_name = |value: Option<&Value>| -> String {
        value
            .and_then(|a| a.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("-")
            .to_string()
    };
    vec![
        raw.get("key")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        project_key.to_string(),
        raw.get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        raw.get("status")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        user_name(raw.get("assignee").filter(|a| !a.is_null())),
        user_name(raw.get("reporter").filter(|r| !r.is_null())),
    ]
}

async fn activity(
    session: &mut Session<'_>,
    public_id: &str,
    since: Option<&str>,
    pagination: &crate::args::PaginationArgs,
) -> Result<(), CliError> {
    let selection = session.selection()?;
    let api = session.api(&selection)?;
    let base = ActivityOptions {
        limit: pagination.limit,
        cursor: pagination.cursor.clone(),
        since: since.map(str::to_string),
    };
    let json_value: Value = if pagination.all {
        let fetch_api = api.clone();
        let public_id = public_id.to_string();
        let page = follow_all(move |cursor| {
            let fetch_api = fetch_api.clone();
            let public_id = public_id.clone();
            let mut opts = base.clone();
            opts.cursor = cursor;
            async move {
                let response = fetch_api
                    .list_user_profile_activity(&public_id, opts)
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
        json!({ "items": page.raw_items, "page": page.page })
    } else {
        let response = api
            .list_user_profile_activity(public_id, base)
            .await
            .map_err(CliError::from_client)?;
        response.raw
    };
    let rows: Vec<Vec<String>> = json_value
        .get("items")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(activity_row_from_raw).collect())
        .unwrap_or_default();
    emit_table(
        session,
        &json_value,
        &[
            "ID",
            "ACTION",
            "ACTOR",
            "ORG",
            "PROJECT",
            "WORK ITEM",
            "CREATED",
        ],
        &rows,
    )
}

fn activity_row_from_raw(raw: &Value) -> Vec<String> {
    vec![
        raw.get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        raw.get("action")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        raw.get("actor")
            .and_then(|a| a.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("-")
            .to_string(),
        raw.get("organization")
            .and_then(|o| o.get("slug"))
            .and_then(Value::as_str)
            .unwrap_or("-")
            .to_string(),
        raw.get("project")
            .and_then(|p| p.get("key"))
            .and_then(Value::as_str)
            .unwrap_or("-")
            .to_string(),
        raw.get("workItem")
            .and_then(|w| w.get("key"))
            .and_then(Value::as_str)
            .unwrap_or("-")
            .to_string(),
        raw.get("createdAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    ]
}

async fn avatar(
    session: &mut Session<'_>,
    public_id: &str,
    output: Option<&str>,
) -> Result<(), CliError> {
    let selection = session.selection()?;
    let api = session.api(&selection)?;
    let download = api
        .get_user_profile_avatar(public_id)
        .await
        .map_err(CliError::from_client)?;
    let target = match output {
        Some(path) => std::path::PathBuf::from(path),
        None => {
            let extension = match download.content_type.as_deref() {
                Some("image/png") => "png",
                Some("image/jpeg") => "jpg",
                Some("image/webp") => "webp",
                _ => "bin",
            };
            // Refuse to write outside the current directory implicitly: use
            // only the final path component of the derived name.
            let name = format!("{public_id}.{extension}");
            let safe = name.rsplit(['/', '\\']).next().unwrap_or(&name);
            std::path::PathBuf::from(safe)
        }
    };
    std::fs::write(&target, &download.bytes)
        .map_err(|err| CliError::general(format!("cannot write {}: {err}", target.display())))?;
    if session.json() {
        emit_json(
            session,
            &json!({
                "publicId": public_id,
                "path": target.display().to_string(),
                "size": download.bytes.len(),
            }),
        )
    } else {
        session
            .out
            .line(&format!(
                "Downloaded avatar for {public_id} ({} bytes) to {}",
                download.bytes.len(),
                target.display()
            ))
            .map_err(CliError::general)
    }
}
