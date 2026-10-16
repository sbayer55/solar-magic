//! Lines, boxes and labels painted over the level picture.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};
use smw_data::RomLayout;
use smw_data::level::{ObjectKind, entrance_pixel};
use smw_render::ownership::TileRect;
use smw_render::scene::LevelScene;
use smw_render::sprites::sprite_tile_position;
use snes_rom::Rom;

use crate::app::ViewState;
use crate::edit::{Drag, EditSession, rect_between};

const SCREEN_GRID: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 110);
const TILE_GRID: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 40);
const EXIT: Color32 = Color32::from_rgb(255, 200, 40);
const ENTRANCE: Color32 = Color32::from_rgb(80, 255, 120);
const MIDWAY: Color32 = Color32::from_rgb(255, 120, 200);
const SELECTION: Color32 = Color32::from_rgb(255, 255, 255);
const BAND: Color32 = Color32::from_rgba_premultiplied(120, 180, 255, 60);

/// Everything the overlays need to know about the frame.
pub(super) struct Frame<'a> {
    /// The rendered scene.
    pub(super) scene: &'a LevelScene,
    /// Selection and drag state.
    pub(super) session: &'a EditSession,
    /// Overlay toggles.
    pub(super) view: &'a ViewState,
    /// For entrance position tables.
    pub(super) rom: &'a Rom,
    /// Table locations.
    pub(super) layout: &'a RomLayout,
    /// Top-left of the picture in scene coordinates.
    pub(super) origin: Pos2,
    /// Scene scale, to keep strokes one screen pixel wide.
    pub(super) zoom: f32,
}

/// Pixel rectangle of a tile rectangle, offset by the picture origin.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn tile_rect(origin: Pos2, r: TileRect) -> Rect {
    let to = |v: usize| f32::from(u16::try_from(v).unwrap_or(u16::MAX)) * 16.0;
    Rect::from_min_size(origin + Vec2::new(to(r.x), to(r.y)), Vec2::new(to(r.w), to(r.h)))
}

/// Paints every enabled overlay. `zoom` keeps strokes and text one screen
/// pixel wide regardless of the scene scale.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
pub(super) fn draw(painter: &Painter, frame: &Frame<'_>) {
    let Frame { scene, session, view, origin, zoom, .. } = *frame;
    let thin = Stroke::new(1.0 / zoom, SCREEN_GRID);
    let font = FontId::monospace(11.0 / zoom);
    let (w, h) = scene.pixel_size();
    let (wf, hf) = (super::to_f32(w), super::to_f32(h));
    let vertical = scene.loaded.vertical;
    let screens = u32::from(scene.loaded.level.header.screens());
    if view.show_tile_grid {
        let faint = Stroke::new(1.0 / zoom, TILE_GRID);
        for x in (0..=w).step_by(16) {
            let xf = super::to_f32(x);
            painter.line_segment([origin + Vec2::new(xf, 0.0), origin + Vec2::new(xf, hf)], faint);
        }
        for y in (0..=h).step_by(16) {
            let yf = super::to_f32(y);
            painter.line_segment([origin + Vec2::new(0.0, yf), origin + Vec2::new(wf, yf)], faint);
        }
    }
    if view.show_screen_grid {
        for screen in 0..screens {
            let at = super::to_f32(screen.saturating_mul(256));
            let (a, b, label) = if vertical {
                (Vec2::new(0.0, at), Vec2::new(wf, at), Vec2::new(2.0, at + 2.0))
            } else {
                (Vec2::new(at, 0.0), Vec2::new(at, hf), Vec2::new(at + 2.0, 2.0))
            };
            painter.line_segment([origin + a, origin + b], thin);
            painter.text(
                origin + label,
                Align2::LEFT_TOP,
                format!("{screen:X}"),
                font.clone(),
                SCREEN_GRID,
            );
        }
    }
    if view.show_exits {
        for object in &session.level.layer1.objects {
            let ObjectKind::ScreenExit { screen, destination, secondary } = object.kind() else {
                continue;
            };
            let at = f32::from(screen) * 256.0;
            let rect = if vertical {
                Rect::from_min_size(origin + Vec2::new(wf - 16.0, at), Vec2::new(16.0, 256.0))
            } else {
                Rect::from_min_size(origin + Vec2::new(at, 0.0), Vec2::new(256.0, 16.0))
            };
            painter.rect_stroke(rect, 0.0, Stroke::new(2.0 / zoom, EXIT), StrokeKind::Inside);
            let text = format!("exit → {}{destination:03X}", if secondary { "sec " } else { "" });
            painter.text(
                rect.min + Vec2::new(4.0, 2.0),
                Align2::LEFT_TOP,
                text,
                font.clone(),
                EXIT,
            );
        }
    }
    if view.show_entrances {
        draw_entrances(painter, frame);
    }
    draw_selection(painter, frame);
}

#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn draw_entrances(painter: &Painter, frame: &Frame<'_>) {
    let Frame { scene, rom, layout, origin, zoom, .. } = *frame;
    let secondary = scene.loaded.level.secondary;
    let Ok((x, y)) =
        entrance_pixel(rom, layout, secondary.entrance_x_index(), secondary.entrance_y_index())
    else {
        return;
    };
    let vertical = scene.loaded.vertical;
    let marker = |screen: u8, color: Color32, label: &str| {
        let screen_px = f32::from(screen) * 256.0;
        let pos = if vertical {
            Vec2::new(f32::from(x), screen_px + f32::from(y))
        } else {
            Vec2::new(screen_px + f32::from(x), f32::from(y))
        };
        let rect = Rect::from_min_size(origin + pos, Vec2::new(16.0, 32.0));
        painter.rect_stroke(rect, 0.0, Stroke::new(2.0 / zoom, color), StrokeKind::Outside);
        painter.text(
            rect.left_bottom() + Vec2::new(0.0, 2.0),
            Align2::LEFT_TOP,
            label,
            FontId::monospace(11.0 / zoom),
            color,
        );
    };
    marker(secondary.entrance_screen(), ENTRANCE, "start");
    marker(secondary.midway_screen(), MIDWAY, "midway");
}

#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn draw_selection(painter: &Painter, frame: &Frame<'_>) {
    let Frame { scene, session, origin, zoom, .. } = *frame;
    let stroke = Stroke::new(2.0 / zoom, SELECTION);
    let layer = crate::edit::render_layer(session.selection.layer);
    for &index in &session.selection.objects {
        if let Some(Some(bounds)) = scene.loaded.ownership.bounds(layer).get(index) {
            painter.rect_stroke(tile_rect(origin, *bounds), 0.0, stroke, StrokeKind::Outside);
        }
    }
    for &index in &session.selection.sprites {
        if let Some(rect) = sprite_rect(scene, origin, index) {
            painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Outside);
        }
    }
    if let Drag::Band { anchor, current } = session.drag {
        let rect = tile_rect(origin, rect_between(anchor, current));
        painter.rect_filled(rect, 0.0, BAND);
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0 / zoom, SELECTION), StrokeKind::Inside);
    }
}

/// Pixel rectangle of sprite `index`'s marker tile.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn sprite_rect(scene: &LevelScene, origin: Pos2, index: usize) -> Option<Rect> {
    let sprite = scene.loaded.level.sprites.sprites.get(index)?;
    let (x, y) = sprite_tile_position(*sprite, scene.loaded.vertical);
    Some(Rect::from_min_size(
        origin + Vec2::new(super::to_f32(x) * 16.0, super::to_f32(y) * 16.0),
        Vec2::splat(16.0),
    ))
}

/// The box around everything selected, in scene coordinates.
pub(super) fn selection_bounds(
    scene: &LevelScene,
    session: &EditSession,
    origin: Pos2,
) -> Option<Rect> {
    let layer = crate::edit::render_layer(session.selection.layer);
    let bounds = scene.loaded.ownership.bounds(layer);
    let objects = session
        .selection
        .objects
        .iter()
        .filter_map(|&i| bounds.get(i).copied().flatten())
        .map(|r| tile_rect(origin, r));
    let sprites = session.selection.sprites.iter().filter_map(|&i| sprite_rect(scene, origin, i));
    objects.chain(sprites).reduce(Rect::union)
}
