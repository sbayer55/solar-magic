//! Top-level application state, menu bar, shortcuts, modals and docking.

use std::path::{Path, PathBuf};

use egui::{Key, KeyboardShortcut, Modifiers};
use egui_dock::{DockArea, DockState, NodeIndex};
use serde::{Deserialize, Serialize};
use smw_data::LevelId;
use smw_data::level::TilePos;
use smw_render::scene::RenderOptions;

use crate::document::Document;
use crate::edit::EditCommand;
use crate::panels::{ALL_TABS, Action, Tab, Viewer};

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
    /// Overlay toggles.
    pub(crate) show_screen_grid: bool,
    /// 16×16 tile grid.
    pub(crate) show_tile_grid: bool,
    /// Screen exit boxes.
    pub(crate) show_exits: bool,
    /// Entrance and midway markers.
    pub(crate) show_entrances: bool,
    /// Counter for drag history groups.
    pub(crate) drag_group: u64,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            options: RenderOptions { layer1: true, layer2: true, sprites: true, owners: false },
            filter: String::new(),
            show_screen_grid: true,
            show_tile_grid: false,
            show_exits: true,
            show_entrances: true,
            drag_group: 0,
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

/// Something that must wait for the user to confirm discarding edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    OpenRom,
    SelectLevel(LevelId),
    Revert,
    Close,
}

/// A modal dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Modal {
    None,
    ConfirmDiscard(Pending),
    ConfirmExpand(PathBuf),
}

/// The application.
pub(crate) struct App {
    settings: Settings,
    view: ViewState,
    doc: Option<Document>,
    error: Option<String>,
    dock: DockState<Tab>,
    modal: Modal,
    close_confirmed: bool,
}

const SETTINGS_KEY: &str = "solar-magic-settings";

const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
const REDO: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);
const REDO_Y: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Y);
const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const COPY: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::C);
const CUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::X);
const PASTE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::V);
const SELECT_ALL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::A);

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
            modal: Modal::None,
            close_confirmed: false,
        };
        if let Some(path) = path {
            app.open(&path);
        }
        app
    }

    fn open(&mut self, path: &Path) {
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

    /// Runs `pending` now, or asks first when edits would be lost.
    fn request(&mut self, pending: Pending) {
        if self.doc.as_ref().is_some_and(Document::is_dirty) {
            self.modal = Modal::ConfirmDiscard(pending);
        } else {
            self.perform(pending);
        }
    }

    fn perform(&mut self, pending: Pending) {
        match pending {
            Pending::OpenRom => self.pick_rom(),
            Pending::SelectLevel(level) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.select(level);
                }
            }
            Pending::Revert => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.revert();
                }
            }
            Pending::Close => {
                self.close_confirmed = true;
            }
        }
    }

    fn save_rom(&mut self, ctx: &egui::Context, save_as: bool) {
        let Some(doc) = self.doc.as_ref() else {
            return;
        };
        let path = if save_as {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("SNES ROM", &["sfc", "smc"])
                .set_file_name(
                    doc.path
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                )
                .save_file()
            else {
                return;
            };
            path
        } else {
            doc.path.clone()
        };
        if doc.save_would_expand() {
            self.modal = Modal::ConfirmExpand(path);
        } else {
            self.save_to(ctx, &path);
        }
    }

    fn save_to(&mut self, ctx: &egui::Context, path: &Path) {
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        match doc.save_to(path) {
            Ok(report) => {
                doc.status = match report {
                    Some(r) if r.expanded => format!(
                        "saved {} (ROM expanded to {} KiB)",
                        path.display(),
                        doc.rom.len() / 1024
                    ),
                    _ => format!("saved {}", path.display()),
                };
                self.settings.last_rom = Some(path.to_owned());
                self.error = None;
            }
            Err(err) => self.error = Some(format!("{err:#}")),
        }
        if self.close_confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn edit_menu_action(&mut self, action: EditAction) {
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        match action {
            EditAction::Undo => doc.undo(),
            EditAction::Redo => doc.redo(),
            EditAction::Copy | EditAction::Cut => {
                let Some(session) = doc.session.as_mut() else {
                    return;
                };
                let selection = session.selection.clone();
                session.clipboard.copy(&session.level, &selection);
                if action == EditAction::Cut {
                    delete_selection(doc);
                }
            }
            EditAction::Paste => {
                let Some(session) = doc.session.as_ref() else {
                    return;
                };
                let at = TilePos::new(0, 0);
                for command in session.clipboard.paste_commands(at) {
                    doc.apply(command, None);
                }
            }
            EditAction::Delete => delete_selection(doc),
            EditAction::SelectAll => {
                if let Some(session) = doc.session.as_mut() {
                    let layer = session.selection.layer;
                    session.selection.objects = session
                        .level
                        .stream(layer)
                        .map(|s| {
                            s.objects
                                .iter()
                                .enumerate()
                                .filter(|(_, o)| !o.is_jump())
                                .map(|(i, _)| i)
                                .collect()
                        })
                        .unwrap_or_default();
                    session.selection.sprites = (0..session.level.sprites.sprites.len()).collect();
                }
            }
            EditAction::Deselect => {
                if let Some(session) = doc.session.as_mut() {
                    session.selection.clear();
                    session.tool = crate::edit::Tool::Select;
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.modal != Modal::None {
            return;
        }
        let mut pressed = Vec::new();
        ctx.input_mut(|i| {
            if i.consume_shortcut(&REDO) || i.consume_shortcut(&REDO_Y) {
                pressed.push(EditAction::Redo);
            }
            if i.consume_shortcut(&UNDO) {
                pressed.push(EditAction::Undo);
            }
            if i.consume_shortcut(&COPY) {
                pressed.push(EditAction::Copy);
            }
            if i.consume_shortcut(&CUT) {
                pressed.push(EditAction::Cut);
            }
            if i.consume_shortcut(&PASTE) {
                pressed.push(EditAction::Paste);
            }
            if i.consume_shortcut(&SELECT_ALL) {
                pressed.push(EditAction::SelectAll);
            }
            if i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace) {
                pressed.push(EditAction::Delete);
            }
            if i.key_pressed(Key::Escape) {
                pressed.push(EditAction::Deselect);
            }
        });
        let save = ctx.input_mut(|i| i.consume_shortcut(&SAVE));
        for action in pressed {
            self.edit_menu_action(action);
        }
        if save {
            self.save_rom(ctx, false);
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                self.file_menu(ui);
                self.edit_menu(ui);
                self.view_menu(ui);
                self.window_menu(ui);
                if let Some(doc) = &self.doc {
                    ui.separator();
                    let dirty = if doc.is_dirty() { "*" } else { "" };
                    ui.label(format!("{}{dirty}  —  level {}", doc.path.display(), doc.level));
                }
            });
        });
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        let has_doc = self.doc.is_some();
        ui.menu_button("File", |ui| {
            if ui.button("Open ROM…").clicked() {
                ui.close();
                self.request(Pending::OpenRom);
            }
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Save").shortcut_text(ui.ctx().format_shortcut(&SAVE)),
                )
                .clicked()
            {
                ui.close();
                self.save_rom(ui.ctx(), false);
            }
            if ui.add_enabled(has_doc, egui::Button::new("Save As…")).clicked() {
                ui.close();
                self.save_rom(ui.ctx(), true);
            }
            if ui.add_enabled(has_doc, egui::Button::new("Revert level")).clicked() {
                ui.close();
                self.request(Pending::Revert);
            }
            ui.separator();
            if ui.button("Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui) {
        let (undo, redo) = self
            .doc
            .as_ref()
            .and_then(|d| d.session.as_ref())
            .map_or((None, None), |s| (s.history.undo_label(), s.history.redo_label()));
        let has_selection = self
            .doc
            .as_ref()
            .and_then(|d| d.session.as_ref())
            .is_some_and(|s| !s.selection.is_empty());
        let has_clipboard = self
            .doc
            .as_ref()
            .and_then(|d| d.session.as_ref())
            .is_some_and(|s| !s.clipboard.is_empty());
        ui.menu_button("Edit", |ui| {
            const DELETE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Delete);
            let items: [MenuItem; 7] = [
                ("Undo", undo, undo.is_some(), &UNDO, EditAction::Undo),
                ("Redo", redo, redo.is_some(), &REDO, EditAction::Redo),
                ("Cut", None, has_selection, &CUT, EditAction::Cut),
                ("Copy", None, has_selection, &COPY, EditAction::Copy),
                ("Paste", None, has_clipboard, &PASTE, EditAction::Paste),
                ("Delete", None, has_selection, &DELETE, EditAction::Delete),
                ("Select all", None, true, &SELECT_ALL, EditAction::SelectAll),
            ];
            for (name, detail, enabled, shortcut, action) in items {
                let text = detail.map_or_else(|| name.to_owned(), |d| format!("{name} {d}"));
                let button =
                    egui::Button::new(text).shortcut_text(ui.ctx().format_shortcut(shortcut));
                if ui.add_enabled(enabled, button).clicked() {
                    ui.close();
                    self.edit_menu_action(action);
                }
            }
        });
    }

    fn view_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("View", |ui| {
            ui.checkbox(&mut self.view.options.layer1, "Layer 1");
            ui.checkbox(&mut self.view.options.layer2, "Layer 2");
            ui.checkbox(&mut self.view.options.sprites, "Sprite markers");
            ui.checkbox(&mut self.view.options.owners, "Tint by object");
            ui.separator();
            ui.checkbox(&mut self.view.show_screen_grid, "Screen boundaries");
            ui.checkbox(&mut self.view.show_tile_grid, "Tile grid");
            ui.checkbox(&mut self.view.show_exits, "Screen exits");
            ui.checkbox(&mut self.view.show_entrances, "Entrances");
        });
    }

    fn window_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Window", |ui| {
            for tab in ALL_TABS {
                let open = self.dock.find_tab(&tab).is_some();
                if ui.selectable_label(open, tab.title()).clicked() {
                    if let Some(location) = self.dock.find_tab(&tab) {
                        self.dock.remove_tab(location);
                    } else {
                        self.dock.push_to_focused_leaf(tab);
                    }
                }
            }
            ui.separator();
            if ui.button("Reset layout").clicked() {
                self.dock = default_layout();
            }
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

    fn modal(&mut self, ctx: &egui::Context) {
        match self.modal.clone() {
            Modal::None => {}
            Modal::ConfirmDiscard(pending) => {
                let mut choice = None;
                egui::Window::new("Unsaved changes").collapsible(false).resizable(false).show(
                    ctx,
                    |ui| {
                        ui.label("The level has unsaved edits.");
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() {
                                choice = Some(1);
                            }
                            if ui.button("Discard").clicked() {
                                choice = Some(2);
                            }
                            if ui.button("Cancel").clicked() {
                                choice = Some(0);
                            }
                        });
                    },
                );
                match choice {
                    Some(1) => {
                        self.modal = Modal::None;
                        if pending == Pending::Close {
                            self.close_confirmed = true;
                        }
                        self.save_rom(ctx, false);
                        if pending != Pending::Close
                            && !self.doc.as_ref().is_some_and(Document::is_dirty)
                        {
                            self.perform(pending);
                        }
                    }
                    Some(2) => {
                        self.modal = Modal::None;
                        if let Some(doc) = self.doc.as_mut() {
                            doc.revert();
                        }
                        self.perform(pending);
                        if self.close_confirmed {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    Some(_) => self.modal = Modal::None,
                    None => {}
                }
            }
            Modal::ConfirmExpand(path) => {
                let mut choice = None;
                egui::Window::new("Expand ROM?").collapsible(false).resizable(false).show(
                    ctx,
                    |ui| {
                        ui.label("Saving a changed level moves its data into expanded space.");
                        ui.label("The ROM will be expanded to 1 MiB (as Lunar Magic does).");
                        ui.horizontal(|ui| {
                            if ui.button("Expand and save").clicked() {
                                choice = Some(true);
                            }
                            if ui.button("Cancel").clicked() {
                                choice = Some(false);
                            }
                        });
                    },
                );
                match choice {
                    Some(true) => {
                        self.modal = Modal::None;
                        self.save_to(ctx, &path);
                    }
                    Some(false) => {
                        self.modal = Modal::None;
                        self.close_confirmed = false;
                    }
                    None => {}
                }
            }
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        if self.close_confirmed || !self.doc.as_ref().is_some_and(Document::is_dirty) {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        self.modal = Modal::ConfirmDiscard(Pending::Close);
    }
}

/// One Edit-menu row: name, detail, enabled, shortcut, action.
type MenuItem = (&'static str, Option<&'static str>, bool, &'static KeyboardShortcut, EditAction);

/// Edit-menu commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
    Deselect,
}

fn delete_selection(doc: &mut Document) {
    let Some(session) = doc.session.as_ref() else {
        return;
    };
    let selection = session.selection.clone();
    if !selection.objects.is_empty() {
        doc.apply(
            EditCommand::DeleteObjects {
                layer: selection.layer,
                indices: selection.object_indices(),
            },
            None,
        );
    }
    if !selection.sprites.is_empty() {
        doc.apply(EditCommand::DeleteSprites { indices: selection.sprite_indices() }, None);
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
        let ctx = ui.ctx().clone();
        self.handle_close_request(&ctx);
        self.shortcuts(&ctx);
        self.menu_bar(ui);
        if let Some(error) = self.error.clone() {
            egui::Panel::top("error").show(ui, |ui| {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            });
        }
        self.modal(&ctx);
        let Some(doc) = self.doc.as_mut() else {
            self.welcome(ui);
            return;
        };
        doc.ensure_texture(&ctx, self.view.options);
        doc.ensure_map16_texture(&ctx);
        let mut viewer = Viewer { doc, view: &mut self.view, actions: Vec::new() };
        DockArea::new(&mut self.dock).show_inside(ui, &mut viewer);
        let actions = viewer.actions;
        let mut select = None;
        for action in actions {
            match action {
                Action::Apply(command) => doc.apply(command, None),
                Action::ApplyGrouped(command, group) => doc.apply(command, Some(group)),
                Action::SelectLevel(level) => select = Some(level),
            }
        }
        if let Some(level) = select {
            self.request(Pending::SelectLevel(level));
        }
    }
}

/// Level view in the middle, level list on the left, editing panels on the right.
fn default_layout() -> DockState<Tab> {
    let mut state = DockState::new(vec![Tab::Level]);
    let surface = state.main_surface_mut();
    let [center, _left] = surface.split_left(NodeIndex::root(), 0.16, vec![Tab::Levels]);
    let [_, right] =
        surface.split_right(center, 0.70, vec![Tab::Edit, Tab::Header, Tab::Exits, Tab::Info]);
    surface.split_below(right, 0.55, vec![Tab::Map16, Tab::Palette]);
    state
}
