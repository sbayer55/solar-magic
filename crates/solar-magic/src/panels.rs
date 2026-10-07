//! The dockable panels.

use egui_dock::TabViewer;
use smw_data::LevelId;
use smw_data::level::Layer2;

use crate::app::ViewState;
use crate::document::Document;

/// Panel kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    /// Zoomable level picture.
    Level,
    /// List of the 512 levels.
    Levels,
    /// Header and layout facts.
    Info,
    /// Map16 pages 0–1 for the level's tileset.
    Map16,
    /// The 16×16 palette.
    Palette,
}

/// Mutable context handed to the dock while drawing.
pub(crate) struct Viewer<'a> {
    /// The open document.
    pub(crate) doc: &'a mut Document,
    /// View toggles.
    pub(crate) view: &'a mut ViewState,
    /// A level the user clicked this frame.
    pub(crate) select: Option<LevelId>,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(format!("{tab:?}"))
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::Level => "Level".into(),
            Tab::Levels => "Levels".into(),
            Tab::Info => "Info".into(),
            Tab::Map16 => "Map16".into(),
            Tab::Palette => "Palette".into(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Level => self.level_view(ui),
            Tab::Levels => self.level_list(ui),
            Tab::Info => self.info(ui),
            Tab::Map16 => self.map16(ui),
            Tab::Palette => self.palette(ui),
        }
    }
}

/// Converts a hover position in level pixels to a tile coordinate.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "float pixel position to tile index; negatives are filtered first"
)]
fn tile_under(local: egui::Vec2) -> Option<(usize, usize)> {
    if local.x < 0.0 || local.y < 0.0 {
        return None;
    }
    Some(((local.x / 16.0) as usize, (local.y / 16.0) as usize))
}

fn to_f32(v: u32) -> f32 {
    f32::from(u16::try_from(v).unwrap_or(u16::MAX))
}

impl Viewer<'_> {
    #[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
    fn level_view(&self, ui: &mut egui::Ui) {
        let Ok(scene) = &self.doc.scene else {
            if let Err(err) = &self.doc.scene {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
            }
            return;
        };
        let Some(texture) = &self.doc.texture else {
            return;
        };
        let (w, h) = scene.pixel_size();
        let size = egui::vec2(to_f32(w), to_f32(h));
        let rect_id = egui::Id::new("scene_rect");
        let mut scene_rect =
            ui.data_mut(|d| d.get_persisted::<egui::Rect>(rect_id)).unwrap_or(egui::Rect::ZERO);
        let mut hovered: Option<(usize, usize)> = None;
        egui::Panel::bottom("status").show(ui, |ui| {
            let status =
                ui.data(|d| d.get_temp::<String>(egui::Id::new("status_text"))).unwrap_or_default();
            ui.label(if status.is_empty() {
                "drag to pan, scroll to zoom".to_owned()
            } else {
                status
            });
        });
        egui::Scene::new().zoom_range(0.25..=8.0).show(ui, &mut scene_rect, |ui| {
            let response = ui
                .add(egui::Image::new(texture).fit_to_exact_size(size).sense(egui::Sense::hover()));
            if let Some(pos) = response.hover_pos() {
                hovered = tile_under(pos - response.rect.min);
            }
        });
        ui.data_mut(|d| d.insert_persisted(rect_id, scene_rect));
        let status = hovered
            .and_then(|(x, y)| {
                scene
                    .layer1_tile(x, y)
                    .map(|t| format!("tile ({x}, {y})  screen {:X}  Map16 {t:03X}", x / 16))
            })
            .unwrap_or_default();
        ui.data_mut(|d| d.insert_temp(egui::Id::new("status_text"), status));
    }

    fn level_list(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.text_edit_singleline(&mut self.view.filter);
        });
        let filter = self.view.filter.trim().to_uppercase();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for id in LevelId::all() {
                let label = format!("{id}");
                if !filter.is_empty() && !label.contains(&filter) {
                    continue;
                }
                if ui.selectable_label(id == self.doc.level, &label).clicked() {
                    self.select = Some(id);
                }
            }
        });
    }

    fn info(&self, ui: &mut egui::Ui) {
        let Ok(scene) = &self.doc.scene else {
            return;
        };
        let level = &scene.loaded.level;
        let h = &level.header;
        let s = &level.secondary;
        let layer2 = match &level.layer2 {
            Layer2::Objects { stream, .. } => {
                format!("{} objects at {}", stream.objects.len(), level.addresses.layer2)
            }
            Layer2::Background { page, .. } => {
                format!("background at {} (page {page})", level.addresses.layer2)
            }
        };
        let rows: [(&str, String); 13] = [
            ("Level", level.id.to_string()),
            ("Layer 1 data", level.addresses.layer1.to_string()),
            ("Layer 2", layer2),
            ("Sprites", format!("{} at {}", level.sprites.sprites.len(), level.addresses.sprites)),
            (
                "Screens",
                format!(
                    "{} ({})",
                    h.screens(),
                    if scene.loaded.vertical { "vertical" } else { "horizontal" }
                ),
            ),
            ("Level mode", format!("{:#04X}", h.level_mode().get())),
            ("Tileset / sprite set", format!("{} / {}", h.tileset(), h.sprite_set())),
            (
                "Palettes fg/bg/sp/back",
                format!(
                    "{} / {} / {} / {}",
                    h.foreground_palette(),
                    h.background_palette(),
                    h.sprite_palette(),
                    h.back_area_color()
                ),
            ),
            ("Music / time", format!("{} / {}s", h.music(), h.time_limit_seconds())),
            (
                "Entrance",
                format!(
                    "screen {} x{} y{} type {}",
                    s.entrance_screen(),
                    s.entrance_x_index(),
                    s.entrance_y_index(),
                    s.entrance_type()
                ),
            ),
            ("Midway screen", s.midway_screen().to_string()),
            ("Layer 1 objects", level.layer1.objects.len().to_string()),
            ("Loader instructions", scene.loaded.steps.to_string()),
        ];
        egui::Grid::new("info").num_columns(2).striped(true).show(ui, |ui| {
            for (k, v) in rows {
                ui.label(k);
                ui.label(v);
                ui.end_row();
            }
        });
    }

    fn map16(&self, ui: &mut egui::Ui) {
        let Some(texture) = &self.doc.map16_texture else {
            return;
        };
        egui::ScrollArea::both().show(ui, |ui| {
            ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(512.0, 1024.0)));
        });
    }

    #[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
    fn palette(&self, ui: &mut egui::Ui) {
        let Ok(scene) = &self.doc.scene else {
            return;
        };
        let cell = 14.0;
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(cell * 16.0, cell * 16.0), egui::Sense::hover());
        let painter = ui.painter();
        for row in 0..16u8 {
            for col in 0..16u8 {
                let [r, g, b, _] = scene
                    .palette
                    .get(usize::from(row), usize::from(col))
                    .unwrap_or_default()
                    .to_rgba8();
                let min = rect.min + egui::vec2(f32::from(col) * cell, f32::from(row) * cell);
                painter.rect_filled(
                    egui::Rect::from_min_size(min, egui::vec2(cell, cell)),
                    0.0,
                    egui::Color32::from_rgb(r, g, b),
                );
            }
        }
    }
}
