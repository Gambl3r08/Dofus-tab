// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Error type shared by the core domain and the platform backends.

use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No backend is available for the current session (e.g. a Wayland
    /// compositor without a supported control interface).
    #[error("no supported window backend for this session: {0}")]
    NoBackend(String),

    /// The backend is available but the operation failed.
    #[error("window backend error: {0}")]
    Backend(String),

    /// The requested window no longer exists; the caller should re-scan.
    #[error("window {0} is gone")]
    WindowGone(u64),

    /// Another Dofus-tab process already holds the named lock.
    #[error(
        "another Dofus-tab process is already running ({0}); the overlay bar and \
         `dofus-tab daemon` cannot register the hotkeys at the same time"
    )]
    AlreadyRunning(String),

    /// A feature has no implementation on this OS yet.
    #[error("{0} is not implemented on this platform yet")]
    Unsupported(&'static str),

    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn json(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        Self::Json {
            path: path.into(),
            source,
        }
    }
}
