// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Turning raw windows into ordered, named tabs.
//!
//! This is the reconciliation step: window handles change on every launch, so
//! each scan re-derives the account behind every window and re-applies the
//! user's saved order and labels.

use crate::ProcessInspector;
use crate::account::{AccountEntry, AccountId};
use crate::config::Config;
use crate::identity;
use crate::launcher::AccountDirectory;
use crate::window::{GameWindow, WindowId};

/// A Dofus client window presented to the user as a numbered tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    /// 1-based position, what the user types in `focus <slot>`.
    pub slot: u32,
    pub window: WindowId,
    pub pid: Option<u32>,
    /// `None` when identity resolution failed (launcher missing or changed).
    pub account: Option<AccountId>,
    /// Display name, already resolved through the priority chain.
    pub label: String,
    /// Raw window title, kept for diagnostics only.
    pub title: String,
}

/// Application id of our own windows.
///
/// Reverse-DNS on purpose: the XDG desktop portal parses the systemd scope name
/// as `app-<id>-<random>`, so an id containing a hyphen — `dofus-tab` — is
/// ambiguous with the `app-<launcher>-<id>-<random>` form and resolves to an
/// empty app id. Verified against `xdg-desktop-portal-kde`.
pub const OWN_APP_ID: &str = "io.github.gambl3r08.DofusTab";

/// Spellings of our own name that must never be taken for a game client.
///
/// Our id ends in "DofusTab", which contains "dofus", so the overlay would
/// otherwise list itself as a tab.
const OWN_NAMES: [&str; 2] = ["dofustab", "dofus-tab"];

/// Whether a window belongs to a Dofus game client.
///
/// Matches on the application class rather than the title, which is a constant
/// `"Dofus"`. The class is the executable-derived name on every backend:
/// `Dofus.x64` from `WM_CLASS` on X11, `Dofus.exe` from the process image on
/// Windows. Only the part before the first dot is considered, so a reverse-DNS
/// id such as our own cannot match on a later segment.
pub fn looks_like_dofus(window: &GameWindow) -> bool {
    let class = window.app_class.to_ascii_lowercase();

    if OWN_NAMES.iter().any(|own| class.contains(own)) {
        return false;
    }

    let name = class.split('.').next().unwrap_or_default();
    name.starts_with("dofus") && !class.contains("ankama") && !class.contains("zaap")
}

/// Builds the ordered tab list from a window scan.
///
/// Ordering: accounts with a saved position come first in that order, then
/// everything else sorted by window id so repeated scans stay stable.
///
/// Label priority: user override, then launcher nickname, then bare account id,
/// then the window handle.
pub fn build_tabs(
    windows: &[GameWindow],
    inspector: &dyn ProcessInspector,
    directory: &AccountDirectory,
    config: &Config,
) -> Vec<Tab> {
    let mut tabs: Vec<Tab> = windows
        .iter()
        .filter(|window| looks_like_dofus(window))
        .map(|window| {
            let account = resolve_account(window, inspector);
            let label = resolve_label(account, directory, config, window.id);
            Tab {
                // Filled in after sorting.
                slot: 0,
                window: window.id,
                pid: window.pid,
                account,
                label,
                title: window.title.clone(),
            }
        })
        .collect();

    tabs.sort_by_key(|tab| {
        let position = tab
            .account
            .and_then(|id| config.position(id))
            .unwrap_or(usize::MAX);
        (position, tab.window)
    });

    for (index, tab) in tabs.iter_mut().enumerate() {
        tab.slot = index as u32 + 1;
    }

    tabs
}

/// Maps a window to its Ankama account through the owning process command line.
fn resolve_account(window: &GameWindow, inspector: &dyn ProcessInspector) -> Option<AccountId> {
    let pid = window.pid?;
    match inspector.cmdline(pid) {
        Ok(args) => identity::account_id_from_cmdline(&args),
        Err(error) => {
            tracing::debug!(pid, %error, "could not read command line");
            None
        }
    }
}

/// Applies the label priority chain.
fn resolve_label(
    account: Option<AccountId>,
    directory: &AccountDirectory,
    config: &Config,
    window: WindowId,
) -> String {
    let Some(account) = account else {
        return format!("window {window}");
    };
    if let Some(label) = config.label(account) {
        return label.to_owned();
    }
    match directory.get(account) {
        Some(entry) => entry.display_name(),
        None => AccountEntry {
            id: account,
            nickname: None,
            tag: None,
        }
        .display_name(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use std::collections::HashMap;

    /// Returns canned command lines so ordering can be tested without an OS.
    struct FakeInspector(HashMap<u32, Vec<String>>);

    impl FakeInspector {
        fn with_accounts(pairs: &[(u32, u64)]) -> Self {
            let map = pairs
                .iter()
                .map(|(pid, account)| {
                    let args = vec![
                        "./Dofus.x64".to_string(),
                        "-logFile".to_string(),
                        format!("/tmp/zaap/dofus.{account}.log"),
                    ];
                    (*pid, args)
                })
                .collect();
            Self(map)
        }
    }

    impl ProcessInspector for FakeInspector {
        fn cmdline(&self, pid: u32) -> Result<Vec<String>> {
            self.0
                .get(&pid)
                .cloned()
                .ok_or(crate::Error::Unsupported("test process"))
        }
    }

    fn window(id: u64, pid: u32, class: &str) -> GameWindow {
        GameWindow {
            id: WindowId(id),
            pid: Some(pid),
            title: "Dofus".to_string(),
            app_class: class.to_string(),
        }
    }

    fn directory() -> AccountDirectory {
        AccountDirectory::parse(
            r#"{"USER_ACCOUNTS":[
                {"id": 111, "nickname": "alpha", "tag": "1000"},
                {"id": 222, "nickname": "beta",  "tag": "2000"}
            ]}"#,
            None,
        )
        .expect("fixture should parse")
    }

    #[test]
    fn game_windows_match_and_the_launcher_does_not() {
        assert!(looks_like_dofus(&window(1, 1, "Dofus.x64")));
        assert!(looks_like_dofus(&window(1, 1, "dofus.exe")));
        assert!(!looks_like_dofus(&window(1, 1, "Ankama Launcher")));
        assert!(!looks_like_dofus(&window(1, 1, "firefox")));
    }

    #[test]
    fn our_own_overlay_is_never_listed_as_a_tab() {
        assert!(!looks_like_dofus(&window(1, 1, OWN_APP_ID)));
        assert!(!looks_like_dofus(&window(1, 1, "Dofus-tab")));
        assert!(!looks_like_dofus(&window(1, 1, "dofustab")));
        assert!(!looks_like_dofus(&window(1, 1, "dofus-tab-bar.exe")));
    }

    #[test]
    fn the_windows_executable_name_matches_too() {
        assert!(looks_like_dofus(&window(1, 1, "Dofus.exe")));
        assert!(!looks_like_dofus(&window(1, 1, "Ankama Launcher.exe")));
        assert!(!looks_like_dofus(&window(1, 1, "zaap.exe")));
    }

    #[test]
    fn a_later_segment_of_a_reverse_dns_id_never_matches() {
        // Only the part before the first dot counts, so an unrelated vendor id
        // that happens to end in "dofus" is not a game client.
        assert!(!looks_like_dofus(&window(1, 1, "com.example.dofus")));
    }

    #[test]
    fn tabs_are_named_after_the_launcher_nickname() {
        let windows = [window(0x20, 11, "Dofus.x64")];
        let inspector = FakeInspector::with_accounts(&[(11, 111)]);

        let tabs = build_tabs(&windows, &inspector, &directory(), &Config::default());

        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].slot, 1);
        assert_eq!(tabs[0].label, "alpha#1000");
        assert_eq!(tabs[0].account, Some(AccountId(111)));
    }

    #[test]
    fn a_user_label_overrides_the_launcher_nickname() {
        let windows = [window(0x20, 11, "Dofus.x64")];
        let inspector = FakeInspector::with_accounts(&[(11, 111)]);
        let mut config = Config::default();
        config.set_label(AccountId(111), "Main Iop");

        let tabs = build_tabs(&windows, &inspector, &directory(), &config);

        assert_eq!(tabs[0].label, "Main Iop");
    }

    #[test]
    fn the_saved_order_wins_over_the_window_handle_order() {
        // Window 0x10 would sort first by handle, but 222 is ordered first.
        let windows = [window(0x10, 11, "Dofus.x64"), window(0x20, 22, "Dofus.x64")];
        let inspector = FakeInspector::with_accounts(&[(11, 111), (22, 222)]);
        let mut config = Config::default();
        config.set_order([AccountId(222), AccountId(111)]);

        let tabs = build_tabs(&windows, &inspector, &directory(), &config);

        assert_eq!(tabs[0].label, "beta#2000");
        assert_eq!(tabs[0].slot, 1);
        assert_eq!(tabs[1].label, "alpha#1000");
        assert_eq!(tabs[1].slot, 2);
    }

    #[test]
    fn unordered_accounts_are_appended_deterministically_by_handle() {
        let windows = [window(0x30, 33, "Dofus.x64"), window(0x10, 11, "Dofus.x64")];
        let inspector = FakeInspector::with_accounts(&[(11, 111), (33, 333)]);

        let tabs = build_tabs(&windows, &inspector, &directory(), &Config::default());

        assert_eq!(tabs[0].window, WindowId(0x10));
        assert_eq!(tabs[1].window, WindowId(0x30));
    }

    #[test]
    fn an_account_missing_from_the_launcher_falls_back_to_its_id() {
        let windows = [window(0x10, 33, "Dofus.x64")];
        let inspector = FakeInspector::with_accounts(&[(33, 333)]);

        let tabs = build_tabs(&windows, &inspector, &directory(), &Config::default());

        assert_eq!(tabs[0].label, "account 333");
    }

    #[test]
    fn a_window_without_identity_still_gets_a_tab() {
        let windows = [window(0x10, 99, "Dofus.x64")];
        let inspector = FakeInspector::with_accounts(&[]);

        let tabs = build_tabs(&windows, &inspector, &directory(), &Config::default());

        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].account, None);
        assert_eq!(tabs[0].label, "window 0x10");
    }
}
