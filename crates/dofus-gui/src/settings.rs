//! Settings window, opened from the tray menu.
//!
//! Shown as a second egui viewport rather than a separate process, so it shares
//! the loaded config and can apply layout changes to the bar as the user drags
//! the sliders.
//!
//! Hotkeys are deliberately read-only here. They belong to the desktop once
//! registered, and on Linux the portal will not let an application reassign
//! them — pointing the user at their system settings is honest, whereas an
//! editable field that silently does nothing would not be.

use dofus_core::config::Config;
use dofus_core::hotkey::RegisteredShortcut;
use dofus_core::layout::{self, Orientation};
use eframe::egui;

/// Bounds of the wrap slider; past eight the account is not a Dofus party.
const MIN_WRAP: u32 = 1;
const MAX_WRAP: u32 = 8;

#[derive(Default)]
pub struct SettingsWindow {
    open: bool,
    /// Set once the user changes something, cleared when it is written out.
    dirty: bool,
    /// Filled in by the engine; empty until hotkeys have been registered.
    shortcuts: Vec<RegisteredShortcut>,
}

/// What the bar should do after the settings window was drawn.
pub struct SettingsOutcome {
    /// Layout changed, so the bar must re-lay itself out.
    pub layout_changed: bool,
}

impl SettingsWindow {
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn set_shortcuts(&mut self, shortcuts: Vec<RegisteredShortcut>) {
        self.shortcuts = shortcuts;
    }

    /// Draws the window if open, applying edits straight onto `config`.
    pub fn show(&mut self, ctx: &egui::Context, config: &mut Config) -> SettingsOutcome {
        let mut outcome = SettingsOutcome {
            layout_changed: false,
        };
        if !self.open {
            return outcome;
        }

        let viewport = egui::ViewportBuilder::default()
            .with_title("Dofus-tab settings")
            .with_inner_size([460.0, 420.0])
            .with_resizable(true);

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("dofus-tab-settings"),
            viewport,
            |ctx, _class| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    outcome.layout_changed = self.draw(ui, config);
                });

                // The window manager close button, as opposed to our own.
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.open = false;
                }
            },
        );

        outcome
    }

    /// Returns true when a layout field changed.
    fn draw(&mut self, ui: &mut egui::Ui, config: &mut Config) -> bool {
        let (mut orientation, mut wrap_at) = config.overlay_layout();
        let before = (orientation, wrap_at);

        ui.heading("Layout");
        ui.add_space(4.0);

        ui.label("Direction the bar fills before wrapping:");
        ui.horizontal(|ui| {
            ui.radio_value(&mut orientation, Orientation::Vertical, "Vertical (narrow)");
            ui.radio_value(
                &mut orientation,
                Orientation::Horizontal,
                "Horizontal (short)",
            );
        });

        ui.add_space(6.0);
        ui.add(egui::Slider::new(&mut wrap_at, MIN_WRAP..=MAX_WRAP).text("Tabs before wrapping"));

        let grid = layout::grid(MAX_WRAP as usize, orientation, wrap_at);
        ui.weak(format!(
            "With {MAX_WRAP} accounts: {} columns x {} rows",
            grid.columns, grid.rows
        ));

        let changed = (orientation, wrap_at) != before;
        if changed {
            config.overlay_orientation = Some(orientation);
            config.overlay_wrap_at = Some(wrap_at);
            self.dirty = true;
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(6.0);

        ui.heading("Hotkeys");
        ui.add_space(4.0);
        self.draw_shortcuts(ui);

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.dirty, egui::Button::new("Save"))
                .clicked()
            {
                match config.save() {
                    Ok(()) => self.dirty = false,
                    Err(error) => tracing::warn!(%error, "could not save the config"),
                }
            }
            if ui.button("Close").clicked() {
                self.open = false;
            }
            if self.dirty {
                ui.weak("unsaved changes");
            }
        });

        changed
    }

    fn draw_shortcuts(&self, ui: &mut egui::Ui) {
        if self.shortcuts.is_empty() {
            ui.weak("No hotkeys registered yet.");
            return;
        }

        let inert = self.shortcuts.iter().filter(|s| !s.is_active()).count();

        egui::Grid::new("hotkeys")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                for shortcut in &self.shortcuts {
                    let dofus_core::hotkey::Action::FocusSlot(slot) = shortcut.action;
                    ui.label(format!("Tab {slot}"));
                    match &shortcut.trigger {
                        Some(trigger) => ui.monospace(trigger),
                        None => ui.colored_label(ui.visuals().warn_fg_color, "not assigned"),
                    };
                    ui.end_row();
                }
            });

        ui.add_space(6.0);
        if inert > 0 {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("{inert} shortcut(s) have no key and will never fire."),
            );
        }
        ui.weak(
            "Hotkeys are owned by the desktop. Change them in your system's keyboard settings.",
        );
    }
}
