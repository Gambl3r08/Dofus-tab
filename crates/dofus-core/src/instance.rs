// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Making sure only one process registers the global hotkeys.
//!
//! Both `dofus-tab daemon` and the overlay bar register the same shortcuts. Run
//! together they fight over them: on Linux the portal hands the same slots to
//! two sessions and every keypress switches twice, and on Windows the second
//! `RegisterHotKey` simply fails. Neither reports anything useful, so the
//! second process refuses to start instead.
//!
//! An advisory file lock is used rather than a pid file because the operating
//! system releases it when the holder dies, however it dies. A pid file left
//! behind by a crash would lock the user out until they deleted it by hand.
//!
//! The locking comes from the standard library, which has covered this since
//! Rust 1.89 — hence the crate's minimum supported version.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::PathBuf;

use crate::error::{Error, Result};

/// Name of the lock shared by everything that registers hotkeys.
pub const HOTKEY_LOCK: &str = "hotkeys";

/// Holds a lock for as long as it is alive.
///
/// Dropping it releases the lock; so does the process exiting for any reason.
#[derive(Debug)]
pub struct InstanceLock {
    /// Kept solely to hold the lock open.
    _file: File,
    path: PathBuf,
}

impl InstanceLock {
    /// File the lock is held on, for diagnostics.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

/// Takes `name` exclusively, failing if another process already holds it.
pub fn acquire(name: &str) -> Result<InstanceLock> {
    let path = lock_path(name)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
    }

    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|source| Error::io(&path, source))?;

    // Non-blocking: the point is to report the clash, not to queue behind it.
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Err(Error::AlreadyRunning(name.to_owned())),
        Err(TryLockError::Error(source)) => return Err(Error::io(&path, source)),
    }

    Ok(InstanceLock { _file: file, path })
}

/// Where the lock file lives.
///
/// The runtime directory is preferred because it is wiped between sessions;
/// the config directory is the fallback for platforms that have no such thing.
fn lock_path(name: &str) -> Result<PathBuf> {
    let base = dirs::runtime_dir()
        .or_else(dirs::config_dir)
        .ok_or(Error::Unsupported("locating a directory for the lock file"))?;

    Ok(base.join("dofus-tab").join(format!("{name}.lock")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_acquisition_of_the_same_name_is_refused() {
        let name = "dofus-tab-test-exclusive";
        let first = acquire(name).expect("the first lock should be granted");

        let second = acquire(name);
        assert!(
            matches!(second, Err(Error::AlreadyRunning(_))),
            "expected AlreadyRunning, got {second:?}"
        );

        drop(first);
    }

    #[test]
    fn releasing_a_lock_lets_the_next_caller_take_it() {
        let name = "dofus-tab-test-released";

        let first = acquire(name).expect("the first lock should be granted");
        drop(first);

        acquire(name).expect("the lock should be free again");
    }

    #[test]
    fn different_names_do_not_collide() {
        let _a = acquire("dofus-tab-test-a").expect("should be granted");
        let _b = acquire("dofus-tab-test-b").expect("should be granted");
    }
}
