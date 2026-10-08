//! The level list.

use smw_data::LevelId;

use super::{Action, Viewer};

impl Viewer<'_> {
    pub(super) fn level_list(&mut self, ui: &mut egui::Ui) {
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
                    self.actions.push(Action::SelectLevel(id));
                }
            }
        });
    }
}
