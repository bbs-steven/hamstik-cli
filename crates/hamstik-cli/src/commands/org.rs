// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! `hamstik org` (list / view / use).

use serde_json::json;

use hamstik_api_client::{ListOptions, OrganizationListItem, PageItems, follow_all};

use crate::app::Session;
use crate::args::{OrgArgs, OrgCommand, PaginationArgs};
use crate::error::CliError;

use super::{emit_json, emit_table, emit_view};

/// Runs the `org` subcommands.
pub async fn run(session: &mut Session<'_>, args: &OrgArgs) -> Result<(), CliError> {
    match &args.command {
        OrgCommand::List(pagination) => list(session, pagination).await,
        OrgCommand::View { slug } => view(session, slug).await,
        OrgCommand::Use { slug } => use_org(session, slug).await,
    }
}

async fn list(session: &mut Session<'_>, pagination: &PaginationArgs) -> Result<(), CliError> {
    let selection = session.selection()?;
    let api = session.api(&selection)?;

    let (rows, json_value) = if pagination.all {
        let limit = pagination.limit;
        let fetch_api = api.clone();
        let page = follow_all(move |cursor| {
            let fetch_api = fetch_api.clone();
            async move {
                let response = fetch_api
                    .list_organizations(ListOptions { limit, cursor })
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
        let rows: Vec<Vec<String>> = page.items.iter().map(org_row).collect();
        let json_value = json!({ "items": page.raw_items, "page": page.page });
        (rows, json_value)
    } else {
        let response = api
            .list_organizations(ListOptions {
                limit: pagination.limit,
                cursor: pagination.cursor.clone(),
            })
            .await
            .map_err(CliError::from_client)?;
        let rows: Vec<Vec<String>> = response.value.items.iter().map(org_row).collect();
        (rows, response.raw)
    };

    emit_table(
        session,
        &json_value,
        &["SLUG", "NAME", "PLAN", "STATE"],
        &rows,
    )
}

fn org_row(item: &OrganizationListItem) -> Vec<String> {
    let state = if item.suspended {
        "suspended"
    } else {
        "active"
    };
    vec![
        item.slug.clone(),
        item.name.clone(),
        item.plan.clone().unwrap_or_default(),
        state.to_string(),
    ]
}

async fn view(session: &mut Session<'_>, slug: &str) -> Result<(), CliError> {
    let selection = session.selection()?;
    let api = session.api(&selection)?;
    let response = api
        .get_organization(slug)
        .await
        .map_err(CliError::from_client)?;
    let org = response.value.clone();
    emit_view(session, &response.raw, slug, |session| {
        let lines = [
            ("name", org.name.clone()),
            ("slug", org.slug.clone()),
            ("plan", org.plan.clone()),
            ("role", org.role.clone()),
            ("default", org.is_default.to_string()),
            ("suspended", org.suspended.to_string()),
        ];
        render_lines(session, &lines)
    })
}

async fn use_org(session: &mut Session<'_>, slug: &str) -> Result<(), CliError> {
    let selection = session.selection()?;
    // SPEC §35: validate the organization through the Public API before
    // persisting it, so a typo never lands in the config.
    let api = session.api(&selection)?;
    let response = api
        .get_organization(slug)
        .await
        .map_err(CliError::from_client)?;
    let org = response.value;

    let profile_name = selection
        .profile
        .clone()
        .ok_or_else(|| CliError::usage("no active profile; run `hamstik auth login` first"))?;
    let mut config = session.config.load()?;
    let profile = config
        .profiles
        .get_mut(&profile_name)
        .ok_or_else(|| CliError::config(format!("no such profile: {profile_name}")))?;
    profile.default_organization = Some(org.slug);
    session.config.save(&config)?;

    if session.json() {
        emit_json(
            session,
            &json!({ "profile": profile_name, "defaultOrganization": slug }),
        )
    } else {
        session
            .out
            .line(&format!(
                "Default organization for profile {profile_name}: {slug}"
            ))
            .map_err(CliError::general)
    }
}

pub(crate) fn render_lines(
    session: &mut Session<'_>,
    lines: &[(&str, String)],
) -> Result<(), CliError> {
    let width = lines.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in lines {
        session
            .out
            .line(&format!("{key:<width$}  {value}"))
            .map_err(CliError::general)?;
    }
    Ok(())
}
