//! The overlay bar itself.
//!
//! Deliberately minimal: one button per detected client arranged in a grid, the
//! active one highlighted, and a grip to drag the whole thing around.
//! Everything expensive happens on the engine thread, so `update` only reads a
//! snapshot.

use std::time::{Duration, Instant};

use dofus_core::config::Config;
use dofus_core::layout::{self, Orientation};
use eframe::egui;

use crate::engine::{Command, EngineHandle};
use crate::settings::{SettingsWindow, TabRow};
use crate::tray::{TrayCommand, TrayHandle};

/// Height of one tab button.
const BUTTON_HEIGHT: f32 = 26.0;
/// Width of one tab button.
const BUTTON_WIDTH: f32 = 150.0;
/// Width reserved for the drag grip on the left.
const GRIP_WIDTH: f32 = 14.0;
/// Padding inside the bar.
const MARGIN: f32 = 4.0;
/// Space between buttons.
const SPACING: f32 = 4.0;
/// Width used when there is nothing to show.
const EMPTY_WIDTH: f32 = 220.0;
/// How long the bar must sit still before its position is written out.
///
/// Saving on every frame of a drag would hammer the disk; saving only on exit
/// would lose the position whenever the process is killed rather than quit.
const POSITION_SETTLE: Duration = Duration::from_millis(800);

/// Height of the window before the first layout pass.
pub const INITIAL_HEIGHT: f32 = BUTTON_HEIGHT + MARGIN * 2.0;

/// One tab, flattened out of the engine snapshot so the mutex is not held
/// while drawing.
struct TabView {
    slot: u32,
    label: String,
    is_active: bool,
}

pub struct BarApp {
    engine: EngineHandle,
    config: Config,
    orientation: Orientation,
    wrap_at: u32,
    /// Last size we asked the window manager for, to avoid resizing every frame.
    requested_size: egui::Vec2,
    /// Position observed on the previous frame.
    position: Option<[f32; 2]>,
    /// When the position last changed, so a move can be saved once it settles.
    moved_at: Option<Instant>,
    /// `None` when the desktop offers no system tray.
    tray: Option<TrayHandle>,
    settings: SettingsWindow,
    /// Whether the bar itself is shown; toggled from the tray.
    visible: bool,
}

impl BarApp {
    pub fn new(engine: EngineHandle, config: Config, tray: Option<TrayHandle>) -> Self {
        let (orientation, wrap_at) = config.overlay_layout();
        Self {
            engine,
            config,
            orientation,
            wrap_at,
            requested_size: egui::Vec2::ZERO,
            position: None,
            moved_at: None,
            tray,
            settings: SettingsWindow::default(),
            visible: true,
        }
    }

    /// Applies whatever the user picked from the tray menu.
    fn handle_tray(&mut self, ctx: &egui::Context) {
        let Some(tray) = &self.tray else {
            return;
        };

        for command in tray.drain() {
            match command {
                TrayCommand::ShowSettings => {
                    // Seed both lists on open: the engine only learns the
                    // shortcuts once registration has happened, and the tabs
                    // change as clients come and go.
                    let (shortcuts, tabs) = self.engine.with_state(|state| {
                        let tabs = state
                            .tabs
                            .iter()
                            .filter_map(|tab| {
                                Some(TabRow {
                                    account: tab.account?,
                                    label: tab.label.clone(),
                                })
                            })
                            .collect();
                        (state.shortcuts.clone(), tabs)
                    });
                    self.settings.set_shortcuts(shortcuts);
                    self.settings.set_tabs(tabs);
                    self.settings.open();
                }
                TrayCommand::ToggleBar => {
                    self.visible = !self.visible;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(self.visible));
                }
                TrayCommand::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
    }

    /// Resizes the window so it fits the grid exactly.
    fn fit_to_content(&mut self, ctx: &egui::Context, grid: layout::Grid) {
        let size = if grid.is_empty() {
            egui::vec2(
                GRIP_WIDTH + MARGIN * 3.0 + EMPTY_WIDTH,
                BUTTON_HEIGHT + MARGIN * 2.0,
            )
        } else {
            let columns = grid.columns as f32;
            let rows = grid.rows as f32;
            egui::vec2(
                GRIP_WIDTH + MARGIN * 3.0 + columns * BUTTON_WIDTH + (columns - 1.0) * SPACING,
                MARGIN * 2.0 + rows * BUTTON_HEIGHT + (rows - 1.0) * SPACING,
            )
        };

        if (size - self.requested_size).length() > f32::EPSILON {
            self.requested_size = size;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }
    }

    /// Remembers where the user dragged the bar, saving once it settles.
    fn track_position(&mut self, ctx: &egui::Context) {
        let Some(rect) = ctx.input(|i| i.viewport().outer_rect) else {
            return;
        };
        let position = [rect.min.x, rect.min.y];

        if self.position != Some(position) {
            self.position = Some(position);
            self.moved_at = Some(Instant::now());
            // Wake up once the window has stopped moving, so the save happens
            // even if nothing else asks for a repaint.
            ctx.request_repaint_after(POSITION_SETTLE);
            return;
        }

        let Some(moved_at) = self.moved_at else {
            return;
        };
        if moved_at.elapsed() >= POSITION_SETTLE {
            self.moved_at = None;
            self.save_position();
        }
    }

    /// Writes the current position out, unless it is already what we stored.
    fn save_position(&mut self) {
        let Some(position) = self.position else {
            return;
        };
        if self.config.overlay_position == Some(position) {
            return;
        }

        self.config.overlay_position = Some(position);
        if let Err(error) = self.config.save() {
            tracing::warn!(%error, "could not save the overlay position");
        }
    }

    /// A dotted strip that drags the window, since the bar has no title bar.
    ///
    /// Spans the full grid height so it stays grabbable in a multi-row layout.
    fn draw_grip(ui: &mut egui::Ui, height: f32) {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(GRIP_WIDTH, height), egui::Sense::drag());

        let colour = ui.visuals().weak_text_color();
        let centre = rect.center();
        for offset in [-5.0, 0.0, 5.0] {
            ui.painter()
                .circle_filled(egui::pos2(centre.x, centre.y + offset), 1.5, colour);
        }

        if response.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        response.on_hover_text("Drag to move the bar");
    }

    /// Draws one tab, returning true when it was clicked.
    fn draw_tab(ui: &mut egui::Ui, tab: &TabView) -> bool {
        let text = format!("{}  {}", tab.slot, tab.label);
        let mut button = egui::Button::new(egui::RichText::new(text).strong())
            .min_size(egui::vec2(BUTTON_WIDTH, BUTTON_HEIGHT))
            .truncate();

        if tab.is_active {
            button = button.fill(ui.visuals().selection.bg_fill);
        }

        ui.add(button)
            .on_hover_text(format!("Focus {} (slot {})", tab.label, tab.slot))
            .clicked()
    }

    /// Lays the tabs out row by row, leaving gaps where the grid is ragged.
    fn draw_grid(&self, ui: &mut egui::Ui, tabs: &[TabView], grid: layout::Grid) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(SPACING, SPACING);

            for row in 0..grid.rows {
                ui.horizontal(|ui| {
                    for column in 0..grid.columns {
                        let index = layout::index_at(
                            column,
                            row,
                            tabs.len(),
                            self.orientation,
                            self.wrap_at,
                        );

                        match index.and_then(|index| tabs.get(index)) {
                            Some(tab) => {
                                if Self::draw_tab(ui, tab) {
                                    self.engine.send(Command::Focus(tab.slot));
                                }
                            }
                            // Keep later columns aligned across ragged rows.
                            None => {
                                ui.allocate_exact_size(
                                    egui::vec2(BUTTON_WIDTH, BUTTON_HEIGHT),
                                    egui::Sense::hover(),
                                );
                            }
                        }
                    }
                });
            }
        });
    }
}

impl eframe::App for BarApp {
    /// Transparent background so the rounded frame does not sit on a grey box.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_tray(ctx);

        let outcome = self.settings.show(ctx, &mut self.config);
        if outcome.layout_changed {
            let (orientation, wrap_at) = self.config.overlay_layout();
            self.orientation = orientation;
            self.wrap_at = wrap_at;
        }
        if outcome.config_changed {
            // The worker builds the tab list, so it needs the new labels and
            // order straight away rather than at the next restart.
            self.engine
                .send(Command::UpdateConfig(Box::new(self.config.clone())));
        }

        self.track_position(ctx);

        // Snapshot first, so the mutex is released before any drawing.
        let (tabs, error) = self.engine.with_state(|state| {
            let tabs: Vec<TabView> = state
                .tabs
                .iter()
                .map(|tab| TabView {
                    slot: tab.slot,
                    label: tab.label.clone(),
                    is_active: state.active == Some(tab.window),
                })
                .collect();
            (tabs, state.error.clone())
        });

        let grid = layout::grid(tabs.len(), self.orientation, self.wrap_at);
        self.fit_to_content(ctx, grid);

        let frame = egui::Frame::NONE
            .fill(ctx.style().visuals.window_fill)
            .inner_margin(MARGIN)
            .corner_radius(6.0);

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            let grip_height = (self.requested_size.y - MARGIN * 2.0).max(BUTTON_HEIGHT);

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = SPACING;
                Self::draw_grip(ui, grip_height);

                if let Some(error) = &error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    return;
                }

                if tabs.is_empty() {
                    ui.weak("No Dofus windows detected");
                    return;
                }

                self.draw_grid(ui, &tabs, grid);
            });
        });
    }

    /// Catches a move that had not settled yet when the user quit.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_position();
    }
}
