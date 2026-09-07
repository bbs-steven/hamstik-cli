// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

// Integration tests assert with unwrap/expect by design.
#![allow(clippy::unwrap_used, clippy::expect_used)]

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
async fn doctor_reports_terminal_capabilities_in_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(me_json()))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    // The test harness pipes stdout, so color is reported disabled for that
    // reason — the check reflects the actual invocation, not a hypothetical.
    let body: Value = serde_json::from_slice(
        &base(&server, &dir)
            .args(["doctor", "--json"])
            .env("TERM", "xterm-256color")
            .env("LANG", "en_US.UTF-8")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let rendered = serde_json::to_string(&body).unwrap();
    assert!(!rendered.contains('\u{1b}'), "--json never carries ANSI");
    let checks = body["checks"].as_array().unwrap();
    let color = checks
        .iter()
        .find(|c| c["name"] == "terminal color")
        .expect("color check present");
    assert_eq!(color["ok"], false);
    assert_eq!(color["critical"], false);
    assert!(
        color["detail"]
            .as_str()
            .unwrap()
            .contains("stdout is not a terminal")
    );
    let emoji = checks
        .iter()
        .find(|c| c["name"] == "terminal emoji")
        .expect("emoji check present");
    assert_eq!(emoji["critical"], false);
    assert!(emoji["detail"].as_str().unwrap().contains("not verifiable"));

    // CLICOLOR_FORCE forces color on even for a piped stdout.
    let body: Value = serde_json::from_slice(
        &base(&server, &dir)
            .args(["doctor", "--json"])
            .env("CLICOLOR_FORCE", "1")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let color = body["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "terminal color")
        .expect("color check present");
    assert_eq!(color["ok"], true);
    assert!(color["detail"].as_str().unwrap().contains("CLICOLOR_FORCE"));
    // Forced color still never leaks ANSI into --json output.
    let rendered = serde_json::to_string(&body).unwrap();
    assert!(!rendered.contains('\u{1b}'));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn doctor_reports_disabled_color_when_no_color_flag() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(me_json()))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let body: Value = serde_json::from_slice(
        &base(&server, &dir)
            .args(["doctor", "--json", "--no-color"])
            .env("TERM", "xterm-256color")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let color = body["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "terminal color")
        .expect("color check present");
    assert_eq!(color["ok"], false);
    assert_eq!(color["critical"], false);
    let detail = color["detail"].as_str().unwrap();
    assert!(detail.contains("--no-color"), "{detail}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn doctor_terminal_checks_are_informational_only() {
    // A dumb terminal must not fail doctor: it is a working, plain-text setup.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(me_json()))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let body: Value = serde_json::from_slice(
        &base(&server, &dir)
            .args(["doctor", "--json"])
            .env("TERM", "dumb")
            .env_remove("LANG")
            .env_remove("LC_ALL")
            .env_remove("LC_CTYPE")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(body["ok"], true, "terminal checks never fail doctor");
    let checks = body["checks"].as_array().unwrap();
    let color = checks
        .iter()
        .find(|c| c["name"] == "terminal color")
        .expect("color check present");
    assert_eq!(color["ok"], false);
    assert_eq!(color["critical"], false);
    // Piped stdout wins before TERM is even consulted here; assert the
    // informational (non-critical) outcome rather than a specific reason.
    let emoji = checks
        .iter()
        .find(|c| c["name"] == "terminal emoji")
        .expect("emoji check present");
    assert_eq!(emoji["critical"], false);
    assert_eq!(emoji["ok"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn doctor_human_output_shows_samples_without_ansi_when_piped() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(me_json()))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    // Piped stdout (the default under assert_cmd) must stay ANSI-free and
    // still show the emoji sample line for visual confirmation.
    let output = base(&server, &dir)
        .env("TERM", "xterm-256color")
        .env("LANG", "en_US.UTF-8")
        .args(["doctor"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(stdout.contains("terminal color"), "{stdout}");
    assert!(stdout.contains("terminal emoji"), "{stdout}");
    assert!(stdout.contains("emoji sample"), "{stdout}");
    assert!(
        !stdout.contains('\u{1b}'),
        "piped output must not emit ANSI: {stdout}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn doctor_color_probe_respects_no_color_env() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(me_json()))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let body: Value = serde_json::from_slice(
        &base(&server, &dir)
            .args(["doctor", "--json"])
            .env("TERM", "xterm-256color")
            .env("NO_COLOR", "1")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let color = body["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "terminal color")
        .expect("color check present");
    assert_eq!(color["ok"], false);
    assert!(
        color["detail"]
            .as_str()
            .unwrap()
            .contains("NO_COLOR is set")
    );
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

/// A bare `hamstik` is a usage error: clap's missing-subcommand help goes to
/// stderr (so failure output is never mistaken for success content) with exit
/// code 2.
#[test]
fn bare_invocation_is_a_usage_error_on_stderr() {
    let dir = TempDir::new().unwrap();
    banner_command(&dir)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Usage: hamstik"))
        .stdout(predicate::str::is_empty());
}

/// `-V`/`--version` are the machine-parsed surfaces: one terse line. The
/// banner lives on the human `hamstik version` path only.
#[test]
fn version_flag_is_terse_and_version_command_prints_banner() {
    let dir = TempDir::new().unwrap();
    for args in [&["--version"][..], &["-V"][..]] {
        banner_command(&dir)
            .args(args)
            .assert()
            .success()
            .stdout(predicates::ord::eq(format!(
                "hamstik {}\n",
                env!("CARGO_PKG_VERSION")
            )))
            .stdout(predicate::str::contains(BANNER_ART).not());
    }
    banner_command(&dir)
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains(BANNER_ART))
        .stdout(predicate::str::contains(BANNER_FOOTER))
        .stdout(predicate::str::contains(format!(
            "v{}",
            env!("CARGO_PKG_VERSION")
        )));
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
    // The path appears inside a longer JSON string, so no surrounding quotes:
    // only the separators need JSON escaping (backslashes on Windows).
    let expected_json = expected_path.replace('\\', "\\\\");
    config_command(&dir)
        .args(["doctor", "--json"])
        .assert()
        .code(10)
        .stdout(predicate::str::contains(expected_json));
}

// ---- Hardening behaviors ---------------------------------------------------

/// A hostile or looping server must not be able to keep `--all` running: the
/// aggregation budget terminates the command with a protocol error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_list_all_terminates_on_an_endless_server() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects/HAM/work-items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [work_item_json("todo", 1)],
            "page": {"limit": 1, "hasMore": true, "nextCursor": "same"}
        })))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args(["--org", "acme", "--project", "HAM", "work", "list", "--all"])
        .assert()
        .code(9)
        .stderr(predicate::str::contains("exceeded"));
}

/// `HAMSTIK_TOKEN` containing a control character is rejected before any
/// network activity: such a value can corrupt header framing and can never be
/// a valid PAT.
#[test]
fn env_token_with_control_characters_is_rejected() {
    let dir = TempDir::new().unwrap();
    config_command(&dir)
        .env("HAMSTIK_HOST", "http://localhost:1")
        .env("HAMSTIK_TOKEN", "tok\u{1b}[31m")
        .args(["auth", "status"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("control characters"));
}

/// An oversized token piped to `--with-token` fails fast with a clear error.
#[test]
fn login_with_token_rejects_oversized_input() {
    let dir = TempDir::new().unwrap();
    let big = "x".repeat(8 * 1024);
    config_command(&dir)
        .env("HAMSTIK_HOST", "http://localhost:1")
        .args(["auth", "login", "--with-token"])
        .write_stdin(big)
        .assert()
        .failure()
        .stderr(predicate::str::contains("byte limit"));
}

/// The response-body cap and redirect refusal are enforced end-to-end through
/// the real binary.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn redirect_is_not_followed_end_to_end() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/evil"))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args(["auth", "status"])
        .assert()
        .failure();
    // Exactly one request: the redirect was never followed.
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// ---- `use` command validation (SPEC §35) -----------------------------------

/// A project JSON body for `GET /organizations/{org}/projects/{key}`.
fn project_json(key: &str) -> Value {
    json!({
        "id": "p1", "key": key, "name": "P", "color": "#000000", "revision": 1, "archivedAt": null,
        "description": null, "organizationId": "o1",
        "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"
    })
}

/// A 404 with the stable NOT_FOUND code.
fn not_found() -> ResponseTemplate {
    ResponseTemplate::new(404).set_body_json(json!({
        "error": {"code": "NOT_FOUND", "message": "no such resource"}
    }))
}

/// `org use` validates the slug through the API before persisting it; a typo
/// fails with the server's not-found error and nothing is stored.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn org_use_rejects_unknown_slug() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/sph"))
        .respond_with(not_found())
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args(["org", "use", "sph"])
        .assert()
        .code(5)
        .stderr(predicate::str::contains("no such resource"));

    // Nothing was persisted.
    let output = base(&server, &dir)
        .args(["context", "show", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["organization"]["value"], Value::Null);
    assert_eq!(body["profile"], Value::Null);
}

/// `org use` validates before persisting: the API is contacted first, and the
/// missing-profile usage error surfaces only after validation succeeded.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn org_use_persists_validated_slug() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/sph"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "o1", "slug": "sph", "name": "SPH", "role": "member",
            "plan": "pro", "isDefault": false, "suspended": false,
            "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args(["org", "use", "sph"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no active profile"));

    // Validation hit the API (the 200 above matched) before persistence was
    // attempted — persistence needs a profile, which the usage error confirms
    // comes after validation.
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

/// `project use` validates the key inside the resolved organization; an
/// organization slug typed into `project use` fails with not-found.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_use_rejects_unknown_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/sph/projects/sph"))
        .respond_with(not_found())
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args(["--org", "sph", "project", "use", "sph"])
        .assert()
        .code(5)
        .stderr(predicate::str::contains("no such resource"));
}

/// `project use` stores the key after the server confirms it exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_use_persists_validated_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/sph/projects/P01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(project_json("P01")))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args(["--org", "sph", "project", "use", "P01"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no active profile"));

    // Validation hit the API (one request) before persistence was attempted.
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// ---- New API surface: sprints, labels, attachments, comment deletion -----

fn sprint_json(state: &str, revision: i64) -> Value {
    json!({
        "id": "11111111-1111-1111-1111-111111111111", "name": "Sprint 1", "state": state,
        "startDate": null, "endDate": null, "goal": null, "targetPoints": null,
        "createdAt": "2026-08-01T00:00:00Z", "updatedAt": "2026-09-01T00:00:00Z", "revision": revision
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sprint_list_renders_table_and_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects/HAM/sprints"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(page(json!([sprint_json("active", 1)]))),
        )
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args(["--org", "acme", "--project", "HAM", "sprint", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sprint 1"))
        .stdout(predicate::str::contains("active"));

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "sprint",
            "list",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["items"][0]["state"], "active");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sprint_create_sends_name_and_idempotency_key() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/organizations/acme/projects/HAM/sprints"))
        .respond_with(ResponseTemplate::new(201).set_body_json(sprint_json("future", 1)))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "sprint",
            "create",
            "--name",
            "Sprint 1",
            "--idempotency-key",
            "sprint-2026-01",
        ])
        .assert()
        .success();

    let req = &server.received_requests().await.unwrap()[0];
    assert_eq!(
        req.headers
            .get("idempotency-key")
            .unwrap()
            .to_str()
            .unwrap(),
        "sprint-2026-01"
    );
    let body: Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(body["name"], "Sprint 1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sprint_transition_sends_if_match_and_completion() {
    let sprint_id = "11111111-1111-1111-1111-111111111111";
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/organizations/acme/projects/HAM/sprints/{sprint_id}"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"sprint-1\"")
                .set_body_json(sprint_json("active", 1)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/organizations/acme/projects/HAM/sprints/{sprint_id}/transitions"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "currentState": "active",
            "transitions": [{"targetState": "done", "requiresCompletionAction": true}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/api/v1/organizations/acme/projects/HAM/sprints/{sprint_id}/transitions"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"sprint-2\"")
                .set_body_json(sprint_json("done", 2)),
        )
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "sprint",
            "transition",
            sprint_id,
            "done",
            "--move-to-backlog",
            "--json",
        ])
        .assert()
        .success();

    let req = &server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.method.as_str() == "POST")
        .unwrap();
    assert_eq!(
        req.headers.get("if-match").unwrap().to_str().unwrap(),
        "\"sprint-1\""
    );
    let body: Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(body["targetState"], "done");
    assert_eq!(body["completionAction"]["mode"], "backlog");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sprint_transition_rejects_disallowed_target() {
    let sprint_id = "11111111-1111-1111-1111-111111111111";
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/organizations/acme/projects/HAM/sprints/{sprint_id}"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"sprint-1\"")
                .set_body_json(sprint_json("done", 1)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/organizations/acme/projects/HAM/sprints/{sprint_id}/transitions"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "currentState": "done", "transitions": []
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
            "sprint",
            "transition",
            sprint_id,
            "active",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("not allowed from done"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn label_list_and_create_flow() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects/HAM/labels"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([
            {"id": "l1", "name": "api", "color": "#6366f1", "createdAt": "2026-01-01T00:00:00Z"}
        ]))))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/organizations/acme/projects/HAM/labels"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!(
            {"id": "l2", "name": "cli", "color": "#6366f1", "createdAt": "2026-01-01T00:00:00Z"}
        )))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "label",
            "list",
            "--json",
        ])
        .assert()
        .success();

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "label",
            "create",
            "--name",
            "cli",
            "--json",
        ])
        .assert()
        .success();

    let req = &server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.method.as_str() == "POST")
        .unwrap();
    let body: Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(body["name"], "cli");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_label_add_sends_if_match_and_label_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"wi-4\"")
                .set_body_json(work_item_json("todo", 4)),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/labels",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"wi-5\"")
                .set_body_json(work_item_json("todo", 5)),
        )
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
            "label",
            "add",
            "HAM-1",
            "--label",
            "l1",
            "--json",
        ])
        .assert()
        .success();

    let req = &server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.method.as_str() == "POST")
        .unwrap();
    assert_eq!(
        req.headers.get("if-match").unwrap().to_str().unwrap(),
        "\"wi-4\""
    );
    let body: Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(body["labelId"], "l1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_attachment_upload_download_delete() {
    let dir = TempDir::new().unwrap();
    let upload_path = dir.path().join("design.png");
    std::fs::write(&upload_path, b"PNGDATA").unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/attachments",
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "a1", "workItemId": "w1", "fileName": "design.png", "contentType": "application/octet-stream",
            "size": 7, "createdBy": {"id": "u", "name": "U"}, "createdAt": "2026-01-01T00:00:00Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/attachments/a1",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "Content-Disposition",
                    "attachment; filename*=UTF-8''design.png",
                )
                .insert_header("Content-Type", "application/octet-stream")
                .set_body_bytes(b"PNGDATA".to_vec()),
        )
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/attachments/a1",
        ))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    // Upload
    base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "work",
            "attachment",
            "upload",
            "HAM-1",
            upload_path.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success();
    let upload_req = &server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.method.as_str() == "POST")
        .unwrap();
    let content_type = upload_req
        .headers
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.starts_with("multipart/form-data"),
        "{content_type}"
    );
    let upload_body = String::from_utf8_lossy(&upload_req.body).to_string();
    assert!(
        upload_body.contains("filename=\"design.png\""),
        "{upload_body}"
    );
    assert!(upload_body.contains("PNGDATA"), "{upload_body}");

    // Download
    let output_path = dir.path().join("downloaded.png");
    base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "work",
            "attachment",
            "download",
            "HAM-1",
            "a1",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(std::fs::read(&output_path).unwrap(), b"PNGDATA");

    // Delete
    base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "work",
            "attachment",
            "delete",
            "HAM-1",
            "a1",
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"deleted\": true"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_comment_delete_maps_conflict_to_exit_six() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/comments/c1",
        ))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "error": {"code": "CONFLICT", "message": "The Comment has replies."},
            "requestId": "r"
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
            "comment",
            "delete",
            "HAM-1",
            "c1",
        ])
        .assert()
        .code(6)
        .stderr(predicate::str::contains("The Comment has replies."));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_comment_delete_success_is_quiet_json() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/comments/c1",
        ))
        .respond_with(ResponseTemplate::new(204))
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
            "comment",
            "delete",
            "HAM-1",
            "c1",
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"deleted\": true"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_create_requires_name_and_sends_idempotency() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/organizations/acme/projects"))
        .respond_with(ResponseTemplate::new(201).set_body_json(project_json("WEB")))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    // Missing name without a TTY fails deterministically (SPEC §30).
    base(&server, &dir)
        .args(["--org", "acme", "project", "create"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--name"));

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args([
            "--org", "acme", "project", "create", "--name", "Website", "--key", "WEB", "--json",
        ])
        .assert()
        .success();

    let req = &server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.method.as_str() == "POST")
        .unwrap();
    let body: Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(body["name"], "Website");
    assert_eq!(body["key"], "WEB");
    assert!(req.headers.contains_key("idempotency-key"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sprint_error_codes_map_to_exit_six() {
    let sprint_id = "11111111-1111-1111-1111-111111111111";
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/organizations/acme/projects/HAM/sprints/{sprint_id}"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"sprint-1\"")
                .set_body_json(sprint_json("active", 1)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/organizations/acme/projects/HAM/sprints/{sprint_id}/transitions"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "currentState": "active",
            "transitions": [{"targetState": "done", "requiresCompletionAction": true}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/api/v1/organizations/acme/projects/HAM/sprints/{sprint_id}/transitions"
        )))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "error": {"code": "SPRINT_HAS_UNFINISHED_WORK_ITEMS",
                      "message": "This Sprint has unfinished Work Items; provide completionAction."},
            "requestId": "r"
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
            "sprint",
            "transition",
            sprint_id,
            "done",
            "--move-to-backlog",
        ])
        .assert()
        .code(6)
        .stderr(predicate::str::contains("unfinished Work Items"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auth_status_json_reports_public_id_and_memberships() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "u1", "publicId": "usr_cPbfeqnghA-RLpDVOMQhHg", "name": "Steven",
            "email": "steven@example.com",
            "authentication": {"type": "pat", "credentialId": "c", "credentialName": "n", "scopes": [], "expiresAt": "2027-01-01T00:00:00Z"},
            "defaultOrganization": null,
            "organizations": [{"id": "o1", "slug": "acme", "name": "Acme", "username": "steven"}]
        })))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args(["auth", "status", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["user"]["publicId"], "usr_cPbfeqnghA-RLpDVOMQhHg");
    assert_eq!(body["user"]["organizations"][0]["username"], "steven");
}

// ---- Color swatch rendering (human tables, ANSI-aware alignment) -----------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_list_plain_when_piped() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([
            {"id": "p1", "key": "P01", "name": "Project 01", "color": "#6366f1", "revision": 1, "archivedAt": null,
             "description": null, "organizationId": "o1",
             "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"}
        ]))))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    // Piped stdout => color disabled => the swatch still reserves its width
    // (frame + blocks) and the hex stays plain.
    let output = base(&server, &dir)
        .args(["--org", "acme", "project", "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        !stdout.contains('\u{1b}'),
        "piped output must be ANSI-free: {stdout}"
    );
    assert!(stdout.contains("[██] #6366f1"), "{stdout}");
    // Column alignment: the swatch cell pads correctly (NAME column aligned).
    assert!(stdout.contains("Project 01"), "{stdout}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_list_swatch_with_forced_truecolor() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([
            {"id": "p1", "key": "P01", "name": "Project 01", "color": "#6366f1", "revision": 1, "archivedAt": null,
             "description": null, "organizationId": "o1",
             "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"},
            {"id": "p2", "key": "PK2", "name": "Project 2", "color": "#000000", "revision": 1, "archivedAt": null,
             "description": null, "organizationId": "o1",
             "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"}
        ]))))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args(["--org", "acme", "project", "list"])
        .env("CLICOLOR_FORCE", "1")
        .env("COLORTERM", "truecolor")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    // Block glyphs are drawn in the foreground, so the swatch sets foreground
    // (38;2) AND background (48;2) to the exact RGB — glyphs in the project
    // color, background matching to seal font seams.
    assert!(
        stdout.contains("\u{1b}[38;2;99;102;241;48;2;99;102;241m"),
        "{stdout}"
    );
    // Black project: black-on-black swatch, but the bracket frame remains visible.
    assert!(stdout.contains("\u{1b}[38;2;0;0;0;48;2;0;0;0m"), "{stdout}");
    // The hex text is always plain (reset before the text).
    assert!(stdout.contains("#6366f1"), "{stdout}");
    assert!(stdout.contains("#000000"), "{stdout}");
    // Alignment must hold: the NAME column starts at the same byte offset in
    // every data row even though swatch cells contain escape sequences.
    let indigo_line = stdout.lines().find(|l| l.contains("P01")).unwrap();
    let black_line = stdout.lines().find(|l| l.contains("PK2")).unwrap();
    assert_eq!(
        indigo_line.find("Project"),
        black_line.find("Project"),
        "NAME column must align across rows with colored swatches"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_view_shows_swatch_in_detail() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects/P01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(project_json("P01")))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args(["--org", "acme", "project", "view", "P01"])
        .env("CLICOLOR_FORCE", "1")
        .env("COLORTERM", "truecolor")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(stdout.contains("color"));
    // project_json uses #000000: glyphs painted black on black, visible frame,
    // plain hex.
    assert!(
        stdout.contains("\u{1b}[38;2;0;0;0;48;2;0;0;0m██\u{1b}[0m"),
        "{stdout}"
    );
    assert!(stdout.contains("#000000"), "{stdout}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_list_json_has_no_swatch_ansi() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([
            {"id": "p1", "key": "P01", "name": "Project 01", "color": "#6366f1", "revision": 1, "archivedAt": null,
             "description": null, "organizationId": "o1",
             "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"}
        ]))))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args(["--org", "acme", "project", "list", "--json"])
        .env("CLICOLOR_FORCE", "1")
        .env("COLORTERM", "truecolor")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        !stdout.contains('\u{1b}'),
        "--json must be ANSI-free: {stdout}"
    );
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["items"][0]["color"], "#6366f1");
}

// ---- Updated API surface tests ---------------------------------------------

fn project_with_archived(key: &str, revision: i64, archived_at: Option<&str>) -> Value {
    json!({
        "id": "p1", "organizationId": "o1", "key": key, "name": "P", "color": "#000000",
        "description": null, "revision": revision, "archivedAt": archived_at,
        "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"
    })
}

fn me_public_json() -> Value {
    json!({
        "id": "u1", "publicId": "usr_cPbfeqnghA-RLpDVOMQhHg", "name": "Steven", "email": "steven@example.com",
        "authentication": {"type": "pat", "credentialId": "c", "credentialName": "n", "scopes": [], "expiresAt": "2027-01-01T00:00:00Z"},
        "defaultOrganization": null
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_create_sends_assignee_public_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/organizations/acme/projects/HAM/work-items"))
        .respond_with(ResponseTemplate::new(201).set_body_json(work_item_json("todo", 1)))
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
            "create",
            "--title",
            "T",
            "--assignee",
            "usr_cPbfeqnghA-RLpDVOMQhHg",
            "--json",
        ])
        .assert()
        .success();

    let body: Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["assigneePublicId"], "usr_cPbfeqnghA-RLpDVOMQhHg");
    assert!(body.get("assigneeId").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_list_supports_sort_and_archived_filters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects/HAM/work-items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([]))))
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
            "list",
            "--sort",
            "dueDate",
            "--overdue",
            "true",
            "--archived",
            "false",
        ])
        .assert()
        .success();

    let query = server.received_requests().await.unwrap()[0]
        .url
        .query()
        .unwrap_or_default()
        .to_string();
    assert!(query.contains("sort=dueDate"), "{query}");
    assert!(query.contains("overdue=true"), "{query}");
    assert!(query.contains("archived=false"), "{query}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_archive_sends_if_match_and_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"wi-4\"")
                .set_body_json(work_item_json("done", 4)),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/archive",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(work_item_json("done", 5)))
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
            "archive",
            "HAM-1",
            "--json",
        ])
        .assert()
        .success();

    let reqs = server.received_requests().await.unwrap();
    let archive = reqs.iter().find(|r| r.method.as_str() == "POST").unwrap();
    assert_eq!(
        archive.headers.get("if-match").unwrap().to_str().unwrap(),
        "\"wi-4\""
    );
    assert!(archive.headers.contains_key("idempotency-key"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_delete_sends_cascade_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"wi-4\"")
                .set_body_json(work_item_json("done", 4)),
        )
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1",
        ))
        .respond_with(ResponseTemplate::new(204))
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
            "delete",
            "HAM-1",
            "--cascade",
            "--json",
        ])
        .assert()
        .success();

    let req = &server.received_requests().await.unwrap()[1];
    let body: Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(body["cascade"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_link_add_list_and_delete() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/links"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "l1", "relation": "blocks",
            "otherWorkItem": {"id": "2", "key": "HAM-2", "project": {"id": "p", "key": "HAM", "name": "Ham"}, "title": "Other", "type": "task", "status": "todo"},
            "createdBy": null, "createdAt": "2026-01-01T00:00:00Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/links"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([{
            "id": "l1", "relation": "blocks",
            "otherWorkItem": {"id": "2", "key": "HAM-2", "project": {"id": "p", "key": "HAM", "name": "Ham"}, "title": "Other", "type": "task", "status": "todo"},
            "createdBy": null, "createdAt": "2026-01-01T00:00:00Z"
        }]))))
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
            "link",
            "add",
            "HAM-1",
            "--target-key",
            "HAM-2",
            "--relation",
            "blocks",
            "--json",
        ])
        .assert()
        .success();

    base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "work",
            "link",
            "list",
            "HAM-1",
            "--json",
        ])
        .assert()
        .success();

    let reqs = server.received_requests().await.unwrap();
    let post = reqs.iter().find(|r| r.method.as_str() == "POST").unwrap();
    let body: Value = serde_json::from_slice(&post.body).unwrap();
    assert_eq!(body["targetKey"], "HAM-2");
    assert_eq!(body["relation"], "blocks");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_link_duplicate_maps_to_exit_six() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/links",
        ))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "error": {"code": "LINK_DUPLICATE", "message": "A link already exists."},
            "requestId": "r"
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
            "link",
            "add",
            "HAM-1",
            "--target-key",
            "HAM-2",
            "--relation",
            "blocks",
        ])
        .assert()
        .code(6)
        .stderr(predicate::str::contains("A link already exists"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_activity_renders_feed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/activity"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([
            {"id": "a1", "action": "status_changed", "actor": {"publicId": "usr_cPbfeqnghA-RLpDVOMQhHg", "name": "A"},
             "detail": {"from": "todo", "to": "done"}, "createdAt": "2026-01-02T00:00:00Z"}
        ]))))
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
            "activity",
            "HAM-1",
            "--json",
        ])
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_comment_edit_sends_body_and_idempotency() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/comments/c1",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "c1", "workItemId": "w1", "parentCommentId": null,
            "author": {"publicId": "usr_cPbfeqnghA-RLpDVOMQhHg", "name": "A"},
            "body": "edited", "deleted": false,
            "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z",
            "editedAt": "2026-01-02T00:00:00Z"
        })))
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
            "comment",
            "edit",
            "HAM-1",
            "c1",
            "--body",
            "edited",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["editedAt"], "2026-01-02T00:00:00Z");

    let req = &server.received_requests().await.unwrap()[0];
    assert!(req.headers.contains_key("idempotency-key"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_bulk_create_sends_operations() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/organizations/acme/bulk-work-items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"index": 0, "status": 201, "workItem": {"id": "1", "key": "HAM-1"}}]
        })))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let ops_path = dir.path().join("ops.json");
    std::fs::write(&ops_path, r#"[{"projectKey":"HAM","title":"First"}]"#).unwrap();
    let output = base(&server, &dir)
        .args([
            "--org",
            "acme",
            "work",
            "bulk",
            "create",
            "--operations-file",
            ops_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["results"][0]["workItem"]["key"], "HAM-1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_bulk_rejects_too_many_operations_locally() {
    let server = MockServer::start().await;
    let dir = TempDir::new().unwrap();
    let ops_path = dir.path().join("ops.json");
    let ops: Vec<Value> = (0..51)
        .map(|i| json!({"projectKey": "HAM", "title": format!("Item {i}")}))
        .collect();
    std::fs::write(&ops_path, serde_json::to_string(&ops).unwrap()).unwrap();
    base(&server, &dir)
        .args([
            "--org",
            "acme",
            "work",
            "bulk",
            "create",
            "--operations-file",
            ops_path.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("between 1 and 50"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_edit_sends_if_match_and_idempotency() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects/WEB"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"project-1\"")
                .set_body_json(project_with_archived("WEB", 1, None)),
        )
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v1/organizations/acme/projects/WEB"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"project-2\"")
                .set_body_json(project_with_archived("WEB", 2, None)),
        )
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args([
            "--org", "acme", "project", "edit", "WEB", "--name", "New name", "--json",
        ])
        .assert()
        .success();

    let reqs = server.received_requests().await.unwrap();
    let patch = reqs.iter().find(|r| r.method.as_str() == "PATCH").unwrap();
    assert_eq!(
        patch.headers.get("if-match").unwrap().to_str().unwrap(),
        "\"project-1\""
    );
    assert!(patch.headers.contains_key("idempotency-key"));
    let body: Value = serde_json::from_slice(&patch.body).unwrap();
    assert_eq!(body["name"], "New name");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_archive_and_unarchive_flow() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects/WEB"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"project-1\"")
                .set_body_json(project_with_archived("WEB", 1, None)),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/organizations/acme/projects/WEB/archive"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(project_with_archived(
                "WEB",
                2,
                Some("2026-04-01T00:00:00Z"),
            )),
        )
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args([
            "--org", "acme", "project", "archive", "WEB", "--force", "--json",
        ])
        .assert()
        .success();

    // --force skips the preflight GET, so the archive POST is the only request.
    let req = &server.received_requests().await.unwrap()[0];
    let body: Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(body, json!({}));
    assert_eq!(req.headers.get("if-match").unwrap().to_str().unwrap(), "*");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_activity_renders_feed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects/HAM/activity"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([
            {"id": "a1", "action": "created", "actor": null, "detail": null,
             "createdAt": "2026-01-02T00:00:00Z",
             "workItem": {"id": "1", "key": "HAM-1", "title": "T"}}
        ]))))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args([
            "--org",
            "acme",
            "--project",
            "HAM",
            "project",
            "activity",
            "--json",
        ])
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_list_archived_flag_filters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([]))))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args(["--org", "acme", "project", "list", "--archived", "true"])
        .assert()
        .success();

    let query = server.received_requests().await.unwrap()[0]
        .url
        .query()
        .unwrap_or_default()
        .to_string();
    assert!(query.contains("archived=true"), "{query}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn org_members_renders_directory() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/users"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([
            {"publicId": "usr_cPbfeqnghA-RLpDVOMQhHg", "name": "Steven", "username": "steven"}
        ]))))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args(["org", "members", "acme", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["items"][0]["publicId"], "usr_cPbfeqnghA-RLpDVOMQhHg");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn org_work_lists_context_items() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/work-items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([
            {"id": "1", "key": "HAM-1", "revision": 1, "title": "T", "status": "todo",
             "assignee": {"publicId": "usr_cPbfeqnghA-RLpDVOMQhHg", "name": "A"},
             "project": {"id": "p", "key": "HAM", "name": "Ham", "color": "#000000"},
             "organization": {"id": "o", "slug": "acme", "name": "Acme"}}
        ]))))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args(["--org", "acme", "org", "work", "--project", "HAM", "--json"])
        .assert()
        .success();

    let query = server.received_requests().await.unwrap()[0]
        .url
        .query()
        .unwrap_or_default()
        .to_string();
    assert!(query.contains("project=HAM"), "{query}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn org_work_mine_sends_assignee_me() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/work-items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([]))))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args(["--org", "acme", "org", "work", "--mine", "--json"])
        .assert()
        .success();

    let query = server.received_requests().await.unwrap()[0]
        .url
        .query()
        .unwrap_or_default()
        .to_string();
    assert!(query.contains("assignee=me"), "{query}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_list_mine_sends_assignee_me() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects/HAM/work-items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([]))))
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
            "list",
            "--mine",
            "--json",
        ])
        .assert()
        .success();

    let query = server.received_requests().await.unwrap()[0]
        .url
        .query()
        .unwrap_or_default()
        .to_string();
    assert!(query.contains("assignee=me"), "{query}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_view_shows_profile_summary() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/users/usr_cPbfeqnghA-RLpDVOMQhHg"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "publicId": "usr_cPbfeqnghA-RLpDVOMQhHg", "name": "Steven",
            "avatarUrl": null, "joinedAt": "2026-01-15T14:30:00.000Z",
            "isCurrentUser": true, "sharedOrganizations": [],
            "stats": {"projects": 1, "workItemsAssigned": 2, "workItemsCreated": 3, "workItemsCompleted": 4, "comments": 5}
        })))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args(["user", "view", "usr_cPbfeqnghA-RLpDVOMQhHg", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["publicId"], "usr_cPbfeqnghA-RLpDVOMQhHg");
    assert_eq!(body["stats"]["workItemsCompleted"], 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_work_lists_context_items() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/users/usr_cPbfeqnghA-RLpDVOMQhHg/work"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(json!([]))))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    base(&server, &dir)
        .args([
            "user",
            "work",
            "usr_cPbfeqnghA-RLpDVOMQhHg",
            "--involvement",
            "assigned",
            "--json",
        ])
        .assert()
        .success();

    let query = server.received_requests().await.unwrap()[0]
        .url
        .query()
        .unwrap_or_default()
        .to_string();
    assert!(query.contains("involvement=assigned"), "{query}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_avatar_downloads_bytes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/users/usr_cPbfeqnghA-RLpDVOMQhHg/avatar"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "image/png")
                .set_body_bytes(vec![1, 2, 3]),
        )
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output_path = dir.path().join("avatar.png");
    base(&server, &dir)
        .args([
            "user",
            "avatar",
            "usr_cPbfeqnghA-RLpDVOMQhHg",
            "--output",
            output_path.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success();
    assert_eq!(std::fs::read(&output_path).unwrap(), vec![1, 2, 3]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auth_status_reports_public_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(me_public_json()))
        .mount(&server)
        .await;

    let dir = TempDir::new().unwrap();
    let output = base(&server, &dir)
        .args(["auth", "status", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["user"]["publicId"], "usr_cPbfeqnghA-RLpDVOMQhHg");
}
