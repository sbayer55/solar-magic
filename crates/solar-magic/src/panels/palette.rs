//! The level palette.

use super::Viewer;

impl Viewer<'_> {
    #[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
    pub(super) fn palette(&self, ui: &mut egui::Ui) {
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
                    .assets
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
