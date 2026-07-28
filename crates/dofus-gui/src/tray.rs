//! System tray icon.
//!
//! An overlay with no title bar and no taskbar entry needs somewhere to live
//! when the user wants to close it or change a setting. The tray is that place.
//!
//! Linux goes through the StatusNotifierItem D-Bus specification rather than
//! `libappindicator`, which is not installed everywhere and would drag in GTK
//! and a second main loop. KDE, GNOME (with an extension) and most panels
//! implement the specification natively.

use std::sync::mpsc::Receiver;
#[cfg(target_os = "linux")]
use std::sync::mpsc::{self, Sender};

/// What the user picked from the tray menu.
///
/// Only constructed on platforms that have a tray backend, so the variants read
/// as dead code on the others even though the bar still matches on them.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrayCommand {
    /// Open the settings window.
    ShowSettings,
    /// Hide the bar if visible, show it otherwise.
    ToggleBar,
    /// Quit the application.
    Quit,
}

/// Keeps the tray alive; dropping this removes the icon.
pub struct TrayHandle {
    commands: Receiver<TrayCommand>,
    #[cfg(target_os = "linux")]
    _service: ksni::blocking::Handle<linux::DofusTray>,
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
#[cfg(target_os = "linux")]
pub fn spawn(repaint: impl Fn() + Send + Sync + 'static) -> Option<TrayHandle> {
    use ksni::blocking::TrayMethods;

    let (sender, commands) = mpsc::channel();
    let tray = linux::DofusTray {
        sender,
        repaint: Box::new(repaint),
    };

    match tray.spawn() {
        Ok(service) => Some(TrayHandle {
            commands,
            _service: service,
        }),
        Err(error) => {
            tracing::warn!(%error, "no system tray available");
            None
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn spawn(_repaint: impl Fn() + Send + Sync + 'static) -> Option<TrayHandle> {
    tracing::debug!("the tray is not implemented on this platform yet");
    None
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{Sender, TrayCommand};
    use dofus_core::registry::OWN_APP_ID;
    use ksni::menu::{MenuItem, StandardItem};

    pub struct DofusTray {
        pub sender: Sender<TrayCommand>,
        /// Wakes the UI thread, which is otherwise idle waiting for events.
        pub repaint: Box<dyn Fn() + Send + Sync>,
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
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            super::embedded_icon().into_iter().collect()
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: "Dofus-tab".to_owned(),
                description: "Switch between Dofus clients".to_owned(),
                ..Default::default()
            }
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            vec![
                Self::item("Settings...", TrayCommand::ShowSettings),
                Self::item("Show/hide bar", TrayCommand::ToggleBar),
                MenuItem::Separator,
                Self::item("Quit", TrayCommand::Quit),
            ]
        }
    }
}

/// Decodes the embedded window icon into the tray's pixel format.
///
/// StatusNotifierItem wants ARGB32 in network byte order, while the decoder
/// hands back RGBA, so the channels are reordered here.
#[cfg(target_os = "linux")]
fn embedded_icon() -> Option<ksni::Icon> {
    let icon = eframe::icon_data::from_png_bytes(crate::ICON_PNG)
        .inspect_err(|error| tracing::warn!(%error, "could not decode the tray icon"))
        .ok()?;

    let data = icon
        .rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]])
        .collect();

    Some(ksni::Icon {
        width: icon.width as i32,
        height: icon.height as i32,
        data,
    })
}
