//! Linux window backends.
//!
//! `x11` covers both real X11 sessions and XWayland clients, which is where the
//! Dofus 3 client lives today. Compositor-specific Wayland backends (KWin via
//! its D-Bus scripting interface, Hyprland via its IPC socket, wlroots via
//! `wlr-foreign-toplevel-management`) are planned behind the same trait.

pub mod portal;
pub mod x11;
