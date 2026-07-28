//! Always-on-top tab bar for Dofus-tab.
//!
//! Runs the same core as the CLI, plus a small always-on-top window and the
//! global hotkey listener, so a single process covers both ways of switching.

mod app;
mod engine;
mod settings;
mod tray;

use anyhow::{Context, Result};
use dofus_core::config::Config;
use eframe::egui;
use tracing_subscriber::EnvFilter;

/// Width used before the first resize; the bar fits itself to its content.
const INITIAL_WIDTH: f32 = 480.0;

/// Window icon, embedded so the binary needs no data files beside it.
pub const ICON_PNG: &[u8] = include_bytes!(
    "../../../packaging/icons/hicolor/256x256/apps/io.github.gambl3r08.DofusTab.png"
);

fn main() -> Result<()> {
    init_tracing();

    let config = Config::load().context("could not read the Dofus-tab config")?;

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Dofus-tab")
        // Must match the installed .desktop file name, which is how the
        // desktop associates the window with its icon and its shortcuts.
        .with_app_id(dofus_core::registry::OWN_APP_ID)
        .with_inner_size([INITIAL_WIDTH, app::INITIAL_HEIGHT])
        .with_decorations(false)
        .with_always_on_top()
        .with_resizable(false)
        .with_transparent(true)
        // An overlay does not belong in the task switcher.
        .with_taskbar(false);

    if let Some(position) = config.overlay_position {
        viewport = viewport.with_position(position);
    }

    match eframe::icon_data::from_png_bytes(ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(error) => tracing::warn!(%error, "could not decode the window icon"),
    }

    let native_options = eframe::NativeOptions {
        viewport,
        event_loop_builder: Some(Box::new(force_x11)),
        ..Default::default()
    };

    eframe::run_native(
        "Dofus-tab",
        native_options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let engine = engine::spawn({
                let ctx = ctx.clone();
                move || ctx.request_repaint()
            });
            engine::spawn_hotkeys(engine.clone());

            let tray = tray::spawn({
                let ctx = ctx.clone();
                move || ctx.request_repaint()
            });

            // winit maps `WindowLevel::AlwaysOnTop` to nothing usable on this
            // window manager, so the hints are applied through our own EWMH
            // backend instead.
            match own_window_id(cc) {
                Some(window) => engine.send(engine::Command::MakeOverlay(window)),
                None => tracing::warn!("could not read our own window handle"),
            }

            Ok(Box::new(app::BarApp::new(engine, config, tray)))
        }),
    )
    .map_err(|error| anyhow::anyhow!("could not start the overlay: {error}"))
}

/// Reads the native handle of the bar's own window.
///
/// Returns `None` on platforms whose handle the backends do not understand yet.
fn own_window_id(cc: &eframe::CreationContext<'_>) -> Option<dofus_core::WindowId> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = cc.window_handle().ok()?;
    match handle.as_raw() {
        // The Xlib window type is 32 bits wide on some targets and 64 on
        // others, so the cast is needed on one and redundant on the other.
        // There is no spelling that satisfies both, hence the allow.
        #[allow(clippy::unnecessary_cast)]
        RawWindowHandle::Xlib(xlib) => Some(dofus_core::WindowId(xlib.window as u64)),
        RawWindowHandle::Xcb(xcb) => Some(dofus_core::WindowId(u64::from(xcb.window.get()))),
        RawWindowHandle::Win32(win32) => Some(dofus_core::WindowId(win32.hwnd.get() as u64)),
        other => {
            tracing::debug!(?other, "unsupported window handle");
            None
        }
    }
}

/// Forces the window onto X11 rather than native Wayland.
///
/// Wayland has no standard way for a regular toplevel to stay above others, so
/// a native-Wayland bar would sink behind the game. Going through XWayland lets
/// the window manager honour `_NET_WM_STATE_ABOVE`, and it costs nothing here:
/// the Dofus client is an XWayland client too.
#[cfg(target_os = "linux")]
fn force_x11(builder: &mut eframe::EventLoopBuilder<eframe::UserEvent>) {
    use winit::platform::x11::EventLoopBuilderExtX11;
    builder.with_x11();
}

#[cfg(not(target_os = "linux"))]
fn force_x11(_builder: &mut eframe::EventLoopBuilder<eframe::UserEvent>) {
    // Other platforms expose a real always-on-top flag, so nothing to force.
}

/// Logs to stderr; the bar has no room for diagnostics.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .init();
}
