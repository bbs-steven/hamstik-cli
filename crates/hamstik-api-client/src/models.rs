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
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

/// Filters accepted by `GET .../work-items`.
#[derive(Debug, Clone, Default)]
pub struct ListWorkItemsQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub q: Option<String>,
    pub status: Vec<String>,
    pub scope: Option<String>,
    pub item_type: Vec<String>,
    pub priority: Vec<String>,
    pub assignee: Option<String>,
    pub sprint: Option<String>,
    pub label: Vec<String>,
    pub label_name: Vec<String>,
    pub parent: Option<String>,
    pub top_level: Option<bool>,
    pub updated_after: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticationContext {
    #[serde(rename = "type")]
    pub auth_type: String,
    pub credential_id: String,
    pub credential_name: String,
    pub scopes: Vec<String>,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationSummary {
    pub id: String,
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub id: String,
    pub name: String,
    pub email: String,
    pub authentication: AuthenticationContext,
    pub default_organization: Option<OrganizationSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub plan: String,
    pub role: String,
    pub is_default: bool,
    pub suspended: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// A member of an organization list response.
///
/// The contract allows either a full [`Organization`] or a compact summary, so
/// the richer fields are optional to satisfy both shapes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationListItem {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub suspended: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub is_default: Option<bool>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationList {
    pub items: Vec<OrganizationListItem>,
    pub page: Page,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub organization_id: String,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectList {
    pub items: Vec<Project>,
    pub page: Page,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprintSummary {
    pub id: String,
    pub name: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemSummary {
    pub id: String,
    pub key: String,
    pub title: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub status: String,
    pub priority: String,
    pub assignee: Option<UserSummary>,
    pub sprint: Option<SprintSummary>,
    pub parent_id: Option<String>,
    pub story_points: Option<i64>,
    pub due_date: Option<String>,
    pub updated_at: String,
    pub revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemParent {
    pub id: String,
    pub key: String,
    pub title: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub status: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub id: String,
    pub key: String,
    pub project_id: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub item_type: String,
    pub status: String,
    pub priority: String,
    pub assignee: Option<UserSummary>,
    pub reporter: Option<UserSummary>,
    pub sprint: Option<SprintSummary>,
    pub parent: Option<WorkItemParent>,
    pub labels: Vec<Label>,
    pub story_points: Option<i64>,
    pub due_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemList {
    pub items: Vec<WorkItemSummary>,
    pub page: Page,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemTransition {
    pub target_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemTransitionList {
    pub current_status: String,
    pub transitions: Vec<WorkItemTransition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub work_item_id: String,
    pub parent_comment_id: Option<String>,
    pub author: UserSummary,
    pub body: Option<String>,
    pub deleted: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentList {
    pub items: Vec<Comment>,
    pub page: Page,
}

/// Body for `POST .../work-items`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkItemRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_points: Option<i64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_points: Option<Option<i64>>,
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
    pub target_status: String,
}

/// Body for `POST .../comments`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCommentRequest {
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_comment_id: Option<String>,
}

#[cfg(test)]
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
    fn organization_list_item_accepts_summary_shape() {
        let raw = r#"{"id":"1","slug":"s","name":"S","suspended":false}"#;
        let item: OrganizationListItem = serde_json::from_str(raw).unwrap();
        assert_eq!(item.slug, "s");
        assert!(item.role.is_none());
    }

    #[test]
    fn update_request_serializes_tri_state() {
        let req = UpdateWorkItemRequest {
            description: Some(None),
            priority: Some("high".to_string()),
            ..Default::default()
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value["description"], serde_json::Value::Null);
        assert_eq!(value["priority"], "high");
        assert!(value.get("title").is_none());
        assert!(!req.is_empty());
    }

    #[test]
    fn update_request_default_is_empty() {
        assert!(UpdateWorkItemRequest::default().is_empty());
    }

    #[test]
    fn create_request_omits_absent_optionals() {
        let req = CreateWorkItemRequest {
            title: "Hi".to_string(),
            ..Default::default()
        };
        let value = serde_json::to_value(&req).unwrap();
        assert_eq!(value, serde_json::json!({ "title": "Hi" }));
    }

    #[test]
    fn me_roundtrip() {
        let raw = r#"{
            "id":"1","name":"Steven","email":"s@example.com",
            "authentication":{"type":"pat","credentialId":"c","credentialName":"n","scopes":[],"expiresAt":"2027-01-01T00:00:00Z"},
            "defaultOrganization":null
        }"#;
        let me: Me = serde_json::from_str(raw).unwrap();
        assert!(me.default_organization.is_none());
        assert_eq!(me.authentication.auth_type, "pat");
    }
}
