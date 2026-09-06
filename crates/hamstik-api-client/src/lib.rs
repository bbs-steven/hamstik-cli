// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Typed client foundation for the Hamstik Public API v1.

/// URL path prefix for every Hamstik Public API v1 endpoint.
pub const API_PREFIX: &str = "/api/v1";

#[cfg(test)]
mod tests {
    use super::API_PREFIX;

    #[test]
    fn api_prefix_is_v1() {
        assert_eq!(API_PREFIX, "/api/v1");
    }
}
