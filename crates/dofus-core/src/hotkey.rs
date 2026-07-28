//! Global hotkey domain model.
//!
//! Hotkeys are bound to *slots*, not to accounts: `Alt+1` always focuses
//! whatever sits in the first tab. That keeps muscle memory stable when an
//! account is not logged in, and it is also what the portal requires — bindings
//! are registered once, up front, and cannot follow a changing window count.
//!
//! Because of that, a fixed pool of [`DEFAULT_MAX_SLOTS`] shortcuts is
//! registered regardless of how many clients are running. Slots with no window
//! behind them simply do nothing when pressed.

use std::collections::BTreeMap;
use std::fmt;

/// Number of slots bound by default, matching the usual multi-account party size.
pub const DEFAULT_MAX_SLOTS: u32 = 8;

/// Highest slot that gets an automatic `Alt+<digit>` trigger.
const HIGHEST_DIGIT_SLOT: u32 = 9;

/// Prefix of the shortcut ids exchanged with the desktop portal.
const FOCUS_SLOT_PREFIX: &str = "focus-slot-";

/// Something the user can trigger with a global hotkey.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Action {
    /// Focus the tab sitting in this 1-based slot.
    FocusSlot(u32),
}

impl Action {
    /// Stable identifier used as the portal shortcut id.
    ///
    /// The portal persists bindings under this id, so it must not change
    /// between releases or users would lose their customised triggers.
    pub fn id(&self) -> String {
        match self {
            Self::FocusSlot(slot) => format!("{FOCUS_SLOT_PREFIX}{slot}"),
        }
    }

    /// Parses back an id received from the portal.
    pub fn from_id(id: &str) -> Option<Self> {
        let slot = id.strip_prefix(FOCUS_SLOT_PREFIX)?.parse().ok()?;
        Some(Self::FocusSlot(slot))
    }

    /// Human readable text shown in the desktop's shortcut settings.
    pub fn description(&self) -> String {
        match self {
            Self::FocusSlot(slot) => format!("Focus Dofus tab {slot}"),
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id())
    }
}

/// An action together with the key combination that triggers it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HotkeyBinding {
    pub action: Action,
    /// Trigger in the freedesktop shortcuts syntax, e.g. `ALT+1`.
    ///
    /// This is a *preferred* trigger: the desktop is free to reassign it if it
    /// clashes with an existing shortcut, and the user can change it later in
    /// their system settings.
    pub trigger: String,
    pub description: String,
}

/// Default trigger for a slot, `Alt+<digit>`.
///
/// Returns `None` past slot 9, where no single digit is left; those slots need
/// an explicit user override.
pub fn default_trigger(slot: u32) -> Option<String> {
    (1..=HIGHEST_DIGIT_SLOT)
        .contains(&slot)
        .then(|| format!("ALT+{slot}"))
}

/// Builds the binding set to register with the desktop.
///
/// `overrides` maps a slot to a user-chosen trigger and wins over the default.
/// Slots with neither a default nor an override are skipped.
pub fn bindings(max_slots: u32, overrides: &BTreeMap<u32, String>) -> Vec<HotkeyBinding> {
    (1..=max_slots)
        .filter_map(|slot| {
            let trigger = overrides
                .get(&slot)
                .cloned()
                .or_else(|| default_trigger(slot))?;
            let action = Action::FocusSlot(slot);
            Some(HotkeyBinding {
                description: action.description(),
                action,
                trigger,
            })
        })
        .collect()
}

/// A trigger broken down into modifiers and a key.
///
/// The freedesktop syntax used in [`HotkeyBinding::trigger`] is what the XDG
/// portal wants, but backends that register hotkeys themselves — Win32
/// `RegisterHotKey`, macOS `RegisterEventHotKey` — need the parts separately.
/// Parsing lives here so it is tested once rather than per platform.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Trigger {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The Windows / Command / Super key.
    pub logo: bool,
    /// Key name as written, e.g. `1` or `F2`. Never a modifier.
    pub key: String,
}

/// Parses a trigger such as `CTRL+SHIFT+F2`.
///
/// Modifier names are matched case-insensitively and accept the common
/// spellings. Returns `None` when there is no key, more than one key, or an
/// empty segment.
pub fn parse_trigger(trigger: &str) -> Option<Trigger> {
    let mut parsed = Trigger::default();
    let mut key = None;

    for segment in trigger.split('+') {
        let segment = segment.trim();
        if segment.is_empty() {
            return None;
        }

        match segment.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => parsed.ctrl = true,
            "ALT" | "OPTION" => parsed.alt = true,
            "SHIFT" => parsed.shift = true,
            "LOGO" | "SUPER" | "META" | "WIN" | "CMD" | "COMMAND" => parsed.logo = true,
            // Anything else is the key, and there may only be one.
            _ if key.is_some() => return None,
            _ => key = Some(segment.to_owned()),
        }
    }

    parsed.key = key?;
    Some(parsed)
}

/// What the desktop actually assigned to one of our shortcuts.
///
/// A registered shortcut is not necessarily a working one: a desktop may accept
/// the registration and still leave the key combination unassigned, in which
/// case pressing it does nothing and no error is ever reported. Surfacing that
/// state is the whole point of this type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredShortcut {
    pub action: Action,
    /// Human readable trigger reported by the desktop, e.g. `Alt+1`.
    ///
    /// `None` means the desktop registered the action without a key, so the
    /// hotkey is inert until the user assigns one in their system settings.
    pub trigger: Option<String>,
}

impl RegisteredShortcut {
    /// Whether this shortcut will actually fire.
    pub fn is_active(&self) -> bool {
        self.trigger.is_some()
    }
}

/// Registers global hotkeys and delivers their activations.
pub trait HotkeyManager {
    /// Registers `bindings` with the desktop.
    ///
    /// May prompt the user for confirmation, so it can block for a while.
    fn bind(&mut self, bindings: &[HotkeyBinding]) -> crate::Result<()>;

    /// Asks the desktop what it actually assigned to our shortcuts.
    fn registered(&mut self) -> crate::Result<Vec<RegisteredShortcut>>;

    /// Blocks forever, invoking `on_activated` every time a hotkey fires.
    fn listen(&mut self, on_activated: &mut dyn FnMut(Action)) -> crate::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_ids_round_trip() {
        let action = Action::FocusSlot(3);
        assert_eq!(action.id(), "focus-slot-3");
        assert_eq!(Action::from_id("focus-slot-3"), Some(action));
    }

    #[test]
    fn unknown_ids_are_rejected() {
        assert_eq!(Action::from_id("quit"), None);
        assert_eq!(Action::from_id("focus-slot-x"), None);
        assert_eq!(Action::from_id(""), None);
    }

    #[test]
    fn defaults_are_alt_plus_the_slot_digit() {
        assert_eq!(default_trigger(1).as_deref(), Some("ALT+1"));
        assert_eq!(default_trigger(9).as_deref(), Some("ALT+9"));
    }

    #[test]
    fn slots_past_nine_have_no_default_trigger() {
        assert_eq!(default_trigger(10), None);
        assert_eq!(default_trigger(0), None);
    }

    #[test]
    fn the_default_set_covers_every_slot() {
        let bound = bindings(DEFAULT_MAX_SLOTS, &BTreeMap::new());

        assert_eq!(bound.len() as u32, DEFAULT_MAX_SLOTS);
        assert_eq!(bound[0].action, Action::FocusSlot(1));
        assert_eq!(bound[0].trigger, "ALT+1");
        assert_eq!(bound[0].description, "Focus Dofus tab 1");
    }

    #[test]
    fn an_override_replaces_the_default_trigger() {
        let overrides = BTreeMap::from([(2, "CTRL+F2".to_string())]);
        let bound = bindings(3, &overrides);

        assert_eq!(bound[0].trigger, "ALT+1");
        assert_eq!(bound[1].trigger, "CTRL+F2");
        assert_eq!(bound[2].trigger, "ALT+3");
    }

    #[test]
    fn every_default_trigger_parses_back() {
        for binding in bindings(DEFAULT_MAX_SLOTS, &BTreeMap::new()) {
            let parsed = parse_trigger(&binding.trigger)
                .unwrap_or_else(|| panic!("{} should parse", binding.trigger));
            let Action::FocusSlot(slot) = binding.action;

            assert!(parsed.alt, "{} should use Alt", binding.trigger);
            assert!(!parsed.ctrl && !parsed.shift && !parsed.logo);
            assert_eq!(parsed.key, slot.to_string());
        }
    }

    #[test]
    fn modifiers_are_case_insensitive_and_accept_aliases() {
        let parsed = parse_trigger("control+Shift+SUPER+f2").expect("should parse");
        assert!(parsed.ctrl && parsed.shift && parsed.logo);
        assert!(!parsed.alt);
        assert_eq!(parsed.key, "f2");
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        assert_eq!(
            parse_trigger(" CTRL + F1 ").expect("should parse").key,
            "F1"
        );
    }

    #[test]
    fn a_bare_key_needs_no_modifier() {
        let parsed = parse_trigger("F12").expect("should parse");
        assert_eq!(parsed.key, "F12");
        assert_eq!(
            parsed,
            Trigger {
                key: "F12".into(),
                ..Trigger::default()
            }
        );
    }

    #[test]
    fn triggers_without_a_key_are_rejected() {
        assert_eq!(parse_trigger("CTRL+ALT"), None);
        assert_eq!(parse_trigger("SHIFT"), None);
    }

    #[test]
    fn triggers_with_two_keys_are_rejected() {
        assert_eq!(parse_trigger("CTRL+A+B"), None);
    }

    #[test]
    fn malformed_triggers_are_rejected() {
        assert_eq!(parse_trigger(""), None);
        assert_eq!(parse_trigger("CTRL+"), None);
        assert_eq!(parse_trigger("+A"), None);
    }

    #[test]
    fn an_override_can_reach_a_slot_with_no_default() {
        let overrides = BTreeMap::from([(12, "CTRL+F12".to_string())]);
        let bound = bindings(12, &overrides);

        assert_eq!(bound.len(), 10, "slots 1-9 plus the overridden 12");
        assert_eq!(bound.last().unwrap().action, Action::FocusSlot(12));
    }
}
