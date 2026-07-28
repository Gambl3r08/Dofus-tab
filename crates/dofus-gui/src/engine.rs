// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! Background worker owning every platform resource.
//!
//! The UI thread must never block on an X server round trip or a portal call,
//! so all of that lives here and the two sides talk over a channel plus a
//! shared snapshot. Hotkey activations feed the same command queue as mouse
//! clicks, which keeps both paths on one code path.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use dofus_core::config::Config;
use dofus_core::hotkey;
use dofus_core::instance;
use dofus_core::launcher::AccountDirectory;
use dofus_core::registry::{self, Tab};
use dofus_core::{ProcessInspector, WindowId, WindowManager};

/// How often the window list is re-scanned when nothing else happens.
///
/// A scan is a handful of X11 round trips, so this is cheap; it exists to catch
/// clients that were launched or closed behind our back.
const REFRESH_INTERVAL: Duration = Duration::from_millis(750);

/// What the UI and the hotkey listener can ask the worker to do.
pub enum Command {
    /// Focus the tab in this 1-based slot.
    Focus(u32),
    /// Apply the overlay window hints to our own window.
    MakeOverlay(WindowId),
    /// Adopt a config edited in the settings window.
    ///
    /// The worker keeps its own copy because it builds the tab list; without
    /// this, a rename or a reorder would not show up until restart.
    UpdateConfig(Box<Config>),
}

/// Snapshot the UI renders from.
#[derive(Default)]
pub struct EngineState {
    pub tabs: Vec<Tab>,
    /// Window that currently holds focus, used to highlight its tab.
    pub active: Option<WindowId>,
    /// Last error, shown in the bar instead of silently rendering nothing.
    pub error: Option<String>,
    /// What the desktop assigned to each hotkey, shown in the settings window.
    pub shortcuts: Vec<hotkey::RegisteredShortcut>,
}

/// Handle the UI keeps to talk to the worker.
#[derive(Clone)]
pub struct EngineHandle {
    commands: Sender<Command>,
    pub(crate) state: Arc<Mutex<EngineState>>,
}

impl EngineHandle {
    /// Sends a command, ignoring failures if the worker already stopped.
    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Runs `read` against the current snapshot.
    pub fn with_state<T>(&self, read: impl FnOnce(&EngineState) -> T) -> T {
        let state = self.state.lock().expect("engine state mutex was poisoned");
        read(&state)
    }
}

/// Starts the worker thread and returns a handle to it.
///
/// `on_change` is invoked whenever the snapshot changes, so the UI can wake up.
pub fn spawn(on_change: impl Fn() + Send + 'static) -> EngineHandle {
    let (commands, inbox) = mpsc::channel();
    let state = Arc::new(Mutex::new(EngineState::default()));

    let handle = EngineHandle {
        commands: commands.clone(),
        state: Arc::clone(&state),
    };

    thread::Builder::new()
        .name("dofus-tab-engine".to_string())
        .spawn(move || run(inbox, state, on_change))
        .expect("the engine thread should start");

    handle
}

/// Starts a thread that turns global hotkeys into [`Command::Focus`].
///
/// Failing to reach the hotkey provider is not fatal: the bar stays usable with
/// the mouse, so the error is logged and the thread exits.
pub fn spawn_hotkeys(handle: EngineHandle) {
    thread::Builder::new()
        .name("dofus-tab-hotkeys".to_string())
        .spawn(move || {
            // Held for the lifetime of the thread; see `dofus_core::instance`.
            let _lock = match instance::acquire(instance::HOTKEY_LOCK) {
                Ok(lock) => lock,
                Err(error) => {
                    tracing::warn!(%error, "not registering hotkeys");
                    return;
                }
            };

            let config = Config::load().unwrap_or_default();
            let bindings = config.hotkey_bindings();

            let mut hotkeys = match dofus_platform::detect_hotkey_manager() {
                Ok(hotkeys) => hotkeys,
                Err(error) => {
                    tracing::warn!(%error, "global hotkeys are unavailable");
                    return;
                }
            };

            if let Err(error) = hotkeys.bind(&bindings) {
                tracing::warn!(%error, "could not register the global shortcuts");
                return;
            }

            publish_shortcuts(hotkeys.as_mut(), &handle);

            let result = hotkeys.listen(&mut |action| {
                let hotkey::Action::FocusSlot(slot) = action;
                handle.send(Command::Focus(slot));
            });

            if let Err(error) = result {
                tracing::warn!(%error, "the hotkey listener stopped");
            }
        })
        .expect("the hotkey thread should start");
}

/// Records what the desktop assigned, and warns about anything left inert.
fn publish_shortcuts(hotkeys: &mut dyn hotkey::HotkeyManager, handle: &EngineHandle) {
    let Ok(registered) = hotkeys.registered() else {
        return;
    };

    let inert = registered.iter().filter(|s| !s.is_active()).count();
    if inert > 0 {
        tracing::warn!(
            inert,
            total = registered.len(),
            "some shortcuts have no key assigned and will never fire"
        );
    }

    handle
        .state
        .lock()
        .expect("engine state mutex was poisoned")
        .shortcuts = registered;
}

/// Worker loop: serve commands, and re-scan whenever the queue goes quiet.
fn run(inbox: Receiver<Command>, state: Arc<Mutex<EngineState>>, on_change: impl Fn()) {
    let mut worker = match Worker::new() {
        Ok(worker) => worker,
        Err(error) => {
            publish_error(&state, error.to_string());
            on_change();
            return;
        }
    };

    loop {
        worker.refresh(&state);
        on_change();

        match inbox.recv_timeout(REFRESH_INTERVAL) {
            Ok(Command::Focus(slot)) => worker.focus(slot, &state),
            Ok(Command::UpdateConfig(config)) => worker.config = *config,
            Ok(Command::MakeOverlay(window)) => {
                if let Err(error) = worker.windows.make_overlay(window) {
                    tracing::warn!(%error, "could not make the bar an overlay");
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            // The UI is gone, so is our reason to run.
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Owns the platform handles; lives entirely on the worker thread.
struct Worker {
    windows: Box<dyn WindowManager>,
    inspector: Box<dyn ProcessInspector>,
    directory: AccountDirectory,
    config: Config,
}

impl Worker {
    fn new() -> dofus_core::Result<Self> {
        Ok(Self {
            windows: dofus_platform::detect_window_manager()?,
            inspector: Box::new(dofus_platform::process_inspector()),
            directory: AccountDirectory::load().unwrap_or_else(|error| {
                tracing::warn!(%error, "could not read the launcher accounts");
                AccountDirectory::empty()
            }),
            config: Config::load().unwrap_or_default(),
        })
    }

    fn refresh(&mut self, state: &Arc<Mutex<EngineState>>) {
        let windows = match self.windows.list_windows() {
            Ok(windows) => windows,
            Err(error) => {
                publish_error(state, error.to_string());
                return;
            }
        };

        let tabs = registry::build_tabs(
            &windows,
            self.inspector.as_ref(),
            &self.directory,
            &self.config,
        );
        let active = self.windows.active_window().unwrap_or_default();

        let mut state = state.lock().expect("engine state mutex was poisoned");
        state.tabs = tabs;
        state.active = active;
        state.error = None;
    }

    fn focus(&mut self, slot: u32, state: &Arc<Mutex<EngineState>>) {
        let target = self.with_state(state, |state| {
            state
                .tabs
                .iter()
                .find(|tab| tab.slot == slot)
                .map(|tab| tab.window)
        });

        let Some(window) = target else {
            tracing::debug!(slot, "no window in this slot");
            return;
        };

        if let Err(error) = self.windows.focus(window) {
            tracing::warn!(slot, %error, "could not focus the window");
            publish_error(state, error.to_string());
        }
    }

    fn with_state<T>(
        &self,
        state: &Arc<Mutex<EngineState>>,
        read: impl FnOnce(&EngineState) -> T,
    ) -> T {
        let state = state.lock().expect("engine state mutex was poisoned");
        read(&state)
    }
}

fn publish_error(state: &Arc<Mutex<EngineState>>, message: String) {
    let mut state = state.lock().expect("engine state mutex was poisoned");
    state.error = Some(message);
}
