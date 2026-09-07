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
use reqwest::redirect::Policy as RedirectPolicy;
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

/// The largest response body accepted from the server (10 MiB).
///
/// Anything larger is rejected instead of buffered: a compromised host (or any
/// intermediate that can spoof responses) must not be able to exhaust memory.
pub const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

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
fn header_content_disposition() -> HeaderName {
    HeaderName::from_static("content-disposition")
}
fn header_content_type() -> HeaderName {
    HeaderName::from_static("content-type")
}

/// Extracts a UTF-8 file name from a `Content-Disposition` header value
/// (`filename=...` or RFC 5987 `filename*=UTF-8''...`).
fn file_name_from_disposition(value: Option<&str>) -> Option<String> {
    let value = value?;
    let star = value.split(';').map(str::trim).find_map(|part| {
        part.strip_prefix("filename*=")
            .and_then(|v| v.rsplit_once("''"))
            .map(|(_, encoded)| encoded.to_string())
    });
    if let Some(encoded) = star {
        return percent_decode(&encoded);
    }
    let plain = value
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("filename="))?
        .trim_matches('"')
        .to_string();
    if plain.is_empty() { None } else { Some(plain) }
}

/// Minimal percent-decoding for RFC 5987 file names.
fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
            let byte = u8::from_str_radix(hex, 16).ok()?;
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn header_str<'a>(headers: &'a HeaderMap, name: &HeaderName) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// Removes control characters from a server-supplied string before it is
/// rendered or echoed.
///
/// The value is attacker-influenced (a hostile server, or any machine that can
/// intercept TLS-less traffic); terminal escape sequences must not survive into
/// CLI output. Horizontal tab is preserved (it is harmless and common in
/// messages); every other control character, including ESC, CR, and NUL, is
/// dropped, which neutralizes escape sequences by removing the introducer. The
/// `code` and `message` fields are also constrained by the API contract to
/// printable characters, so this can only ever be defense-in-depth.
#[must_use]
pub fn sanitize_server_text(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() || *c == '\t')
        .collect::<String>()
        .trim_end()
        .to_string()
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

/// A downloaded attachment's bytes plus the file name suggested by the server.
#[derive(Debug, Clone)]
pub struct DownloadedAttachment {
    /// The attachment's original bytes.
    pub bytes: Vec<u8>,
    /// The file name from `Content-Disposition`, when the server sent one.
    pub file_name: Option<String>,
    /// The declared `Content-Type`, when the server sent one.
    pub content_type: Option<String>,
    /// Correlation id (`X-Request-Id` header).
    pub request_id: Option<String>,
}

/// A file to upload as a `multipart/form-data` part.
#[derive(Debug, Clone)]
pub struct MultipartFile {
    /// The file name recorded with the upload.
    pub file_name: String,
    /// The MIME type; `None` uses `application/octet-stream`.
    pub content_type: Option<String>,
    /// The file bytes.
    pub bytes: Vec<u8>,
}

/// Construction options for [`HamstikClient`].
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Timeout for establishing a TCP/TLS connection.
    pub connect_timeout: Duration,
    /// Total timeout for one request, including body read.
    pub request_timeout: Duration,
    /// The `User-Agent` sent with every request.
    pub user_agent: String,
    /// Retry/backoff policy; [`RetryPolicy::none`] disables retries.
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
            .use_rustls_tls()
            // This client talks to one fixed origin with a bearer token. It
            // must never follow a redirect to another host: the response would
            // be attributed to the original host and the token-bearing
            // handshake would be replayed elsewhere.
            .redirect(RedirectPolicy::none());

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

            if let Some(response) = self
                .retry_or_pass(response, spec.retryable, attempt)
                .await?
            {
                return self.finalize(response).await;
            }
            // A retry was scheduled; rebuild the request for the next attempt.
            attempt += 1;
        }
    }

    /// Decides whether a response should be retried.
    ///
    /// When a retry is warranted, sleeps (honoring `Retry-After`) and returns
    /// `None`, consuming the response. Otherwise returns the response for
    /// finalization. A `429` whose `Retry-After` exceeds [`MAX_RETRY_AFTER`]
    /// surfaces the rate-limit error immediately instead of blocking.
    async fn retry_or_pass(
        &self,
        response: Response,
        retryable: bool,
        attempt: u32,
    ) -> Result<Option<Response>, ClientError> {
        let status = response.status().as_u16();
        let last_attempt = attempt + 1 >= self.policy.attempts;
        if !retryable || last_attempt || !is_retryable_status(status) {
            return Ok(Some(response));
        }
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
        Ok(None)
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

    /// Sends a request expecting a structured error or `204 No Content`.
    async fn send_void(&self, spec: RequestSpec<'_>) -> Result<(), ClientError> {
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
            if let Some(response) = self
                .retry_or_pass(response, spec.retryable, attempt)
                .await?
            {
                let status = response.status().as_u16();
                if (200..300).contains(&status) {
                    return Ok(());
                }
                return Err(self.to_api_error(response).await);
            }
            attempt += 1;
        }
    }

    /// Sends a request expecting binary bytes (attachment download), capping
    /// the body at [`MAX_BODY_BYTES`].
    async fn send_bytes(&self, spec: RequestSpec<'_>) -> Result<DownloadedAttachment, ClientError> {
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
            if let Some(response) = self
                .retry_or_pass(response, spec.retryable, attempt)
                .await?
            {
                let status = response.status().as_u16();
                if !(200..300).contains(&status) {
                    return Err(self.to_api_error(response).await);
                }
                let headers = response.headers().clone();
                let bytes = read_body_capped(response).await?;
                return Ok(DownloadedAttachment {
                    bytes: bytes.to_vec(),
                    file_name: file_name_from_disposition(header_str(
                        &headers,
                        &header_content_disposition(),
                    )),
                    content_type: header_str(&headers, &header_content_type()).map(str::to_string),
                    request_id: header_str(&headers, &header_request_id())
                        .map(sanitize_server_text)
                        .filter(|id| !id.is_empty()),
                });
            }
            attempt += 1;
        }
    }

    /// Uploads a file as `multipart/form-data` with a single `file` part.
    async fn send_multipart(
        &self,
        spec: &RequestSpec<'_>,
        file_name: &str,
        content_type: Option<&str>,
        bytes: Vec<u8>,
    ) -> Result<ApiResponse<Attachment>, ClientError> {
        let mut attempt = 0u32;
        loop {
            let form = reqwest::multipart::Form::new().part(
                "file",
                reqwest::multipart::Part::bytes(bytes.clone())
                    .file_name(file_name.to_string())
                    .mime_str(content_type.unwrap_or("application/octet-stream"))
                    .map_err(|err| ClientError::Protocol(format!("invalid content type: {err}")))?,
            );
            let request = self.build_multipart(spec, form)?;
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
            if let Some(response) = self
                .retry_or_pass(response, spec.retryable, attempt)
                .await?
            {
                return self.finalize(response).await;
            }
            attempt += 1;
        }
    }

    fn build_multipart(
        &self,
        spec: &RequestSpec<'_>,
        form: reqwest::multipart::Form,
    ) -> Result<reqwest::RequestBuilder, ClientError> {
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
        Ok(builder.multipart(form))
    }

    async fn finalize<T>(&self, response: Response) -> Result<ApiResponse<T>, ClientError>
    where
        T: DeserializeOwned,
    {
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = read_body_capped(response).await?;

        let raw: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .map_err(|err| ClientError::Protocol(format!("invalid JSON response: {err}")))?
        };

        if (200..300).contains(&status) {
            // Deserialize directly from the raw value; `&Value` implements
            // `Deserializer`, so the whole body never needs to be cloned.
            let value: T = T::deserialize(&raw).map_err(|err| {
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
        // Best effort: a body that fails the cap or the parse still yields a
        // status-only API error.
        let bytes = read_body_capped(response).await.unwrap_or_default();
        let raw: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        self.api_error_from(status, &raw, &headers)
    }

    fn api_error_from(&self, status: u16, raw: &Value, headers: &HeaderMap) -> ClientError {
        let error = raw.get("error");
        let code = error
            .and_then(|e| e.get("code"))
            .and_then(Value::as_str)
            .map(sanitize_server_text)
            .filter(|code| !code.is_empty())
            .unwrap_or_else(|| default_code_for_status(status).to_string());
        let message = error
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .map(sanitize_server_text)
            .filter(|message| !message.is_empty())
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

/// Reads a response body, enforcing [`MAX_BODY_BYTES`].
///
/// `Content-Length` above the cap is rejected before reading. When the length
/// is unknown, the body is streamed chunk-by-chunk and the read aborts as soon
/// as the running total would exceed the cap, so an oversized body never
/// buffers in memory.
async fn read_body_capped(response: Response) -> Result<bytes::Bytes, ClientError> {
    if response
        .content_length()
        .is_some_and(|length| usize::try_from(length).is_ok_and(|len| len > MAX_BODY_BYTES))
    {
        return Err(ClientError::Protocol(format!(
            "response body exceeds the {MAX_BODY_BYTES} byte limit"
        )));
    }

    let mut stream = response;
    let mut buffer = bytes::BytesMut::with_capacity(8 * 1024);
    while let Some(chunk) = stream
        .chunk()
        .await
        .map_err(|err| ClientError::Network(err.to_string()))?
    {
        if buffer.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(ClientError::Protocol(format!(
                "response body exceeds the {MAX_BODY_BYTES} byte limit"
            )));
        }
        buffer.extend_from_slice(&chunk);
    }
    Ok(buffer.freeze())
}

fn extract_request_id(raw: &Value, headers: &HeaderMap) -> Option<String> {
    raw.get("requestId")
        .and_then(Value::as_str)
        .or_else(|| header_str(headers, &header_request_id()))
        .map(sanitize_server_text)
        .filter(|id| !id.is_empty())
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
                        .map(sanitize_server_text)
                        .collect()
                })
                .unwrap_or_default();
            map.insert(sanitize_server_text(key), list);
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
///
/// Every method maps to exactly one endpoint; transport concerns (auth,
/// retries, idempotency, error mapping) are shared in [`HamstikClient`].
#[async_trait]
pub trait HamstikApi: Send + Sync {
    /// `GET /me`: the authenticated identity and credential context.
    async fn whoami(&self) -> Result<ApiResponse<Me>, ClientError>;
    /// `GET /organizations`: the organizations the user belongs to.
    async fn list_organizations(
        &self,
        opts: ListOptions,
    ) -> Result<ApiResponse<OrganizationList>, ClientError>;
    /// `GET /organizations/{slug}`: one organization.
    async fn get_organization(
        &self,
        org_slug: &str,
    ) -> Result<ApiResponse<Organization>, ClientError>;
    /// `GET /organizations/{slug}/projects`: the organization's projects.
    async fn list_projects(
        &self,
        org_slug: &str,
        opts: ListOptions,
    ) -> Result<ApiResponse<ProjectList>, ClientError>;
    /// `GET /organizations/{slug}/projects/{key}`: one project.
    async fn get_project(
        &self,
        org_slug: &str,
        project_key: &str,
    ) -> Result<ApiResponse<Project>, ClientError>;
    /// `GET .../work-items`: query work items with filters.
    async fn list_work_items(
        &self,
        org_slug: &str,
        project_key: &str,
        query: ListWorkItemsQuery,
    ) -> Result<ApiResponse<WorkItemList>, ClientError>;
    /// `POST .../work-items`: create a work item (idempotent).
    async fn create_work_item(
        &self,
        org_slug: &str,
        project_key: &str,
        body: &CreateWorkItemRequest,
        idempotency_key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError>;
    /// `GET .../work-items/{key}`: one work item (captures the `ETag`).
    async fn get_work_item(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError>;
    /// `PATCH .../work-items/{key}`: conditional update via `If-Match`.
    async fn update_work_item(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        body: &UpdateWorkItemRequest,
        if_match: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError>;
    /// `GET .../work-items/{key}/transitions`: permitted status transitions.
    async fn list_transitions(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
    ) -> Result<ApiResponse<WorkItemTransitionList>, ClientError>;
    /// `POST .../work-items/{key}/transitions`: move the item to a status.
    async fn transition_work_item(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        body: &TransitionRequest,
        if_match: &str,
        idempotency_key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError>;
    /// `GET .../work-items/{key}/comments`: list comments.
    async fn list_comments(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        opts: ListOptions,
    ) -> Result<ApiResponse<CommentList>, ClientError>;
    /// `POST .../work-items/{key}/comments`: add a comment (idempotent).
    async fn create_comment(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        body: &CreateCommentRequest,
        idempotency_key: &str,
    ) -> Result<ApiResponse<Comment>, ClientError>;
    /// `DELETE .../work-items/{key}/comments/{commentId}`: remove an authored
    /// comment with no replies (204 on success).
    async fn delete_comment(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        comment_id: &str,
    ) -> Result<(), ClientError>;
    /// `POST .../projects`: create a Project (Organization administrators,
    /// idempotent).
    async fn create_project(
        &self,
        org_slug: &str,
        body: &CreateProjectRequest,
        idempotency_key: &str,
    ) -> Result<ApiResponse<Project>, ClientError>;
    /// `GET .../sprints`: list the Project's Sprints (dated first).
    async fn list_sprints(
        &self,
        org_slug: &str,
        project_key: &str,
        opts: ListOptions,
    ) -> Result<ApiResponse<SprintList>, ClientError>;
    /// `POST .../sprints`: create a Sprint (idempotent).
    async fn create_sprint(
        &self,
        org_slug: &str,
        project_key: &str,
        body: &CreateSprintRequest,
        idempotency_key: &str,
    ) -> Result<ApiResponse<Sprint>, ClientError>;
    /// `GET .../sprints/{id}`: one Sprint (captures the Sprint `ETag`).
    async fn get_sprint(
        &self,
        org_slug: &str,
        project_key: &str,
        sprint_id: &str,
    ) -> Result<ApiResponse<Sprint>, ClientError>;
    /// `GET .../sprints/{id}/transitions`: permitted Sprint state transitions.
    async fn list_sprint_transitions(
        &self,
        org_slug: &str,
        project_key: &str,
        sprint_id: &str,
    ) -> Result<ApiResponse<SprintTransitionList>, ClientError>;
    /// `POST .../sprints/{id}/transitions`: move the Sprint to a state
    /// (idempotent, `If-Match` required).
    async fn transition_sprint(
        &self,
        org_slug: &str,
        project_key: &str,
        sprint_id: &str,
        body: &TransitionSprintRequest,
        if_match: &str,
        idempotency_key: &str,
    ) -> Result<ApiResponse<Sprint>, ClientError>;
    /// `GET .../labels`: list the Project's labels.
    async fn list_labels(
        &self,
        org_slug: &str,
        project_key: &str,
        opts: ListOptions,
    ) -> Result<ApiResponse<ProjectLabelList>, ClientError>;
    /// `POST .../labels`: create a Project label (idempotent).
    async fn create_label(
        &self,
        org_slug: &str,
        project_key: &str,
        body: &CreateLabelRequest,
        idempotency_key: &str,
    ) -> Result<ApiResponse<ProjectLabel>, ClientError>;
    /// `POST .../work-items/{key}/labels`: attach a label (idempotent,
    /// `If-Match` required).
    async fn attach_label(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        label_id: &str,
        if_match: &str,
        idempotency_key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError>;
    /// `DELETE .../work-items/{key}/labels/{labelId}`: detach a label
    /// (idempotent, `If-Match` required).
    async fn detach_label(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        label_id: &str,
        if_match: &str,
        idempotency_key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError>;
    /// `GET .../work-items/{key}/attachments`: list attachment metadata.
    async fn list_attachments(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        opts: ListOptions,
    ) -> Result<ApiResponse<AttachmentList>, ClientError>;
    /// Uploads a file as `multipart/form-data` with a single `file` part.
    /// The bytes are bundled into a [`MultipartFile`] so the argument count
    /// stays reviewable. Idempotent.
    async fn upload_attachment(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        file: &MultipartFile,
        idempotency_key: &str,
    ) -> Result<ApiResponse<Attachment>, ClientError>;
    /// `GET .../work-items/{key}/attachments/{id}`: download the original
    /// bytes. Returns the bytes alongside the `Content-Disposition` file name.
    async fn download_attachment(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        attachment_id: &str,
    ) -> Result<DownloadedAttachment, ClientError>;
    /// `DELETE .../work-items/{key}/attachments/{id}`: remove an attachment
    /// (creator or Organization administrator; 204 on success).
    async fn delete_attachment(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        attachment_id: &str,
    ) -> Result<(), ClientError>;
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

    async fn delete_comment(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        comment_id: &str,
    ) -> Result<(), ClientError> {
        self.send_void(RequestSpec {
            method: Method::DELETE,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
                key.to_string(),
                "comments".to_string(),
                comment_id.to_string(),
            ],
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn create_project(
        &self,
        org_slug: &str,
        body: &CreateProjectRequest,
        idempotency_key: &str,
    ) -> Result<ApiResponse<Project>, ClientError> {
        let payload = serde_json::to_value(body)
            .map_err(|err| ClientError::Protocol(format!("invalid request body: {err}")))?;
        self.send_json(RequestSpec {
            method: Method::POST,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
            ],
            query: Vec::new(),
            headers: vec![(header_idempotency_key(), idempotency_key.to_string())],
            body: Some(&payload),
            retryable: true,
        })
        .await
    }

    async fn list_sprints(
        &self,
        org_slug: &str,
        project_key: &str,
        opts: ListOptions,
    ) -> Result<ApiResponse<SprintList>, ClientError> {
        let mut query = Vec::new();
        push_list(&mut query, &opts);
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "sprints".to_string(),
            ],
            query,
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn create_sprint(
        &self,
        org_slug: &str,
        project_key: &str,
        body: &CreateSprintRequest,
        idempotency_key: &str,
    ) -> Result<ApiResponse<Sprint>, ClientError> {
        let payload = serde_json::to_value(body)
            .map_err(|err| ClientError::Protocol(format!("invalid request body: {err}")))?;
        self.send_json(RequestSpec {
            method: Method::POST,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "sprints".to_string(),
            ],
            query: Vec::new(),
            headers: vec![(header_idempotency_key(), idempotency_key.to_string())],
            body: Some(&payload),
            retryable: true,
        })
        .await
    }

    async fn get_sprint(
        &self,
        org_slug: &str,
        project_key: &str,
        sprint_id: &str,
    ) -> Result<ApiResponse<Sprint>, ClientError> {
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "sprints".to_string(),
                sprint_id.to_string(),
            ],
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn list_sprint_transitions(
        &self,
        org_slug: &str,
        project_key: &str,
        sprint_id: &str,
    ) -> Result<ApiResponse<SprintTransitionList>, ClientError> {
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "sprints".to_string(),
                sprint_id.to_string(),
                "transitions".to_string(),
            ],
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn transition_sprint(
        &self,
        org_slug: &str,
        project_key: &str,
        sprint_id: &str,
        body: &TransitionSprintRequest,
        if_match: &str,
        idempotency_key: &str,
    ) -> Result<ApiResponse<Sprint>, ClientError> {
        let payload = serde_json::to_value(body)
            .map_err(|err| ClientError::Protocol(format!("invalid request body: {err}")))?;
        self.send_json(RequestSpec {
            method: Method::POST,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "sprints".to_string(),
                sprint_id.to_string(),
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

    async fn list_labels(
        &self,
        org_slug: &str,
        project_key: &str,
        opts: ListOptions,
    ) -> Result<ApiResponse<ProjectLabelList>, ClientError> {
        let mut query = Vec::new();
        push_list(&mut query, &opts);
        self.send_json(RequestSpec {
            method: Method::GET,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "labels".to_string(),
            ],
            query,
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn create_label(
        &self,
        org_slug: &str,
        project_key: &str,
        body: &CreateLabelRequest,
        idempotency_key: &str,
    ) -> Result<ApiResponse<ProjectLabel>, ClientError> {
        let payload = serde_json::to_value(body)
            .map_err(|err| ClientError::Protocol(format!("invalid request body: {err}")))?;
        self.send_json(RequestSpec {
            method: Method::POST,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "labels".to_string(),
            ],
            query: Vec::new(),
            headers: vec![(header_idempotency_key(), idempotency_key.to_string())],
            body: Some(&payload),
            retryable: true,
        })
        .await
    }

    async fn attach_label(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        label_id: &str,
        if_match: &str,
        idempotency_key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError> {
        let payload = serde_json::json!({ "labelId": label_id });
        self.send_json(RequestSpec {
            method: Method::POST,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
                key.to_string(),
                "labels".to_string(),
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

    async fn detach_label(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        label_id: &str,
        if_match: &str,
        idempotency_key: &str,
    ) -> Result<ApiResponse<WorkItem>, ClientError> {
        self.send_json(RequestSpec {
            method: Method::DELETE,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
                key.to_string(),
                "labels".to_string(),
                label_id.to_string(),
            ],
            query: Vec::new(),
            headers: vec![
                (IF_MATCH.clone(), if_match.to_string()),
                (header_idempotency_key(), idempotency_key.to_string()),
            ],
            body: None,
            retryable: true,
        })
        .await
    }

    async fn list_attachments(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        opts: ListOptions,
    ) -> Result<ApiResponse<AttachmentList>, ClientError> {
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
                "attachments".to_string(),
            ],
            query,
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn upload_attachment(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        file: &MultipartFile,
        idempotency_key: &str,
    ) -> Result<ApiResponse<Attachment>, ClientError> {
        let spec = RequestSpec {
            method: Method::POST,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
                key.to_string(),
                "attachments".to_string(),
            ],
            query: Vec::new(),
            headers: vec![(header_idempotency_key(), idempotency_key.to_string())],
            body: None,
            retryable: true,
        };
        self.send_multipart(
            &spec,
            &file.file_name,
            file.content_type.as_deref(),
            file.bytes.clone(),
        )
        .await
    }

    async fn download_attachment(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        attachment_id: &str,
    ) -> Result<DownloadedAttachment, ClientError> {
        self.send_bytes(RequestSpec {
            method: Method::GET,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
                key.to_string(),
                "attachments".to_string(),
                attachment_id.to_string(),
            ],
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }

    async fn delete_attachment(
        &self,
        org_slug: &str,
        project_key: &str,
        key: &str,
        attachment_id: &str,
    ) -> Result<(), ClientError> {
        self.send_void(RequestSpec {
            method: Method::DELETE,
            segments: vec![
                "organizations".to_string(),
                org_slug.to_string(),
                "projects".to_string(),
                project_key.to_string(),
                "work-items".to_string(),
                key.to_string(),
                "attachments".to_string(),
                attachment_id.to_string(),
            ],
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            retryable: true,
        })
        .await
    }
}

#[cfg(test)]
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::retry::NoopSleeper;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client(uri: &str) -> HamstikClient {
        let host = Host::parse(uri).unwrap();
        HamstikClient::with_sleeper(
            host,
            SecretString::from("tok".to_string()),
            ClientConfig {
                retry: RetryPolicy::none(),
                ..ClientConfig::default()
            },
            Arc::new(NoopSleeper),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn oversized_body_is_rejected_not_buffered() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/me"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; MAX_BODY_BYTES + 1]))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let err = client.whoami().await.unwrap_err();
        assert!(
            matches!(err, ClientError::Protocol(ref m) if m.contains("exceeds")),
            "{err}"
        );
    }

    #[tokio::test]
    async fn redirects_are_not_followed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/me"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", "/elsewhere"))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        // A redirect must surface as its own status (protocol error), never be
        // silently followed: the token must not travel to another origin.
        assert!(client.whoami().await.is_err());
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "redirect must not be followed");
    }

    #[tokio::test]
    async fn server_error_text_is_sanitized_of_control_characters() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/me"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "requestId": "req-1\u{7}",
                "error": {
                    "code": "FORBIDDEN\u{1b}[2J",
                    "message": "no\u{1b}]0;evil\u{7}pe"
                }
            })))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let err = client.whoami().await.unwrap_err();
        let api = err.as_api().unwrap();
        // The ESC introducer is removed, so `[2J` cannot reassemble into an
        // escape sequence even though its printable chars survive.
        assert_eq!(api.code, "FORBIDDEN[2J");
        assert_eq!(api.message, "no]0;evilpe");
        assert_eq!(api.request_id.as_deref(), Some("req-1"));
    }

    #[test]
    fn sanitize_removes_control_characters_only() {
        assert_eq!(sanitize_server_text("plain"), "plain");
        assert_eq!(sanitize_server_text("tab\tkept"), "tab\tkept");
        assert_eq!(sanitize_server_text("esc\u{1b}[31m"), "esc[31m");
        assert_eq!(sanitize_server_text("nul\u{0}"), "nul");
        assert_eq!(sanitize_server_text("cr\rlf\n"), "crlf");
        assert_eq!(sanitize_server_text("kept \t "), "kept");
        assert_eq!(sanitize_server_text("emoji 🐹 ok"), "emoji 🐹 ok");
    }
}
