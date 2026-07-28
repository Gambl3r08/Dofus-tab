//! Persisted user preferences: custom labels and tab order.
//!
//! Everything is keyed by [`AccountId`] because that is the only identifier
//! stable across restarts — window handles and pids are not.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::account::AccountId;
use crate::error::{Error, Result};

/// User preferences, stored as JSON next to the platform config directory.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Overrides for the launcher-provided nickname, keyed by account id.
    pub labels: BTreeMap<u64, String>,
    /// Preferred tab order. Accounts absent from this list are appended.
    pub order: Vec<u64>,
}

impl Config {
    /// Loads the config, returning defaults when it does not exist yet.
    pub fn load() -> Result<Self> {
        let Some(path) = default_config_path() else {
            return Ok(Self::default());
        };
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path).map_err(|source| Error::io(path, source))?;
        serde_json::from_str(&raw).map_err(|source| Error::json(path, source))
    }

    /// Writes the config to the default location, creating parent directories.
    pub fn save(&self) -> Result<()> {
        let Some(path) = default_config_path() else {
            return Err(Error::Unsupported("locating the user config directory"));
        };
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(|source| Error::json(path, source))?;
        std::fs::write(path, raw).map_err(|source| Error::io(path, source))
    }

    /// User-defined label for an account, if any.
    pub fn label(&self, id: AccountId) -> Option<&str> {
        self.labels.get(&id.0).map(String::as_str)
    }

    /// Sets a custom label, or clears it when `label` is empty.
    pub fn set_label(&mut self, id: AccountId, label: &str) {
        if label.is_empty() {
            self.labels.remove(&id.0);
        } else {
            self.labels.insert(id.0, label.to_owned());
        }
    }

    /// Saved position of an account, lowest first.
    pub fn position(&self, id: AccountId) -> Option<usize> {
        self.order.iter().position(|saved| *saved == id.0)
    }

    /// Replaces the saved order.
    pub fn set_order(&mut self, ids: impl IntoIterator<Item = AccountId>) {
        self.order = ids.into_iter().map(|id| id.0).collect();
    }
}

/// Default location of the Dofus-tab config file for this OS.
pub fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("dofus-tab").join("config.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_round_trips() {
        let mut config = Config::default();
        config.set_label(AccountId(1), "Main Iop");
        assert_eq!(config.label(AccountId(1)), Some("Main Iop"));
    }

    #[test]
    fn empty_label_clears_the_override() {
        let mut config = Config::default();
        config.set_label(AccountId(1), "Main Iop");
        config.set_label(AccountId(1), "");
        assert_eq!(config.label(AccountId(1)), None);
    }

    #[test]
    fn position_reflects_the_saved_order() {
        let mut config = Config::default();
        config.set_order([AccountId(30), AccountId(10)]);
        assert_eq!(config.position(AccountId(30)), Some(0));
        assert_eq!(config.position(AccountId(10)), Some(1));
        assert_eq!(config.position(AccountId(99)), None);
    }

    #[test]
    fn serialization_round_trips() {
        let mut config = Config::default();
        config.set_label(AccountId(62833850), "Enutrof");
        config.set_order([AccountId(62833850)]);

        let raw = serde_json::to_string(&config).expect("should serialize");
        let restored: Config = serde_json::from_str(&raw).expect("should deserialize");

        assert_eq!(restored.label(AccountId(62833850)), Some("Enutrof"));
        assert_eq!(restored.position(AccountId(62833850)), Some(0));
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let config: Config = serde_json::from_str("{}").expect("should deserialize");
        assert!(config.labels.is_empty());
        assert!(config.order.is_empty());
    }
}
