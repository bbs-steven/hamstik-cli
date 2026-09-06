// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Stable process exit codes.
//!
//! These numbers are part of the CLI's public contract (SPEC §52) and must not
//! be casually reassigned. Every error surfaced by the CLI ultimately maps to
//! exactly one of these values.

/// Command completed successfully.
pub const SUCCESS: i32 = 0;
/// General/uncategorized failure.
pub const GENERAL: i32 = 1;
/// Usage error or invalid local command input.
pub const USAGE: i32 = 2;
/// Authentication failure (missing/invalid credential).
pub const AUTHENTICATION: i32 = 3;
/// Authorization or insufficient scope.
pub const AUTHORIZATION: i32 = 4;
/// Resource not found.
pub const NOT_FOUND: i32 = 5;
/// Conflict / concurrency / idempotency conflict.
pub const CONFLICT: i32 = 6;
/// Rate limited.
pub const RATE_LIMITED: i32 = 7;
/// Network / transport failure.
pub const NETWORK: i32 = 8;
/// Server / internal API failure.
pub const SERVER: i32 = 9;
/// Local configuration or credential-store failure.
pub const CONFIGURATION: i32 = 10;
