// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Operating-system adapters implementing the `dofus-core` ports.
//!
//! # Backend selection
//!
//! Backends are probed in order of preference and the first one that connects
//! wins. On Linux the X11 backend is tried first because the Dofus 3 client is
//! a Unity build whose SDL layer selects the X11 video driver, so the game runs
//! through XWayland even inside a Wayland session — verified on KDE Plasma 6.
//!
//! Native-Wayland backends (KWin, Hyprland, wlroots) are planned for the day
//! that default changes, and for clients started with `-force-wayland`.

pub mod autostart;
pub mod process;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

use dofus_core::{ProcessInspector, Result, WindowManager};

/// Connects to the best available window backend for the current session.
#[cfg(target_os = "linux")]
pub fn detect_window_manager() -> Result<Box<dyn WindowManager>> {
    match linux::x11::X11Backend::connect() {
        Ok(backend) => {
            tracing::debug!("using the x11 backend");
            Ok(Box::new(backend))
        }
        Err(error) => {
            tracing::debug!(%error, "x11 backend unavailable");
            Err(dofus_core::Error::NoBackend(format!(
                "X11 unavailable ({error}); native Wayland backends are not implemented yet"
            )))
        }
    }
}

/// Connects to the best available window backend for the current session.
#[cfg(target_os = "windows")]
pub fn detect_window_manager() -> Result<Box<dyn WindowManager>> {
    Ok(Box::new(windows::window::WindowsBackend::connect()?))
}

/// Connects to the best available window backend for the current session.
#[cfg(target_os = "macos")]
pub fn detect_window_manager() -> Result<Box<dyn WindowManager>> {
    Ok(Box::new(macos::window::MacBackend::connect()?))
}

/// Connects to the best available window backend for the current session.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn detect_window_manager() -> Result<Box<dyn WindowManager>> {
    Err(dofus_core::Error::Unsupported(
        "window management on this operating system",
    ))
}

/// Connects to the global hotkey provider for the current session.
#[cfg(target_os = "linux")]
pub fn detect_hotkey_manager() -> Result<Box<dyn dofus_core::hotkey::HotkeyManager>> {
    let portal = linux::portal::PortalHotkeys::connect().map_err(|error| {
        dofus_core::Error::NoBackend(format!(
            "the XDG GlobalShortcuts portal is unavailable ({error}); \
             KDE Plasma implements it, GNOME and wlroots may not"
        ))
    })?;
    Ok(Box::new(portal))
}

/// Connects to the global hotkey provider for the current session.
#[cfg(target_os = "windows")]
pub fn detect_hotkey_manager() -> Result<Box<dyn dofus_core::hotkey::HotkeyManager>> {
    Ok(Box::new(windows::hotkeys::WindowsHotkeys::connect()?))
}

/// Connects to the global hotkey provider for the current session.
#[cfg(target_os = "macos")]
pub fn detect_hotkey_manager() -> Result<Box<dyn dofus_core::hotkey::HotkeyManager>> {
    Ok(Box::new(macos::hotkeys::MacHotkeys::connect()?))
}

/// Connects to the global hotkey provider for the current session.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn detect_hotkey_manager() -> Result<Box<dyn dofus_core::hotkey::HotkeyManager>> {
    Err(dofus_core::Error::Unsupported(
        "global hotkeys on this operating system",
    ))
}

/// Returns the process inspector for the current OS.
pub fn process_inspector() -> impl ProcessInspector {
    process::OsProcessInspector
}
