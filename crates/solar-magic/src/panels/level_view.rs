//! The zoomable level picture with selection, dragging and placement.

use egui::{CursorIcon, DragPanButtons, Key, PointerButton, Sense};
use smw_data::level::{ObjectKind, ObjectSpec, SpriteSpec, TilePos};
use smw_render::scene::LevelScene;

use super::camera::{self, CameraCommand, LevelExtent};
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
        let target = CameraTarget { level: doc.level, scene, session, size };
        update_camera(ui, view, &target, &mut scene_rect);
        let space_pan = space_held(ui);
        nav_scene(space_pan).show(ui, &mut scene_rect, |ui| {
            let sense = if space_pan { Sense::hover() } else { Sense::click_and_drag() };
            let response = ui.add(egui::Image::new(texture).fit_to_exact_size(size).sense(sense));
            let origin = response.rect.min;
            ui.data_mut(|d| d.insert_temp(egui::Id::new(ORIGIN_ID), origin));
            nav_feedback(ui, &response, space_pan, view);
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

/// Memory key of the picture's top-left in scene coordinates.
const ORIGIN_ID: &str = "scene_origin";
/// Memory key of the level view's size last frame, for the zoom readout.
const VIEWPORT_ID: &str = "scene_viewport";

/// What the camera frames.
struct CameraTarget<'a> {
    level: smw_data::LevelId,
    scene: &'a LevelScene,
    session: &'a EditSession,
    size: egui::Vec2,
}

/// Draws the navigation toolbar, re-fits on a new level and applies any
/// pending camera command to `scene_rect`.
fn update_camera(
    ui: &mut egui::Ui,
    view: &mut crate::app::ViewState,
    target: &CameraTarget<'_>,
    scene_rect: &mut egui::Rect,
) {
    let origin = ui.data(|d| d.get_temp(egui::Id::new(ORIGIN_ID))).unwrap_or(egui::Pos2::ZERO);
    let screens = target.scene.loaded.level.header.screens();
    let extent = LevelExtent::new(origin, target.size, target.scene.loaded.vertical, screens);
    toolbar(ui, view, &extent, *scene_rect);
    let viewport = ui.available_size_before_wrap();
    ui.data_mut(|d| d.insert_temp(egui::Id::new(VIEWPORT_ID), viewport));
    if view.camera_level != Some(target.level) || *scene_rect == egui::Rect::ZERO {
        view.camera_level = Some(target.level);
        view.camera_request = Some(CameraCommand::FitLevel);
    }
    if let Some(command) = view.camera_request.take() {
        let selection = super::overlays::selection_bounds(target.scene, target.session, origin);
        *scene_rect = camera::apply(command, *scene_rect, viewport, &extent, selection);
        ui.ctx().request_repaint();
    }
}

/// The pan-and-zoom container; Space lets the left button pan too.
fn nav_scene(space_pan: bool) -> egui::Scene {
    let pan_buttons = if space_pan {
        DragPanButtons::all()
    } else {
        DragPanButtons::MIDDLE | DragPanButtons::SECONDARY
    };
    egui::Scene::new().zoom_range(camera::ZOOM_MIN..=camera::ZOOM_MAX).drag_pan_buttons(pan_buttons)
}

/// Grab cursor while Space-panning; middle double-click recenters.
fn nav_feedback(
    ui: &egui::Ui,
    response: &egui::Response,
    space_pan: bool,
    view: &mut crate::app::ViewState,
) {
    if space_pan && response.hovered() {
        let grabbing = ui.input(|i| i.pointer.primary_down());
        ui.ctx().set_cursor_icon(if grabbing { CursorIcon::Grabbing } else { CursorIcon::Grab });
    }
    if response.double_clicked_by(PointerButton::Middle) {
        view.camera_request = Some(CameraCommand::Recenter);
    }
}

/// Space held over the editor (not while typing): drag pans with any button.
fn space_held(ui: &egui::Ui) -> bool {
    !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_down(Key::Space))
}

/// A toolbar button that queues `command`, with its shortcut in the tooltip.
fn camera_button(
    ui: &mut egui::Ui,
    view: &mut crate::app::ViewState,
    text: &str,
    selected: bool,
    command: CameraCommand,
) {
    let tip = camera::shortcut_for(command).map_or_else(
        || command.label(),
        |s| format!("{}  ({})", command.label(), ui.ctx().format_shortcut(s)),
    );
    if ui.selectable_label(selected, text).on_hover_text(tip).clicked() {
        view.camera_request = Some(command);
    }
}

/// Fit, zoom presets, zoom readout, recenter and screen navigation.
fn toolbar(
    ui: &mut egui::Ui,
    view: &mut crate::app::ViewState,
    extent: &LevelExtent,
    scene_rect: egui::Rect,
) {
    let viewport = ui.data(|d| d.get_temp(egui::Id::new(VIEWPORT_ID))).unwrap_or(egui::Vec2::ZERO);
    let zoom = camera::zoom_of(scene_rect, viewport);
    egui::Panel::top("level_nav").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            camera_button(ui, view, "Fit", false, CameraCommand::FitLevel);
            camera_button(ui, view, "Fit height", false, CameraCommand::FitHeight);
            camera_button(ui, view, "Selection", false, CameraCommand::FitSelection);
            ui.separator();
            camera_button(ui, view, "−", false, CameraCommand::ZoomStep(-1));
            for z in 1..=4u8 {
                let on = (zoom - f32::from(z)).abs() < 0.01;
                camera_button(ui, view, &format!("{z}×"), on, CameraCommand::Zoom(z));
            }
            camera_button(ui, view, "+", false, CameraCommand::ZoomStep(1));
            ui.label(format!("{:.0}%", zoom * 100.0));
            ui.separator();
            camera_button(ui, view, "Recenter", false, CameraCommand::Recenter);
            ui.separator();
            screen_controls(ui, view, extent, scene_rect);
        });
    });
}

/// Start, previous, screen number, next, end.
fn screen_controls(
    ui: &mut egui::Ui,
    view: &mut crate::app::ViewState,
    extent: &LevelExtent,
    scene_rect: egui::Rect,
) {
    camera_button(ui, view, "⏮", false, CameraCommand::Start);
    camera_button(ui, view, "◀", false, CameraCommand::ScreenStep(-1));
    ui.label("Screen");
    let mut screen = camera::screen_at(scene_rect, extent);
    let last = extent.screens.saturating_sub(1);
    if super::hex_byte(ui, &mut screen, last) {
        view.camera_request = Some(CameraCommand::GoToScreen(screen));
    }
    ui.label(format!("/ {last:X}"));
    camera_button(ui, view, "▶", false, CameraCommand::ScreenStep(1));
    camera_button(ui, view, "⏭", false, CameraCommand::End);
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
                "left: select/drag · middle, right or space+drag: pan · wheel: scroll · \
                 ⌘/Ctrl+wheel or pinch: zoom · View > Navigate for shortcuts"
                    .to_owned()
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
