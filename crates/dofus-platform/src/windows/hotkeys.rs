// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Global hotkeys through `RegisterHotKey`.
//!
//! Win32 needs no permission dance: a hotkey is registered against the calling
//! thread and delivered as `WM_HOTKEY` to that thread's message queue. The
//! catch is that registration and the message loop must happen on the *same*
//! thread, so [`bind`](HotkeyManager::bind) starts a thread that does both and
//! reports back through channels.
//!
//! Registration can fail outright when another application already owns a
//! combination, which is reported through
//! [`registered`](HotkeyManager::registered) as a shortcut with no trigger.

use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dofus_core::hotkey::{
    Action, HotkeyBinding, HotkeyManager, RegisteredShortcut, Trigger, parse_trigger,
};
use dofus_core::{Error, Result};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

/// How long `bind` waits for the worker to report what it managed to register.
const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(2);

/// Global hotkeys backed by the Win32 hotkey API.
#[derive(Default)]
pub struct WindowsHotkeys {
    /// Fed by the worker thread once `bind` has run.
    activations: Option<Receiver<Action>>,
    /// Filled in by the worker as it registers each shortcut.
    registered: Arc<Mutex<Vec<RegisteredShortcut>>>,
}

impl WindowsHotkeys {
    pub fn connect() -> Result<Self> {
        Ok(Self::default())
    }
}

impl HotkeyManager for WindowsHotkeys {
    fn bind(&mut self, bindings: &[HotkeyBinding]) -> Result<()> {
        let mut wanted: BTreeMap<i32, (Action, Trigger)> = BTreeMap::new();
        for binding in bindings {
            let Action::FocusSlot(slot) = binding.action;
            let Some(trigger) = parse_trigger(&binding.trigger) else {
                tracing::warn!(trigger = %binding.trigger, "could not parse the trigger");
                continue;
            };
            wanted.insert(slot as i32, (binding.action, trigger));
        }

        let (activations, activation_rx) = mpsc::channel();
        let (ready, ready_rx) = mpsc::channel();
        let registered = Arc::clone(&self.registered);

        std::thread::Builder::new()
            .name("dofus-tab-hotkeys".to_string())
            .spawn(move || run(wanted, registered, activations, ready))
            .map_err(|source| Error::Backend(format!("could not start the listener: {source}")))?;

        // Wait for the worker to finish registering so `registered` reports the
        // truth rather than an empty list on the first call.
        ready_rx
            .recv_timeout(REGISTRATION_TIMEOUT)
            .map_err(|_| Error::Backend("the hotkey thread did not report back".to_string()))?;

        self.activations = Some(activation_rx);
        Ok(())
    }

    fn registered(&mut self) -> Result<Vec<RegisteredShortcut>> {
        Ok(self
            .registered
            .lock()
            .expect("the registration mutex was poisoned")
            .clone())
    }

    fn poll(&mut self) -> Vec<Action> {
        match &self.activations {
            Some(activations) => activations.try_iter().collect(),
            None => Vec::new(),
        }
    }
}

/// Registers the hotkeys and pumps messages until the receiver goes away.
fn run(
    wanted: BTreeMap<i32, (Action, Trigger)>,
    registered: Arc<Mutex<Vec<RegisteredShortcut>>>,
    activations: mpsc::Sender<Action>,
    ready: mpsc::Sender<()>,
) {
    let mut outcome = Vec::new();

    for (id, (action, trigger)) in &wanted {
        let modifiers = to_modifiers(trigger);
        let Some(key) = to_virtual_key(&trigger.key) else {
            tracing::warn!(key = %trigger.key, "unknown key name, skipping");
            outcome.push(RegisteredShortcut {
                action: *action,
                trigger: None,
            });
            continue;
        };

        // A null window handle posts WM_HOTKEY to this thread's queue.
        let trigger = match unsafe { RegisterHotKey(None, *id, modifiers | MOD_NOREPEAT, key) } {
            Ok(()) => Some(describe(trigger)),
            Err(error) => {
                tracing::warn!(%error, trigger = %describe(trigger), "hotkey already taken");
                None
            }
        };
        outcome.push(RegisteredShortcut {
            action: *action,
            trigger,
        });
    }

    *registered
        .lock()
        .expect("the registration mutex was poisoned") = outcome;
    let _ = ready.send(());

    let mut message = MSG::default();
    // A null window handle pumps thread messages, which is where WM_HOTKEY
    // lands. GetMessageW returns 0 on WM_QUIT and -1 on error.
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
        if message.message != WM_HOTKEY {
            continue;
        }
        let id = message.wParam.0 as i32;
        match wanted.get(&id) {
            // A send failure means nobody is polling any more.
            Some((action, _)) if activations.send(*action).is_err() => break,
            Some(_) => {}
            None => tracing::debug!(id, "ignoring an unknown hotkey"),
        }
    }

    for id in wanted.keys() {
        let _ = unsafe { UnregisterHotKey(None, *id) };
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
