// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Linux window backends.
//!
//! `x11` covers both real X11 sessions and XWayland clients, which is where the
//! Dofus 3 client lives today. Compositor-specific Wayland backends (KWin via
//! its D-Bus scripting interface, Hyprland via its IPC socket, wlroots via
//! `wlr-foreign-toplevel-management`) are planned behind the same trait.

pub mod portal;
pub mod x11;
