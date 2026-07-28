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
pub mod identity;
pub mod launcher;
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
}

/// Reads process metadata, used to map a window back to an Ankama account.
pub trait ProcessInspector {
    /// Full argument vector of `pid`, including `argv[0]`.
    fn cmdline(&self, pid: u32) -> Result<Vec<String>>;
}
