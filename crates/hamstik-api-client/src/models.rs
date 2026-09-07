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
    /// The user's display name.
    pub name: String,
    /// The user's email address.
    pub email: String,
    /// How the request was authenticated.
    pub authentication: AuthenticationContext,
    /// The user's default organization, when one is set.
    pub default_organization: Option<OrganizationSummary>,
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

/// A label resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    /// The label's id.
    pub id: String,
    /// The label's display name.
    pub name: String,
    /// Display color as a hex string.
    pub color: String,
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
            "id":"u1","name":"N","email":"n@x",
            "authentication":{"type":"pat","credentialId":"c","credentialName":"n","scopes":[],"expiresAt":"2027-01-01T00:00:00Z"},
            "defaultOrganization":null
        }"##;
        let me: Me = serde_json::from_str(raw).unwrap();
        assert_eq!(me.authentication.expires_at, "2027-01-01T00:00:00Z");
    }
}
