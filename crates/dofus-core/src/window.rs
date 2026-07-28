//! Window value objects exchanged between backends and the core.

use std::fmt;

/// Opaque handle to a top-level window.
///
/// Backends encode their native handle in here: an X11 `Window` id, a KWin
/// internal uuid hash, an `HWND`, a `CGWindowID`. It is only meaningful to the
/// backend that produced it and is **not** stable across restarts, which is why
/// account identity is resolved separately (see [`crate::identity`]).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct WindowId(pub u64);

impl fmt::Display for WindowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:x}", self.0)
    }
}

/// A top-level window as reported by the host window manager.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameWindow {
    pub id: WindowId,
    /// Owning process. `None` when the backend cannot expose it (some
    /// compositors do not publish a pid for every surface).
    pub pid: Option<u32>,
    /// Current window title. For Dofus 3 this is always `"Dofus"`, so it is
    /// shown for diagnostics only and never used to identify an account.
    pub title: String,
    /// Application class, e.g. `"Dofus.x64"`. Used to filter game clients.
    pub app_class: String,
}
