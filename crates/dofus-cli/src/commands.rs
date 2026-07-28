// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Implementation of the CLI subcommands.

use anyhow::{Context, Result, bail};
use dofus_core::config::{self, Config};
use dofus_core::hotkey;
use dofus_core::instance;
use dofus_core::launcher::{self, AccountDirectory};
use dofus_core::registry::{self, Tab};

/// Scans the session and returns the current tabs plus the loaded preferences.
fn scan() -> Result<(Vec<Tab>, Config, AccountDirectory)> {
    let mut manager =
        dofus_platform::detect_window_manager().context("could not connect to a window manager")?;
    let inspector = dofus_platform::process_inspector();

    let config = Config::load().context("could not read the Dofus-tab config")?;
    // A missing or renamed launcher file must not break the tool: tabs simply
    // fall back to user labels or bare account ids.
    let directory = AccountDirectory::load().unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read the launcher accounts");
        AccountDirectory::empty()
    });

    let windows = manager.list_windows().context("could not list windows")?;
    let tabs = registry::build_tabs(&windows, &inspector, &directory, &config);

    Ok((tabs, config, directory))
}

pub fn list() -> Result<()> {
    let (tabs, _, _) = scan()?;

    if tabs.is_empty() {
        println!("No Dofus windows found. Is the game running?");
        return Ok(());
    }

    for tab in &tabs {
        let account = tab
            .account
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let pid = tab
            .pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "-".to_string());

        println!(
            "{slot:>3}  {label:<28}  account={account:<12} pid={pid:<8} window={window}",
            slot = tab.slot,
            label = tab.label,
            window = tab.window,
        );
    }

    Ok(())
}

pub fn focus(target: &str) -> Result<()> {
    let (tabs, _, _) = scan()?;

    if tabs.is_empty() {
        bail!("no Dofus windows found");
    }

    let tab = resolve_target(&tabs, target)?;

    let mut manager = dofus_platform::detect_window_manager()?;
    manager
        .focus(tab.window)
        .with_context(|| format!("could not focus {}", tab.label))?;

    println!("Focused {} ({})", tab.label, tab.window);
    Ok(())
}

/// Resolves a slot number, or a case-insensitive substring of a label.
fn resolve_target<'a>(tabs: &'a [Tab], target: &str) -> Result<&'a Tab> {
    if let Ok(slot) = target.parse::<u32>() {
        return tabs
            .iter()
            .find(|tab| tab.slot == slot)
            .with_context(|| format!("no tab in slot {slot}; there are {}", tabs.len()));
    }

    let needle = target.to_lowercase();
    let matches: Vec<&Tab> = tabs
        .iter()
        .filter(|tab| tab.label.to_lowercase().contains(&needle))
        .collect();

    match matches.as_slice() {
        [tab] => Ok(tab),
        [] => bail!("no tab matches {target:?}"),
        many => {
            let labels: Vec<&str> = many.iter().map(|tab| tab.label.as_str()).collect();
            bail!("{target:?} is ambiguous, it matches: {}", labels.join(", "))
        }
    }
}

pub fn label(slot: u32, label: &str) -> Result<()> {
    let (tabs, mut config, _) = scan()?;

    let tab = tabs
        .iter()
        .find(|tab| tab.slot == slot)
        .with_context(|| format!("no tab in slot {slot}"))?;

    let account = tab.account.with_context(|| {
        format!(
            "slot {slot} has no resolved account, so a label could not be stored; \
             run `dofus-tab doctor` to see why"
        )
    })?;

    config.set_label(account, label);
    config.save().context("could not save the config")?;

    if label.is_empty() {
        println!("Cleared the label for account {account}");
    } else {
        println!("Account {account} is now labelled {label:?}");
    }

    Ok(())
}

pub fn save_order() -> Result<()> {
    let (tabs, mut config, _) = scan()?;

    let ordered: Vec<_> = tabs.iter().filter_map(|tab| tab.account).collect();
    if ordered.is_empty() {
        bail!("no identified accounts to save");
    }

    let saved = ordered.len();
    config.set_order(ordered);
    config.save().context("could not save the config")?;

    println!("Saved the order of {saved} account(s)");
    Ok(())
}

/// Registers the global hotkeys and focuses tabs until interrupted.
pub fn daemon() -> Result<()> {
    // Taken before anything else: two processes registering the same shortcuts
    // is a silent failure on every platform, so refuse early and say why.
    let _lock =
        instance::acquire(instance::HOTKEY_LOCK).context("could not start the hotkey daemon")?;

    let config = Config::load().context("could not read the Dofus-tab config")?;
    let bindings = config.hotkey_bindings();
    if bindings.is_empty() {
        bail!("no hotkeys configured");
    }

    let mut hotkeys =
        dofus_platform::detect_hotkey_manager().context("could not reach a hotkey provider")?;

    println!("Registering {} shortcuts:", bindings.len());
    for binding in &bindings {
        println!("  {:<12} {}", binding.trigger, binding.description);
    }
    println!("Your desktop may ask you to confirm them the first time.");

    hotkeys
        .bind(&bindings)
        .context("could not register the global shortcuts")?;

    report_inert_shortcuts(hotkeys.as_mut());

    println!("Listening. Press Ctrl+C to stop.");

    // Rescan on every activation rather than caching: a scan costs a few
    // milliseconds and it is always right, even if a client was just launched
    // or closed.
    hotkeys
        .listen(&mut |action| {
            let hotkey::Action::FocusSlot(slot) = action;
            if let Err(error) = focus_slot(slot) {
                eprintln!("Could not focus slot {slot}: {error:#}");
            }
        })
        .context("the hotkey listener stopped")
}

/// Warns about shortcuts the desktop accepted but left without a key.
///
/// This failure mode is silent otherwise: registration succeeds, nothing is
/// logged, and the hotkeys simply never fire. KDE does exactly this when a
/// previous permission prompt was dismissed.
fn report_inert_shortcuts(hotkeys: &mut dyn hotkey::HotkeyManager) {
    let registered = match hotkeys.registered() {
        Ok(registered) => registered,
        Err(error) => {
            tracing::debug!(%error, "could not list the registered shortcuts");
            return;
        }
    };

    let inert: Vec<_> = registered.iter().filter(|s| !s.is_active()).collect();
    if inert.is_empty() {
        return;
    }

    eprintln!(
        "\nWarning: your desktop registered {} of {} shortcuts without a key, so they will \
         not fire.",
        inert.len(),
        registered.len()
    );
    eprintln!("Assign them in your desktop's keyboard shortcut settings, under Dofus-tab.");
}

/// Focuses the tab in `slot`, doing nothing when no client occupies it.
fn focus_slot(slot: u32) -> Result<()> {
    let (tabs, _, _) = scan()?;

    let Some(tab) = tabs.iter().find(|tab| tab.slot == slot) else {
        tracing::debug!(slot, "no window in this slot");
        return Ok(());
    };

    let mut manager = dofus_platform::detect_window_manager()?;
    manager.focus(tab.window)?;

    println!("Focused {} ({})", tab.label, tab.window);
    Ok(())
}

pub fn doctor() -> Result<()> {
    println!("== Dofus-tab doctor ==");

    match dofus_platform::detect_window_manager() {
        Ok(mut manager) => {
            println!("backend           : {}", manager.backend_name());
            match manager.list_windows() {
                Ok(windows) => {
                    let dofus = windows
                        .iter()
                        .filter(|w| registry::looks_like_dofus(w))
                        .count();
                    println!("windows managed   : {}", windows.len());
                    println!("dofus windows     : {dofus}");
                }
                Err(error) => println!("windows managed   : failed ({error})"),
            }
        }
        Err(error) => println!("backend           : unavailable ({error})"),
    }

    match launcher::default_settings_path() {
        Some(path) => {
            let state = if path.exists() { "found" } else { "missing" };
            println!("launcher settings : {state} at {}", path.display());
        }
        None => println!("launcher settings : no config directory for this OS"),
    }

    match AccountDirectory::load() {
        Ok(directory) => println!("launcher accounts : {}", directory.len()),
        Err(error) => println!("launcher accounts : failed ({error})"),
    }

    match config::default_config_path() {
        Some(path) => {
            let state = if path.exists() {
                "found"
            } else {
                "not created yet"
            };
            println!("dofus-tab config  : {state} at {}", path.display());
        }
        None => println!("dofus-tab config  : no config directory for this OS"),
    }

    report_hotkey_state();

    Ok(())
}

/// Reports what the desktop assigned to each shortcut.
///
/// Shortcuts are only visible once `daemon` has registered them at least once,
/// so an empty list here means "not registered yet", not "broken".
fn report_hotkey_state() {
    let mut hotkeys = match dofus_platform::detect_hotkey_manager() {
        Ok(hotkeys) => hotkeys,
        Err(error) => {
            println!("hotkey provider   : unavailable ({error})");
            return;
        }
    };

    println!("hotkey provider   : available");

    match hotkeys.registered() {
        Ok(registered) if registered.is_empty() => {
            println!("hotkeys           : none registered yet, run `dofus-tab daemon`");
        }
        Ok(registered) => {
            for shortcut in &registered {
                let trigger = shortcut.trigger.as_deref().unwrap_or("NOT ASSIGNED");
                println!("  {:<16} {}", shortcut.action.id(), trigger);
            }
            let inert = registered.iter().filter(|s| !s.is_active()).count();
            if inert > 0 {
                println!(
                    "hotkeys           : {inert} of {} have no key; assign them in your \
                     desktop's shortcut settings",
                    registered.len()
                );
            }
        }
        Err(error) => println!("hotkeys           : could not list them ({error})"),
    }
}
