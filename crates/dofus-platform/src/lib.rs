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

pub mod process;

#[cfg(target_os = "linux")]
pub mod linux;

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
#[cfg(not(target_os = "linux"))]
pub fn detect_window_manager() -> Result<Box<dyn WindowManager>> {
    Err(dofus_core::Error::Unsupported(
        "window management on this operating system",
    ))
}

/// Returns the process inspector for the current OS.
pub fn process_inspector() -> impl ProcessInspector {
    process::OsProcessInspector
}
