// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! End-to-end CLI tests: drive the compiled `hamstik` binary against a local
//! wiremock server. Credentials use the ephemeral `HAMSTIK_TOKEN` path so the
//! tests never touch an OS keyring.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A page envelope helper.
fn page(items: Value) -> Value {
    json!({ "items": items, "page": { "limit": 50, "hasMore": false, "nextCursor": null } })
}

fn work_item_json(status: &str, revision: i64) -> Value {
    json!({
        "id": "1", "key": "HAM-1", "projectId": "2", "title": "T", "description": null,
        "type": "task", "status": status, "priority": "low", "assignee": null, "reporter": null,
        "sprint": null, "parent": null, "labels": [],
        "storyPoints": null, "dueDate": null,
        "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z", "revision": revision
    })
}

fn me_json() -> Value {
    json!({
        "id": "u1", "name": "Steven", "email": "steven@example.com",
        "authentication": {"type": "pat", "credentialId": "c", "credentialName": "n", "scopes": [], "expiresAt": "2027-01-01T00:00:00Z"},
        "defaultOrganization": null
    })
}

/// Base command wired to `server` + a throwaway config, using an ephemeral token.
fn base(server: &MockServer, dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("hamstik").expect("hamstik binary");
    cmd.env("HAMSTIK_CONFIG", dir.path().join("config.toml"));
    cmd.env("HAMSTIK_HOST", server.uri());
    cmd.env("HAMSTIK_TOKEN", "secret-token");
    cmd.env_remove("HAMSTIK_PROFILE");
    cmd.env_remove("HAMSTIK_ORG");
    cmd.env_remove("HAMSTIK_PROJECT");
    cmd.current_dir(dir.path());
    cmd.arg("--no-retry");
    cmd
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn org_list_json_emits_raw_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([
            {"id": "o1", "slug": "acme", "name": "Acme", "suspended": false, "plan": "pro"}
        ]))))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args(["org", "list", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["items"][0]["slug"], "acme");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn org_list_renders_table() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([
            {"id": "o1", "slug": "acme", "name": "Acme", "suspended": false, "plan": "pro"}
        ]))))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args(["org", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("SLUG"))
        .stdout(predicate::str::contains("Acme"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn org_view_not_found_maps_to_exit_five() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": {"code": "NOT_FOUND", "message": "no such organization"}
        })))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args(["org", "view", "missing"])
        .assert()
        .code(5)
        .stderr(predicate::str::contains("no such organization"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auth_status_uses_token_and_reports_identity() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(me_json()))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args(["auth", "status", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["user"]["email"], "steven@example.com");

    let req = &server.received_requests().await.unwrap()[0];
    assert_eq!(
        req.headers.get("authorization").unwrap().to_str().unwrap(),
        "Bearer secret-token"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_create_sends_idempotency_key_and_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/organizations/acme/projects/HAM/work-items"))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("ETag", "\"rev-1\"")
                .set_body_json(work_item_json("todo", 1)),
        )
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "work",
            "create",
            "--title",
            "Ship it",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["key"], "HAM-1");

    let req = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.method.as_str() == "POST")
        .unwrap();
    assert!(req.headers.contains_key("idempotency-key"));
    let sent: Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(sent["title"], "Ship it");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_start_verifies_transition_and_sends_if_match() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"rev-7\"")
                .set_body_json(work_item_json("todo", 7)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/transitions",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "currentStatus": "todo",
            "transitions": [{"targetStatus": "in_progress"}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/transitions",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"rev-8\"")
                .set_body_json(work_item_json("in_progress", 8)),
        )
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "work",
            "start",
            "HAM-1",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["status"], "in_progress");

    let post = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.method.as_str() == "POST")
        .unwrap();
    assert_eq!(
        post.headers.get("if-match").unwrap().to_str().unwrap(),
        "\"rev-7\""
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_transition_rejects_disallowed_target() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"rev-7\"")
                .set_body_json(work_item_json("todo", 7)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/transitions",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "currentStatus": "todo",
            "transitions": [{"targetStatus": "in_review"}]
        })))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "work",
            "transition",
            "HAM-1",
            "done",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("not allowed"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_list_requires_project() {
    let dir = TempDir::new().unwrap();
    // A mock server is needed for HAMSTIK_HOST but should never be contacted.
    let server = MockServer::start().await;
    base(&server, &dir)
        .args(["--org", "acme", "work", "list"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no project"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn doctor_reports_ready_when_authenticated() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(me_json()))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["ok"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn doctor_fails_without_credentials() {
    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .env_remove("HAMSTIK_TOKEN")
        .args(["doctor"])
        .assert()
        .code(3)
        .stdout(predicate::str::contains("no token available"));
}
