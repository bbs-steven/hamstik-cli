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

// ---- Banner (identity) surfaces -------------------------------------------

/// The banner art fragment used for presence/absence assertions (literal, not a
/// regex — `predicates::str::contains` matches substrings).
const BANNER_ART: &str = r"(\___/)";
const BANNER_FOOTER: &str = "© Blackboard Studios LLC";

/// A `hamstik` command isolated from any host/token/keyring. Only used for the
/// identity surfaces (help/version/bare/completion/parse-errors), which are
/// short-circuited during parsing and never touch the network or OS keyring.
fn banner_command(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("hamstik").expect("hamstik binary");
    cmd.env("HAMSTIK_CONFIG", dir.path().join("config.toml"));
    for var in [
        "HAMSTIK_HOST",
        "HAMSTIK_TOKEN",
        "HAMSTIK_PROFILE",
        "HAMSTIK_ORG",
        "HAMSTIK_PROJECT",
    ] {
        cmd.env_remove(var);
    }
    cmd.current_dir(dir.path());
    cmd
}

#[test]
fn root_help_prints_banner_to_stdout() {
    let dir = TempDir::new().unwrap();
    banner_command(&dir)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(BANNER_ART))
        .stdout(predicate::str::contains(BANNER_FOOTER))
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")))
        .stdout(predicate::str::contains("Usage: hamstik"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn short_help_prints_banner_to_stdout() {
    let dir = TempDir::new().unwrap();
    banner_command(&dir)
        .arg("-h")
        .assert()
        .success()
        .stdout(predicate::str::contains(BANNER_ART))
        .stdout(predicate::str::contains(BANNER_FOOTER));
}

#[test]
fn bare_invocation_prints_banner_to_stdout_and_exits_two() {
    let dir = TempDir::new().unwrap();
    banner_command(&dir)
        .assert()
        .code(2)
        .stdout(predicate::str::contains(BANNER_ART))
        .stdout(predicate::str::contains(BANNER_FOOTER))
        .stderr(predicate::str::is_empty());
}

#[test]
fn version_surfaces_print_banner() {
    let dir = TempDir::new().unwrap();
    for args in [&["--version"][..], &["-V"][..], &["version"][..]] {
        banner_command(&dir)
            .args(args)
            .assert()
            .success()
            .stdout(predicate::str::contains(BANNER_ART))
            .stdout(predicate::str::contains(BANNER_FOOTER))
            .stdout(predicate::str::contains(format!(
                "v{}",
                env!("CARGO_PKG_VERSION")
            )));
    }
}

#[test]
fn version_json_is_machine_readable_and_banner_free() {
    let dir = TempDir::new().unwrap();
    let output = banner_command(&dir)
        .args(["version", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains(BANNER_ART), "banner leaked into --json");
    let body: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn subcommand_help_and_completion_are_banner_free() {
    let dir = TempDir::new().unwrap();
    for args in [
        &["work", "--help"][..],
        &["auth", "login", "--help"][..],
        &["completion", "bash"][..],
    ] {
        banner_command(&dir)
            .args(args)
            .assert()
            .success()
            .stdout(predicate::str::contains(BANNER_ART).not());
    }
}

#[test]
fn unrecognized_subcommand_has_no_banner() {
    let dir = TempDir::new().unwrap();
    banner_command(&dir)
        .arg("bogus")
        .assert()
        .code(2)
        .stdout(predicate::str::contains(BANNER_ART).not())
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

// ---- Profile removal and broken config files ------------------------------

/// A `hamstik` command with an isolated config and no host/token/profile, for
/// commands that work purely on config state.
fn config_command(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("hamstik").expect("hamstik binary");
    cmd.env("HAMSTIK_CONFIG", dir.path().join("config.toml"));
    for var in [
        "HAMSTIK_HOST",
        "HAMSTIK_TOKEN",
        "HAMSTIK_PROFILE",
        "HAMSTIK_ORG",
        "HAMSTIK_PROJECT",
    ] {
        cmd.env_remove(var);
    }
    cmd.current_dir(dir.path());
    cmd
}

const TWO_PROFILES: &str = r#"version = 1
active_profile = "one"

[profiles.one]
host = "https://one.test"
user_id = "u1"
email = "one@example.com"

[profiles.two]
host = "https://two.test"
user_id = "u2"
email = "two@example.com"
"#;

#[test]
fn auth_forget_removes_the_profile_and_repairs_the_active_one() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("config.toml"), TWO_PROFILES).unwrap();

    let output = config_command(&dir)
        .args(["auth", "forget", "one", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["forget"], true);
    assert_eq!(body["profile"], "one");
    assert_eq!(body["host"], "https://one.test");
    // Local-only: a PAT is never revoked by the CLI.
    assert_eq!(body["revoked"], false);
    // `two` is the only profile left, so it becomes active.
    assert_eq!(body["activeProfile"], "two");

    let written = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(!written.contains("one.test"), "{written}");
    assert!(written.contains("two.test"), "{written}");
    assert!(written.contains(r#"active_profile = "two""#), "{written}");
}

#[test]
fn auth_forget_the_last_profile_leaves_no_active_profile() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        r#"version = 1
active_profile = "one"

[profiles.one]
host = "https://one.test"
user_id = "u1"
email = "one@example.com"
"#,
    )
    .unwrap();

    let output = config_command(&dir)
        .args(["auth", "forget", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    // No profile named: the active one is the target.
    assert_eq!(body["profile"], "one");
    assert_eq!(body["activeProfile"], Value::Null);
    let written = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(!written.contains("one.test"), "{written}");
}

#[test]
fn auth_forget_unknown_profile_lists_what_exists() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("config.toml"), TWO_PROFILES).unwrap();

    config_command(&dir)
        .args(["auth", "forget", "nope"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no such profile"))
        .stderr(predicate::str::contains("configured profiles: one, two"));
}

#[test]
fn auth_forget_without_any_profile_is_a_usage_error() {
    let dir = TempDir::new().unwrap();
    config_command(&dir)
        .args(["auth", "forget"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no profile to forget"));
}

#[test]
fn corrupt_config_names_the_file_and_exits_configuration() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "version = 1\n[[oops\n").unwrap();

    let expected_path = path.display().to_string();
    config_command(&dir)
        .args(["auth", "list"])
        .assert()
        .code(10)
        .stderr(predicate::str::contains("invalid configuration"))
        .stderr(predicate::str::contains(expected_path))
        .stderr(predicate::str::contains("hint:"));
}

#[test]
fn doctor_reports_a_corrupt_config_instead_of_refusing_to_run() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "version = 1\n[[oops\n").unwrap();

    let expected_path = path.display().to_string();
    // `doctor` is the command people reach for when config is broken, so it must
    // still run and report, not bail before printing anything.
    config_command(&dir)
        .args(["doctor"])
        .assert()
        .code(10)
        .stdout(predicate::str::contains("[FAIL] configuration file"))
        .stdout(predicate::str::contains(expected_path))
        .stdout(predicate::str::contains("not ready."));
}

#[test]
fn doctor_reports_a_corrupt_context_file_with_its_path() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".hamstik.toml");
    std::fs::write(&path, "version = 1\nhost = 42\n").unwrap();

    let expected_path = path.display().to_string();
    config_command(&dir)
        .args(["doctor", "--json"])
        .assert()
        .code(10)
        .stdout(predicate::str::contains(expected_path));
}
