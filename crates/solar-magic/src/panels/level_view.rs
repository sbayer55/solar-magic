//! The zoomable level picture with selection, dragging and placement.

use egui::{DragPanButtons, PointerButton, Sense};
use smw_data::level::{ObjectKind, ObjectSpec, SpriteSpec, TilePos};
use smw_render::scene::LevelScene;

use super::{Action, Viewer, tile_under, to_f32};
use crate::edit::{Drag, EditCommand, EditSession, Hit, Tool, band_select, hit_test, rect_between};

/// What happened to the pointer over the picture this frame.
struct Pointer {
    hovered: Option<TilePos>,
    drag_started: bool,
    dragged: bool,
    drag_stopped: bool,
    clicked: bool,
    shift: bool,
}

impl Viewer<'_> {
    #[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
    pub(super) fn level_view(&mut self, ui: &mut egui::Ui) {
        let doc = &mut *self.doc;
        let scene = match &doc.scene {
            Ok(scene) => scene,
            Err(err) => {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
                return;
            }
        };
        let (Some(texture), Some(session)) = (&doc.texture, doc.session.as_mut()) else {
            return;
        };
        let view = &mut *self.view;
        let (w, h) = scene.pixel_size();
        let size = egui::vec2(to_f32(w), to_f32(h));
        let rect_id = egui::Id::new("scene_rect");
        let mut scene_rect =
            ui.data_mut(|d| d.get_persisted::<egui::Rect>(rect_id)).unwrap_or(egui::Rect::ZERO);
        let mut hovered: Option<TilePos> = None;
        let mut actions: Vec<Action> = Vec::new();
        status_bar(ui, &doc.status);
        egui::Scene::new()
            .zoom_range(0.25..=8.0)
            .drag_pan_buttons(DragPanButtons::MIDDLE | DragPanButtons::SECONDARY)
            .show(ui, &mut scene_rect, |ui| {
                let response = ui.add(
                    egui::Image::new(texture)
                        .fit_to_exact_size(size)
                        .sense(Sense::click_and_drag()),
                );
                let origin = response.rect.min;
                let zoom = ui
                    .ctx()
                    .layer_transform_to_global(ui.layer_id())
                    .map_or(1.0, |t| t.scaling)
                    .max(0.01);
                let frame = super::overlays::Frame {
                    scene,
                    session,
                    view,
                    rom: &doc.rom,
                    layout: &doc.layout,
                    origin,
                    zoom,
                };
                super::overlays::draw(ui.painter(), &frame);
                let to_tile = |pos: egui::Pos2| {
                    tile_under(pos - origin).map(|(x, y)| {
                        TilePos::new(
                            u16::try_from(x).unwrap_or(u16::MAX),
                            u16::try_from(y).unwrap_or(u16::MAX),
                        )
                    })
                };
                hovered = response.hover_pos().and_then(to_tile);
                let pointer = Pointer {
                    hovered: response.interact_pointer_pos().and_then(to_tile).or(hovered),
                    drag_started: response.drag_started_by(PointerButton::Primary),
                    dragged: response.dragged_by(PointerButton::Primary),
                    drag_stopped: response.drag_stopped_by(PointerButton::Primary),
                    clicked: response.clicked_by(PointerButton::Primary),
                    shift: ui.input(|i| i.modifiers.shift),
                };
                handle_pointer(scene, session, view, &pointer, &mut actions);
            });
        ui.data_mut(|d| d.insert_persisted(rect_id, scene_rect));
        let status = hovered.map(|t| describe_tile(scene, session, t)).unwrap_or_default();
        ui.data_mut(|d| d.insert_temp(egui::Id::new("status_text"), status));
        self.actions.append(&mut actions);
    }
}

/// The hover text and the document's last message.
fn status_bar(ui: &mut egui::Ui, message: &str) {
    egui::Panel::bottom("status").show(ui, |ui| {
        let status =
            ui.data(|d| d.get_temp::<String>(egui::Id::new("status_text"))).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(if status.is_empty() {
                "left: select/drag, middle or right: pan, scroll: zoom".to_owned()
            } else {
                status
            });
            if !message.is_empty() {
                ui.separator();
                ui.colored_label(egui::Color32::LIGHT_YELLOW, message);
            }
        });
    });
}

/// Selection, drag and placement logic for one frame.
fn handle_pointer(
    scene: &LevelScene,
    session: &mut EditSession,
    view: &mut crate::app::ViewState,
    pointer: &Pointer,
    actions: &mut Vec<Action>,
) {
    let Some(tile) = pointer.hovered else {
        if pointer.drag_stopped {
            finish_drag(scene, session, view, actions);
        }
        return;
    };
    let layer = session.selection.layer;
    let sprites_visible = view.options.sprites;
    let hit_at =
        |t: TilePos| hit_test(scene, layer, (usize::from(t.x), usize::from(t.y)), sprites_visible);
    if pointer.drag_started && session.tool == Tool::Select {
        let hit = hit_at(tile);
        if hit == Hit::Nothing {
            session.drag = Drag::Band { anchor: tile, current: tile };
        } else {
            if !session.selection.contains(hit) {
                session.selection.click(hit, pointer.shift);
            }
            view.drag_group = view.drag_group.wrapping_add(1);
            session.drag = Drag::Move { anchor: tile, applied: (0, 0), group: view.drag_group };
        }
    } else if pointer.dragged {
        match session.drag {
            Drag::Band { anchor, .. } => session.drag = Drag::Band { anchor, current: tile },
            Drag::Move { anchor, applied, group } => {
                let delta = (
                    i32::from(tile.x).saturating_sub(i32::from(anchor.x)),
                    i32::from(tile.y).saturating_sub(i32::from(anchor.y)),
                );
                if delta != applied {
                    let step =
                        (delta.0.saturating_sub(applied.0), delta.1.saturating_sub(applied.1));
                    if !session.selection.objects.is_empty() {
                        let indices = session.selection.object_indices();
                        actions.push(Action::ApplyGrouped(
                            EditCommand::MoveObjects { layer, indices, delta: step },
                            group,
                        ));
                    }
                    if !session.selection.sprites.is_empty() {
                        let indices = session.selection.sprite_indices();
                        actions.push(Action::ApplyGrouped(
                            EditCommand::MoveSprites { indices, delta: step },
                            group,
                        ));
                    }
                    session.drag = Drag::Move { anchor, applied: delta, group };
                }
            }
            Drag::None => {}
        }
    }
    if pointer.drag_stopped {
        finish_drag(scene, session, view, actions);
    }
    if pointer.clicked {
        match session.tool {
            Tool::Select => session.selection.click(hit_at(tile), pointer.shift),
            Tool::PlaceObject { number, third } => {
                let spec = ObjectSpec { number, third, pos: tile };
                actions.push(Action::Apply(EditCommand::InsertObject { layer, spec }));
            }
            Tool::PlaceSprite { number, extra_bits } => {
                let spec = SpriteSpec { number, extra_bits, pos: tile };
                actions.push(Action::Apply(EditCommand::InsertSprite { spec }));
            }
        }
    }
}

fn finish_drag(
    scene: &LevelScene,
    session: &mut EditSession,
    view: &crate::app::ViewState,
    _actions: &mut [Action],
) {
    if let Drag::Band { anchor, current } = session.drag {
        let rect = rect_between(anchor, current);
        let picked = band_select(scene, session.selection.layer, rect, view.options.sprites);
        if session.tool == Tool::Select {
            session.selection.objects.extend(picked.objects);
            session.selection.sprites.extend(picked.sprites);
        }
    }
    session.drag = Drag::None;
}

/// Status text for the tile under the pointer.
fn describe_tile(scene: &LevelScene, session: &EditSession, tile: TilePos) -> String {
    let (x, y) = (usize::from(tile.x), usize::from(tile.y));
    let Some(map16) = scene.layer1_tile(x, y) else {
        return String::new();
    };
    let layer = session.selection.layer;
    let owner = hit_test(scene, layer, (x, y), true);
    let who = match owner {
        Hit::Object(index) => session
            .level
            .stream(layer)
            .and_then(|s| s.objects.get(index))
            .map_or_else(String::new, |o| format!("  object #{index}: {}", kind_name(o.kind()))),
        Hit::Sprite(index) => session
            .level
            .sprites
            .sprites
            .get(index)
            .map_or_else(String::new, |s| format!("  sprite #{index}: {:02X}", s.number())),
        Hit::Nothing => String::new(),
    };
    let screen = if scene.loaded.vertical { y / 16 } else { x / 16 };
    format!("tile ({x}, {y})  screen {screen:X}  Map16 {map16:03X}{who}")
}

/// Short description of an object kind.
pub(crate) fn kind_name(kind: ObjectKind) -> String {
    match kind {
        ObjectKind::Standard { number, size } => format!("standard {number:02X} size {size:02X}"),
        ObjectKind::Extended { number } => format!("extended {number:02X}"),
        ObjectKind::ScreenExit { screen, destination, secondary } => {
            format!(
                "exit on screen {screen:X} to {}{destination:03X}",
                if secondary { "secondary " } else { "" }
            )
        }
        ObjectKind::ScreenJump { screen } => format!("screen jump to {screen:X}"),
        ObjectKind::ScreenExitExt { screen, destination, secondary, .. } => format!(
            "LM exit on screen {screen:X} to {}{destination:03X}",
            if secondary { "secondary " } else { "" }
        ),
        ObjectKind::DirectMap16 { tile, size } => format!("Map16 tile {tile:03X} size {size:02X}"),
        ObjectKind::DirectMap16Ext { tile, size, .. } => {
            format!("Map16 tile {tile:04X} size {size:02X}")
        }
    }
}
