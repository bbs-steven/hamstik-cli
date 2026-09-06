// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Typed, retrying client for the Hamstik Public API v1.
//!
//! [`HamstikApi`] is the single trait the CLI depends on (a mock lives beside
//! it in the CLI's dev-dependencies). [`HamstikClient`] is the production
//! implementation built on `reqwest`. All transport concerns — authentication,
//! idempotency, ETag capture, retry/backoff, error mapping, and request-id
//! propagation — are handled centrally in [`HamstikClient::send_json`]; the
//! trait methods only assemble request shapes.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderName, HeaderValue, IF_MATCH};
use reqwest::{Certificate, Client, Method, Response};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{ApiError, ClientError};
use crate::host::Host;
use crate::models::*;
use crate::retry::{
    MAX_RETRY_AFTER, RetryPolicy, SharedSleeper, TokioSleeper, backoff_delay, is_retryable_status,
    parse_retry_after,
};

fn header_etag_name() -> HeaderName {
    HeaderName::from_static("etag")
}
fn header_idempotency_key() -> HeaderName {
    HeaderName::from_static("idempotency-key")
}
fn header_idempotency_replayed() -> HeaderName {
    HeaderName::from_static("idempotency-replayed")
}
fn header_request_id() -> HeaderName {
    HeaderName::from_static("x-request-id")
}
fn header_retry_after() -> HeaderName {
    HeaderName::from_static("retry-after")
}

fn header_str<'a>(headers: &'a HeaderMap, name: &HeaderName) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// A successful, decoded API response.
#[derive(Debug, Clone)]
pub struct ApiResponse<T> {
    /// The typed, deserialized body used for human rendering.
    pub value: T,
    /// The raw JSON body, echoed verbatim for `--json` fidelity.
    pub raw: Value,
    /// Correlation id (`requestId` from the envelope, or `X-Request-Id`).
    pub request_id: Option<String>,
    /// The `ETag` header, when the endpoint returns one.
    pub etag: Option<String>,
    /// True when the server signaled this was an idempotent replay.
    pub idempotency_replayed: bool,
}

/// Construction options for [`HamstikClient`].
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub user_agent: String,
    pub retry: RetryPolicy,
    /// Additional PEM root certificates (from `--ca-bundle` / env).
    pub ca_pem: Vec<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            user_agent: String::from("hamstik-cli"),
            retry: RetryPolicy::default(),
            ca_pem: Vec::new(),
        }
    }
}

/// The production [`HamstikApi`] implementation.
#[derive(Clone)]
pub struct HamstikClient {
    http: Client,
    host: Host,
    token: SecretString,
    policy: RetryPolicy,
    sleeper: SharedSleeper,
}

impl HamstikClient {
    /// Builds a client using the real Tokio sleeper.
    pub fn new(host: Host, token: SecretString, config: ClientConfig) -> Result<Self, ClientError> {
        Self::with_sleeper(host, token, config, Arc::new(TokioSleeper))
    }

    /// Builds a client with an injected sleeper (tests pass a no-op sleeper).
    pub fn with_sleeper(
        host: Host,
        token: SecretString,
        config: ClientConfig,
        sleeper: SharedSleeper,
    ) -> Result<Self, ClientError> {
        let mut builder = Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .user_agent(config.user_agent.clone())
            .use_rustls_tls();

        for pem in &config.ca_pem {
            let cert = Certificate::from_pem(pem.as_bytes())
                .map_err(|err| ClientError::Protocol(format!("invalid CA bundle: {err}")))?;
            builder = builder.add_root_certificate(cert);
        }

        let http = builder
            .build()
            .map_err(|err| ClientError::Protocol(format!("failed to build HTTP client: {err}")))?;

        Ok(Self {
            http,
            host,
            token,
            policy: config.retry,
            sleeper,
        })
    }

    async fn send_json<T>(&self, spec: RequestSpec<'_>) -> Result<ApiResponse<T>, ClientError>
    where
        T: DeserializeOwned,
    {
        let mut attempt = 0u32;
        loop {
            let request = self.build(&spec)?;
            let response = match request.send().await {
                Ok(response) => response,
                Err(err) => {
                    let transient = err.is_connect() || err.is_timeout();
                    if spec.retryable && transient && attempt + 1 < self.policy.attempts {
                        self.sleeper
                            .sleep(backoff_delay(&self.policy, attempt))
                            .await;
                        attempt += 1;
                        continue;
                    }
                    return Err(ClientError::Network(err.to_string()));
                }
            };

            let status = response.status().as_u16();
            let last_attempt = attempt + 1 >= self.policy.attempts;

            if spec.retryable && !last_attempt && is_retryable_status(status) {
                let wait = match status {
                    429 => match retry_after_from(&response) {
                        Some(delay) if delay > MAX_RETRY_AFTER => {
                            // Server asks us to wait longer than we are willing:
                            // surface the rate-limit error immediately (exit 7).
                            return Err(self.to_api_error(response).await);
                        }
                        Some(delay) => delay,
                        None => backoff_delay(&self.policy, attempt),
                    },
                    _ => backoff_delay(&self.policy, attempt),
                };
                self.sleeper.sleep(wait).await;
                attempt += 1;
                continue;
            }

            return self.finalize(response).await;
        }
    }

    fn build(&self, spec: &RequestSpec<'_>) -> Result<reqwest::RequestBuilder, ClientError> {
        let segments: Vec<&str> = spec.segments.iter().map(String::as_str).collect();
        let url = self.host.resource_url(&segments)?;

        let auth = HeaderValue::from_str(&format!("Bearer {}", self.token.expose_secret()))
            .map_err(|_| ClientError::Protocol("token contains invalid characters".into()))?;

        let mut builder = self
            .http
            .request(spec.method.clone(), url)
            .header(AUTHORIZATION, auth)
            .header(ACCEPT, HeaderValue::from_static("application/json"));

        for (name, value) in &spec.headers {
            let header = HeaderValue::from_str(value)
                .map_err(|_| ClientError::Protocol(format!("invalid value for header {name}")))?;
            builder = builder.header(name.clone(), header);
        }

        if !spec.query.is_empty() {
            builder = builder.query(&spec.query);
        }
        if let Some(body) = spec.body {
            builder = builder.json(body);
        }
        Ok(builder)
    }

    async fn finalize<T>(&self, response: Response) -> Result<ApiResponse<T>, ClientError>
    where
        T: DeserializeOwned,
    {
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = response
            .bytes()
            .await
            .map_err(|err| ClientError::Network(err.to_string()))?;

        let raw: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .map_err(|err| ClientError::Protocol(format!("invalid JSON response: {err}")))?
        };

        if (200..300).contains(&status) {
            let value: T = serde_json::from_value(raw.clone()).map_err(|err| {
                ClientError::Protocol(format!("response does not match expected shape: {err}"))
            })?;
            let request_id = extract_request_id(&raw, &headers);
            let etag = header_str(&headers, &header_etag_name()).map(str::to_string);
            let idempotency_replayed = header_str(&headers, &header_idempotency_replayed())
                .is_some_and(|v| !v.eq_ignore_ascii_case("false"));
            Ok(ApiResponse {
                value,
                raw,
                request_id,
                etag,
                idempotency_replayed,
            })
        } else {
            Err(self.api_error_from(status, &raw, &headers))
        }
    }

    async fn to_api_error(&self, response: Response) -> ClientError {
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = response.bytes().await.unwrap_or_default();
        let raw: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        self.api_error_from(status, &raw, &headers)
    }

    fn api_error_from(&self, status: u16, raw: &Value, headers: &HeaderMap) -> ClientError {
        let error = raw.get("error");
        let code = error
            .and_then(|e| e.get("code"))
            .and_then(Value::as_str)
            .unwrap_or_else(|| default_code_for_status(status))
            .to_string();
        let message = error
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("request failed with status {status}"));
        let retry_after = header_str(headers, &header_retry_after())
            .and_then(|v| parse_retry_after(v, SystemTime::now()));

        ClientError::Api(ApiError {
            status,
            code,
            message,
            request_id: extract_request_id(raw, headers),
            field_errors: parse_field_errors(error.and_then(|e| e.get("fieldErrors"))),
            retry_after,
        })
    }
}

fn retry_after_from(response: &Response) -> Option<Duration> {
    header_str(response.headers(), &header_retry_after())
        .and_then(|v| parse_retry_after(v, SystemTime::now()))
}

fn extract_request_id(raw: &Value, headers: &HeaderMap) -> Option<String> {
    raw.get("requestId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| header_str(headers, &header_request_id()).map(str::to_string))
}

fn parse_field_errors(value: Option<&Value>) -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::new();
    if let Some(object) = value.and_then(Value::as_object) {
        for (key, values) in object {
            let list = values
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            map.insert(key.clone(), list);
        }
    }
    map
}

fn default_code_for_status(status: u16) -> &'static str {
    match status {
        400 => "VALIDATION_ERROR",
        401 => "AUTH_REQUIRED",
        403 => "FORBIDDEN",
        404 => "NOT_FOUND",
        409 => "CONFLICT",
        412 => "REVISION_CONFLICT",
        413 => "PAYLOAD_TOO_LARGE",
        428 => "PRECONDITION_REQUIRED",
        429 => "RATE_LIMITED",
        500 => "INTERNAL_ERROR",
        _ => "INTERNAL_ERROR",
    }
}

/// Internal, transport-level description of a single logical request.
struct RequestSpec<'a> {
    method: Method,
    segments: Vec<String>,
    query: Vec<(String, String)>,
    headers: Vec<(HeaderName, String)>,
    body: Option<&'a Value>,
    retryable: bool,
}

fn push_list(query: &mut Vec<(String, String)>, opts: &ListOptions) {
    if let Some(limit) = opts.limit {
        query.push(("limit".to_string(), limit.to_string()));
    }
    if let Some(cursor) = &opts.cursor {
        query.push(("cursor".to_string(), cursor.clone()));
    }
}

fn work_item_query(q: &ListWorkItemsQuery) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(limit) = q.limit {
        out.push(("limit".to_string(), limit.to_string()));
    }
    if let Some(cursor) = &q.cursor {
        out.push(("cursor".to_string(), cursor.clone()));
    }
    if let Some(text) = &q.q {
        out.push(("q".to_string(), text.clone()));
    }
    for status in &q.status {
        out.push(("status".to_string(), status.clone()));
    }
    if let Some(scope) = &q.scope {
        out.push(("scope".to_string(), scope.clone()));
    }
    for item_type in &q.item_type {
        out.push(("type".to_string(), item_type.clone()));
    }
    for priority in &q.priority {
        out.push(("priority".to_string(), priority.clone()));
    }
    if let Some(assignee) = &q.assignee {
        out.push(("assignee".to_string(), assignee.clone()));
    }
    if let Some(sprint) = &q.sprint {
        out.push(("sprint".to_string(), sprint.clone()));
    }
    for label in &q.label {
        out.push(("label".to_string(), label.clone()));
    }
    for name in &q.label_name {
        out.push(("labelName".to_string(), name.clone()));
    }
    if let Some(parent) = &q.parent {
        out.push(("parent".to_string(), parent.clone()));
    }
    if let Some(top_level) = q.top_level {
        out.push((
            "topLevel".to_string(),
            if top_level { "1" } else { "0" }.to_string(),
        ));
    }
    if let Some(updated_after) = &q.updated_after {
        out.push(("updatedAfter".to_string(), updated_after.clone()));
    }
    out
}

/// The complete, typed surface of the Hamstik Public API used by the CLI.
#[async_trait]
pub trait HamstikApi: Send + Sync {
    async fn whoami(&self) -> Result<ApiResponse<Me>, ClientError>;
    async fn list_organizations(
        &self,
        opts: ListOptions,
    ) -> Result<ApiResponse<OrganizationList>, ClientError>;
    async fn get_organization(
        &self,
        org_slug: &str,
    ) -> Result<ApiResponse<Organization>, ClientError>;
    async fn list_projects(
        &self,
        org_slug: &str,
        opts: ListOptions,
    ) -> Result<ApiResponse<ProjectList>, ClientError>;
    async fn get_project(
        &self,
        org_slug: &str,
        project_key: &str,
    ) -> Result<ApiResponse<Project>, ClientError>;
    async fn list_work_items(
        &self,
        org_slug: &str,
        project_key: &str,
        query: ListWorkItemsQuery,
    ) -> Result<ApiResponse<WorkItemList>, ClientError>;
    async fn create_work_item(
        &self,
        org_slug: &str,
        project_key: &str,
        body: &CreateWorkItemRequest,
        idempotency_key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError>;
    async fn get_work_item(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError>;
    async fn update_work_item(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        body: &UpdateWorkItemRequest,
        if_match: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError>;
    async fn list_transitions(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
    ) -> Result<ApiResponse<WorkItemTransitionList>, ClientError>;
    async fn transition_work_item(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        body: &TransitionRequest,
        if_match: &str,
        idempotency_key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError>;
    async fn list_comments(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        opts: ListOptions,
    ) -> Result<ApiResponse<CommentList>, ClientError>;
    async fn create_comment(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        body: &CreateCommentRequest,
        idempotency_key: &str,
    ) -> Result<ApiResponse<Comment>, ClientError>;
}

#[async_trait]
impl HamstikApi for HamstikClient {
    async fn whoami(&self) -> Result<ApiResponse<Me>, ClientError> {
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec!["me".to_string()],
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn list_organizations(
        &self,
        opts: ListOptions,
    ) -> Result<ApiResponse<OrganizationList>, ClientError> {
        let mut query = Vec::new();
        push_list(&mut query, &opts);
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec!["organizations".to_string()],
            query,
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn get_organization(
        &self,
        org_slug: &str,
    ) -> Result<ApiResponse<Organization>, ClientError> {
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec!["organizations".to_string(), org_slug.to_string()],
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn list_projects(
        &self,
        org_slug: &str,
        opts: ListOptions,
    ) -> Result<ApiResponse<ProjectList>, ClientError> {
        let mut query = Vec::new();
        push_list(&mut query, &opts);
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
            ],
            query,
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn get_project(
        &self,
        org_slug: &str,
        project_key: &str,
    ) -> Result<ApiResponse<Project>, ClientError> {
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
            ],
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn list_work_items(
        &self,
        org_slug: &str,
        project_key: &str,
        query: ListWorkItemsQuery,
    ) -> Result<ApiResponse<WorkItemList>, ClientError> {
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
            ],
            query: work_item_query(&query),
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn create_work_item(
        &self,
        org_slug: &str,
        project_key: &str,
        body: &CreateWorkItemRequest,
        idempotency_key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError> {
        let payload = serde_json::to_value(body)
            .map_err(|err| ClientError::Protocol(format!("invalid request body: {err}")))?;
        self.send_json(RequestSpec {
            method: Method::POST,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
            ],
            query: Vec::new(),
            headers: vec![(header_idempotency_key(), idempotency_key.to_string())],
            body: Some(&payload),
            retryable: true,
        })
        .await
    }

    async fn get_work_item(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError> {
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
                key.to_string(),
            ],
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn update_work_item(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        body: &UpdateWorkItemRequest,
        if_match: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError> {
        let payload = serde_json::to_value(body)
            .map_err(|err| ClientError::Protocol(format!("invalid request body: {err}")))?;
        self.send_json(RequestSpec {
            method: Method::PATCH,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
                key.to_string(),
            ],
            query: Vec::new(),
            headers: vec![(IF_MATCH.clone(), if_match.to_string())],
            body: Some(&payload),
            // PATCH is never auto-retried.
            retryable: false,
        })
        .await
    }

    async fn list_transitions(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
    ) -> Result<ApiResponse<WorkItemTransitionList>, ClientError> {
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
                key.to_string(),
                "transitions".to_string(),
            ],
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn transition_work_item(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        body: &TransitionRequest,
        if_match: &str,
        idempotency_key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError> {
        let payload = serde_json::to_value(body)
            .map_err(|err| ClientError::Protocol(format!("invalid request body: {err}")))?;
        self.send_json(RequestSpec {
            method: Method::POST,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
                key.to_string(),
                "transitions".to_string(),
            ],
            query: Vec::new(),
            headers: vec![
                (IF_MATCH.clone(), if_match.to_string()),
                (header_idempotency_key(), idempotency_key.to_string()),
            ],
            body: Some(&payload),
            retryable: true,
        })
        .await
    }

    async fn list_comments(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        opts: ListOptions,
    ) -> Result<ApiResponse<CommentList>, ClientError> {
        let mut query = Vec::new();
        push_list(&mut query, &opts);
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
                key.to_string(),
                "comments".to_string(),
            ],
            query,
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn create_comment(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        body: &CreateCommentRequest,
        idempotency_key: &str,
    ) -> Result<ApiResponse<Comment>, ClientError> {
        let payload = serde_json::to_value(body)
            .map_err(|err| ClientError::Protocol(format!("invalid request body: {err}")))?;
        self.send_json(RequestSpec {
            method: Method::POST,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
                key.to_string(),
                "comments".to_string(),
            ],
            query: Vec::new(),
            headers: vec![(header_idempotency_key(), idempotency_key.to_string())],
            body: Some(&payload),
            retryable: true,
        })
        .await
    }
}
