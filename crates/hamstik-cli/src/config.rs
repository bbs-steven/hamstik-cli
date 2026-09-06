// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Global configuration file (`config.toml`).
//!
//! Stores non-secret profile metadata only (SPEC §23). Writes are atomic and
//! size-capped; unknown fields are rejected so drift from a future CLI is loud.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::CliError;

/// Current configuration schema version.
pub const CONFIG_VERSION: u32 = 1;

/// Maximum accepted config file size (1 MiB).
pub const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// Non-secret metadata about one authenticated account on one host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub host: String,
    pub user_id: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_organization: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_project: Option<String>,
}

/// On-disk configuration document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_profile: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            active_profile: None,
            profiles: BTreeMap::new(),
        }
    }
}

/// Reads and writes the config file at a fixed location.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads the config, returning an empty document when the file is absent.
    pub fn load(&self) -> Result<ConfigFile, CliError> {
        if !self.path.exists() {
            return Ok(ConfigFile::default());
        }
        let metadata = fs::metadata(self.path())
            .map_err(|err| CliError::config(format!("cannot read config: {err}")))?;
        if metadata.len() > MAX_CONFIG_BYTES {
            return Err(CliError::config(
                "configuration file is too large (exceeds 1 MiB limit)",
            ));
        }
        let contents = fs::read_to_string(self.path())
            .map_err(|err| CliError::config(format!("cannot read config: {err}")))?;
        toml::from_str(&contents)
            .map_err(|err| CliError::config(format!("invalid configuration: {err}")))
    }

    /// Atomically writes the config, creating parent directories as needed.
    pub fn save(&self, config: &ConfigFile) -> Result<(), CliError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                CliError::config(format!("cannot create config directory: {err}"))
            })?;
        }
        let serialized = toml::to_string_pretty(config)
            .map_err(|err| CliError::config(format!("cannot serialize config: {err}")))?;
        let bytes = serialized.as_bytes();
        if bytes.len() as u64 > MAX_CONFIG_BYTES {
            return Err(CliError::config("configuration file exceeds 1 MiB limit"));
        }

        let temp = self.path.with_extension("toml.tmp");
        {
            let mut file = fs::File::create(&temp)
                .map_err(|err| CliError::config(format!("cannot write config: {err}")))?;
            file.write_all(bytes)
                .map_err(|err| CliError::config(format!("cannot write config: {err}")))?;
            file.flush()
                .map_err(|err| CliError::config(format!("cannot write config: {err}")))?;
        }
        fs::rename(&temp, self.path())
            .map_err(|err| CliError::config(format!("cannot finalize config: {err}")))?;
        Ok(())
    }
}

/// Validates a profile name against `^[A-Za-z0-9._-]{1,64}$`.
pub fn validate_profile_name(name: &str) -> Result<(), CliError> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if valid {
        Ok(())
    } else {
        Err(CliError::config(format!(
            "invalid profile name {name:?}: use letters, digits, dot, underscore, or hyphen (1-64 chars)"
        )))
    }
}

/// Replaces characters that are not permitted in a profile name with a hyphen,
/// so auto-generated names (e.g. from hosts that include a port) stay valid.
fn sanitize_profile_segment(segment: &str) -> String {
    segment
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Produces a `<host-without-scheme>-<email-localpart>` profile name, reduced
/// to characters accepted by [`validate_profile_name`] (a host port's colon
/// becomes a hyphen).
#[must_use]
pub fn profile_auto_name(host: &str, email: &str) -> String {
    let authority = host.split_once("://").map(|(_, rest)| rest).unwrap_or(host);
    let authority = authority.trim_matches('/');
    let local = email.split('@').next().unwrap_or(email);
    format!(
        "{}-{}",
        sanitize_profile_segment(authority),
        sanitize_profile_segment(local)
    )
}

/// Returns a name not already used by an unrelated profile.
#[must_use]
pub fn unique_profile_name(config: &ConfigFile, base: &str, user_id: &str, host: &str) -> String {
    let mut candidate = base.to_string();
    let mut index = 2;
    while let Some(existing) = config.profiles.get(&candidate) {
        if existing.user_id == user_id && existing.host == host {
            break;
        }
        candidate = format!("{base}-{index}");
        index += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_empty_config() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("config.toml"));
        let config = store.load().unwrap();
        assert_eq!(config, ConfigFile::default());
    }

    #[test]
    fn roundtrips_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("nested").join("config.toml"));
        let mut config = ConfigFile {
            active_profile: Some("hamstik.com-steven".to_string()),
            ..Default::default()
        };
        config.profiles.insert(
            "hamstik.com-steven".to_string(),
            Profile {
                host: "https://hamstik.com".to_string(),
                user_id: "u1".to_string(),
                email: "steven@example.com".to_string(),
                default_organization: Some("acme".to_string()),
                default_project: None,
            },
        );
        store.save(&config).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn rejects_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "version = 1\nmystery = true\n").unwrap();
        let store = ConfigStore::new(path);
        assert!(store.load().is_err());
    }

    #[test]
    fn auto_name_strips_scheme_and_domain() {
        assert_eq!(
            profile_auto_name("https://hamstik.com", "steven@example.com"),
            "hamstik.com-steven"
        );
    }

    #[test]
    fn auto_name_sanitizes_host_port() {
        let name = profile_auto_name("http://localhost:3000", "test@hamstik.dev");
        assert_eq!(name, "localhost-3000-test");
        validate_profile_name(&name).unwrap();
    }

    #[test]
    fn unique_name_appends_suffix_on_collision() {
        let mut config = ConfigFile::default();
        config.profiles.insert(
            "h-steven".to_string(),
            Profile {
                host: "https://a".to_string(),
                user_id: "other".to_string(),
                email: "steven@a".to_string(),
                default_organization: None,
                default_project: None,
            },
        );
        let name = unique_profile_name(&config, "h-steven", "mine", "https://b");
        assert_eq!(name, "h-steven-2");
    }

    #[test]
    fn unique_name_reuses_same_identity() {
        let mut config = ConfigFile::default();
        config.profiles.insert(
            "h-steven".to_string(),
            Profile {
                host: "https://b".to_string(),
                user_id: "mine".to_string(),
                email: "steven@b".to_string(),
                default_organization: None,
                default_project: None,
            },
        );
        let name = unique_profile_name(&config, "h-steven", "mine", "https://b");
        assert_eq!(name, "h-steven");
    }

    #[test]
    fn validates_profile_names() {
        validate_profile_name("hamstik.com-steven").unwrap();
        assert!(validate_profile_name("bad name").is_err());
        assert!(validate_profile_name("").is_err());
    }
}
