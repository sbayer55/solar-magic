//! Screen exits and secondary exits.

use std::collections::BTreeMap;

use smw_data::level::fields::secondary_exit;
use smw_data::level::{ExitId, ScreenExitInfo, SecondaryExit};

use super::{Action, Viewer, hex_word};
use crate::document::Document;
use crate::edit::EditCommand;

impl Viewer<'_> {
    pub(super) fn exits_panel(&mut self, ui: &mut egui::Ui) {
        let mut actions = Vec::new();
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(session) = self.doc.session.as_ref() {
                ui.heading("Screen exits");
                let exits = session.level.screen_exits();
                screen_exit_table(ui, session.level.header.screens(), &exits, &mut actions);
                ui.add_space(8.0);
            }
            ui.heading("Secondary exits");
            secondary_exit_editor(ui, self.doc, &mut actions);
        });
        self.actions.append(&mut actions);
    }
}

fn screen_exit_table(
    ui: &mut egui::Ui,
    screens: u8,
    exits: &BTreeMap<u8, ScreenExitInfo>,
    actions: &mut Vec<Action>,
) {
    egui::Grid::new("screen_exits").num_columns(4).striped(true).show(ui, |ui| {
        ui.label("screen");
        ui.label("destination");
        ui.label("secondary");
        ui.label("");
        ui.end_row();
        for screen in 0..screens {
            ui.label(format!("{screen:X}"));
            if let Some(mut info) = exits.get(&screen).copied() {
                let mut changed = hex_word(ui, &mut info.destination, 0x1FF);
                changed |= ui.checkbox(&mut info.secondary, "").changed();
                if changed {
                    actions.push(Action::Apply(EditCommand::SetScreenExit {
                        screen,
                        info: Some(info),
                    }));
                }
                if ui.button("remove").clicked() {
                    actions.push(Action::Apply(EditCommand::SetScreenExit { screen, info: None }));
                }
            } else {
                ui.label("—");
                ui.label("");
                if ui.button("add").clicked() {
                    let info = ScreenExitInfo { destination: 0, secondary: false };
                    actions.push(Action::Apply(EditCommand::SetScreenExit {
                        screen,
                        info: Some(info),
                    }));
                }
            }
            ui.end_row();
        }
    });
}

fn secondary_exit_editor(ui: &mut egui::Ui, doc: &Document, actions: &mut Vec<Action>) {
    let id_key = egui::Id::new("secondary_exit_id");
    let mut id_value: u16 = ui.data(|d| d.get_temp(id_key)).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.label("exit");
        hex_word(ui, &mut id_value, 0x1FF);
    });
    ui.data_mut(|d| d.insert_temp(id_key, id_value));
    let (Some(id), Some(session)) = (ExitId::new(id_value), doc.session.as_ref()) else {
        return;
    };
    let in_rom = SecondaryExit::load(&doc.rom, &doc.layout, id).ok();
    let Some(mut exit) = session.exits.get(&id).copied().or(in_rom) else {
        return;
    };
    let mut changed = false;
    egui::Grid::new("secondary_exit").num_columns(2).striped(true).show(ui, |ui| {
        for field in &secondary_exit::ALL {
            ui.label(field.name);
            let mut value = u32::from(exit.get(*field));
            let range = 0..=u32::from(field.max());
            if ui.add(egui::DragValue::new(&mut value).range(range)).changed()
                && exit.set(*field, u8::try_from(value).unwrap_or(0)).is_ok()
            {
                changed = true;
            }
            ui.end_row();
        }
    });
    if changed {
        actions.push(Action::Apply(EditCommand::SetSecondaryExit { id, exit }));
    }
    if in_rom.is_some_and(|stored| stored != exit) {
        ui.label("modified (written on Save)");
    }
}
