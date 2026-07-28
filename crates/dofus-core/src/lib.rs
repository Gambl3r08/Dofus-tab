// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Platform-agnostic core of Dofus-tab.
//!
//! This crate owns the domain model and the *ports* (traits) that platform
//! backends implement. It deliberately contains no OS-specific code so the
//! whole account/ordering logic stays unit-testable without a display server.
//!
//! The two ports are [`WindowManager`] (enumerate and focus windows) and
//! [`ProcessInspector`] (read a process command line), both implemented by the
//! `dofus-platform` crate.

pub mod account;
pub mod config;
pub mod error;
pub mod hotkey;
pub mod identity;
pub mod launcher;
pub mod layout;
pub mod registry;
pub mod window;

pub use error::{Error, Result};
pub use window::{GameWindow, WindowId};

/// Enumerates and focuses the windows managed by the host window manager.
///
/// Implementations are expected to be cheap to call repeatedly: the CLI calls
/// [`WindowManager::list_windows`] on every command and a future GUI will poll
/// it (or refresh it from window events) continuously.
pub trait WindowManager {
    /// Short identifier of the active backend, e.g. `"x11"`. Shown by `doctor`.
    fn backend_name(&self) -> &'static str;

    /// Returns every top-level window currently managed by the compositor.
    ///
    /// Filtering down to Dofus clients is the caller's job, see
    /// [`registry::looks_like_dofus`].
    fn list_windows(&mut self) -> Result<Vec<GameWindow>>;

    /// Gives focus to `id` and raises it above other windows.
    fn focus(&mut self, id: WindowId) -> Result<()>;

    /// The window that currently holds focus, when the backend can tell.
    ///
    /// Used to highlight the active tab; a backend that cannot answer returns
    /// `Ok(None)` rather than failing, since this is cosmetic.
    fn active_window(&mut self) -> Result<Option<WindowId>>;

    /// Turns `id` into an overlay: kept above other windows and hidden from the
    /// task switcher.
    ///
    /// Called by the bar on its own window, because GUI toolkits do not
    /// reliably apply these hints on every window manager.
    fn make_overlay(&mut self, id: WindowId) -> Result<()>;
}

/// Reads process metadata, used to map a window back to an Ankama account.
pub trait ProcessInspector {
    /// Full argument vector of `pid`, including `argv[0]`.
    fn cmdline(&self, pid: u32) -> Result<Vec<String>>;
}
