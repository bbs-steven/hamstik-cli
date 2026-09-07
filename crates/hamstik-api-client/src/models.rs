// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Wire models mirroring the Hamstik Public API v1 contract.
//!
//! Scalar identifiers are modeled as `String` and timestamps as `String`
//! (RFC 3339) to remain resilient to additive server changes and to avoid
//! coupling the CLI to a date/time crate. Known enum values are validated on the
//! request side (CLI); responses are stored verbatim.

use serde::{Deserialize, Serialize};

use crate::pagination::Page;

/// A paginated list query for endpoints that only take `limit`/`cursor`.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// Maximum items per page (server-capped).
    pub limit: Option<u32>,
    /// Opaque continuation cursor from a previous page's `nextCursor`.
    pub cursor: Option<String>,
}

/// Filters accepted by `GET .../work-items`.
#[derive(Debug, Clone, Default)]
pub struct ListWorkItemsQuery {
    /// Maximum items per page (server-capped).
    pub limit: Option<u32>,
    /// Opaque continuation cursor from a previous page's `nextCursor`.
    pub cursor: Option<String>,
    /// Full-text search over titles and descriptions.
    pub q: Option<String>,
    /// Filter by status; multiple values are OR-ed.
    pub status: Vec<String>,
    /// Hierarchy scope of the result set (server-defined, e.g. `all`).
    pub scope: Option<String>,
    /// Filter by work item type; multiple values are OR-ed.
    pub item_type: Vec<String>,
    /// Filter by priority; multiple values are OR-ed.
    pub priority: Vec<String>,
    /// Filter by assignee user id (`me` is accepted by the server).
    pub assignee: Option<String>,
    /// Filter by sprint id.
    pub sprint: Option<String>,
    /// Filter by label id; multiple values are OR-ed.
    pub label: Vec<String>,
    /// Filter by label name; multiple values are OR-ed.
    pub label_name: Vec<String>,
    /// Only items with this parent id.
    pub parent: Option<String>,
    /// Only items without a parent (top-level).
    pub top_level: Option<bool>,
    /// Only items updated after this RFC 3339 timestamp.
    pub updated_after: Option<String>,
}

/// The authenticated user's identity, from `GET /me`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    /// The user's id.
    pub id: String,
    /// The user's immutable public identifier (e.g. `usr_...`).
    #[serde(default)]
    pub public_id: Option<String>,
    /// The user's display name.
    pub name: String,
    /// The user's email address.
    pub email: String,
    /// How the request was authenticated.
    pub authentication: AuthenticationContext,
    /// The user's default organization, when one is set.
    pub default_organization: Option<OrganizationSummary>,
    /// The Organizations available through the credential, each with the
    /// Organization-scoped username.
    #[serde(default)]
    pub organizations: Vec<MeOrganization>,
}

/// An Organization available to the authenticated user.
///
/// A username is a membership identity unique only inside its Organization
/// and may be absent when the membership has no username.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeOrganization {
    /// The organization's id.
    pub id: String,
    /// The organization's URL slug.
    pub slug: String,
    /// The organization's display name.
    pub name: String,
    /// The user's username inside this Organization, when one is set.
    #[serde(default)]
    pub username: Option<String>,
}

/// How the current request was authenticated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticationContext {
    /// Credential kind (e.g. `pat`).
    #[serde(rename = "type")]
    pub auth_type: String,
    /// The server-side credential id.
    pub credential_id: String,
    /// The credential's human-readable name.
    pub credential_name: String,
    /// Scopes granted to the credential.
    pub scopes: Vec<String>,
    /// When the credential expires (RFC 3339).
    pub expires_at: String,
}

/// A compact organization reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationSummary {
    /// The organization's id.
    pub id: String,
    /// The organization's URL slug.
    pub slug: String,
    /// The organization's display name.
    pub name: String,
}

/// A full organization resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    /// The organization's id.
    pub id: String,
    /// The organization's URL slug.
    pub slug: String,
    /// The organization's display name.
    pub name: String,
    /// Free-form description, when set.
    pub description: Option<String>,
    /// Billing plan identifier.
    pub plan: String,
    /// The current user's role in the organization.
    pub role: String,
    /// Whether this is the user's default organization.
    pub is_default: bool,
    /// Whether the organization is suspended.
    pub suspended: bool,
    /// Creation timestamp (RFC 3339).
    pub created_at: String,
    /// Last-update timestamp (RFC 3339).
    pub updated_at: String,
}

/// A member of an organization list response.
///
/// The contract allows either a full [`Organization`] or a compact summary, so
/// the richer fields are optional to satisfy both shapes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationListItem {
    /// The organization's id.
    pub id: String,
    /// The organization's URL slug.
    pub slug: String,
    /// The organization's display name.
    pub name: String,
    /// Whether the organization is suspended.
    pub suspended: bool,
    /// Free-form description, when set.
    #[serde(default)]
    pub description: Option<String>,
    /// Billing plan identifier, when included.
    #[serde(default)]
    pub plan: Option<String>,
    /// The current user's role, when included.
    #[serde(default)]
    pub role: Option<String>,
    /// Whether this is the user's default organization, when included.
    #[serde(default)]
    pub is_default: Option<bool>,
    /// Creation timestamp, when included.
    #[serde(default)]
    pub created_at: Option<String>,
    /// Last-update timestamp, when included.
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// A paginated organization list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationList {
    /// One page of organizations.
    pub items: Vec<OrganizationListItem>,
    /// Pagination metadata.
    pub page: Page,
}

/// A project resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// The project's id.
    pub id: String,
    /// The id of the organization the project belongs to.
    pub organization_id: String,
    /// The project's short key (used in paths).
    pub key: String,
    /// The project's display name.
    pub name: String,
    /// Free-form description, when set.
    pub description: Option<String>,
    /// Display color as a hex string.
    pub color: String,
    /// Creation timestamp (RFC 3339).
    pub created_at: String,
    /// Last-update timestamp (RFC 3339).
    pub updated_at: String,
}

/// A paginated project list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectList {
    /// One page of projects.
    pub items: Vec<Project>,
    /// Pagination metadata.
    pub page: Page,
}

/// A compact user reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummary {
    /// The user's id.
    pub id: String,
    /// The user's display name.
    pub name: String,
}

/// A compact sprint reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprintSummary {
    /// The sprint's id.
    pub id: String,
    /// The sprint's display name.
    pub name: String,
    /// The sprint's state (server-defined).
    pub state: String,
}

/// A full Sprint resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sprint {
    /// The sprint's id.
    pub id: String,
    /// The sprint's display name.
    pub name: String,
    /// The sprint's lifecycle state (`future`, `active`, or `done`).
    pub state: String,
    /// Planned start date (RFC 3339), when the Sprint is dated.
    pub start_date: Option<String>,
    /// Planned end date (RFC 3339), when the Sprint is dated.
    pub end_date: Option<String>,
    /// The Sprint goal, when set.
    pub goal: Option<String>,
    /// The target story-point total, when set.
    pub target_points: Option<i64>,
    /// Creation timestamp (RFC 3339).
    pub created_at: String,
    /// Last-update timestamp (RFC 3339).
    pub updated_at: String,
    /// Optimistic-concurrency revision (matches the `sprint-N` `ETag`).
    pub revision: i64,
}

/// A paginated Sprint list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprintList {
    /// One page of sprints.
    pub items: Vec<Sprint>,
    /// Pagination metadata.
    pub page: Page,
}

/// One permitted Sprint state transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SprintTransition {
    /// The state this transition moves the Sprint to (`active` or `done`).
    pub target_state: String,
    /// Whether completing this Sprint requires a completion action
    /// (unfinished Work Items remain).
    pub requires_completion_action: bool,
}

/// The transition surface of a Sprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SprintTransitionList {
    /// The Sprint's current state.
    pub current_state: String,
    /// Transitions permitted from the current state.
    pub transitions: Vec<SprintTransition>,
}

/// A label resource on a work item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    /// The label's id.
    pub id: String,
    /// The label's display name.
    pub name: String,
    /// Display color as a hex string.
    pub color: String,
}

/// A Project label resource (from the Project label list/create endpoints).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLabel {
    /// The label's id.
    pub id: String,
    /// The label's display name (stored lowercase).
    pub name: String,
    /// Display color as a hex string.
    pub color: String,
    /// Creation timestamp (RFC 3339).
    pub created_at: String,
}

/// A paginated Project label list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLabelList {
    /// One page of labels.
    pub items: Vec<ProjectLabel>,
    /// Pagination metadata.
    pub page: Page,
}

/// A work item in list responses (summary shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemSummary {
    /// The work item's id.
    pub id: String,
    /// The work item's human key (e.g. `HAM-1`).
    pub key: String,
    /// The work item's title.
    pub title: String,
    /// The work item type (e.g. `task`).
    #[serde(rename = "type")]
    pub item_type: String,
    /// The current status.
    pub status: String,
    /// The current priority.
    pub priority: String,
    /// The assignee, when assigned.
    pub assignee: Option<UserSummary>,
    /// The sprint, when scheduled.
    pub sprint: Option<SprintSummary>,
    /// The parent work item id, when nested.
    pub parent_id: Option<String>,
    /// Story points, when estimated.
    pub story_points: Option<i64>,
    /// Due date (RFC 3339), when set.
    pub due_date: Option<String>,
    /// Last-update timestamp (RFC 3339).
    pub updated_at: String,
    /// Optimistic-concurrency revision.
    pub revision: i64,
}

/// The parent reference on a full work item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemParent {
    /// The parent's id.
    pub id: String,
    /// The parent's human key.
    pub key: String,
    /// The parent's title.
    pub title: String,
    /// The parent's type.
    #[serde(rename = "type")]
    pub item_type: String,
    /// The parent's status.
    pub status: String,
    /// The parent's priority.
    pub priority: String,
}

/// A full work item resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    /// The work item's id.
    pub id: String,
    /// The work item's human key (e.g. `HAM-1`).
    pub key: String,
    /// The id of the project the item belongs to.
    pub project_id: String,
    /// The work item's title.
    pub title: String,
    /// Free-form description, when set.
    pub description: Option<String>,
    /// The work item type (e.g. `task`).
    #[serde(rename = "type")]
    pub item_type: String,
    /// The current status.
    pub status: String,
    /// The current priority.
    pub priority: String,
    /// The assignee, when assigned.
    pub assignee: Option<UserSummary>,
    /// The reporter, when recorded.
    pub reporter: Option<UserSummary>,
    /// The sprint, when scheduled.
    pub sprint: Option<SprintSummary>,
    /// The parent work item, when nested.
    pub parent: Option<WorkItemParent>,
    /// Labels attached to the item.
    pub labels: Vec<Label>,
    /// Story points, when estimated.
    pub story_points: Option<i64>,
    /// Due date (RFC 3339), when set.
    pub due_date: Option<String>,
    /// Creation timestamp (RFC 3339).
    pub created_at: String,
    /// Last-update timestamp (RFC 3339).
    pub updated_at: String,
    /// Optimistic-concurrency revision (matches the `ETag`).
    pub revision: i64,
}

/// A paginated work item list (summary shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemList {
    /// One page of work item summaries.
    pub items: Vec<WorkItemSummary>,
    /// Pagination metadata.
    pub page: Page,
}

/// One permitted status transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemTransition {
    /// The status this transition moves the item to.
    pub target_status: String,
}

/// The transition surface of a work item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemTransitionList {
    /// The item's current status.
    pub current_status: String,
    /// Transitions permitted from the current status.
    pub transitions: Vec<WorkItemTransition>,
}

/// A comment resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    /// The comment's id.
    pub id: String,
    /// The id of the work item the comment belongs to.
    pub work_item_id: String,
    /// The parent comment id, when this is a reply.
    pub parent_comment_id: Option<String>,
    /// The comment's author.
    pub author: UserSummary,
    /// The comment body; absent when the comment was deleted.
    pub body: Option<String>,
    /// Whether the comment was deleted.
    pub deleted: bool,
    /// Creation timestamp (RFC 3339).
    pub created_at: String,
    /// Last-update timestamp (RFC 3339).
    pub updated_at: String,
}

/// A paginated comment list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentList {
    /// One page of comments.
    pub items: Vec<Comment>,
    /// Pagination metadata.
    pub page: Page,
}

/// A Work Item attachment resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    /// The attachment's id.
    pub id: String,
    /// The id of the work item the attachment belongs to.
    pub work_item_id: String,
    /// The sanitized file name.
    pub file_name: String,
    /// The MIME content type.
    pub content_type: String,
    /// The file size in bytes.
    pub size: i64,
    /// The creator, when recorded.
    pub created_by: Option<UserSummary>,
    /// Creation timestamp (RFC 3339).
    pub created_at: String,
}

/// A paginated attachment list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentList {
    /// One page of attachments.
    pub items: Vec<Attachment>,
    /// Pagination metadata.
    pub page: Page,
}

/// Body for `POST .../work-items`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkItemRequest {
    /// The new item's title (required).
    pub title: String,
    /// The new item's description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The item type, when not the server default.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    /// The initial status, when not the server default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The priority, when not the server default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// The assignee's user id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    /// The sprint id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint_id: Option<String>,
    /// The parent work item id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Story points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_points: Option<i64>,
    /// Due date (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
}

/// Body for `PATCH .../work-items/{key}`.
///
/// Uses tri-state `Option<Option<T>>` fields: outer `None` omits the field,
/// `Some(None)` sends an explicit `null` (clear), `Some(Some(v))` sets a value.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkItemRequest {
    /// New title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// New description; `Some(None)` clears it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    /// New item type.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    /// New priority.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// New assignee; `Some(None)` unassigns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<Option<String>>,
    /// New sprint; `Some(None)` unschedules.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint_id: Option<Option<String>>,
    /// New parent; `Some(None)` detaches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Option<String>>,
    /// New story points; `Some(None)` clears the estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_points: Option<Option<i64>>,
    /// New due date; `Some(None)` clears it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<Option<String>>,
}

impl UpdateWorkItemRequest {
    /// Returns true when the request would send at least one field.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.item_type.is_none()
            && self.priority.is_none()
            && self.assignee_id.is_none()
            && self.sprint_id.is_none()
            && self.parent_id.is_none()
            && self.story_points.is_none()
            && self.due_date.is_none()
    }
}

/// Body for `POST .../transitions`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionRequest {
    /// The status to move the item to (validated by the server).
    pub target_status: String,
}

/// Body for `POST .../comments`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCommentRequest {
    /// The comment body.
    pub body: String,
    /// The parent comment id, when replying.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_comment_id: Option<String>,
}

/// Body for `POST .../projects`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    /// The new project's name (required).
    pub name: String,
    /// The project's short key; omitted to use the server's canonical
    /// suggestion from the name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Free-form description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Display color as a hex string (e.g. `#3b82f6`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Body for `POST .../sprints`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSprintRequest {
    /// The new sprint's name (required).
    pub name: String,
    /// Planned start date (RFC 3339); `Some(None)` sends an explicit null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<Option<String>>,
    /// Planned end date (RFC 3339); `Some(None)` sends an explicit null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<Option<String>>,
    /// The Sprint goal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    /// The target story-point total.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_points: Option<i64>,
}

/// What happens to unfinished Work Items when a Sprint is completed.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "mode", rename_all_fields = "camelCase")]
pub enum CompletionAction {
    /// Move unfinished Work Items back to the backlog.
    #[serde(rename = "backlog")]
    Backlog,
    /// Move unfinished Work Items to a future Sprint in the same Project.
    #[serde(rename = "sprint")]
    Sprint {
        /// The future Sprint that receives the unfinished Work Items.
        target_sprint_id: String,
    },
}

/// Body for `POST .../sprints/{id}/transitions`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionSprintRequest {
    /// The state to move the Sprint to (`active` or `done`).
    pub target_state: String,
    /// Required when completing a Sprint with unfinished Work Items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_action: Option<CompletionAction>,
}

/// Body for `POST .../labels` (create a Project label).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLabelRequest {
    /// The label's name (trimmed and stored lowercase by the server).
    pub name: String,
    /// Display color as a hex string; the server defaults to `#6366f1`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[cfg(test)]
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn work_item_roundtrips_full_resource() {
        let raw = r##"{
            "id":"1","key":"HAM-1","projectId":"2","title":"T","description":null,
            "type":"task","status":"todo","priority":"low","assignee":null,"reporter":{"id":"r","name":"R"},
            "sprint":null,"parent":null,"labels":[{"id":"l","name":"api","color":"#fff"}],
            "storyPoints":3,"dueDate":null,"createdAt":"2026-01-01T00:00:00Z",
            "updatedAt":"2026-01-02T00:00:00Z","revision":4
        }"##;
        let wi: WorkItem = serde_json::from_str(raw).unwrap();
        assert_eq!(wi.item_type, "task");
        assert_eq!(wi.reporter.as_ref().unwrap().name, "R");
        assert_eq!(wi.labels.len(), 1);
        assert_eq!(wi.revision, 4);
    }

    #[test]
    fn organization_list_item_accepts_both_shapes() {
        let raw = r#"{"id":"1","slug":"acme","name":"Acme","suspended":false}"#;
        let item: OrganizationListItem = serde_json::from_str(raw).unwrap();
        assert_eq!(item.plan, None);
    }

    #[test]
    fn create_request_serializes_camel_case() {
        let req = CreateWorkItemRequest {
            title: "T".into(),
            story_points: Some(3),
            ..Default::default()
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["storyPoints"], 3);
        assert!(value.get("assigneeId").is_none());
    }

    #[test]
    fn update_request_serializes_tri_state_clear() {
        let req = UpdateWorkItemRequest {
            assignee_id: Some(None),
            ..Default::default()
        };
        let value = serde_json::to_value(&req).unwrap();
        assert!(value["assigneeId"].is_null());
    }

    #[test]
    fn me_parses_camel_case() {
        let raw = r##"{
            "id":"u1","publicId":"usr_cPbfeqnghA-RLpDVOMQhHg","name":"N","email":"n@x",
            "authentication":{"type":"pat","credentialId":"c","credentialName":"n","scopes":[],"expiresAt":"2027-01-01T00:00:00Z"},
            "defaultOrganization":null,
            "organizations":[{"id":"o1","slug":"acme","name":"Acme","username":"n"}]
        }"##;
        let me: Me = serde_json::from_str(raw).unwrap();
        assert_eq!(me.authentication.expires_at, "2027-01-01T00:00:00Z");
        assert_eq!(me.public_id.as_deref(), Some("usr_cPbfeqnghA-RLpDVOMQhHg"));
        assert_eq!(me.organizations[0].username.as_deref(), Some("n"));
    }

    #[test]
    fn me_accepts_legacy_shape_without_new_fields() {
        let raw = r##"{
            "id":"u1","name":"N","email":"n@x",
            "authentication":{"type":"pat","credentialId":"c","credentialName":"n","scopes":[],"expiresAt":"2027-01-01T00:00:00Z"},
            "defaultOrganization":null
        }"##;
        let me: Me = serde_json::from_str(raw).unwrap();
        assert_eq!(me.public_id, None);
        assert!(me.organizations.is_empty());
    }

    #[test]
    fn sprint_roundtrips_full_resource() {
        let raw = r##"{
            "id":"s1","name":"Sprint 1","state":"active","startDate":"2026-09-01T00:00:00Z",
            "endDate":null,"goal":"Ship","targetPoints":40,
            "createdAt":"2026-08-01T00:00:00Z","updatedAt":"2026-09-01T00:00:00Z","revision":2
        }"##;
        let sprint: Sprint = serde_json::from_str(raw).unwrap();
        assert_eq!(sprint.state, "active");
        assert_eq!(sprint.target_points, Some(40));
        assert_eq!(sprint.revision, 2);
    }

    #[test]
    fn sprint_transition_list_parses() {
        let raw = r##"{
            "currentState":"active",
            "transitions":[{"targetState":"done","requiresCompletionAction":true}]
        }"##;
        let list: SprintTransitionList = serde_json::from_str(raw).unwrap();
        assert_eq!(list.current_state, "active");
        assert!(list.transitions[0].requires_completion_action);
    }

    #[test]
    fn attachment_parses_camel_case() {
        let raw = r##"{
            "id":"a1","workItemId":"w1","fileName":"design.png","contentType":"image/png",
            "size":1234,"createdBy":{"id":"u","name":"U"},"createdAt":"2026-01-01T00:00:00Z"
        }"##;
        let attachment: Attachment = serde_json::from_str(raw).unwrap();
        assert_eq!(attachment.file_name, "design.png");
        assert_eq!(attachment.size, 1234);
    }

    #[test]
    fn completion_action_serializes_tagged() {
        let backlog = serde_json::to_value(CompletionAction::Backlog).unwrap();
        assert_eq!(backlog["mode"], "backlog");
        let sprint = serde_json::to_value(CompletionAction::Sprint {
            target_sprint_id: "s2".into(),
        })
        .unwrap();
        assert_eq!(sprint["mode"], "sprint");
        assert_eq!(sprint["targetSprintId"], "s2");
    }

    #[test]
    fn create_sprint_request_omits_unset_nullables() {
        let req = CreateSprintRequest {
            name: "S".into(),
            ..Default::default()
        };
        let value = serde_json::to_value(&req).unwrap();
        assert!(value.get("startDate").is_none());
        assert!(value.get("targetPoints").is_none());
    }
}
