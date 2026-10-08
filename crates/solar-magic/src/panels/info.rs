//! Read-only facts about the loaded level.

use smw_data::level::Layer2;

use super::Viewer;

impl Viewer<'_> {
    pub(super) fn info(&self, ui: &mut egui::Ui) {
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
        let rows: [(&str, String); 14] = [
            ("Level", level.id.to_string()),
            ("ROM", format!("{} ({} KiB)", self.doc.path.display(), self.doc.rom.len() / 1024)),
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
}
