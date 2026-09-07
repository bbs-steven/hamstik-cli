// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! `hamstik work` (list / view / create / edit / transitions / transition /
//! start / close / comment).

use serde_json::{Value, json};

use hamstik_api_client::{
    CreateCommentRequest, CreateWorkItemRequest, ListOptions, ListWorkItemsQuery, PageItems,
    TransitionRequest, UpdateWorkItemRequest, WorkItem, follow_all, generate_key, validate_key,
};

use crate::app::Session;
use crate::args::{
    CommentArgs, CommentCommand, WorkArgs, WorkAttachmentArgs, WorkAttachmentCommand, WorkCommand,
    WorkCreateArgs, WorkEditArgs, WorkLabelArgs, WorkLabelCommand, WorkListArgs,
};
use crate::error::CliError;
use crate::input::resolve_text;

use super::org::render_lines;
use super::{emit_json, emit_table, emit_view};

/// Runs the `work` subcommands.
pub async fn run(session: &mut Session<'_>, args: &WorkArgs) -> Result<(), CliError> {
    match &args.command {
        WorkCommand::List(list_args) => list(session, list_args).await,
        WorkCommand::View { key } => view(session, key).await,
        WorkCommand::Create(create_args) => create(session, create_args).await,
        WorkCommand::Edit(edit_args) => edit(session, edit_args).await,
        WorkCommand::Transitions { key } => transitions(session, key).await,
        WorkCommand::Transition { key, target } => {
            transition_to(session, key, target.as_str()).await
        }
        WorkCommand::Start { key } => transition_to(session, key, "in_progress").await,
        WorkCommand::Close { key } => transition_to(session, key, "done").await,
        WorkCommand::Label(label_args) => label(session, label_args).await,
        WorkCommand::Attachment(attachment_args) => attachment(session, attachment_args).await,
        WorkCommand::Comment(comment_args) => comment(session, comment_args).await,
    }
}

fn idem_key(flag: Option<String>) -> Result<String, CliError> {
    match flag {
        Some(key) => {
            validate_key(&key).map_err(|err| CliError::usage(err.to_string()))?;
            Ok(key)
        }
        None => Ok(generate_key()),
    }
}

fn read_text(inline: Option<String>, file: Option<&str>) -> Result<Option<String>, CliError> {
    let mut stdin = std::io::stdin();
    resolve_text(inline, file, &mut stdin)
        .map_err(|err| CliError::general(format!("cannot read text: {err}")))
}

fn build_query(args: &WorkListArgs) -> ListWorkItemsQuery {
    let assignee = args.assignee.clone().or_else(|| {
        if args.mine {
            Some("me".to_string())
        } else {
            None
        }
    });
    ListWorkItemsQuery {
        limit: args.pagination.limit,
        cursor: args.pagination.cursor.clone(),
        q: args.search.clone(),
        status: args.status.iter().map(|s| s.as_str().to_string()).collect(),
        scope: args.scope.map(|s| s.as_str().to_string()),
        item_type: args
            .item_type
            .iter()
            .map(|t| t.as_str().to_string())
            .collect(),
        priority: args
            .priority
            .iter()
            .map(|p| p.as_str().to_string())
            .collect(),
        assignee,
        sprint: args.sprint.clone(),
        label: args.label.clone(),
        label_name: args.label_name.clone(),
        parent: args.parent.clone(),
        top_level: if args.top_level { Some(true) } else { None },
        updated_after: args.updated_after.clone(),
    }
}

async fn list(session: &mut Session<'_>, args: &WorkListArgs) -> Result<(), CliError> {
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;
    let project = session.require_project(&selection)?;
    let api = session.api(&selection)?;
    let query = build_query(args);

    if args.pagination.all {
        let org = org.clone();
        let project = project.clone();
        let base = query.clone();
        let fetch_api = api.clone();
        let page = follow_all(move |cursor| {
            let fetch_api = fetch_api.clone();
            let org = org.clone();
            let project = project.clone();
            let mut query = base.clone();
            query.cursor = cursor;
            async move {
                let response = fetch_api.list_work_items(&org, &project, query).await?;
                Ok(PageItems::new(
                    response.value.items,
                    &response.raw,
                    response.value.page,
                ))
            }
        })
        .await
        .map_err(CliError::from_client)?;
        let rows: Vec<Vec<String>> = page.items.iter().map(summary_row).collect();
        let json_value = json!({ "items": page.raw_items, "page": page.page });
        emit_table(
            session,
            &json_value,
            &["KEY", "TITLE", "STATUS", "TYPE", "PRIORITY", "ASSIGNEE"],
            &rows,
        )
    } else {
        let response = api
            .list_work_items(&org, &project, query)
            .await
            .map_err(CliError::from_client)?;
        let rows: Vec<Vec<String>> = response.value.items.iter().map(summary_row).collect();
        emit_table(
            session,
            &response.raw,
            &["KEY", "TITLE", "STATUS", "TYPE", "PRIORITY", "ASSIGNEE"],
            &rows,
        )
    }
}

fn summary_row(item: &hamstik_api_client::WorkItemSummary) -> Vec<String> {
    vec![
        item.key.clone(),
        item.title.clone(),
        item.status.clone(),
        item.item_type.clone(),
        item.priority.clone(),
        item.assignee
            .clone()
            .map(|a| a.name)
            .unwrap_or_else(|| "-".to_string()),
    ]
}

async fn view(session: &mut Session<'_>, key: &str) -> Result<(), CliError> {
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;
    let project = session.require_project(&selection)?;
    let api = session.api(&selection)?;
    let response = api
        .get_work_item(&org, &project, key)
        .await
        .map_err(CliError::from_client)?;
    let item = response.value.clone();
    emit_view(session, &response.raw, key, |session| {
        render_work_item(session, &item)
    })
}

fn render_work_item(session: &mut Session<'_>, item: &WorkItem) -> Result<(), CliError> {
    let lines = [
        ("key", item.key.clone()),
        ("title", item.title.clone()),
        ("type", item.item_type.clone()),
        ("status", item.status.clone()),
        ("priority", item.priority.clone()),
        (
            "assignee",
            item.assignee
                .clone()
                .map(|a| a.name)
                .unwrap_or_else(|| "-".to_string()),
        ),
        (
            "sprint",
            item.sprint
                .clone()
                .map(|s| s.name)
                .unwrap_or_else(|| "-".to_string()),
        ),
        (
            "parent",
            item.parent
                .clone()
                .map(|p| p.key)
                .unwrap_or_else(|| "-".to_string()),
        ),
        (
            "story points",
            item.story_points
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
        (
            "due date",
            item.due_date.clone().unwrap_or_else(|| "-".to_string()),
        ),
        ("revision", item.revision.to_string()),
    ];
    render_lines(session, &lines)?;
    if let Some(description) = &item.description {
        session.out.line("").map_err(CliError::general)?;
        session.out.line(description).map_err(CliError::general)?;
    }
    Ok(())
}

async fn create(session: &mut Session<'_>, args: &WorkCreateArgs) -> Result<(), CliError> {
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;
    let project = session.require_project(&selection)?;

    let title = match &args.title {
        Some(title) => title.clone(),
        None if session.can_prompt() => session
            .prompt
            .read_line("Title: ")
            .map_err(|err| CliError::general(format!("prompt failed: {err}")))?,
        None => return Err(CliError::usage("missing required option --title")),
    };
    if title.trim().is_empty() {
        return Err(CliError::usage("title must not be empty"));
    }

    let description = read_text(args.description.clone(), args.description_file.as_deref())?;

    let body = CreateWorkItemRequest {
        title,
        description,
        item_type: args.item_type.map(|t| t.as_str().to_string()),
        status: args.status.map(|s| s.as_str().to_string()),
        priority: args.priority.map(|p| p.as_str().to_string()),
        assignee_id: args.assignee.clone(),
        sprint_id: args.sprint.clone(),
        parent_id: args.parent.clone(),
        story_points: args.story_points,
        due_date: args.due_date.clone(),
    };
    let idempotency = idem_key(args.idempotency_key.clone())?;

    let api = session.api(&selection)?;
    let response = api
        .create_work_item(&org, &project, &body, &idempotency)
        .await
        .map_err(CliError::from_client)?;
    if response.idempotency_replayed {
        session
            .out
            .warn("note: request replayed (idempotent duplicate)");
    }
    let item = response.value.clone();
    emit_view(session, &response.raw, &item.key.clone(), |session| {
        render_work_item(session, &item)
    })
}

async fn edit(session: &mut Session<'_>, args: &WorkEditArgs) -> Result<(), CliError> {
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;
    let project = session.require_project(&selection)?;
    let api = session.api(&selection)?;

    let description = if args.clear_description {
        Some(None)
    } else {
        read_text(args.description.clone(), args.description_file.as_deref())?.map(Some)
    };

    let body = UpdateWorkItemRequest {
        title: args.title.clone(),
        description,
        item_type: args.item_type.map(|t| t.as_str().to_string()),
        priority: args.priority.map(|p| p.as_str().to_string()),
        assignee_id: tri(args.clear_assignee, args.assignee.clone()),
        sprint_id: tri(args.clear_sprint, args.sprint.clone()),
        parent_id: tri(args.clear_parent, args.parent.clone()),
        story_points: tri(args.clear_story_points, args.story_points),
        due_date: tri(args.clear_due_date, args.due_date.clone()),
    };
    if body.is_empty() {
        return Err(CliError::usage("no changes specified"));
    }

    let if_match = if args.force {
        "*".to_string()
    } else {
        let current = api
            .get_work_item(&org, &project, &args.key)
            .await
            .map_err(CliError::from_client)?;
        current.etag.ok_or_else(|| {
            CliError::protocol("server did not return an ETag; re-run with --force")
        })?
    };

    let response = api
        .update_work_item(&org, &project, &args.key, &body, &if_match)
        .await
        .map_err(CliError::from_client)?;
    let item = response.value.clone();
    emit_view(session, &response.raw, &item.key.clone(), |session| {
        render_work_item(session, &item)
    })
}

/// Builds a tri-state field: `None` (unset), `Some(None)` (clear), or `Some(Some(v))`.
fn tri<T>(clear: bool, value: Option<T>) -> Option<Option<T>> {
    if clear { Some(None) } else { value.map(Some) }
}

async fn transitions(session: &mut Session<'_>, key: &str) -> Result<(), CliError> {
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;
    let project = session.require_project(&selection)?;
    let api = session.api(&selection)?;
    let response = api
        .list_transitions(&org, &project, key)
        .await
        .map_err(CliError::from_client)?;
    let list = response.value.clone();
    emit_view(session, &response.raw, key, |session| {
        session
            .out
            .line(&format!("current status: {}", list.current_status))
            .map_err(CliError::general)?;
        if list.transitions.is_empty() {
            session
                .out
                .line("(no transitions available)")
                .map_err(CliError::general)?;
        } else {
            session.out.line("available:").map_err(CliError::general)?;
            for transition in &list.transitions {
                session
                    .out
                    .line(&format!("  {}", transition.target_status))
                    .map_err(CliError::general)?;
            }
        }
        Ok(())
    })
}

async fn transition_to(session: &mut Session<'_>, key: &str, target: &str) -> Result<(), CliError> {
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;
    let project = session.require_project(&selection)?;
    let api = session.api(&selection)?;

    let current = api
        .get_work_item(&org, &project, key)
        .await
        .map_err(CliError::from_client)?;
    let current_status = current.value.status.clone();
    let etag = current.etag.clone();

    if current_status == target {
        session.out.warn(&format!("{key} is already {target}"));
        if session.json() {
            return emit_json(session, &current.raw);
        }
        let item = current.value.clone();
        return emit_view(session, &current.raw, key, |session| {
            render_work_item(session, &item)
        });
    }

    let allowed = api
        .list_transitions(&org, &project, key)
        .await
        .map_err(CliError::from_client)?;
    let permitted = allowed
        .value
        .transitions
        .iter()
        .any(|t| t.target_status == target);
    if !permitted {
        let options: Vec<&str> = allowed
            .value
            .transitions
            .iter()
            .map(|t| t.target_status.as_str())
            .collect();
        return Err(CliError::usage(format!(
            "transition to {target} is not allowed from {current_status}; available: {}",
            if options.is_empty() {
                "none".to_string()
            } else {
                options.join(", ")
            }
        )));
    }

    let if_match =
        etag.ok_or_else(|| CliError::protocol("server did not return an ETag for the work item"))?;
    let idempotency = generate_key();
    let response = api
        .transition_work_item(
            &org,
            &project,
            key,
            &TransitionRequest {
                target_status: target.to_string(),
            },
            &if_match,
            &idempotency,
        )
        .await
        .map_err(CliError::from_client)?;
    if response.idempotency_replayed {
        session
            .out
            .warn("note: request replayed (idempotent duplicate)");
    }
    let item = response.value.clone();
    emit_view(session, &response.raw, key, |session| {
        render_work_item(session, &item)
    })
}

async fn label(session: &mut Session<'_>, args: &WorkLabelArgs) -> Result<(), CliError> {
    let (command, label, key, force, idempotency_key) = match &args.command {
        WorkLabelCommand::Add {
            key,
            label,
            force,
            idempotency_key,
        } => ("add", label, key, *force, idempotency_key.as_deref()),
        WorkLabelCommand::Remove {
            key,
            label,
            force,
            idempotency_key,
        } => ("remove", label, key, *force, idempotency_key.as_deref()),
    };
    let selection = session.selection()?;
    let org = session.require_org(&selection)?;
    let project = session.require_project(&selection)?;
    let api = session.api(&selection)?;

    let if_match = if force {
        "*".to_string()
    } else {
        let current = api
            .get_work_item(&org, &project, key)
            .await
            .map_err(CliError::from_client)?;
        current.etag.ok_or_else(|| {
            CliError::protocol("server did not return an ETag; re-run with --force")
        })?
    };

    let idempotency = match idempotency_key {
        Some(key) => {
            validate_key(key).map_err(|err| CliError::usage(err.to_string()))?;
            key.to_string()
        }
        None => generate_key(),
    };

    let response = match command {
        "add" => api
            .attach_label(&org, &project, key, label, &if_match, &idempotency)
            .await
            .map_err(CliError::from_client)?,
        _ => api
            .detach_label(&org, &project, key, label, &if_match, &idempotency)
            .await
            .map_err(CliError::from_client)?,
    };
    if response.idempotency_replayed {
        session
            .out
            .warn("note: request replayed (idempotent duplicate)");
    }
    let item = response.value.clone();
    emit_view(session, &response.raw, &item.key.clone(), |session| {
        render_work_item(session, &item)
    })
}

async fn attachment(session: &mut Session<'_>, args: &WorkAttachmentArgs) -> Result<(), CliError> {
    match &args.command {
        WorkAttachmentCommand::List { key, pagination } => {
            let selection = session.selection()?;
            let org = session.require_org(&selection)?;
            let project = session.require_project(&selection)?;
            let api = session.api(&selection)?;
            let response = api
                .list_attachments(
                    &org,
                    &project,
                    key,
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
                .map(|a| {
                    vec![
                        a.id.clone(),
                        a.file_name.clone(),
                        a.content_type.clone(),
                        a.size.to_string(),
                        a.created_by
                            .clone()
                            .map(|u| u.name)
                            .unwrap_or_else(|| "-".to_string()),
                        a.created_at.clone(),
                    ]
                })
                .collect();
            emit_table(
                session,
                &response.raw,
                &["ID", "FILE", "TYPE", "SIZE", "BY", "CREATED"],
                &rows,
            )
        }
        WorkAttachmentCommand::Upload {
            key,
            file,
            file_name,
            content_type,
            idempotency_key,
        } => {
            let selection = session.selection()?;
            let org = session.require_org(&selection)?;
            let project = session.require_project(&selection)?;
            let (bytes, name) = read_upload(file, file_name.as_deref()).map_err(CliError::usage)?;
            let upload = hamstik_api_client::client::MultipartFile {
                file_name: name,
                content_type: content_type.clone(),
                bytes,
            };
            let idempotency = idem_key(idempotency_key.clone())?;
            let api = session.api(&selection)?;
            let response = api
                .upload_attachment(&org, &project, key, &upload, &idempotency)
                .await
                .map_err(CliError::from_client)?;
            if response.idempotency_replayed {
                session
                    .out
                    .warn("note: request replayed (idempotent duplicate)");
            }
            let attachment_id = response.value.id.clone();
            emit_view(session, &response.raw, &attachment_id.clone(), |session| {
                render_attachment(session, &response.value)
            })
        }
        WorkAttachmentCommand::Download {
            key,
            attachment_id,
            output,
        } => {
            let selection = session.selection()?;
            let org = session.require_org(&selection)?;
            let project = session.require_project(&selection)?;
            let api = session.api(&selection)?;
            let download = api
                .download_attachment(&org, &project, key, attachment_id)
                .await
                .map_err(CliError::from_client)?;
            let target = match output {
                Some(path) => std::path::PathBuf::from(path),
                None => {
                    let name = download
                        .file_name
                        .clone()
                        .unwrap_or_else(|| format!("{attachment_id}.bin"));
                    // Refuse to write outside the current directory implicitly:
                    // use only the final path component of the server-suggested name.
                    let safe = name.rsplit(['/', '\\']).next().unwrap_or(&name);
                    std::path::PathBuf::from(safe)
                }
            };
            std::fs::write(&target, &download.bytes).map_err(|err| {
                CliError::general(format!("cannot write {}: {err}", target.display()))
            })?;
            if session.json() {
                emit_json(
                    session,
                    &json!({
                        "attachmentId": attachment_id,
                        "path": target.display().to_string(),
                        "size": download.bytes.len(),
                    }),
                )
            } else {
                session
                    .out
                    .line(&format!(
                        "Downloaded {} ({} bytes) to {}",
                        attachment_id,
                        download.bytes.len(),
                        target.display()
                    ))
                    .map_err(CliError::general)
            }
        }
        WorkAttachmentCommand::Delete { key, attachment_id } => {
            let selection = session.selection()?;
            let org = session.require_org(&selection)?;
            let project = session.require_project(&selection)?;
            let api = session.api(&selection)?;
            api.delete_attachment(&org, &project, key, attachment_id)
                .await
                .map_err(CliError::from_client)?;
            if session.json() {
                emit_json(
                    session,
                    &json!({ "deleted": true, "attachmentId": attachment_id }),
                )
            } else {
                session
                    .out
                    .line(&format!("Deleted attachment {attachment_id}"))
                    .map_err(CliError::general)
            }
        }
    }
}

fn render_attachment(
    session: &mut Session<'_>,
    attachment: &hamstik_api_client::Attachment,
) -> Result<(), CliError> {
    let lines = [
        ("id", attachment.id.clone()),
        ("file", attachment.file_name.clone()),
        ("type", attachment.content_type.clone()),
        ("size", attachment.size.to_string()),
        (
            "by",
            attachment
                .created_by
                .clone()
                .map(|u| u.name)
                .unwrap_or_else(|| "-".to_string()),
        ),
        ("created", attachment.created_at.clone()),
    ];
    render_lines(session, &lines)
}

/// Reads upload bytes from a path or stdin (`-`), capping at the response-body
/// budget (10 MiB), and derives a default file name from the path.
fn read_upload(file: &str, explicit_name: Option<&str>) -> Result<(Vec<u8>, String), String> {
    const MAX_UPLOAD_BYTES: usize = hamstik_api_client::client::MAX_BODY_BYTES;
    let (bytes, default_name) = if file == "-" {
        use std::io::Read;
        let mut buffer = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buffer)
            .map_err(|err| format!("cannot read stdin: {err}"))?;
        if buffer.len() > MAX_UPLOAD_BYTES {
            return Err(format!("upload exceeds the {MAX_UPLOAD_BYTES} byte limit"));
        }
        (buffer, "attachment".to_string())
    } else {
        let path = std::path::Path::new(file);
        let metadata =
            std::fs::metadata(path).map_err(|err| format!("cannot read {file}: {err}"))?;
        if metadata.len() > MAX_UPLOAD_BYTES as u64 {
            return Err(format!("upload exceeds the {MAX_UPLOAD_BYTES} byte limit"));
        }
        let bytes = std::fs::read(path).map_err(|err| format!("cannot read {file}: {err}"))?;
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("attachment")
            .to_string();
        (bytes, name)
    };
    let name = explicit_name.map(str::to_string).unwrap_or(default_name);
    Ok((bytes, name))
}

async fn comment(session: &mut Session<'_>, args: &CommentArgs) -> Result<(), CliError> {
    match &args.command {
        CommentCommand::List { key, pagination } => {
            let selection = session.selection()?;
            let org = session.require_org(&selection)?;
            let project = session.require_project(&selection)?;
            let api = session.api(&selection)?;
            let response = api
                .list_comments(
                    &org,
                    &project,
                    key,
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
                .map(|c| {
                    let body = c.body.clone().unwrap_or_else(|| "(deleted)".to_string());
                    let preview: String = body.chars().take(60).collect();
                    vec![c.author.name.clone(), c.created_at.clone(), preview]
                })
                .collect();
            emit_table(
                session,
                &response.raw,
                &["AUTHOR", "CREATED", "BODY"],
                &rows,
            )
        }
        CommentCommand::Add {
            key,
            body,
            body_file,
            parent,
            idempotency_key,
        } => {
            let selection = session.selection()?;
            let org = session.require_org(&selection)?;
            let project = session.require_project(&selection)?;
            let text = read_text(body.clone(), body_file.as_deref())?.ok_or_else(|| {
                CliError::usage("missing comment body; use --body or --body-file")
            })?;
            if text.trim().is_empty() {
                return Err(CliError::usage("comment body must not be empty"));
            }
            let idempotency = idem_key(idempotency_key.clone())?;
            let api = session.api(&selection)?;
            let response = api
                .create_comment(
                    &org,
                    &project,
                    key,
                    &CreateCommentRequest {
                        body: text,
                        parent_comment_id: parent.clone(),
                    },
                    &idempotency,
                )
                .await
                .map_err(CliError::from_client)?;
            if response.idempotency_replayed {
                session
                    .out
                    .warn("note: request replayed (idempotent duplicate)");
            }
            let comment_id = response.value.id.clone();
            emit_view(session, &response.raw, &comment_id.clone(), |session| {
                render_comment(session, &response.raw)
            })
        }
        CommentCommand::Delete { key, comment_id } => {
            let selection = session.selection()?;
            let org = session.require_org(&selection)?;
            let project = session.require_project(&selection)?;
            let api = session.api(&selection)?;
            api.delete_comment(&org, &project, key, comment_id)
                .await
                .map_err(CliError::from_client)?;
            if session.json() {
                emit_json(
                    session,
                    &json!({ "deleted": true, "commentId": comment_id }),
                )
            } else {
                session
                    .out
                    .line(&format!("Deleted comment {comment_id}"))
                    .map_err(CliError::general)
            }
        }
    }
}

fn render_comment(session: &mut Session<'_>, raw: &Value) -> Result<(), CliError> {
    let body = raw
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("(no body)");
    session.out.line(body).map_err(CliError::general)?;
    Ok(())
}
