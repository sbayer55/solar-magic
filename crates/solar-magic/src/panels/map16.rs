//! The Map16 sheet.

use super::Viewer;

impl Viewer<'_> {
    pub(super) fn map16(&self, ui: &mut egui::Ui) {
        let Some(texture) = &self.doc.map16_texture else {
            return;
        };
        egui::ScrollArea::both().show(ui, |ui| {
            ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(512.0, 1024.0)));
        });
    }
}
