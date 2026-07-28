// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Global hotkeys on macOS.
//!
//! Backed by `global-hotkey`, which wraps Carbon's `RegisterEventHotKey`. That
//! API needs no permission from the user — unlike a `CGEventTap`, which
//! requires Accessibility access granted by hand.
//!
//! # Main thread only
//!
//! Carbon delivers hotkeys through the application's event target, and only a
//! `CFRunLoop` on the **main thread** pumps it. This backend therefore reports
//! [`requires_main_thread`](HotkeyManager::requires_main_thread), and must be
//! created and polled from a thread that runs such a loop.
//!
//! The overlay bar qualifies: winit runs the loop on the main thread and the
//! bar polls from its frame callback. `dofus-tab daemon` does not, which is why
//! it refuses to pretend on this platform rather than waiting for keypresses
//! that can never arrive.

use std::collections::HashMap;

use dofus_core::hotkey::{
    Action, HotkeyBinding, HotkeyManager, RegisteredShortcut, Trigger, parse_trigger,
};
use dofus_core::{Error, Result};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

/// Global hotkeys backed by Carbon, via `global-hotkey`.
pub struct MacHotkeys {
    manager: GlobalHotKeyManager,
    /// Maps the crate's generated hotkey id to the action it triggers.
    actions: HashMap<u32, Action>,
    /// What each binding ended up with, for `registered`.
    outcome: Vec<RegisteredShortcut>,
}

impl MacHotkeys {
    /// Creates the manager. Must be called from the main thread.
    pub fn connect() -> Result<Self> {
        let manager = GlobalHotKeyManager::new().map_err(|error| {
            Error::Backend(format!("could not create the hotkey manager: {error}"))
        })?;

        Ok(Self {
            manager,
            actions: HashMap::new(),
            outcome: Vec::new(),
        })
    }
}

impl HotkeyManager for MacHotkeys {
    fn bind(&mut self, bindings: &[HotkeyBinding]) -> Result<()> {
        self.actions.clear();
        self.outcome.clear();

        for binding in bindings {
            let Some(trigger) = parse_trigger(&binding.trigger) else {
                tracing::warn!(trigger = %binding.trigger, "could not parse the trigger");
                self.outcome.push(RegisteredShortcut {
                    action: binding.action,
                    trigger: None,
                });
                continue;
            };

            let Some(code) = to_code(&trigger.key) else {
                tracing::warn!(key = %trigger.key, "unknown key name, skipping");
                self.outcome.push(RegisteredShortcut {
                    action: binding.action,
                    trigger: None,
                });
                continue;
            };

            let hotkey = HotKey::new(Some(to_modifiers(&trigger)), code);
            match self.manager.register(hotkey) {
                Ok(()) => {
                    self.actions.insert(hotkey.id, binding.action);
                    self.outcome.push(RegisteredShortcut {
                        action: binding.action,
                        trigger: Some(describe(&trigger)),
                    });
                }
                Err(error) => {
                    tracing::warn!(%error, trigger = %binding.trigger, "hotkey already taken");
                    self.outcome.push(RegisteredShortcut {
                        action: binding.action,
                        trigger: None,
                    });
                }
            }
        }

        if self.actions.is_empty() {
            return Err(Error::Backend(
                "no hotkey could be registered; another application already owns them".to_string(),
            ));
        }

        Ok(())
    }

    fn registered(&mut self) -> Result<Vec<RegisteredShortcut>> {
        Ok(self.outcome.clone())
    }

    fn poll(&mut self) -> Vec<Action> {
        GlobalHotKeyEvent::receiver()
            .try_iter()
            // Both press and release arrive; acting on both would switch twice.
            .filter(|event| event.state == HotKeyState::Pressed)
            .filter_map(|event| self.actions.get(&event.id).copied())
            .collect()
    }

    fn requires_main_thread(&self) -> bool {
        true
    }
}

/// Maps parsed modifiers onto the crate's flags.
fn to_modifiers(trigger: &Trigger) -> Modifiers {
    let mut modifiers = Modifiers::empty();
    if trigger.ctrl {
        modifiers |= Modifiers::CONTROL;
    }
    if trigger.alt {
        modifiers |= Modifiers::ALT;
    }
    if trigger.shift {
        modifiers |= Modifiers::SHIFT;
    }
    if trigger.logo {
        modifiers |= Modifiers::META;
    }
    modifiers
}

/// Maps a key name onto a physical key code.
///
/// Covers digits, letters and the function keys, which is everything the
/// default and realistic custom triggers use. The codes are positional, so
/// `Digit1` is the `1` of the number row whatever the keyboard layout says.
fn to_code(key: &str) -> Option<Code> {
    let key = key.to_ascii_uppercase();
    let mut chars = key.chars();

    match (chars.next(), chars.next()) {
        (Some(digit @ '0'..='9'), None) => Some(match digit {
            '0' => Code::Digit0,
            '1' => Code::Digit1,
            '2' => Code::Digit2,
            '3' => Code::Digit3,
            '4' => Code::Digit4,
            '5' => Code::Digit5,
            '6' => Code::Digit6,
            '7' => Code::Digit7,
            '8' => Code::Digit8,
            _ => Code::Digit9,
        }),
        (Some('A'..='Z'), None) => format!("Key{key}").parse().ok(),
        (Some('F'), Some(_)) => {
            let number: u32 = key[1..].parse().ok()?;
            (1..=24).contains(&number).then(|| key.parse().ok())?
        }
        _ => None,
    }
}

/// Renders a trigger back for display, e.g. `Ctrl+Alt+F2`.
fn describe(trigger: &Trigger) -> String {
    let mut parts = Vec::new();
    if trigger.ctrl {
        parts.push("Ctrl");
    }
    if trigger.alt {
        parts.push("Option");
    }
    if trigger.shift {
        parts.push("Shift");
    }
    if trigger.logo {
        parts.push("Command");
    }
    parts.push(&trigger.key);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_map_to_positional_codes() {
        assert_eq!(to_code("1"), Some(Code::Digit1));
        assert_eq!(to_code("9"), Some(Code::Digit9));
        assert_eq!(to_code("0"), Some(Code::Digit0));
    }

    #[test]
    fn letters_and_function_keys_map() {
        assert_eq!(to_code("a"), Some(Code::KeyA));
        assert_eq!(to_code("Z"), Some(Code::KeyZ));
        assert_eq!(to_code("F1"), Some(Code::F1));
        assert_eq!(to_code("F12"), Some(Code::F12));
    }

    #[test]
    fn unsupported_key_names_are_rejected() {
        assert_eq!(to_code("F0"), None);
        assert_eq!(to_code("F25"), None);
        assert_eq!(to_code("Space"), None);
        assert_eq!(to_code(""), None);
    }

    #[test]
    fn every_default_trigger_maps_to_a_code() {
        for binding in dofus_core::hotkey::bindings(
            dofus_core::hotkey::DEFAULT_MAX_SLOTS,
            &std::collections::BTreeMap::new(),
        ) {
            let trigger = parse_trigger(&binding.trigger).expect("should parse");
            assert!(
                to_code(&trigger.key).is_some(),
                "{} should map",
                binding.trigger
            );
            assert_eq!(to_modifiers(&trigger), Modifiers::ALT);
        }
    }

    #[test]
    fn descriptions_use_the_mac_modifier_names() {
        let trigger = parse_trigger("ALT+1").expect("should parse");
        assert_eq!(describe(&trigger), "Option+1");
    }
}
