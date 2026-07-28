// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Starting the bar with the desktop session.
//!
//! On Linux this is the freedesktop autostart specification: a desktop entry
//! dropped into `~/.config/autostart` is launched when the session begins.
//!
//! The entry points at the running executable rather than at a bare command
//! name, for the same reason the installed desktop entry does: the XDG portal
//! only accepts our application identity when `Exec` resolves to the binary
//! that is actually running, and without that identity the global hotkeys are
//! refused.

use dofus_core::Result;

/// Whether Dofus-tab is set to start with the session.
pub fn is_enabled() -> bool {
    imp::is_enabled()
}

/// Turns starting with the session on or off.
pub fn set_enabled(enabled: bool) -> Result<()> {
    imp::set_enabled(enabled)
}

/// Whether this platform can manage autostart at all.
pub const fn is_supported() -> bool {
    cfg!(target_os = "linux")
}

#[cfg(target_os = "linux")]
mod imp {
    use std::path::{Path, PathBuf};

    use dofus_core::registry::OWN_APP_ID;
    use dofus_core::{Error, Result};

    fn entry_path() -> Option<PathBuf> {
        Some(
            dirs::config_dir()?
                .join("autostart")
                .join(format!("{OWN_APP_ID}.desktop")),
        )
    }

    pub fn is_enabled() -> bool {
        entry_path().is_some_and(|path| path.exists())
    }

    pub fn set_enabled(enabled: bool) -> Result<()> {
        let path = entry_path().ok_or(Error::Unsupported("locating the autostart directory"))?;
        let executable = std::env::current_exe()
            .map_err(|source| Error::io("the running executable", source))?;

        write_entry(&path, &executable, enabled)
    }

    /// Creates or removes the entry at `path`.
    ///
    /// Split out from [`set_enabled`] so it can be exercised against a
    /// throwaway directory instead of the user's real session config.
    fn write_entry(path: &Path, executable: &Path, enabled: bool) -> Result<()> {
        if !enabled {
            return match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                // Already off; nothing to undo.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(source) => Err(Error::io(path, source)),
            };
        }

        let parent = path
            .parent()
            .ok_or(Error::Unsupported("locating the autostart directory"))?;
        std::fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;

        let entry = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Dofus-tab\n\
             Comment=Switch between Dofus client windows with global hotkeys\n\
             Exec={}\n\
             Icon={OWN_APP_ID}\n\
             Terminal=false\n\
             X-GNOME-Autostart-enabled=true\n",
            executable.display()
        );

        std::fs::write(path, entry).map_err(|source| Error::io(path, source))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A directory of our own, so tests never touch the real session config
        /// and never race each other over a shared file.
        fn scratch(name: &str) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("dofus-tab-autostart-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            dir.join("autostart").join("entry.desktop")
        }

        #[test]
        fn enabling_creates_the_entry_and_disabling_removes_it() {
            let path = scratch("round-trip");
            let executable = Path::new("/opt/dofus-tab/dofus-tab-bar");

            write_entry(&path, executable, true).expect("should create the entry");
            assert!(path.exists(), "the entry should exist after enabling");

            write_entry(&path, executable, false).expect("should remove the entry");
            assert!(!path.exists(), "the entry should be gone after disabling");
        }

        #[test]
        fn disabling_when_already_off_is_not_an_error() {
            let path = scratch("already-off");
            let executable = Path::new("/opt/dofus-tab/dofus-tab-bar");

            write_entry(&path, executable, false).expect("first disable");
            write_entry(&path, executable, false).expect("disabling twice must be harmless");
        }

        #[test]
        fn the_entry_points_at_the_given_executable() {
            let path = scratch("exec-line");
            let executable = Path::new("/opt/dofus-tab/dofus-tab-bar");

            write_entry(&path, executable, true).expect("should create the entry");
            let entry = std::fs::read_to_string(&path).expect("the entry should be readable");

            assert!(
                entry.contains("Exec=/opt/dofus-tab/dofus-tab-bar"),
                "the portal only accepts our identity when Exec resolves to the running binary"
            );
            assert!(entry.contains(&format!("Icon={OWN_APP_ID}")));
            assert!(entry.starts_with("[Desktop Entry]"));
        }

        #[test]
        fn enabling_twice_overwrites_rather_than_failing() {
            let path = scratch("overwrite");

            write_entry(&path, Path::new("/old/path"), true).expect("first write");
            write_entry(&path, Path::new("/new/path"), true).expect("second write");

            let entry = std::fs::read_to_string(&path).expect("readable");
            assert!(entry.contains("Exec=/new/path"));
            assert!(!entry.contains("Exec=/old/path"));
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use dofus_core::{Error, Result};

    pub fn is_enabled() -> bool {
        false
    }

    pub fn set_enabled(_enabled: bool) -> Result<()> {
        // Windows wants a Run registry key, macOS a LaunchAgent plist. Neither
        // is guessed at here; see the platform notes in the README.
        Err(Error::Unsupported("starting with the session"))
    }
}
