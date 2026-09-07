// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

// Integration tests assert with unwrap/expect by design.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! End-to-end transport tests for [`HamstikClient`] against a local wiremock.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use hamstik_api_client::retry::NoopSleeper;
use hamstik_api_client::{
    ClientConfig, CreateCommentRequest, CreateWorkItemRequest, HamstikApi, HamstikClient,
    ListOptions, PageItems, RetryPolicy, TransitionRequest, UpdateWorkItemRequest, follow_all,
};
use secrecy::SecretString;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn client_for(url: &str) -> HamstikClient {
    let host = hamstik_api_client::Host::parse(url).expect("loopback host");
    let config = ClientConfig {
        retry: RetryPolicy {
            attempts: 3,
            base: Duration::from_millis(1),
            max_delay: Duration::from_millis(2),
            jitter: false,
        },
        ..ClientConfig::default()
    };
    HamstikClient::with_sleeper(
        host,
        SecretString::from("test-token"),
        config,
        Arc::new(NoopSleeper),
    )
    .expect("client")
}

#[tokio::test]
async fn sends_auth_and_accept_and_captures_request_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Request-Id", "req-123")
                .set_body_json(json!({
                    "id": "1",
                    "name": "Steven",
                    "email": "s@example.com",
                    "authentication": {"type":"pat","credentialId":"c","credentialName":"n","scopes":[],"expiresAt":"2027-01-01T00:00:00Z"},
                    "defaultOrganization": null
                })),
        )
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let resp = client.whoami().await.unwrap();

    assert_eq!(resp.value.email, "s@example.com");
    assert_eq!(resp.request_id.as_deref(), Some("req-123"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    let auth = received[0]
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(auth, "Bearer test-token");
    assert_eq!(
        received[0].headers.get("accept").unwrap().to_str().unwrap(),
        "application/json"
    );
}

#[tokio::test]
async fn maps_error_envelope_to_api_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": {"code": "NOT_FOUND", "message": "no such organization"},
            "requestId": "err-req"
        })))
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let err = client.get_organization("acme").await.unwrap_err();
    let api = err.as_api().expect("api error");
    assert_eq!(api.status, 404);
    assert_eq!(api.code, "NOT_FOUND");
    assert_eq!(api.message, "no such organization");
    assert_eq!(api.request_id.as_deref(), Some("err-req"));
}

#[tokio::test]
async fn retries_transient_failure_then_succeeds() {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    let body = json!({
        "id": "1", "name": "Steven", "email": "s@example.com",
        "authentication": {"type":"pat","credentialId":"c","credentialName":"n","scopes":[],"expiresAt":"2027-01-01T00:00:00Z"},
        "defaultOrganization": null
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(move |_: &Request| {
            let n = count.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(503)
            } else {
                ResponseTemplate::new(200).set_body_json(body.clone())
            }
        })
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let resp = client.whoami().await.unwrap();
    assert_eq!(resp.value.name, "Steven");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn does_not_retry_patch() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({
            "error": {"code":"INTERNAL_ERROR","message":"down"}, "requestId":"r"
        })))
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let body = UpdateWorkItemRequest {
        title: Some("x".to_string()),
        ..Default::default()
    };
    let err = client
        .update_work_item("acme", "HAM", "HAM-1", &body, "*")
        .await
        .unwrap_err();
    assert_eq!(err.as_api().unwrap().status, 503);
    // PATCH must be attempted exactly once.
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn honors_short_retry_after_and_retries_429() {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations"))
        .respond_with(move |_: &Request| {
            let n = count.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(429).insert_header("Retry-After", "1")
            } else {
                ResponseTemplate::new(200).set_body_json(
                    json!({"items": [], "page": {"limit":50,"hasMore":false,"nextCursor":null}}),
                )
            }
        })
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let resp = client
        .list_organizations(ListOptions::default())
        .await
        .unwrap();
    assert!(resp.value.items.is_empty());
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn gives_up_immediately_on_long_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "3600")
                .set_body_json(
                    json!({"error":{"code":"RATE_LIMITED","message":"slow down"},"requestId":"r"}),
                ),
        )
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let err = client
        .list_organizations(ListOptions::default())
        .await
        .unwrap_err();
    let api = err.as_api().unwrap();
    assert_eq!(api.code, "RATE_LIMITED");
    assert_eq!(api.status, 429);
    // Must not retry a >30s directive.
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn captures_etag_and_idempotency_replay() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"rev-7\"")
                .set_body_json(work_item_json()),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/organizations/acme/projects/HAM/work-items"))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("ETag", "\"rev-1\"")
                .insert_header("Idempotency-Replayed", "true")
                .set_body_json(work_item_json()),
        )
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let got = client.get_work_item("acme", "HAM", "HAM-1").await.unwrap();
    assert_eq!(got.etag.as_deref(), Some("\"rev-7\""));

    let created = client
        .create_work_item(
            "acme",
            "HAM",
            &CreateWorkItemRequest {
                title: "T".to_string(),
                ..Default::default()
            },
            "key-12345678",
        )
        .await
        .unwrap();
    assert!(created.idempotency_replayed);

    let post = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.method.as_str() == "POST")
        .unwrap();
    assert_eq!(
        post.headers
            .get("idempotency-key")
            .unwrap()
            .to_str()
            .unwrap(),
        "key-12345678"
    );
}

#[tokio::test]
async fn creates_comment_with_idempotency_key() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/comments",
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id":"c1","workItemId":"w1","parentCommentId":null,
            "author":{"id":"u","name":"U"},"body":"hello","deleted":false,
            "createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let resp = client
        .create_comment(
            "acme",
            "HAM",
            "HAM-1",
            &CreateCommentRequest {
                body: "hello".to_string(),
                parent_comment_id: None,
            },
            "comment-key-1",
        )
        .await
        .unwrap();
    assert_eq!(resp.value.body.as_deref(), Some("hello"));
}

#[tokio::test]
async fn transitions_send_if_match_and_idempotency() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v1/organizations/acme/projects/HAM/work-items/HAM-1/transitions",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"rev-2\"")
                .set_body_json(work_item_json()),
        )
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let resp = client
        .transition_work_item(
            "acme",
            "HAM",
            "HAM-1",
            &TransitionRequest {
                target_status: "in_progress".to_string(),
            },
            "\"rev-1\"",
            "transition-key-1",
        )
        .await
        .unwrap();
    assert_eq!(resp.etag.as_deref(), Some("\"rev-2\""));

    let req = &server.received_requests().await.unwrap()[0];
    assert_eq!(
        req.headers.get("if-match").unwrap().to_str().unwrap(),
        "\"rev-1\""
    );
    assert!(req.headers.contains_key("idempotency-key"));
}

#[tokio::test]
async fn follows_all_pages_through_trait() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations/acme/projects"))
        .respond_with(|req: &Request| {
            let has_cursor = req.url.query().unwrap_or_default().contains("cursor=next");
            let body = if has_cursor {
                json!({"items":[{"id":"2","organizationId":"o","key":"B","name":"B","description":null,"color":"#000","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}],
                        "page":{"limit":1,"hasMore":false,"nextCursor":null}})
            } else {
                json!({"items":[{"id":"1","organizationId":"o","key":"A","name":"A","description":null,"color":"#000","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}],
                        "page":{"limit":1,"hasMore":true,"nextCursor":"next"}})
            };
            ResponseTemplate::new(200).set_body_json(body)
        })
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let collected = follow_all(|cursor| {
        let client = client.clone();
        async move {
            let opts = ListOptions {
                limit: Some(1),
                cursor,
            };
            let resp = client.list_projects("acme", opts).await?;
            Ok(PageItems::new(resp.value.items, &resp.raw, resp.value.page))
        }
    })
    .await
    .unwrap();

    assert_eq!(collected.items.len(), 2);
    assert_eq!(collected.items[1].key, "B");
}

fn work_item_json() -> serde_json::Value {
    json!({
        "id":"1","key":"HAM-1","projectId":"2","title":"T","description":null,
        "type":"task","status":"todo","priority":"low","assignee":null,"reporter":null,
        "sprint":null,"parent":null,"labels":[],
        "storyPoints":null,"dueDate":null,
        "createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","revision":4
    })
}

// ---- Hardening behaviors ---------------------------------------------------

/// A redirect response must surface as its own error and never be followed:
/// the bearer token must not travel to another origin, and the response must
/// not be presented as if it came from the original host.
#[tokio::test]
async fn redirects_are_never_followed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/evil"))
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    assert!(client.whoami().await.is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

/// An oversized response body must be rejected mid-stream instead of buffered:
/// a compromised host must not be able to exhaust memory.
#[tokio::test]
async fn oversized_response_body_is_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![
            b'x';
            hamstik_api_client::client::MAX_BODY_BYTES
                + 1
        ]))
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let err = client.whoami().await.unwrap_err();
    assert!(err.to_string().contains("exceeds"), "{err}");
}

/// Terminal escape sequences from a hostile server must not survive into
/// error messages or request ids.
#[tokio::test]
async fn server_error_text_has_control_characters_stripped() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/me"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "requestId": "req\u{1b}[31m-1",
            "error": {"code": "FORBIDDEN\u{1b}[2J", "message": "no\u{7}pe"}
        })))
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let err = client.whoami().await.unwrap_err();
    let api = err.as_api().unwrap();
    // The ESC introducer is removed, so a printable tail cannot reassemble
    // into a live escape sequence.
    assert_eq!(api.code, "FORBIDDEN[2J");
    assert_eq!(api.message, "nope");
    assert_eq!(api.request_id.as_deref(), Some("req[31m-1"));
}

/// `--all` aggregation must terminate with an error against a server that
/// always claims there is another page.
#[tokio::test]
async fn follow_all_terminates_on_an_endless_server() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/organizations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{"id":"1","slug":"a","name":"A","suspended":false}],
            "page": {"limit": 1, "hasMore": true, "nextCursor": "same"}
        })))
        .mount(&server)
        .await;

    let client = client_for(&server.uri());
    let result = follow_all(|cursor| {
        let client = client.clone();
        async move {
            let resp = client
                .list_organizations(ListOptions {
                    limit: Some(1),
                    cursor,
                })
                .await?;
            Ok(PageItems::new(resp.value.items, &resp.raw, resp.value.page))
        }
    })
    .await;
    assert!(result.is_err(), "endless pagination must fail");
}
