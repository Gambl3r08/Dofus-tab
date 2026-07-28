// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Persisted user preferences: custom labels and tab order.
//!
//! Everything is keyed by [`AccountId`] because that is the only identifier
//! stable across restarts — window handles and pids are not.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::account::AccountId;
use crate::error::{Error, Result};
use crate::hotkey;
use crate::layout::{self, Orientation};

/// User preferences, stored as JSON next to the platform config directory.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Overrides for the launcher-provided nickname, keyed by account id.
    pub labels: BTreeMap<u64, String>,
    /// Preferred tab order. Accounts absent from this list are appended.
    pub order: Vec<u64>,
    /// Custom hotkey triggers, keyed by 1-based slot.
    ///
    /// Slots absent from this map use `Alt+<slot>`. Values use the freedesktop
    /// shortcuts syntax, e.g. `CTRL+SHIFT+F2`.
    pub hotkeys: BTreeMap<u32, String>,
    /// How many slots get a hotkey. Defaults to [`hotkey::DEFAULT_MAX_SLOTS`].
    pub max_slots: Option<u32>,
    /// Last position of the overlay bar, in logical screen coordinates.
    ///
    /// `None` lets the window manager place it on first launch.
    pub overlay_position: Option<[f32; 2]>,
    /// Direction the overlay fills before wrapping.
    ///
    /// Defaults to [`Orientation::Vertical`], which keeps the bar narrow.
    pub overlay_orientation: Option<Orientation>,
    /// Tabs shown before the overlay wraps to a new column or row.
    ///
    /// Defaults to [`layout::DEFAULT_WRAP_AT`].
    pub overlay_wrap_at: Option<u32>,
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

    /// Moves `accounts` to the front of the saved order, in the order given.
    ///
    /// Accounts that were remembered but are not in `accounts` — typically ones
    /// that are simply not logged in right now — keep their relative order
    /// after them, so reordering the running clients never loses a preference
    /// for an absent one.
    pub fn reorder(&mut self, accounts: &[AccountId]) {
        let moved: Vec<u64> = accounts.iter().map(|id| id.0).collect();
        let rest = self.order.iter().copied().filter(|id| !moved.contains(id));

        self.order = moved.iter().copied().chain(rest).collect();
    }

    /// Overlay grid settings, defaults merged with the user's overrides.
    pub fn overlay_layout(&self) -> (Orientation, u32) {
        (
            self.overlay_orientation.unwrap_or_default(),
            self.overlay_wrap_at.unwrap_or(layout::DEFAULT_WRAP_AT),
        )
    }

    /// Hotkey bindings to register, defaults merged with the user's overrides.
    pub fn hotkey_bindings(&self) -> Vec<hotkey::HotkeyBinding> {
        hotkey::bindings(
            self.max_slots.unwrap_or(hotkey::DEFAULT_MAX_SLOTS),
            &self.hotkeys,
        )
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
    fn reorder_puts_the_given_accounts_first() {
        let mut config = Config::default();
        config.set_order([AccountId(1), AccountId(2), AccountId(3)]);

        config.reorder(&[AccountId(3), AccountId(1)]);

        assert_eq!(config.position(AccountId(3)), Some(0));
        assert_eq!(config.position(AccountId(1)), Some(1));
        assert_eq!(config.position(AccountId(2)), Some(2));
    }

    #[test]
    fn reorder_keeps_accounts_that_are_not_running() {
        let mut config = Config::default();
        config.set_order([AccountId(10), AccountId(20), AccountId(30)]);

        // Only 30 and 10 are logged in; 20 must survive the reorder.
        config.reorder(&[AccountId(30), AccountId(10)]);

        assert_eq!(config.order, vec![30, 10, 20]);
    }

    #[test]
    fn reorder_accepts_accounts_not_seen_before() {
        let mut config = Config::default();
        config.reorder(&[AccountId(7), AccountId(8)]);
        assert_eq!(config.order, vec![7, 8]);
    }

    #[test]
    fn reorder_never_duplicates_an_account() {
        let mut config = Config::default();
        config.set_order([AccountId(1), AccountId(2)]);

        config.reorder(&[AccountId(2), AccountId(1)]);

        assert_eq!(config.order, vec![2, 1]);
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
        assert!(config.hotkeys.is_empty());
        assert_eq!(config.max_slots, None);
        assert_eq!(config.overlay_orientation, None);
        assert_eq!(config.overlay_wrap_at, None);
    }

    #[test]
    fn the_overlay_defaults_to_a_narrow_grid() {
        assert_eq!(
            Config::default().overlay_layout(),
            (Orientation::Vertical, layout::DEFAULT_WRAP_AT)
        );
    }

    #[test]
    fn the_overlay_layout_can_be_overridden() {
        let config = Config {
            overlay_orientation: Some(Orientation::Horizontal),
            overlay_wrap_at: Some(2),
            ..Config::default()
        };
        assert_eq!(config.overlay_layout(), (Orientation::Horizontal, 2));
    }

    #[test]
    fn a_default_config_binds_alt_plus_digit_for_every_slot() {
        let bindings = Config::default().hotkey_bindings();

        assert_eq!(bindings.len() as u32, hotkey::DEFAULT_MAX_SLOTS);
        assert_eq!(bindings[0].trigger, "ALT+1");
        assert_eq!(bindings[0].action, hotkey::Action::FocusSlot(1));
    }

    #[test]
    fn hotkey_overrides_and_slot_count_are_honoured() {
        let config = Config {
            max_slots: Some(2),
            hotkeys: BTreeMap::from([(2, "CTRL+F2".to_string())]),
            ..Config::default()
        };

        let bindings = config.hotkey_bindings();

        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].trigger, "ALT+1");
        assert_eq!(bindings[1].trigger, "CTRL+F2");
    }
}
