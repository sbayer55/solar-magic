//! Top-level application state, menu bar, and docking layout.

use std::path::PathBuf;

use egui_dock::{DockArea, DockState, NodeIndex};
use serde::{Deserialize, Serialize};
use smw_data::LevelId;
use smw_render::scene::RenderOptions;

use crate::document::Document;
use crate::panels::{Tab, Viewer};

/// Persisted between runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Settings {
    /// Last ROM opened.
    pub(crate) last_rom: Option<PathBuf>,
    /// Last level viewed.
    pub(crate) last_level: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self { last_rom: None, last_level: 0x105 }
    }
}

/// Per-session view toggles.
#[derive(Debug, Clone)]
pub(crate) struct ViewState {
    /// What the level view draws.
    pub(crate) options: RenderOptions,
    /// Level list filter text.
    pub(crate) filter: String,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            options: RenderOptions { layer1: true, layer2: true, sprites: true },
            filter: String::new(),
        }
    }
}

/// Command-line overrides for what to open at startup.
#[derive(Debug, Clone, Default)]
pub(crate) struct Launch {
    /// ROM to open instead of the remembered one.
    pub(crate) rom: Option<PathBuf>,
    /// Level to show instead of the remembered one.
    pub(crate) level: Option<u16>,
}

/// The application.
pub(crate) struct App {
    settings: Settings,
    view: ViewState,
    doc: Option<Document>,
    error: Option<String>,
    dock: DockState<Tab>,
}

const SETTINGS_KEY: &str = "solar-magic-settings";

impl App {
    /// Creates the app, opening the launch ROM or else the remembered one.
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, launch: Launch) -> Self {
        let mut settings: Settings = cc
            .storage
            .and_then(|s| s.get_string(SETTINGS_KEY))
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        if let Some(level) = launch.level {
            settings.last_level = level;
        }
        let path = launch.rom.or_else(|| settings.last_rom.clone());
        let mut app = Self {
            settings,
            view: ViewState::default(),
            doc: None,
            error: None,
            dock: default_layout(),
        };
        if let Some(path) = path {
            app.open(&path);
        }
        app
    }

    fn open(&mut self, path: &std::path::Path) {
        let level = LevelId::new(self.settings.last_level).unwrap_or(LevelId::MAX);
        match Document::open(path, level) {
            Ok(doc) => {
                self.doc = Some(doc);
                self.error = None;
                self.settings.last_rom = Some(path.to_owned());
            }
            Err(err) => self.error = Some(format!("{err:#}")),
        }
    }

    fn pick_rom(&mut self) {
        let picked = rfd::FileDialog::new().add_filter("SNES ROM", &["sfc", "smc"]).pick_file();
        if let Some(path) = picked {
            self.open(&path);
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open ROM…").clicked() {
                        ui.close();
                        self.pick_rom();
                    }
                    if ui.button("Quit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.view.options.layer1, "Layer 1");
                    ui.checkbox(&mut self.view.options.layer2, "Layer 2");
                    ui.checkbox(&mut self.view.options.sprites, "Sprite markers");
                    if ui.button("Reset layout").clicked() {
                        self.dock = default_layout();
                    }
                });
                if let Some(doc) = &self.doc {
                    ui.separator();
                    ui.label(format!("{}  —  level {}", doc.path.display(), doc.level));
                }
            });
        });
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.heading("Solar Magic");
                ui.label("Open a Super Mario World ROM to begin.");
                if ui.button("Open ROM…").clicked() {
                    self.pick_rom();
                }
            });
        });
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(doc) = &self.doc {
            self.settings.last_level = doc.level.get();
        }
        if let Ok(json) = serde_json::to_string(&self.settings) {
            storage.set_string(SETTINGS_KEY, json);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.menu_bar(ui);
        if let Some(error) = self.error.clone() {
            egui::Panel::top("error").show(ui, |ui| {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            });
        }
        let Some(doc) = self.doc.as_mut() else {
            self.welcome(ui);
            return;
        };
        doc.ensure_texture(ui.ctx(), self.view.options);
        doc.ensure_map16_texture(ui.ctx());
        let mut viewer = Viewer { doc, view: &mut self.view, select: None };
        DockArea::new(&mut self.dock).show_inside(ui, &mut viewer);
        if let Some(level) = viewer.select {
            doc.select(level);
        }
    }
}

/// Level view in the middle, level list on the left, info and Map16 on the right.
fn default_layout() -> DockState<Tab> {
    let mut state = DockState::new(vec![Tab::Level]);
    let surface = state.main_surface_mut();
    let [center, _left] = surface.split_left(NodeIndex::root(), 0.18, vec![Tab::Levels]);
    let [_, right] = surface.split_right(center, 0.72, vec![Tab::Info, Tab::Palette]);
    surface.split_below(right, 0.4, vec![Tab::Map16]);
    state
}
