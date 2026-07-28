// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Reading the account directory published by the Ankama Launcher.
//!
//! The launcher (an Electron app internally named `zaap`) keeps a `Settings`
//! JSON file in the platform config directory:
//!
//! | OS      | Path                                          |
//! |---------|-----------------------------------------------|
//! | Linux   | `~/.config/zaap/Settings`                      |
//! | Windows | `%APPDATA%\zaap\Settings`                      |
//! | macOS   | `~/Library/Application Support/zaap/Settings`  |
//!
//! Its `USER_ACCOUNTS` array maps account ids to nicknames, which is what turns
//! an anonymous `"Dofus"` window into a named tab.
//!
//! # Privacy
//!
//! That file also holds the login e-mail and session material. This module
//! reads **only** `id`, `nickname` and `tag`, and never writes to the file.
//!
//! # Stability
//!
//! The format is undocumented and may change without notice, so every failure
//! here is non-fatal: callers degrade to user-assigned labels instead.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::account::{AccountEntry, AccountId};
use crate::error::{Error, Result};

/// Key of the account array inside the launcher settings file.
const USER_ACCOUNTS_KEY: &str = "USER_ACCOUNTS";

/// Accounts known to the Ankama Launcher, keyed by account id.
#[derive(Clone, Debug, Default)]
pub struct AccountDirectory {
    entries: BTreeMap<AccountId, AccountEntry>,
    source: Option<PathBuf>,
}

impl AccountDirectory {
    /// A directory with no accounts, used when the launcher is not installed.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Loads the directory from the launcher's default location.
    ///
    /// Returns an empty directory when the launcher is not installed rather
    /// than failing: Dofus-tab still works with user-assigned labels.
    pub fn load() -> Result<Self> {
        let Some(path) = default_settings_path() else {
            tracing::debug!("no platform config directory, skipping launcher lookup");
            return Ok(Self::empty());
        };
        if !path.exists() {
            tracing::debug!(path = %path.display(), "launcher settings not found");
            return Ok(Self::empty());
        }
        Self::load_from(&path)
    }

    /// Loads the directory from an explicit path.
    pub fn load_from(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path).map_err(|source| Error::io(path, source))?;
        Self::parse(&raw, Some(path.to_path_buf()))
    }

    /// Parses launcher settings from memory.
    pub fn parse(raw: &str, source: Option<PathBuf>) -> Result<Self> {
        let root: Value = serde_json::from_str(raw)
            .map_err(|e| Error::json(source.clone().unwrap_or_default(), e))?;

        let mut entries = BTreeMap::new();
        if let Some(accounts) = root.get(USER_ACCOUNTS_KEY).and_then(Value::as_array) {
            for account in accounts {
                let Some(entry) = parse_entry(account) else {
                    continue;
                };
                entries.insert(entry.id, entry);
            }
        } else {
            tracing::debug!("launcher settings have no {USER_ACCOUNTS_KEY} array");
        }

        Ok(Self { entries, source })
    }

    pub fn get(&self, id: AccountId) -> Option<&AccountEntry> {
        self.entries.get(&id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// File the directory was loaded from, for `doctor` output.
    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }
}

/// Reads one `USER_ACCOUNTS` element, ignoring every field but the display ones.
fn parse_entry(account: &Value) -> Option<AccountEntry> {
    let id = account.get("id").and_then(flexible_u64).map(AccountId)?;
    Some(AccountEntry {
        id,
        nickname: account.get("nickname").and_then(flexible_string),
        tag: account.get("tag").and_then(flexible_string),
    })
}

/// Reads a number that the launcher may encode as either a JSON number or string.
fn flexible_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

/// Reads a string that the launcher may encode as either a JSON string or number.
fn flexible_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Default location of the Ankama Launcher settings file for this OS.
pub fn default_settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("zaap").join("Settings"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors the real launcher layout, including the nested `gameList` whose
    /// small `id` values must not be mistaken for account ids.
    const SETTINGS: &str = r#"{
        "LANGUAGE": "es",
        "USER_ACCOUNTS": [
            {
                "id": 62833850,
                "login": "someone@example.com",
                "nickname": "firstaccount",
                "tag": "1546",
                "gameList": [{ "id": 1 }, { "id": 18 }]
            },
            {
                "id": "62845763",
                "nickname": "secondaccount",
                "tag": 1535,
                "gameList": [{ "id": 3 }]
            }
        ]
    }"#;

    fn directory() -> AccountDirectory {
        AccountDirectory::parse(SETTINGS, None).expect("settings should parse")
    }

    #[test]
    fn reads_every_account() {
        assert_eq!(directory().len(), 2);
    }

    #[test]
    fn resolves_an_account_to_its_display_name() {
        let dir = directory();
        let entry = dir
            .get(AccountId(62833850))
            .expect("account should be present");
        assert_eq!(entry.display_name(), "firstaccount#1546");
    }

    #[test]
    fn accepts_ids_and_tags_encoded_as_either_json_type() {
        let dir = directory();
        let entry = dir
            .get(AccountId(62845763))
            .expect("account should be present");
        assert_eq!(entry.display_name(), "secondaccount#1535");
    }

    #[test]
    fn does_not_confuse_nested_game_ids_with_account_ids() {
        let dir = directory();
        assert!(dir.get(AccountId(1)).is_none());
        assert!(dir.get(AccountId(18)).is_none());
    }

    #[test]
    fn missing_user_accounts_yields_an_empty_directory() {
        let dir = AccountDirectory::parse(r#"{"LANGUAGE":"es"}"#, None).expect("should parse");
        assert!(dir.is_empty());
    }

    #[test]
    fn malformed_json_is_reported() {
        assert!(AccountDirectory::parse("{ not json", None).is_err());
    }

    #[test]
    fn entries_without_an_id_are_skipped() {
        let dir = AccountDirectory::parse(r#"{"USER_ACCOUNTS":[{"nickname":"x"}]}"#, None)
            .expect("should parse");
        assert!(dir.is_empty());
    }
}
