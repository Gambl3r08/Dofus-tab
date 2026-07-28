//! Global hotkeys on macOS — deliberately not implemented yet.
//!
//! # Why this is a stub
//!
//! The obvious API, Carbon's `RegisterEventHotKey`, works and needs no user
//! permission, but it delivers through the *application's* Carbon event target,
//! which is only pumped by a `CFRunLoop` on the **main thread**. Our
//! [`HotkeyManager`] contract is a blocking `listen` called from a worker
//! thread, which is what the X11 and Win32 backends do and what both front ends
//! are built around. On macOS that thread would never receive anything, and the
//! failure mode is silence — exactly the bug class that cost this project an
//! afternoon on KDE.
//!
//! Making it work means restructuring both front ends so the hotkey listener
//! owns the main thread, or handing the registration to whatever already owns
//! it — winit's event loop in the bar, a `CFRunLoop` we start ourselves in the
//! CLI daemon. That is a real design change, not a missing function, and it is
//! not something to get right without a Mac to run it on.
//!
//! The `global-hotkey` crate solves the Carbon plumbing and is the likely
//! implementation, but it carries the same main-thread requirement.
//!
//! Until then `dofus-tab list`, `focus` and the overlay bar all work on macOS;
//! only the keyboard shortcuts are missing, and they say so out loud rather
//! than failing quietly.

use dofus_core::hotkey::{Action, HotkeyBinding, HotkeyManager, RegisteredShortcut};
use dofus_core::{Error, Result};

/// Placeholder that reports its own absence instead of pretending to work.
pub struct MacHotkeys;

impl MacHotkeys {
    pub fn connect() -> Result<Self> {
        Err(unsupported())
    }
}

impl HotkeyManager for MacHotkeys {
    fn bind(&mut self, _bindings: &[HotkeyBinding]) -> Result<()> {
        Err(unsupported())
    }

    fn registered(&mut self) -> Result<Vec<RegisteredShortcut>> {
        Err(unsupported())
    }

    fn listen(&mut self, _on_activated: &mut dyn FnMut(Action)) -> Result<()> {
        Err(unsupported())
    }
}

fn unsupported() -> Error {
    Error::Unsupported(
        "global hotkeys on macOS; Carbon delivers them on the main thread, \
         which the current listener design does not own",
    )
}
