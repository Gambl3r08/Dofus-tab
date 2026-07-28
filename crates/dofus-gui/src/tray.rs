// Dofus-tab, a cross-platform window switcher for Dofus multi-accounting.
// Copyright (C) 2026 Roberto Lozada
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. It comes with ABSOLUTELY NO WARRANTY. See the LICENSE file for the
// full text.

//! System tray icon.
//!
//! An overlay with no title bar and no taskbar entry needs somewhere to live
//! when the user wants to close it or change a setting. The tray is that place.
//!
//! # Two backends
//!
//! Windows and macOS use `tray-icon`, which wraps the native APIs. Linux does
//! not: `tray-icon`'s Linux path needs `libayatana-appindicator`, which is not
//! installed everywhere, and pulls in GTK plus a second main loop. The
//! StatusNotifierItem D-Bus specification is spoken directly instead, which KDE
//! and most panels implement natively and which costs no extra dependency
//! beyond the D-Bus client already used for the hotkey portal.
//!
//! Both backends funnel into the same channel, so the bar sees one interface.
//!
//! # Threading
//!
//! [`spawn`] must be called from the main thread. macOS requires it for any
//! status item, and Windows needs the tray's window to belong to the thread
//! that pumps messages — which is the one running the event loop.

use std::sync::mpsc::Receiver;

/// What the user picked from the tray menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrayCommand {
    /// Open the settings window.
    ShowSettings,
    /// Hide the bar if visible, show it otherwise.
    ToggleBar,
    /// Quit the application.
    Quit,
}

/// Labels, kept in one place so both backends agree.
const SETTINGS_LABEL: &str = "Settings...";
const TOGGLE_LABEL: &str = "Show/hide bar";
const QUIT_LABEL: &str = "Quit";
const TOOLTIP: &str = "Dofus-tab - switch between Dofus clients";

/// Whatever went wrong while installing the tray.
///
/// Each backend has its own error type and none of them is interesting beyond
/// its message, so they are boxed rather than unified into an enum.
type BuildError = Box<dyn std::error::Error>;

/// Keeps the tray alive; dropping this removes the icon.
pub struct TrayHandle {
    commands: Receiver<TrayCommand>,
    /// The platform object whose lifetime the icon depends on.
    _backend: imp::Backend,
}

impl TrayHandle {
    /// Returns every command queued since the last call.
    pub fn drain(&self) -> Vec<TrayCommand> {
        self.commands.try_iter().collect()
    }
}

/// Installs the tray icon.
///
/// Returns `None` when no tray is available, which is not fatal: the bar and
/// the hotkeys keep working, the user just loses the menu.
///
/// `repaint` wakes the UI thread, which is otherwise idle waiting for events.
pub fn spawn(repaint: impl Fn() + Send + Sync + 'static) -> Option<TrayHandle> {
    let (sender, commands) = std::sync::mpsc::channel();

    match imp::build(sender, Box::new(repaint)) {
        Ok(backend) => Some(TrayHandle {
            commands,
            _backend: backend,
        }),
        Err(error) => {
            tracing::warn!(%error, "no system tray available");
            None
        }
    }
}

/// Decodes the embedded window icon into raw RGBA.
fn icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let icon = eframe::icon_data::from_png_bytes(crate::ICON_PNG)
        .inspect_err(|error| tracing::warn!(%error, "could not decode the tray icon"))
        .ok()?;

    Some((icon.rgba, icon.width, icon.height))
}

#[cfg(target_os = "linux")]
mod imp {
    use std::sync::mpsc::Sender;

    use dofus_core::registry::OWN_APP_ID;
    use ksni::blocking::TrayMethods;
    use ksni::menu::{MenuItem, StandardItem};

    use super::{QUIT_LABEL, SETTINGS_LABEL, TOGGLE_LABEL, TOOLTIP, TrayCommand};

    /// Handle to the running StatusNotifierItem service.
    pub type Backend = ksni::blocking::Handle<DofusTray>;

    pub fn build(
        sender: Sender<TrayCommand>,
        repaint: Box<dyn Fn() + Send + Sync>,
    ) -> Result<Backend, super::BuildError> {
        Ok(DofusTray { sender, repaint }.spawn()?)
    }

    pub struct DofusTray {
        sender: Sender<TrayCommand>,
        repaint: Box<dyn Fn() + Send + Sync>,
    }

    impl DofusTray {
        fn send(&self, command: TrayCommand) {
            let _ = self.sender.send(command);
            (self.repaint)();
        }

        fn item(label: &str, command: TrayCommand) -> MenuItem<Self> {
            StandardItem {
                label: label.to_owned(),
                activate: Box::new(move |tray: &mut Self| tray.send(command)),
                ..Default::default()
            }
            .into()
        }
    }

    impl ksni::Tray for DofusTray {
        /// Clicking the icon opens the menu rather than doing something
        /// invisible; there is no obvious default action for a switcher.
        const MENU_ON_ACTIVATE: bool = true;

        fn id(&self) -> String {
            OWN_APP_ID.to_owned()
        }

        fn title(&self) -> String {
            "Dofus-tab".to_owned()
        }

        /// Resolved from the installed hicolor theme, which the install script
        /// populates under this same name.
        fn icon_name(&self) -> String {
            OWN_APP_ID.to_owned()
        }

        /// Embedded fallback, so the icon still shows before installation.
        ///
        /// StatusNotifierItem wants ARGB32 in network byte order, while the
        /// decoder hands back RGBA, so the channels are reordered here.
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            let Some((rgba, width, height)) = super::icon_rgba() else {
                return Vec::new();
            };

            let data = rgba
                .chunks_exact(4)
                .flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]])
                .collect();

            vec![ksni::Icon {
                width: width as i32,
                height: height as i32,
                data,
            }]
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: "Dofus-tab".to_owned(),
                description: TOOLTIP.to_owned(),
                ..Default::default()
            }
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            vec![
                Self::item(SETTINGS_LABEL, TrayCommand::ShowSettings),
                Self::item(TOGGLE_LABEL, TrayCommand::ToggleBar),
                MenuItem::Separator,
                Self::item(QUIT_LABEL, TrayCommand::Quit),
            ]
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod imp {
    use std::sync::mpsc::Sender;

    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    use super::{QUIT_LABEL, SETTINGS_LABEL, TOGGLE_LABEL, TOOLTIP, TrayCommand};

    /// The icon itself; dropping it removes the tray entry.
    pub type Backend = TrayIcon;

    pub fn build(
        sender: Sender<TrayCommand>,
        repaint: Box<dyn Fn() + Send + Sync>,
    ) -> Result<Backend, super::BuildError> {
        let settings = MenuItem::new(SETTINGS_LABEL, true, None);
        let toggle = MenuItem::new(TOGGLE_LABEL, true, None);
        let quit = MenuItem::new(QUIT_LABEL, true, None);

        let menu = Menu::new();
        menu.append_items(&[&settings, &toggle, &PredefinedMenuItem::separator(), &quit])?;

        // Menu events arrive on a global channel with no wake-up of their own,
        // so they are translated here and the UI thread is nudged.
        let (settings_id, toggle_id, quit_id) = (
            settings.id().clone(),
            toggle.id().clone(),
            quit.id().clone(),
        );
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let command = if event.id == settings_id {
                TrayCommand::ShowSettings
            } else if event.id == toggle_id {
                TrayCommand::ToggleBar
            } else if event.id == quit_id {
                TrayCommand::Quit
            } else {
                return;
            };

            let _ = sender.send(command);
            repaint();
        }));

        let mut builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(TOOLTIP);

        if let Some((rgba, width, height)) = super::icon_rgba() {
            match Icon::from_rgba(rgba, width, height) {
                Ok(icon) => builder = builder.with_icon(icon),
                Err(error) => tracing::warn!(%error, "could not build the tray icon"),
            }
        }

        Ok(builder.build()?)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
mod imp {
    use std::sync::mpsc::Sender;

    use super::TrayCommand;

    pub type Backend = ();

    pub fn build(
        _sender: Sender<TrayCommand>,
        _repaint: Box<dyn Fn() + Send + Sync>,
    ) -> Result<Backend, super::BuildError> {
        Err("the tray is not implemented on this platform".into())
    }
}
