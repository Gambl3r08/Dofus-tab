//! Global hotkeys through `RegisterHotKey`.
//!
//! Win32 needs no permission dance: a hotkey is registered against the calling
//! thread and delivered as `WM_HOTKEY` to that thread's message queue. The
//! catch is that registration and the message loop must happen on the *same*
//! thread, so [`bind`](HotkeyManager::bind) only records what to register and
//! the work happens inside [`listen`](HotkeyManager::listen).
//!
//! Registration can also fail outright when another application already owns
//! the combination, which is reported through
//! [`registered`](HotkeyManager::registered) as a shortcut with no trigger.

use std::collections::BTreeMap;

use dofus_core::hotkey::{
    Action, HotkeyBinding, HotkeyManager, RegisteredShortcut, Trigger, parse_trigger,
};
use dofus_core::{Error, Result};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

/// Global hotkeys backed by the Win32 hotkey API.
#[derive(Default)]
pub struct WindowsHotkeys {
    /// Slot-derived hotkey id mapped to the action it triggers.
    wanted: BTreeMap<i32, (Action, Trigger)>,
    /// Filled in by `listen`, once registration has actually been attempted.
    outcome: BTreeMap<i32, Option<String>>,
}

impl WindowsHotkeys {
    pub fn connect() -> Result<Self> {
        Ok(Self::default())
    }

    /// Registers every wanted hotkey, recording which ones were accepted.
    ///
    /// Must run on the thread that will pump the messages.
    fn register_all(&mut self) {
        for (id, (_, trigger)) in &self.wanted {
            let modifiers = to_modifiers(trigger);
            let Some(key) = to_virtual_key(&trigger.key) else {
                tracing::warn!(key = %trigger.key, "unknown key name, skipping");
                self.outcome.insert(*id, None);
                continue;
            };

            // A null window handle posts WM_HOTKEY to this thread's queue.
            let result = unsafe { RegisterHotKey(None, *id, modifiers | MOD_NOREPEAT, key) };
            match result {
                Ok(()) => {
                    self.outcome.insert(*id, Some(describe(trigger)));
                }
                Err(error) => {
                    tracing::warn!(%error, trigger = %describe(trigger), "hotkey already taken");
                    self.outcome.insert(*id, None);
                }
            }
        }
    }

    fn unregister_all(&self) {
        for id in self.wanted.keys() {
            let _ = unsafe { UnregisterHotKey(None, *id) };
        }
    }
}

impl HotkeyManager for WindowsHotkeys {
    fn bind(&mut self, bindings: &[HotkeyBinding]) -> Result<()> {
        self.wanted.clear();
        self.outcome.clear();

        for binding in bindings {
            let Action::FocusSlot(slot) = binding.action;
            let Some(trigger) = parse_trigger(&binding.trigger) else {
                tracing::warn!(trigger = %binding.trigger, "could not parse the trigger");
                continue;
            };
            self.wanted.insert(slot as i32, (binding.action, trigger));
        }

        Ok(())
    }

    fn registered(&mut self) -> Result<Vec<RegisteredShortcut>> {
        Ok(self
            .wanted
            .iter()
            .map(|(id, (action, trigger))| RegisteredShortcut {
                action: *action,
                // Before `listen` runs nothing is registered yet, so report the
                // intended trigger rather than claiming it is inert.
                trigger: match self.outcome.get(id) {
                    Some(outcome) => outcome.clone(),
                    None => Some(describe(trigger)),
                },
            })
            .collect())
    }

    fn listen(&mut self, on_activated: &mut dyn FnMut(Action)) -> Result<()> {
        self.register_all();

        if self.outcome.values().all(Option::is_none) {
            self.unregister_all();
            return Err(Error::Backend(
                "no hotkey could be registered; another application already owns them".to_string(),
            ));
        }

        let mut message = MSG::default();
        // A null window handle pumps thread messages, which is where WM_HOTKEY
        // lands. GetMessageW returns 0 on WM_QUIT and -1 on error.
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
            if message.message != WM_HOTKEY {
                continue;
            }
            let id = message.wParam.0 as i32;
            match self.wanted.get(&id) {
                Some((action, _)) => on_activated(*action),
                None => tracing::debug!(id, "ignoring an unknown hotkey"),
            }
        }

        self.unregister_all();
        Ok(())
    }
}

impl Drop for WindowsHotkeys {
    fn drop(&mut self) {
        self.unregister_all();
    }
}

/// Maps parsed modifiers onto the Win32 flags.
fn to_modifiers(trigger: &Trigger) -> HOT_KEY_MODIFIERS {
    let mut modifiers = HOT_KEY_MODIFIERS(0);
    if trigger.ctrl {
        modifiers |= MOD_CONTROL;
    }
    if trigger.alt {
        modifiers |= MOD_ALT;
    }
    if trigger.shift {
        modifiers |= MOD_SHIFT;
    }
    if trigger.logo {
        modifiers |= MOD_WIN;
    }
    modifiers
}

/// Maps a key name onto a Win32 virtual key code.
///
/// Covers digits, letters and the function keys, which is everything the
/// default and realistic custom triggers use.
fn to_virtual_key(key: &str) -> Option<u32> {
    let key = key.to_ascii_uppercase();
    let mut chars = key.chars();

    match (chars.next(), chars.next()) {
        // VK codes for '0'-'9' and 'A'-'Z' are their ASCII values.
        (Some(c @ '0'..='9'), None) | (Some(c @ 'A'..='Z'), None) => Some(c as u32),
        (Some('F'), Some(_)) => {
            let number: u32 = key[1..].parse().ok()?;
            // VK_F1 is 0x70 and the function keys run consecutively to VK_F24.
            (1..=24).contains(&number).then(|| 0x70 + number - 1)
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
        parts.push("Alt");
    }
    if trigger.shift {
        parts.push("Shift");
    }
    if trigger.logo {
        parts.push("Win");
    }
    parts.push(&trigger.key);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_and_letters_map_to_their_ascii_codes() {
        assert_eq!(to_virtual_key("1"), Some(0x31));
        assert_eq!(to_virtual_key("9"), Some(0x39));
        assert_eq!(to_virtual_key("a"), Some(0x41));
        assert_eq!(to_virtual_key("Z"), Some(0x5A));
    }

    #[test]
    fn function_keys_map_to_their_vk_range() {
        assert_eq!(to_virtual_key("F1"), Some(0x70));
        assert_eq!(to_virtual_key("F12"), Some(0x7B));
        assert_eq!(to_virtual_key("F24"), Some(0x87));
    }

    #[test]
    fn unsupported_key_names_are_rejected() {
        assert_eq!(to_virtual_key("F0"), None);
        assert_eq!(to_virtual_key("F25"), None);
        assert_eq!(to_virtual_key("Space"), None);
        assert_eq!(to_virtual_key(""), None);
    }

    #[test]
    fn every_default_trigger_maps_to_a_virtual_key() {
        for binding in dofus_core::hotkey::bindings(
            dofus_core::hotkey::DEFAULT_MAX_SLOTS,
            &std::collections::BTreeMap::new(),
        ) {
            let trigger = parse_trigger(&binding.trigger).expect("should parse");
            assert!(
                to_virtual_key(&trigger.key).is_some(),
                "{} should map",
                binding.trigger
            );
            assert_eq!(to_modifiers(&trigger), MOD_ALT);
        }
    }

    #[test]
    fn descriptions_are_human_readable() {
        let trigger = parse_trigger("CTRL+SHIFT+F2").expect("should parse");
        assert_eq!(describe(&trigger), "Ctrl+Shift+F2");
    }
}
