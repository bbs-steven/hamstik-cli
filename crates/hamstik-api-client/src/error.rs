// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Canonical error types shared by the Hamstik API client.

use std::collections::BTreeMap;
use std::time::Duration;

use thiserror::Error;

/// Errors produced while validating or normalizing a configured host.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum HostError {
    #[error("host must not be empty")]
    Empty,
    #[error("invalid host URL: {0}")]
    InvalidUrl(String),
    #[error("unsupported URL scheme {0:?}; only https (or http for loopback) is allowed")]
    UnsupportedScheme(String),
    #[error("insecure http is only permitted for loopback hosts")]
    InsecureScheme,
    #[error("embedded credentials are not allowed in the host URL")]
    CredentialsForbidden,
    #[error("query and fragment components are not allowed in the host URL")]
    QueryFragmentForbidden,
    #[error("host URL must not include a path; the /api/v1 prefix is added automatically")]
    PathForbidden,
}

/// A structured error returned by the Public API.
///
/// The server's stable error `code` is always preserved verbatim so callers can
/// branch on values such as `REVISION_CONFLICT` rather than parsing prose.
#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct ApiError {
    pub status: u16,
    pub code: String,
    pub message: String,
    pub request_id: Option<String>,
    pub field_errors: BTreeMap<String, Vec<String>>,
    pub retry_after: Option<Duration>,
}

impl ApiError {
    /// Returns true when the error carries the given stable server code.
    pub fn is_code(&self, code: &str) -> bool {
        self.code == code
    }
}

/// The single canonical client error type.
#[derive(Debug, Error)]
pub enum ClientError {
    /// The server returned a structured (or parseable) error envelope.
    #[error(transparent)]
    Api(#[from] ApiError),
    /// A transport-level failure occurred before a usable response arrived.
    #[error("network error: {0}")]
    Network(String),
    /// The server response violated the expected contract.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// A configured host was invalid.
    #[error(transparent)]
    Host(#[from] HostError),
}

impl ClientError {
    /// Extracts the [`ApiError`] if this is an API error.
    pub fn as_api(&self) -> Option<&ApiError> {
        match self {
            ClientError::Api(api) => Some(api),
            _ => None,
        }
    }
}
