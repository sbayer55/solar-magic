//! Primary and secondary header fields ("Change Properties in Header").

use smw_data::level::fields::{BitField, primary, secondary, sprite_header};

use super::{Action, Viewer};
use crate::edit::EditCommand;

impl Viewer<'_> {
    pub(super) fn header_panel(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.doc.session.as_ref() else {
            ui.label("No level loaded.");
            return;
        };
        let level = &session.level;
        let mut actions = Vec::new();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Primary header");
            egui::Grid::new("primary").num_columns(2).striped(true).show(ui, |ui| {
                ui.label("screens");
                let mut screens = u32::from(level.header.screens());
                let max = u32::from(level.max_screens());
                if ui.add(egui::DragValue::new(&mut screens).range(1..=max)).changed() {
                    actions.push(Action::Apply(EditCommand::SetScreens {
                        screens: u8::try_from(screens).unwrap_or(1),
                    }));
                }
                ui.end_row();
                for field in
                    primary::ALL.iter().filter(|f| f.name != primary::SCREENS_MINUS_ONE.name)
                {
                    if let Some(value) = field_row(ui, *field, &level.header.raw) {
                        actions.push(Action::Apply(EditCommand::SetHeaderField {
                            field: *field,
                            value,
                        }));
                    }
                }
            });
            ui.add_space(8.0);
            ui.heading("Secondary header");
            egui::Grid::new("secondary").num_columns(2).striped(true).show(ui, |ui| {
                for field in &secondary::ALL {
                    if let Some(value) = field_row(ui, *field, &level.secondary.raw) {
                        actions.push(Action::Apply(EditCommand::SetSecondaryField {
                            field: *field,
                            value,
                        }));
                    }
                }
            });
            ui.add_space(8.0);
            ui.heading("Sprite header");
            egui::Grid::new("sprite_header").num_columns(2).striped(true).show(ui, |ui| {
                for field in &sprite_header::ALL {
                    ui.label(field.name);
                    ui.label(format!("{}", field.get(&[level.sprites.header])));
                    ui.end_row();
                }
            });
        });
        self.actions.append(&mut actions);
    }
}

/// One editable row; returns the new value when it changed.
fn field_row(ui: &mut egui::Ui, field: BitField, raw: &[u8]) -> Option<u8> {
    ui.label(field.name);
    let mut value = u32::from(field.get(raw));
    let changed =
        ui.add(egui::DragValue::new(&mut value).range(0..=u32::from(field.max()))).changed();
    ui.end_row();
    changed.then(|| u8::try_from(value).unwrap_or_else(|_| field.max()))
}
