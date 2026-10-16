//! The dockable panels.

pub(crate) mod camera;
mod debug;
mod exits;
mod header;
mod info;
mod level_view;
mod levels;
mod map16;
mod overlays;
mod palette;
mod play;
mod tools;

use egui_dock::TabViewer;
use smw_data::LevelId;

use crate::app::ViewState;
use crate::document::Document;
use crate::edit::EditCommand;

/// Panel kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    /// Zoomable level picture.
    Level,
    /// List of the 512 levels.
    Levels,
    /// Header and layout facts.
    Info,
    /// Tool choice and the selected item's properties.
    Edit,
    /// Primary and secondary header fields.
    Header,
    /// Screen exits and secondary exits.
    Exits,
    /// Map16 pages 0–1 for the level's tileset.
    Map16,
    /// The 16×16 palette.
    Palette,
    /// The level running in the emulator.
    Play,
    /// Registers, disassembly, breakpoints, watches, memory, trace.
    Debug,
}

/// Every tab, for the Window menu.
pub(crate) const ALL_TABS: [Tab; 10] = [
    Tab::Level,
    Tab::Levels,
    Tab::Info,
    Tab::Edit,
    Tab::Header,
    Tab::Exits,
    Tab::Map16,
    Tab::Palette,
    Tab::Play,
    Tab::Debug,
];

impl Tab {
    /// Display name.
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Level => "Level",
            Self::Levels => "Levels",
            Self::Info => "Info",
            Self::Edit => "Edit",
            Self::Header => "Header",
            Self::Exits => "Exits",
            Self::Map16 => "Map16",
            Self::Palette => "Palette",
            Self::Play => "Play",
            Self::Debug => "Debug",
        }
    }
}

/// Something a panel asked the app to do once the dock has been drawn.
#[derive(Debug)]
pub(crate) enum Action {
    /// Apply an undoable command.
    Apply(EditCommand),
    /// Apply a command merged into a drag group.
    ApplyGrouped(EditCommand, u64),
    /// Switch to another level (after a dirty check).
    SelectLevel(LevelId),
    /// Start playing the level at the entrance nearest to a tile.
    PlayFrom(smw_data::level::TilePos),
}

/// Mutable context handed to the dock while drawing.
pub(crate) struct Viewer<'a> {
    /// The open document.
    pub(crate) doc: &'a mut Document,
    /// View toggles.
    pub(crate) view: &'a mut ViewState,
    /// Requests collected while drawing.
    pub(crate) actions: Vec<Action>,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(format!("{tab:?}"))
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Level => self.level_view(ui),
            Tab::Levels => self.level_list(ui),
            Tab::Info => self.info(ui),
            Tab::Edit => self.edit_panel(ui),
            Tab::Header => self.header_panel(ui),
            Tab::Exits => self.exits_panel(ui),
            Tab::Map16 => self.map16(ui),
            Tab::Palette => self.palette(ui),
            Tab::Play => self.play_panel(ui),
            Tab::Debug => self.debug_panel(ui),
        }
    }
}

/// Converts a position in level pixels to a tile coordinate.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "float pixel position to tile index; negatives are filtered first"
)]
pub(crate) fn tile_under(local: egui::Vec2) -> Option<(usize, usize)> {
    if local.x < 0.0 || local.y < 0.0 {
        return None;
    }
    Some(((local.x / 16.0) as usize, (local.y / 16.0) as usize))
}

pub(crate) fn to_f32(v: u32) -> f32 {
    f32::from(u16::try_from(v).unwrap_or(u16::MAX))
}

/// A hex `DragValue` for a byte-sized field with an inclusive maximum.
pub(crate) fn hex_byte(ui: &mut egui::Ui, value: &mut u8, max: u8) -> bool {
    let mut v = u32::from(*value);
    let changed = ui
        .add(egui::DragValue::new(&mut v).range(0..=u32::from(max)).hexadecimal(2, false, true))
        .changed();
    if changed {
        *value = u8::try_from(v).unwrap_or(max).min(max);
    }
    changed
}

/// A hex `DragValue` for a 16-bit field.
pub(crate) fn hex_word(ui: &mut egui::Ui, value: &mut u16, max: u16) -> bool {
    let mut v = u32::from(*value);
    let changed = ui
        .add(egui::DragValue::new(&mut v).range(0..=u32::from(max)).hexadecimal(3, false, true))
        .changed();
    if changed {
        *value = u16::try_from(v).unwrap_or(max).min(max);
    }
    changed
}
