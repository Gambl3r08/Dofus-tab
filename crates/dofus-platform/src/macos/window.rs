// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! AppKit window backend.
//!
//! # Applications, not windows
//!
//! This backend enumerates *running applications* rather than windows, and uses
//! the process id as the [`WindowId`]. That is not a shortcut taken for
//! convenience — it is what the platform actually rewards:
//!
//! - Every Dofus client is its own process, so one application is one account.
//!   Focusing the application is exactly what the user wants.
//! - `NSRunningApplication::activate` needs no special permission, whereas
//!   per-window control means the Accessibility API, which the user has to
//!   grant by hand in System Settings and which silently returns nothing until
//!   they do.
//! - Reading window titles through `CGWindowListCopyWindowInfo` needs the
//!   Screen Recording permission on modern macOS, and we do not need titles
//!   anyway: accounts are identified from the command line, not the title.
//!
//! The limitation is that a client showing several windows would collapse into
//! one tab. Dofus shows one main window per client, so this holds in practice.

use dofus_core::{Error, GameWindow, Result, WindowId, WindowManager};
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

/// Window manager backend for macOS.
pub struct MacBackend;

impl MacBackend {
    /// Nothing to connect to; AppKit is always available in a GUI session.
    pub fn connect() -> Result<Self> {
        Ok(Self)
    }
}

/// Describes a running application the way the rest of the code expects.
fn describe(app: &NSRunningApplication) -> GameWindow {
    let pid = app.processIdentifier();

    // The executable file name, to match what the other backends report as the
    // application class: `Dofus.x64` on X11, `Dofus.exe` on Windows.
    let app_class = app
        .executableURL()
        .and_then(|url| url.path())
        .map(|path| path.to_string())
        .and_then(|path| path.rsplit('/').next().map(str::to_owned))
        .unwrap_or_default();

    let title = app
        .localizedName()
        .map(|name| name.to_string())
        .unwrap_or_default();

    GameWindow {
        // The process id doubles as the window handle here; see the module docs.
        id: WindowId(pid as u64),
        pid: u32::try_from(pid).ok(),
        title,
        app_class,
    }
}

/// Looks a running application up by process id.
fn application(pid: i32) -> Option<objc2::rc::Retained<NSRunningApplication>> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
}

impl WindowManager for MacBackend {
    fn backend_name(&self) -> &'static str {
        "appkit"
    }

    fn list_windows(&mut self) -> Result<Vec<GameWindow>> {
        let workspace = NSWorkspace::sharedWorkspace();
        let running = workspace.runningApplications();

        Ok(running.iter().map(|app| describe(&app)).collect())
    }

    fn active_window(&mut self) -> Result<Option<WindowId>> {
        let workspace = NSWorkspace::sharedWorkspace();
        let frontmost = workspace.frontmostApplication();

        Ok(frontmost.map(|app| {
            let pid = app.processIdentifier();
            WindowId(pid as u64)
        }))
    }

    /// No-op: winit sets the window level correctly on macOS.
    ///
    /// Unlike X11, where `WindowLevel::AlwaysOnTop` never reaches the window
    /// manager, the AppKit path works, so there is nothing to patch up here.
    fn make_overlay(&mut self, _id: WindowId) -> Result<()> {
        Ok(())
    }

    fn focus(&mut self, id: WindowId) -> Result<()> {
        let pid = i32::try_from(id.0).map_err(|_| Error::WindowGone(id.0))?;
        let app = application(pid).ok_or(Error::WindowGone(id.0))?;

        // `ActivateAllWindows` brings every window of the client forward. The
        // older `ActivateIgnoringOtherApps` is deprecated since macOS 14 and
        // now has no effect, so it is deliberately not used.
        let activated = app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);

        if !activated {
            return Err(Error::Backend(format!(
                "macOS refused to activate the application with pid {pid}"
            )));
        }

        Ok(())
    }
}
