// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Validated, normalized Hamstik host URLs.

use std::fmt;

use url::Url;

use crate::error::HostError;

/// The default production Hamstik host.
pub const DEFAULT_HOST: &str = "https://hamstik.com";

/// A validated Hamstik host origin.
///
/// A `Host` always stores a bare origin (`scheme://host[:port]`). The `/api/v1`
/// prefix and resource path segments are appended by the client and are always
/// percent-encoded, so callers can pass arbitrary slugs/keys safely.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Host {
    url: Url,
    base: String,
}

impl Host {
    /// Builds the default production host.
    pub fn default_host() -> Result<Self, HostError> {
        Self::parse(DEFAULT_HOST)
    }

    /// Parses and validates a user-supplied host string.
    ///
    /// A missing scheme defaults to `https`. HTTPS is always required except
    /// for loopback hosts (`localhost`, `127.0.0.1`, any `127/8`, or `::1`)
    /// where plain `http` is permitted for local development.
    pub fn parse(input: &str) -> Result<Self, HostError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(HostError::Empty);
        }

        let candidate = if trimmed.contains("://") {
            trimmed.to_string()
        } else {
            format!("https://{trimmed}")
        };

        let mut url =
            Url::parse(&candidate).map_err(|err| HostError::InvalidUrl(err.to_string()))?;

        match url.scheme() {
            "https" => {}
            "http" => {
                if !is_loopback(&url) {
                    return Err(HostError::InsecureScheme);
                }
            }
            other => return Err(HostError::UnsupportedScheme(other.to_string())),
        }

        if !url.username().is_empty() || url.password().is_some() {
            return Err(HostError::CredentialsForbidden);
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(HostError::QueryFragmentForbidden);
        }

        let path = url.path();
        let path_is_root = path.is_empty() || path == "/";
        if !path_is_root {
            return Err(HostError::PathForbidden);
        }

        // Normalize to a bare origin so trailing slashes compare equal.
        url.set_path("/");
        let base = url.as_str().trim_end_matches('/').to_string();

        Ok(Self { url, base })
    }

    /// The normalized origin string without a trailing slash.
    pub fn as_str(&self) -> &str {
        &self.base
    }

    /// The percent-encoded path for a resource under the `/api/v1` prefix.
    ///
    /// Each supplied segment is percent-encoded individually.
    pub fn resource_url(&self, segments: &[&str]) -> Result<Url, HostError> {
        let mut url = self.url.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| HostError::InvalidUrl("host is not a base URL".to_string()))?;
            path.clear();
            path.push("api");
            path.push("v1");
            for segment in segments {
                path.push(segment);
            }
        }
        Ok(url)
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.base)
    }
}

fn is_loopback(url: &Url) -> bool {
    match url.host_str() {
        Some("localhost") | Some("::1") => true,
        Some(host) => host == "[::1]" || host.starts_with("127."),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_host() {
        let host = Host::default_host().unwrap();
        assert_eq!(host.as_str(), "https://hamstik.com");
    }

    #[test]
    fn trailing_slash_normalizes_equal() {
        let a = Host::parse("https://hamstik.com").unwrap();
        let b = Host::parse("https://hamstik.com/").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.as_str(), b.as_str());
    }

    #[test]
    fn adds_https_when_scheme_missing() {
        let host = Host::parse("hamstik.com").unwrap();
        assert_eq!(host.as_str(), "https://hamstik.com");
    }

    #[test]
    fn preserves_non_default_port() {
        let host = Host::parse("https://hamstik.test:8443").unwrap();
        assert_eq!(host.as_str(), "https://hamstik.test:8443");
    }

    #[test]
    fn rejects_insecure_non_loopback() {
        assert_eq!(
            Host::parse("http://evil.example"),
            Err(HostError::InsecureScheme)
        );
    }

    #[test]
    fn allows_http_loopback_variants() {
        assert!(Host::parse("http://localhost:3000").is_ok());
        assert!(Host::parse("http://127.0.0.1:3000").is_ok());
        assert!(Host::parse("http://[::1]:3000").is_ok());
    }

    #[test]
    fn rejects_credentials_query_fragment_and_path() {
        assert_eq!(
            Host::parse("https://user:pw@hamstik.com"),
            Err(HostError::CredentialsForbidden)
        );
        assert_eq!(
            Host::parse("https://hamstik.com?x=1"),
            Err(HostError::QueryFragmentForbidden)
        );
        assert_eq!(
            Host::parse("https://hamstik.com#frag"),
            Err(HostError::QueryFragmentForbidden)
        );
        assert_eq!(
            Host::parse("https://hamstik.com/api/v1"),
            Err(HostError::PathForbidden)
        );
    }

    #[test]
    fn rejects_unsupported_scheme() {
        assert!(matches!(
            Host::parse("ftp://hamstik.com"),
            Err(HostError::UnsupportedScheme(_))
        ));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(Host::parse("   "), Err(HostError::Empty));
    }

    #[test]
    fn resource_url_appends_prefix_and_encodes_segments() {
        let host = Host::default_host().unwrap();
        let url = host
            .resource_url(&["organizations", "black board", "projects"])
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://hamstik.com/api/v1/organizations/black%20board/projects"
        );
    }

    #[test]
    fn resource_url_has_no_trailing_slash_when_no_segments() {
        let host = Host::default_host().unwrap();
        assert_eq!(
            host.resource_url(&[]).unwrap().as_str(),
            "https://hamstik.com/api/v1"
        );
    }
}
