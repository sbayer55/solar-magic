//! The Objects panel: a filterable grid of object and sprite thumbnails.
//! Click one to place it with every click in the level view, or drag it onto
//! the level.

use egui::{Color32, Sense, Stroke, StrokeKind, vec2};

use super::Viewer;
use crate::edit::Tool;
use crate::picker::{
    Category, KindFilter, PickItem, PickerEntry, PickerState, SortBy, THUMB_RANGE,
};

/// Cells at least this big show the id under the picture.
const LABEL_MIN: f32 = 40.0;
/// Height of the id line.
const LABEL_HEIGHT: f32 = 13.0;
/// Space around the picture inside a cell.
const PAD: f32 = 3.0;
const SELECTED: Color32 = Color32::from_rgb(0x5A, 0x9B, 0xE6);

impl Viewer<'_> {
    pub(super) fn picker_panel(&mut self, ui: &mut egui::Ui) {
        let picker = &mut self.view.picker;
        if picker.pump(ui.ctx(), self.doc) {
            ui.ctx().request_repaint();
        }
        let Some(session) = self.doc.session.as_mut() else {
            ui.label("No level loaded.");
            return;
        };
        controls(ui, picker);
        let (drawn, total) = picker.progress();
        if drawn < total {
            ui.label(format!("Drawing thumbnails… {drawn} of {total}"));
        }
        ui.separator();
        let footer = if matches!(session.tool, Tool::Select) { 44.0 } else { 64.0 };
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .max_height((ui.available_height() - footer).max(60.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    for entry in picker.visible() {
                        let selected = entry.item.matches(session.tool);
                        if cell(ui, entry, picker.thumb_size, selected).clicked() {
                            clicked = Some(entry.item);
                        }
                    }
                });
            });
        if let Some(item) = clicked {
            session.tool = if item.matches(session.tool) { Tool::Select } else { item.tool() };
        }
        ui.separator();
        placing_bar(ui, picker, &mut session.tool);
        drag_preview(ui.ctx(), picker);
    }
}

/// Kind switch, search, category, sort and thumbnail size.
fn controls(ui: &mut egui::Ui, picker: &mut PickerState) {
    ui.horizontal(|ui| {
        let before = picker.kind;
        ui.selectable_value(&mut picker.kind, KindFilter::All, "All");
        ui.selectable_value(&mut picker.kind, KindFilter::Objects, "Objects");
        ui.selectable_value(&mut picker.kind, KindFilter::Sprites, "Sprites");
        if picker.kind != before && picker.category.is_some_and(|c| !kind_has(picker.kind, c)) {
            picker.category = None;
        }
    });
    ui.add(
        egui::TextEdit::singleline(&mut picker.query)
            .hint_text("Search: pipe, koopa, 1C")
            .desired_width(f32::INFINITY),
    );
    ui.horizontal(|ui| {
        let shown = picker.category.map_or("All categories", Category::title);
        egui::ComboBox::from_id_salt("picker_category").selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut picker.category, None, "All categories");
            for c in Category::ALL.into_iter().filter(|&c| kind_has(picker.kind, c)) {
                ui.selectable_value(&mut picker.category, Some(c), c.title());
            }
        });
        egui::ComboBox::from_id_salt("picker_sort")
            .selected_text(format!("Sort: {}", picker.sort.title()))
            .show_ui(ui, |ui| {
                for s in SortBy::ALL {
                    ui.selectable_value(&mut picker.sort, s, s.title());
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Size");
        ui.add(egui::Slider::new(&mut picker.thumb_size, THUMB_RANGE).step_by(8.0).suffix(" px"));
    });
}

/// Whether the kind switch includes category `c`.
const fn kind_has(kind: KindFilter, c: Category) -> bool {
    match kind {
        KindFilter::All => true,
        KindFilter::Objects => c.is_object(),
        KindFilter::Sprites => !c.is_object(),
    }
}

/// One grid cell: the picture scaled to fit a `size` square, the id below.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn cell(ui: &mut egui::Ui, entry: &PickerEntry, size: f32, selected: bool) -> egui::Response {
    let label = size >= LABEL_MIN;
    let extra = if label { LABEL_HEIGHT } else { 0.0 };
    let padded = 2.0f32.mul_add(PAD, size);
    let outer = vec2(padded, padded + extra);
    let (rect, response) = ui.allocate_exact_size(outer, Sense::click_and_drag());
    response.dnd_set_drag_payload(entry.item);
    let visuals = ui.visuals();
    let (fill, stroke) = if selected {
        (visuals.selection.bg_fill.gamma_multiply(0.5), Stroke::new(2.0, SELECTED))
    } else if response.hovered() {
        (visuals.extreme_bg_color, visuals.widgets.hovered.bg_stroke)
    } else {
        (visuals.extreme_bg_color, visuals.widgets.noninteractive.bg_stroke)
    };
    let painter = ui.painter_at(rect);
    painter.rect(rect, 3.0, fill, stroke, StrokeKind::Inside);
    let square = egui::Rect::from_min_size(rect.min + vec2(PAD, PAD), vec2(size, size));
    painter.image(
        entry.thumb.texture.id(),
        fit(square, entry.thumb.tiles),
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    if label {
        painter.text(
            egui::pos2(rect.center().x, square.max.y + 1.0),
            egui::Align2::CENTER_TOP,
            entry.item.id_label(),
            egui::FontId::monospace(10.0),
            visuals.weak_text_color(),
        );
    }
    let (w, h) = entry.thumb.tiles;
    response.on_hover_text(format!("{}\n{}  ·  {w}×{h} tiles", entry.name, entry.item.id_label()))
}

/// The largest rectangle with the picture's aspect ratio centred in `square`.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn fit(square: egui::Rect, (w, h): (usize, usize)) -> egui::Rect {
    let (w, h) = (super::to_f32(to_u32(w)).max(1.0), super::to_f32(to_u32(h)).max(1.0));
    let scale = square.width() / w.max(h);
    egui::Rect::from_center_size(square.center(), vec2(w * scale, h * scale))
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// What is being placed, with a button to stop.
fn placing_bar(ui: &mut egui::Ui, picker: &PickerState, tool: &mut Tool) {
    if matches!(*tool, Tool::Select) {
        hint(ui, "Click an item, then click the level to place it. Or drag it onto the level.");
        return;
    }
    let name = picker.entry_for(*tool).map_or_else(
        || match *tool {
            Tool::PlaceObject { number, third } => format!("object {number:02X} {third:02X}"),
            Tool::PlaceSprite { number, .. } => format!("sprite {number:02X}"),
            Tool::Select => String::new(),
        },
        |e| format!("{} ({})", e.name, e.item.id_label()),
    );
    ui.horizontal(|ui| {
        ui.colored_label(SELECTED, format!("Placing {name}"));
        if ui.small_button("✕").on_hover_text("Stop placing (Esc)").clicked() {
            *tool = Tool::Select;
        }
    });
    hint(ui, "Click the level to place one. Esc stops.");
}

/// Dim help text that wraps to the panel width.
fn hint(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).weak()).wrap());
}

/// The dragged item's picture under the pointer.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn drag_preview(ctx: &egui::Context, picker: &PickerState) {
    let Some(item) = egui::DragAndDrop::payload::<PickItem>(ctx) else {
        return;
    };
    let (Some(pos), Some(entry)) = (ctx.pointer_latest_pos(), picker.entry_for(item.tool())) else {
        return;
    };
    let size = picker.thumb_size;
    egui::Area::new(egui::Id::new("picker_drag_preview"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + vec2(12.0, 12.0))
        .interactable(false)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
            ui.painter().image(
                entry.thumb.texture.id(),
                fit(rect, entry.thumb.tiles),
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::from_white_alpha(200),
            );
        });
}
