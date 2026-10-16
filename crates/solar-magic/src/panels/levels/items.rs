//! Painting one level as a list row or a gallery card.

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use smw_data::LevelId;

/// Row height in points.
pub(super) const ROW_HEIGHT: f32 = 22.0;
/// Space below a card's picture for its name and meta line.
const CARD_TEXT_HEIGHT: f32 = 36.0;
/// Gap between cards.
pub(super) const CARD_GAP: f32 = 8.0;
/// Narrowest card worth drawing; the grid picks as many columns as fit.
pub(super) const CARD_MIN_WIDTH: f32 = 150.0;

/// A level as the list shows it.
pub(super) struct Item {
    pub(super) id: LevelId,
    /// The user's name, or a derived hint such as `from 105 scr 07`.
    pub(super) name: String,
    /// Whether `name` is a derived hint rather than a given name.
    pub(super) derived: bool,
    /// `20 scr · T7 · 1 exit`.
    pub(super) meta: String,
    /// The open level.
    pub(super) current: bool,
    /// The keyboard cursor.
    pub(super) cursor: bool,
    /// Unsaved edits.
    pub(super) modified: bool,
    /// The shared blank level.
    pub(super) unused: bool,
}

/// What a row or card drew for the caller to act on.
pub(super) struct ItemResponse {
    pub(super) response: egui::Response,
    /// The rect the name occupies (where an inline rename goes).
    pub(super) name_rect: Rect,
}

/// The colours an item is painted with.
struct Paint {
    bg: Option<Color32>,
    stroke: Option<Stroke>,
    text: Color32,
    id: Color32,
    meta: Color32,
}

fn paint_for(ui: &egui::Ui, item: &Item, hovered: bool) -> Paint {
    let visuals = ui.visuals();
    let selection = visuals.selection;
    let accent = Color32::from_rgb(127, 184, 255);
    let (bg, stroke) = if item.current {
        (Some(selection.bg_fill), None)
    } else {
        let bg = hovered.then_some(visuals.widgets.hovered.weak_bg_fill);
        let stroke = item.cursor.then_some(Stroke::new(1.0, selection.stroke.color));
        (bg, stroke)
    };
    let text = if item.current {
        selection.stroke.color
    } else if item.unused {
        visuals.weak_text_color()
    } else {
        visuals.text_color()
    };
    let id = if item.current {
        selection.stroke.color
    } else if item.unused {
        visuals.weak_text_color()
    } else {
        accent
    };
    let meta = if item.current {
        selection.stroke.color.gamma_multiply(0.8)
    } else {
        visuals.weak_text_color()
    };
    Paint { bg, stroke, text, id, meta }
}

/// The amber "modified" dot.
pub(super) const MODIFIED: Color32 = Color32::from_rgb(240, 180, 41);

/// One list row: dot, number, name, meta. When the row is too narrow for
/// both, the name wins and the meta shrinks to its first part, then goes.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
pub(super) fn row(ui: &mut egui::Ui, item: &Item) -> ItemResponse {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::click());
    let name_left = rect.left() + 58.0;
    let right = rect.right() - 8.0;
    let name_rect =
        Rect::from_min_max(Pos2::new(name_left, rect.top()), Pos2::new(right, rect.bottom()));
    if !ui.is_rect_visible(rect) {
        return ItemResponse { response, name_rect };
    }
    let paint = paint_for(ui, item, response.hovered());
    let painter = ui.painter();
    if let Some(bg) = paint.bg {
        painter.rect_filled(rect, CornerRadius::ZERO, bg);
    }
    if let Some(stroke) = paint.stroke {
        painter.rect_stroke(rect.shrink(0.5), CornerRadius::ZERO, stroke, egui::StrokeKind::Inside);
    }
    if item.modified {
        painter.circle_filled(Pos2::new(rect.left() + 10.0, rect.center().y), 3.0, MODIFIED);
    }
    painter.text(
        Pos2::new(rect.left() + 18.0, rect.center().y),
        Align2::LEFT_CENTER,
        item.id.to_string(),
        FontId::monospace(13.0),
        paint.id,
    );
    let name_color = if item.derived { paint.meta } else { paint.text };
    let name = painter.layout_no_wrap(item.name.clone(), FontId::proportional(13.0), name_color);
    let room = right - name_left - name.size().x - 8.0;
    let meta = [item.meta.as_str(), item.meta.split(" · ").next().unwrap_or_default()]
        .into_iter()
        .map(|text| painter.layout_no_wrap(text.to_owned(), FontId::monospace(11.0), paint.meta))
        .find(|galley| galley.size().x <= room);
    let name_right = meta.as_ref().map_or(right, |galley| {
        painter.galley(
            Pos2::new(right - galley.size().x, rect.center().y - galley.size().y / 2.0),
            std::sync::Arc::clone(galley),
            paint.meta,
        );
        right - galley.size().x - 8.0
    });
    let name_clip = Rect::from_min_max(name_rect.min, Pos2::new(name_right, rect.bottom()));
    painter.with_clip_rect(name_clip).galley(
        Pos2::new(name_left, rect.center().y - name.size().y / 2.0),
        name,
        name_color,
    );
    ItemResponse { response, name_rect: name_clip }
}

/// A group header: arrow, name, slot range and count, clipped to the row.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
pub(super) fn group_header(
    ui: &mut egui::Ui,
    open: bool,
    name: &str,
    range: &str,
    count: usize,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT + 2.0), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let visuals = ui.visuals();
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, visuals.faint_bg_color);
    let y = rect.center().y;
    let arrow = if open { "⏷" } else { "⏵" };
    painter.text(
        Pos2::new(rect.left() + 6.0, y),
        Align2::LEFT_CENTER,
        arrow,
        FontId::proportional(10.0),
        visuals.weak_text_color(),
    );
    let title = painter.text(
        Pos2::new(rect.left() + 20.0, y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(12.5),
        visuals.strong_text_color(),
    );
    let count_rect = painter.text(
        Pos2::new(rect.right() - 8.0, y),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::monospace(11.0),
        visuals.weak_text_color(),
    );
    let range_clip = Rect::from_min_max(
        Pos2::new(title.right() + 8.0, rect.top()),
        Pos2::new(count_rect.left() - 8.0, rect.bottom()),
    );
    painter.with_clip_rect(range_clip).text(
        Pos2::new(range_clip.left(), y),
        Align2::LEFT_CENTER,
        range,
        FontId::monospace(11.0),
        visuals.weak_text_color(),
    );
    response
}

/// What a card has to show in its picture area.
pub(super) enum Picture<'a> {
    Ready(&'a egui::TextureHandle),
    Rendering,
    Failed,
}

/// The height of a card `width` points wide.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
pub(super) fn card_height(width: f32) -> f32 {
    let aspect =
        super::super::to_f32(crate::thumbs::HEIGHT) / super::super::to_f32(crate::thumbs::WIDTH);
    width.mul_add(aspect, CARD_TEXT_HEIGHT)
}

/// One gallery card: picture, number badge, name, meta.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
pub(super) fn card(
    ui: &mut egui::Ui,
    item: &Item,
    picture: &Picture<'_>,
    width: f32,
) -> ItemResponse {
    let size = Vec2::new(width, card_height(width));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let picture_rect =
        Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.bottom() - CARD_TEXT_HEIGHT));
    let name_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 8.0, picture_rect.bottom() + 2.0),
        Pos2::new(rect.right() - 8.0, picture_rect.bottom() + 20.0),
    );
    if !ui.is_rect_visible(rect) {
        return ItemResponse { response, name_rect };
    }
    let paint = paint_for(ui, item, response.hovered());
    let painter = ui.painter();
    let radius = CornerRadius::same(4);
    painter.rect_filled(rect, radius, ui.visuals().extreme_bg_color.gamma_multiply(0.6));
    paint_picture(painter, picture_rect, picture, ui.visuals().weak_text_color());
    let outline = if item.current {
        Stroke::new(2.0, ui.visuals().selection.stroke.color)
    } else if item.cursor {
        Stroke::new(1.5, ui.visuals().selection.stroke.color.gamma_multiply(0.6))
    } else {
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
    };
    if let Some(bg) = paint.bg.filter(|_| !item.current) {
        painter.rect_filled(rect, radius, bg.gamma_multiply(0.5));
    }
    painter.rect_stroke(rect, radius, outline, egui::StrokeKind::Inside);
    badge(painter, Pos2::new(rect.left() + 6.0, rect.top() + 6.0), &item.id.to_string());
    if item.modified {
        painter.circle_filled(Pos2::new(rect.right() - 10.0, rect.top() + 10.0), 4.0, MODIFIED);
    }
    let name_color = if item.derived { paint.meta } else { ui.visuals().strong_text_color() };
    painter.with_clip_rect(name_rect).text(
        Pos2::new(name_rect.left(), name_rect.center().y),
        Align2::LEFT_CENTER,
        &item.name,
        FontId::proportional(13.0),
        name_color,
    );
    let meta_clip = Rect::from_min_max(
        Pos2::new(rect.left() + 8.0, name_rect.bottom()),
        Pos2::new(rect.right() - 8.0, rect.bottom()),
    );
    painter.with_clip_rect(meta_clip).text(
        Pos2::new(meta_clip.left(), meta_clip.center().y - 1.0),
        Align2::LEFT_CENTER,
        &item.meta,
        FontId::monospace(11.0),
        ui.visuals().weak_text_color(),
    );
    ItemResponse { response, name_rect }
}

#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn paint_picture(painter: &egui::Painter, rect: Rect, picture: &Picture<'_>, text: Color32) {
    let radius = CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 };
    match picture {
        Picture::Ready(texture) => {
            painter.rect_filled(rect, radius, Color32::from_rgb(14, 42, 90));
            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            painter.image(texture.id(), rect, uv, Color32::WHITE);
        }
        Picture::Rendering => {
            painter.rect_filled(rect, radius, Color32::from_rgb(14, 42, 90));
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "rendering…",
                FontId::proportional(11.0),
                text,
            );
        }
        Picture::Failed => {
            painter.rect_filled(rect, radius, Color32::from_rgb(60, 30, 30));
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "no picture",
                FontId::proportional(11.0),
                text,
            );
        }
    }
}

/// A small dark label with the level number, drawn over the picture.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn badge(painter: &egui::Painter, at: Pos2, text: &str) {
    let font = FontId::monospace(12.0);
    let galley = painter.layout_no_wrap(text.to_owned(), font, Color32::WHITE);
    let rect = Rect::from_min_size(at, galley.size() + Vec2::new(10.0, 4.0));
    painter.rect_filled(rect, CornerRadius::same(3), Color32::from_black_alpha(160));
    painter.galley(rect.min + Vec2::new(5.0, 2.0), galley, Color32::WHITE);
}

/// A list-or-gallery toggle: three bars or four squares, painted.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
pub(super) fn view_toggle(
    ui: &mut egui::Ui,
    grid: bool,
    selected: bool,
    tip: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(26.0, 22.0), Sense::click());
    let visuals = ui.style().interact_selectable(&response, selected);
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(3),
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let color = visuals.fg_stroke.color;
    let c = rect.center();
    if grid {
        for (dx, dy) in [(-4.0, -4.0), (1.0, -4.0), (-4.0, 1.0), (1.0, 1.0)] {
            let square = Rect::from_min_size(Pos2::new(c.x + dx, c.y + dy), Vec2::splat(3.0));
            painter.rect_filled(square, CornerRadius::ZERO, color);
        }
    } else {
        for dy in [-4.0, 0.0, 4.0] {
            let y = c.y + dy;
            painter.line_segment(
                [Pos2::new(c.x - 5.0, y), Pos2::new(c.x + 5.0, y)],
                Stroke::new(1.5, color),
            );
        }
    }
    response.on_hover_text(tip)
}
