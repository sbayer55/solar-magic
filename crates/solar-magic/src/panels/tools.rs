//! The Edit panel: tool choice and the selected item's properties.

use smw_data::level::{Layer, Layer2, RawObject, Sprite, TilePos};

use super::{Action, Viewer, hex_byte, hex_word};
use crate::edit::{EditCommand, EditSession, Tool};

impl Viewer<'_> {
    pub(super) fn edit_panel(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.doc.session.as_mut() else {
            ui.label("No level loaded.");
            return;
        };
        let mut actions = Vec::new();
        ui.heading("Tool");
        tool_picker(ui, session);
        layer_picker(ui, session);
        ui.separator();
        ui.heading("Selection");
        selection_editor(ui, session, &mut actions);
        self.actions.append(&mut actions);
    }
}

fn tool_picker(ui: &mut egui::Ui, session: &mut EditSession) {
    let mut tool = session.tool;
    ui.radio_value(&mut tool, Tool::Select, "Select / move");
    let (mut number, mut third) = match tool {
        Tool::PlaceObject { number: n, third: t } => (n, t),
        Tool::Select | Tool::PlaceSprite { .. } => (0x01, 0x00),
    };
    ui.horizontal(|ui| {
        let placing = matches!(tool, Tool::PlaceObject { .. });
        let clicked = ui.radio(placing, "Place object").clicked();
        ui.label("no.");
        let a = hex_byte(ui, &mut number, 0x3F);
        ui.label("size/ext");
        let b = hex_byte(ui, &mut third, 0xFF);
        if clicked || a || b {
            tool = Tool::PlaceObject { number, third };
        }
    });
    let (mut sprite_number, mut extra) = match tool {
        Tool::PlaceSprite { number: n, extra_bits: e } => (n, e),
        Tool::Select | Tool::PlaceObject { .. } => (0x00, 0),
    };
    ui.horizontal(|ui| {
        let placing = matches!(tool, Tool::PlaceSprite { .. });
        let clicked = ui.radio(placing, "Place sprite").clicked();
        ui.label("no.");
        let a = hex_byte(ui, &mut sprite_number, 0xFF);
        ui.label("extra");
        let b = hex_byte(ui, &mut extra, 3);
        if clicked || a || b {
            tool = Tool::PlaceSprite { number: sprite_number, extra_bits: extra };
        }
    });
    session.tool = tool;
}

fn layer_picker(ui: &mut egui::Ui, session: &mut EditSession) {
    ui.horizontal(|ui| {
        ui.label("Layer");
        let has_layer2 = matches!(session.level.layer2, Layer2::Objects { .. });
        let mut layer = session.selection.layer;
        ui.radio_value(&mut layer, Layer::One, "1");
        ui.add_enabled_ui(has_layer2, |ui| ui.radio_value(&mut layer, Layer::Two, "2"));
        if layer != session.selection.layer {
            session.selection.clear();
            session.selection.layer = layer;
        }
    });
}

fn selection_editor(ui: &mut egui::Ui, session: &EditSession, actions: &mut Vec<Action>) {
    let selection = &session.selection;
    let vertical = session.level.vertical;
    let first_object = selection.objects.iter().next().copied();
    let first_sprite = selection.sprites.iter().next().copied();
    match (selection.objects.len(), selection.sprites.len(), first_object, first_sprite) {
        (0, 0, _, _) => {
            ui.label("Nothing selected. Click or drag in the level view.");
        }
        (1, 0, Some(index), _) => {
            let object = session.level.stream(selection.layer).and_then(|s| s.objects.get(index));
            if let Some(object) = object.copied() {
                object_editor(ui, (selection.layer, index), object, vertical, actions);
            }
        }
        (0, 1, _, Some(index)) => {
            if let Some(sprite) = session.level.sprites.sprites.get(index).copied() {
                sprite_editor(ui, index, sprite, vertical, actions);
            }
        }
        (o, s, _, _) => {
            ui.label(format!("{o} objects and {s} sprites selected."));
        }
    }
    if !selection.is_empty() && ui.button("Delete selection").clicked() {
        if !selection.objects.is_empty() {
            actions.push(Action::Apply(EditCommand::DeleteObjects {
                layer: selection.layer,
                indices: selection.object_indices(),
            }));
        }
        if !selection.sprites.is_empty() {
            actions.push(Action::Apply(EditCommand::DeleteSprites {
                indices: selection.sprite_indices(),
            }));
        }
    }
}

fn object_editor(
    ui: &mut egui::Ui,
    (layer, index): (Layer, usize),
    object: RawObject,
    vertical: bool,
    actions: &mut Vec<Action>,
) {
    ui.label(format!("Object #{index}: {}", super::level_view::kind_name(object.kind())));
    ui.label(format!("bytes {:02X?}  stream screen {:X}", object.as_bytes(), object.screen));
    let Some(pos) = object.position(vertical) else {
        ui.label("Exits and screen jumps are edited in the Exits panel.");
        return;
    };
    let (mut number, mut third) = (object.number(), object.third());
    let (mut x, mut y) = (pos.x, pos.y);
    let mut changed = false;
    egui::Grid::new("object_props").num_columns(2).show(ui, |ui| {
        ui.label("number");
        changed |= hex_byte(ui, &mut number, 0x3F);
        ui.end_row();
        ui.label("size / ext");
        changed |= hex_byte(ui, &mut third, 0xFF);
        ui.end_row();
        ui.label("x");
        changed |= hex_word(ui, &mut x, 0x1FF);
        ui.end_row();
        ui.label("y");
        changed |= hex_word(ui, &mut y, 0x1FF);
        ui.end_row();
    });
    if changed && let Some(replacement) = RawObject::at(number, third, TilePos::new(x, y), vertical)
    {
        actions.push(Action::Apply(EditCommand::ReplaceObject {
            layer,
            index,
            object: replacement,
        }));
    }
}

fn sprite_editor(
    ui: &mut egui::Ui,
    index: usize,
    sprite: Sprite,
    vertical: bool,
    actions: &mut Vec<Action>,
) {
    ui.label(format!("Sprite #{index}: {:02X}  bytes {:02X?}", sprite.number(), sprite.bytes));
    let pos = sprite.position(vertical);
    let (mut number, mut extra) = (sprite.number(), sprite.extra_bits());
    let (mut x, mut y) = (pos.x, pos.y);
    let mut changed = false;
    egui::Grid::new("sprite_props").num_columns(2).show(ui, |ui| {
        ui.label("number");
        changed |= hex_byte(ui, &mut number, 0xFF);
        ui.end_row();
        ui.label("extra bits");
        changed |= hex_byte(ui, &mut extra, 3);
        ui.end_row();
        ui.label("x");
        changed |= hex_word(ui, &mut x, 0x1FF);
        ui.end_row();
        ui.label("y");
        changed |= hex_word(ui, &mut y, 0x1FF);
        ui.end_row();
    });
    if changed && let Some(replacement) = Sprite::at(number, extra, TilePos::new(x, y), vertical) {
        actions.push(Action::Apply(EditCommand::ReplaceSprite { index, sprite: replacement }));
    }
}
