//! The zoomable level picture with selection, dragging and placement.

use egui::{DragPanButtons, PointerButton, Sense};
use smw_data::level::{ObjectKind, ObjectSpec, SpriteSpec, TilePos};
use smw_render::scene::LevelScene;

use super::{Action, Viewer, tile_under, to_f32};
use crate::edit::{Drag, EditCommand, EditSession, Hit, Tool, band_select, hit_test, rect_between};
use crate::picker::PickItem;

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
                let clicked_tile = response.interact_pointer_pos().and_then(to_tile);
                play_from_menu(ui, &response, clicked_tile, &mut actions);
                let pointer = Pointer {
                    hovered: response.interact_pointer_pos().and_then(to_tile).or(hovered),
                    drag_started: response.drag_started_by(PointerButton::Primary),
                    dragged: response.dragged_by(PointerButton::Primary),
                    drag_stopped: response.drag_stopped_by(PointerButton::Primary),
                    clicked: response.clicked_by(PointerButton::Primary),
                    shift: ui.input(|i| i.modifiers.shift),
                };
                handle_pointer(scene, session, view, &pointer, &mut actions);
                let mut place = Place { session, view, actions: &mut actions };
                drop_and_ghost(ui, &response, origin, &mut place);
            });
        ui.data_mut(|d| d.insert_persisted(rect_id, scene_rect));
        if hovered.is_some() {
            view.last_hovered = hovered;
        }
        let status = hovered.map(|t| describe_tile(scene, session, t)).unwrap_or_default();
        ui.data_mut(|d| d.insert_temp(egui::Id::new("status_text"), status));
        self.actions.append(&mut actions);
    }
}

/// What placing from the picker touches.
struct Place<'a> {
    session: &'a EditSession,
    view: &'a mut crate::app::ViewState,
    actions: &'a mut Vec<Action>,
}

/// The tile under the pointer, even while something is being dragged (when
/// egui reports no hover position). `origin` is the picture's top-left.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn pointer_tile(ui: &egui::Ui, origin: egui::Pos2) -> Option<TilePos> {
    let ctx = ui.ctx();
    let global = ctx.pointer_latest_pos()?;
    let local = ctx.layer_transform_from_global(ui.layer_id()).map_or(global, |t| t * global);
    let (x, y) = tile_under(local - origin)?;
    Some(TilePos::new(u16::try_from(x).ok()?, u16::try_from(y).ok()?))
}

/// Places a picker item dropped on the picture, and shows a ghost of what a
/// drop or a click would place.
fn drop_and_ghost(
    ui: &egui::Ui,
    response: &egui::Response,
    origin: egui::Pos2,
    place: &mut Place<'_>,
) {
    let Some(tile) = pointer_tile(ui, origin) else {
        return;
    };
    if let Some(item) = response.dnd_release_payload::<PickItem>() {
        if let Some(command) = insert_command(place.session, item.tool(), tile) {
            place.actions.push(Action::Apply(command));
            place.view.picker.note_used(*item);
        }
        return;
    }
    let tool = match response.dnd_hover_payload::<PickItem>() {
        Some(item) => item.tool(),
        None if response.hovered() => place.session.tool,
        None => return,
    };
    if tool != Tool::Select {
        draw_ghost(ui.painter(), &place.view.picker, tool, (origin, tile));
    }
}

/// The picture of what `tool` would place at `tile`, or an outline when the
/// picker has no picture of it.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn draw_ghost(
    painter: &egui::Painter,
    picker: &crate::picker::PickerState,
    tool: Tool,
    (origin, tile): (egui::Pos2, TilePos),
) {
    let at = |dx: i32, dy: i32| {
        let tiles =
            |n: u16, d: i32| (f32::from(n) + f32::from(i16::try_from(d).unwrap_or(0))) * 16.0;
        origin + egui::vec2(tiles(tile.x, dx), tiles(tile.y, dy))
    };
    let stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    let Some(entry) = picker.entry_for(tool) else {
        let rect = egui::Rect::from_min_size(at(0, 0), egui::vec2(16.0, 16.0));
        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
        return;
    };
    let (w, h) = entry.thumb.tiles;
    let size = egui::vec2(to_f32(to_u32(w)) * 16.0, to_f32(to_u32(h)) * 16.0);
    let rect = egui::Rect::from_min_size(at(entry.thumb.offset.0, entry.thumb.offset.1), size);
    painter.image(
        entry.thumb.texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::from_white_alpha(160),
    );
    painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The command that places what `tool` places at `tile` (`None` for the
/// select tool).
const fn insert_command(session: &EditSession, tool: Tool, tile: TilePos) -> Option<EditCommand> {
    let layer = session.selection.layer;
    match tool {
        Tool::PlaceSprite { number, extra_bits } => {
            Some(EditCommand::InsertSprite { spec: SpriteSpec { number, extra_bits, pos: tile } })
        }
        Tool::PlaceObject { number, third } => {
            Some(EditCommand::InsertObject { layer, spec: ObjectSpec { number, third, pos: tile } })
        }
        Tool::Select => None,
    }
}

/// Right-click menu offering to play the level from the clicked tile.
fn play_from_menu(
    ui: &egui::Ui,
    response: &egui::Response,
    clicked_tile: Option<TilePos>,
    actions: &mut Vec<Action>,
) {
    let menu_tile_id = egui::Id::new("play_from_tile");
    if response.secondary_clicked()
        && let Some(tile) = clicked_tile
    {
        ui.data_mut(|d| d.insert_temp(menu_tile_id, tile));
    }
    response.context_menu(|menu| {
        let tile = menu.data(|d| d.get_temp::<TilePos>(menu_tile_id));
        if let Some(tile) = tile
            && menu.button(format!("Play from here (screen {:X})", tile.x / 16)).clicked()
        {
            actions.push(Action::PlayFrom(tile));
            menu.close();
        }
    });
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
        if let Some(command) = insert_command(session, session.tool, tile) {
            actions.push(Action::Apply(command));
            view.picker.note_tool_used(session.tool);
        } else {
            session.selection.click(hit_at(tile), pointer.shift);
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
