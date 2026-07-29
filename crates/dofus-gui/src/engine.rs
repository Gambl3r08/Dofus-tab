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

/// Registers the global hotkeys, to be polled from the UI thread.
///
/// Deliberately *not* on a worker thread: macOS only delivers hotkeys to the
/// thread running the application event loop, which is the one drawing the bar.
/// Returning `None` is not fatal — the bar stays usable with the mouse.
pub fn bind_hotkeys() -> Option<HotkeySession> {
    // Held for as long as the session lives; see `dofus_core::instance`.
    let lock = match instance::acquire(instance::HOTKEY_LOCK) {
        Ok(lock) => lock,
        Err(error) => {
            tracing::warn!(%error, "not registering hotkeys");
            return None;
        }
    };

    let config = Config::load().unwrap_or_default();
    let mut hotkeys = match dofus_platform::detect_hotkey_manager() {
        Ok(hotkeys) => hotkeys,
        Err(error) => {
            tracing::warn!(%error, "global hotkeys are unavailable");
            return None;
        }
    };

    if let Err(error) = hotkeys.bind(&config.hotkey_bindings()) {
        tracing::warn!(%error, "could not register the global shortcuts");
        return None;
    }

    Some(HotkeySession {
        hotkeys,
        _lock: lock,
    })
}

/// Live hotkey registration, polled once per frame.
pub struct HotkeySession {
    hotkeys: Box<dyn hotkey::HotkeyManager>,
    _lock: instance::InstanceLock,
}

impl HotkeySession {
    /// Returns the slots activated since the last call.
    pub fn poll(&mut self) -> Vec<u32> {
        self.hotkeys
            .poll()
            .into_iter()
            .map(|hotkey::Action::FocusSlot(slot)| slot)
            .collect()
    }

    /// What the desktop assigned, and a warning for anything left inert.
    pub fn registered(&mut self) -> Vec<hotkey::RegisteredShortcut> {
        let Ok(registered) = self.hotkeys.registered() else {
            return Vec::new();
        };

        let inert = registered.iter().filter(|s| !s.is_active()).count();
        if inert > 0 {
            tracing::warn!(
                inert,
                total = registered.len(),
                "some shortcuts have no key assigned and will never fire"
            );
        }

        registered
    }
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
        Ok(Self::with_parts(
            dofus_platform::detect_window_manager()?,
            Box::new(dofus_platform::process_inspector()),
            AccountDirectory::load().unwrap_or_else(|error| {
                tracing::warn!(%error, "could not read the launcher accounts");
                AccountDirectory::empty()
            }),
            Config::load().unwrap_or_default(),
        ))
    }

    /// Builds a worker from explicit parts.
    ///
    /// Exists so the tests can drive it with fake backends; the real
    /// constructor is [`Worker::new`].
    fn with_parts(
        windows: Box<dyn WindowManager>,
        inspector: Box<dyn ProcessInspector>,
        directory: AccountDirectory,
        config: Config,
    ) -> Self {
        Self {
            windows,
            inspector,
            directory,
            config,
        }
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use dofus_core::GameWindow;
    use dofus_core::account::AccountId;

    use super::*;

    /// Window backend driven entirely from the test.
    #[derive(Default)]
    struct FakeWindows {
        windows: Vec<GameWindow>,
        active: Option<WindowId>,
        /// Every window `focus` was asked to raise, in order.
        focused: Arc<Mutex<Vec<WindowId>>>,
        /// When set, `list_windows` fails with this message instead.
        list_error: Option<String>,
        /// When set, `focus` fails with this message instead.
        focus_error: Option<String>,
    }

    impl WindowManager for FakeWindows {
        fn backend_name(&self) -> &'static str {
            "fake"
        }

        fn list_windows(&mut self) -> dofus_core::Result<Vec<GameWindow>> {
            match &self.list_error {
                Some(message) => Err(dofus_core::Error::Backend(message.clone())),
                None => Ok(self.windows.clone()),
            }
        }

        fn active_window(&mut self) -> dofus_core::Result<Option<WindowId>> {
            Ok(self.active)
        }

        fn make_overlay(&mut self, _id: WindowId) -> dofus_core::Result<()> {
            Ok(())
        }

        fn focus(&mut self, id: WindowId) -> dofus_core::Result<()> {
            self.focused
                .lock()
                .expect("the focus log mutex was poisoned")
                .push(id);
            match &self.focus_error {
                Some(message) => Err(dofus_core::Error::Backend(message.clone())),
                None => Ok(()),
            }
        }
    }

    /// Hands out the launcher command line for whatever pid is asked for.
    struct FakeInspector {
        calls: AtomicUsize,
    }

    impl ProcessInspector for FakeInspector {
        fn cmdline(&self, pid: u32) -> dofus_core::Result<Vec<String>> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(vec![
                "./Dofus.x64".to_string(),
                "-logFile".to_string(),
                // The account id mirrors the pid so tests can predict it.
                format!("/tmp/zaap/dofus.{pid}.log"),
            ])
        }
    }

    fn window(id: u64, pid: u32, class: &str) -> GameWindow {
        GameWindow {
            id: WindowId(id),
            pid: Some(pid),
            title: "Dofus".to_string(),
            app_class: class.to_string(),
        }
    }

    fn worker_with(windows: FakeWindows) -> (Worker, Arc<Mutex<Vec<WindowId>>>) {
        let focused = Arc::clone(&windows.focused);
        let worker = Worker::with_parts(
            Box::new(windows),
            Box::new(FakeInspector {
                calls: AtomicUsize::new(0),
            }),
            AccountDirectory::parse(
                r#"{"USER_ACCOUNTS":[{"id": 11, "nickname": "alpha", "tag": "1"}]}"#,
                None,
            )
            .expect("fixture should parse"),
            Config::default(),
        );
        (worker, focused)
    }

    fn state() -> Arc<Mutex<EngineState>> {
        Arc::new(Mutex::new(EngineState::default()))
    }

    #[test]
    fn refresh_publishes_only_the_dofus_windows() {
        let (mut worker, _) = worker_with(FakeWindows {
            windows: vec![
                window(0x10, 11, "Dofus.x64"),
                window(0x20, 22, "firefox"),
                window(0x30, 33, "Ankama Launcher"),
            ],
            ..FakeWindows::default()
        });
        let state = state();

        worker.refresh(&state);

        let published = state.lock().expect("poisoned");
        assert_eq!(published.tabs.len(), 1);
        assert_eq!(published.tabs[0].window, WindowId(0x10));
        assert_eq!(published.tabs[0].slot, 1);
    }

    #[test]
    fn refresh_resolves_account_names_through_the_inspector() {
        let (mut worker, _) = worker_with(FakeWindows {
            windows: vec![window(0x10, 11, "Dofus.x64")],
            ..FakeWindows::default()
        });
        let state = state();

        worker.refresh(&state);

        let published = state.lock().expect("poisoned");
        assert_eq!(published.tabs[0].account, Some(AccountId(11)));
        assert_eq!(published.tabs[0].label, "alpha#1");
    }

    #[test]
    fn refresh_publishes_the_active_window() {
        let (mut worker, _) = worker_with(FakeWindows {
            windows: vec![window(0x10, 11, "Dofus.x64")],
            active: Some(WindowId(0x10)),
            ..FakeWindows::default()
        });
        let state = state();

        worker.refresh(&state);

        assert_eq!(state.lock().expect("poisoned").active, Some(WindowId(0x10)));
    }

    #[test]
    fn a_failing_scan_publishes_the_error_and_keeps_the_previous_tabs() {
        let (mut worker, _) = worker_with(FakeWindows {
            windows: vec![window(0x10, 11, "Dofus.x64")],
            ..FakeWindows::default()
        });
        let state = state();
        worker.refresh(&state);

        // The backend starts failing; the bar should say so rather than
        // silently emptying itself.
        let (mut broken, _) = worker_with(FakeWindows {
            list_error: Some("X server went away".to_string()),
            ..FakeWindows::default()
        });
        broken.refresh(&state);

        let published = state.lock().expect("poisoned");
        assert!(
            published
                .error
                .as_deref()
                .is_some_and(|e| e.contains("X server went away"))
        );
        assert_eq!(
            published.tabs.len(),
            1,
            "the last known tabs should survive"
        );
    }

    #[test]
    fn a_successful_scan_clears_a_previous_error() {
        let state = state();
        publish_error(&state, "something broke".to_string());

        let (mut worker, _) = worker_with(FakeWindows {
            windows: vec![window(0x10, 11, "Dofus.x64")],
            ..FakeWindows::default()
        });
        worker.refresh(&state);

        assert_eq!(state.lock().expect("poisoned").error, None);
    }

    #[test]
    fn focus_raises_the_window_in_that_slot() {
        let (mut worker, focused) = worker_with(FakeWindows {
            windows: vec![window(0x10, 11, "Dofus.x64"), window(0x20, 22, "Dofus.x64")],
            ..FakeWindows::default()
        });
        let state = state();
        worker.refresh(&state);

        worker.focus(2, &state);

        assert_eq!(*focused.lock().expect("poisoned"), vec![WindowId(0x20)]);
    }

    #[test]
    fn focusing_an_empty_slot_does_nothing() {
        let (mut worker, focused) = worker_with(FakeWindows {
            windows: vec![window(0x10, 11, "Dofus.x64")],
            ..FakeWindows::default()
        });
        let state = state();
        worker.refresh(&state);

        // Slot 5 has no client behind it; pressing Alt+5 must be a no-op.
        worker.focus(5, &state);

        assert!(focused.lock().expect("poisoned").is_empty());
        assert_eq!(state.lock().expect("poisoned").error, None);
    }

    #[test]
    fn a_failing_focus_is_reported() {
        let (mut worker, _) = worker_with(FakeWindows {
            windows: vec![window(0x10, 11, "Dofus.x64")],
            focus_error: Some("window is gone".to_string()),
            ..FakeWindows::default()
        });
        let state = state();
        worker.refresh(&state);

        worker.focus(1, &state);

        let published = state.lock().expect("poisoned");
        assert!(
            published
                .error
                .as_deref()
                .is_some_and(|e| e.contains("window is gone"))
        );
    }

    #[test]
    fn a_renamed_account_shows_up_without_a_restart() {
        let (mut worker, _) = worker_with(FakeWindows {
            windows: vec![window(0x10, 11, "Dofus.x64")],
            ..FakeWindows::default()
        });
        let state = state();
        worker.refresh(&state);
        assert_eq!(state.lock().expect("poisoned").tabs[0].label, "alpha#1");

        // What `Command::UpdateConfig` does when the settings window edits a label.
        let mut config = Config::default();
        config.set_label(AccountId(11), "Main Iop");
        worker.config = config;
        worker.refresh(&state);

        assert_eq!(state.lock().expect("poisoned").tabs[0].label, "Main Iop");
    }
}
