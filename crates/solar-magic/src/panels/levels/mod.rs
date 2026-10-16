//! The level list: search, filter chips, recents and the 512 slots grouped
//! by how they are reached, as rows or as thumbnail cards.

mod items;
mod query;

use std::collections::VecDeque;
use std::fmt::Write as _;

use egui::{Key, Modifiers};
use smw_data::LevelId;
use smw_data::level::{Catalogue, LevelSummary};

use self::items::{Item, ItemResponse, Picture};
use super::{Action, Viewer};
use crate::document::Document;
use crate::names::LevelNames;

/// Which slots the chips keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Scope {
    #[default]
    All,
    Overworld,
    Sublevels,
}

/// The order of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Sort {
    #[default]
    Number,
    Name,
    Screens,
    Tileset,
}

/// An inline rename in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rename {
    id: LevelId,
    text: String,
    focused: bool,
}

impl Rename {
    fn start(names: &LevelNames, id: LevelId) -> Self {
        Self { id, text: names.get(id).unwrap_or_default().to_owned(), focused: false }
    }
}

/// The search box, so the arrow keys work while it has focus.
fn search_id() -> egui::Id {
    egui::Id::new("levels-search")
}
/// Recent levels kept (the open one is not shown among them).
const RECENT_KEPT: usize = 8;
/// Recent chips shown.
const RECENT_SHOWN: usize = 4;

/// The list's own state.
#[derive(Debug, Clone, Default)]
pub(crate) struct LevelsView {
    /// Search box text.
    pub(crate) query: String,
    /// Cards instead of rows.
    pub(crate) grid: bool,
    /// Overworld / sublevel chip.
    pub(crate) scope: Scope,
    /// Only the level with unsaved edits.
    pub(crate) modified_only: bool,
    /// Skip the shared blank slots.
    pub(crate) hide_unused: bool,
    /// Only one tileset.
    pub(crate) tileset: Option<u8>,
    /// Sort choice.
    pub(crate) sort: Sort,
    /// Keyboard cursor; the open level when `None`.
    pub(crate) cursor: Option<LevelId>,
    /// A name being edited.
    pub(crate) renaming: Option<Rename>,
    /// Levels visited before the open one, newest first.
    pub(crate) recent: VecDeque<LevelId>,
}

impl LevelsView {
    /// Notes that `id` was the open level before switching away.
    pub(crate) fn remember(&mut self, id: LevelId) {
        self.recent.retain(|&r| r != id);
        self.recent.push_front(id);
        self.recent.truncate(RECENT_KEPT);
    }
}

impl Viewer<'_> {
    pub(super) fn level_list(&mut self, ui: &mut egui::Ui) {
        let doc = &mut *self.doc;
        let view = &mut self.view.levels;
        let modified = doc.is_dirty().then_some(doc.level);
        let filter = query::Filter { catalogue: &doc.catalogue, names: &doc.names, view, modified };
        let shown = filter.visible();
        let mut actions = Vec::new();
        keyboard(ui, view, doc, &shown, &mut actions);
        if view.grid {
            doc.ensure_thumbnails(ui.ctx());
        }
        header(ui, view, shown.len());
        toolbar(ui, view);
        ui.separator();
        let mut lane = Lane { doc, view, actions: &mut actions };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            recent_row(ui, &mut lane);
            let (overworld, sublevels): (Vec<LevelId>, Vec<LevelId>) =
                shown.iter().partition(|id| id.is_overworld());
            let groups = [
                ("Overworld", "000–024 · 101–13B", overworld),
                ("Sublevels", "025–100 · 13C–1FF", sublevels),
            ];
            for (name, range, ids) in &groups {
                if ids.is_empty() {
                    continue;
                }
                group(ui, &mut lane, name, range, ids);
            }
        });
        footer(ui, view);
        self.actions.append(&mut actions);
    }
}

/// What drawing the list needs to read and change.
struct Lane<'a> {
    doc: &'a mut Document,
    view: &'a mut LevelsView,
    actions: &'a mut Vec<Action>,
}

/// Count and the list/gallery toggle.
fn header(ui: &mut egui::Ui, view: &mut LevelsView, shown: usize) {
    ui.horizontal(|ui| {
        ui.weak(format!("{} slots · {shown} shown", LevelId::COUNT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if items::view_toggle(ui, true, view.grid, "Gallery (⌘⇧V)").clicked() {
                view.grid = true;
            }
            if items::view_toggle(ui, false, !view.grid, "List (⌘⇧V)").clicked() {
                view.grid = false;
            }
        });
    });
}

/// Search box, sort menu and the filter chips.
fn toolbar(ui: &mut egui::Ui, view: &mut LevelsView) {
    ui.horizontal(|ui| {
        let sort_label = match view.sort {
            Sort::Number => "Sort: number",
            Sort::Name => "Sort: name",
            Sort::Screens => "Sort: screens",
            Sort::Tileset => "Sort: tileset",
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button(sort_label, |ui| {
                ui.radio_value(&mut view.sort, Sort::Number, "Number");
                ui.radio_value(&mut view.sort, Sort::Name, "Name");
                ui.radio_value(&mut view.sort, Sort::Screens, "Screens");
                ui.radio_value(&mut view.sort, Sort::Tileset, "Tileset");
            });
            let search = egui::TextEdit::singleline(&mut view.query)
                .id(search_id())
                .hint_text("id, name, tileset:7, exits>0")
                .desired_width(ui.available_width());
            ui.add(search);
        });
    });
    ui.horizontal_wrapped(|ui| {
        for (scope, label) in
            [(Scope::All, "All"), (Scope::Overworld, "Overworld"), (Scope::Sublevels, "Sublevels")]
        {
            if ui.selectable_label(view.scope == scope, label).clicked() {
                view.scope = scope;
            }
        }
        ui.toggle_value(&mut view.modified_only, "Modified");
        ui.toggle_value(&mut view.hide_unused, "Hide unused");
        let tileset_label =
            view.tileset.map_or_else(|| "Tileset: any".to_owned(), |t| format!("Tileset: {t}"));
        ui.menu_button(tileset_label, |ui| {
            if ui.radio(view.tileset.is_none(), "Any").clicked() {
                view.tileset = None;
            }
            for t in 0..16u8 {
                if ui.radio(view.tileset == Some(t), format!("Tileset {t}")).clicked() {
                    view.tileset = Some(t);
                }
            }
        });
    });
}

/// Keyboard hints and the modified legend.
fn footer(ui: &mut egui::Ui, view: &LevelsView) {
    ui.separator();
    ui.horizontal(|ui| {
        let mode = if view.grid { "list" } else { "gallery" };
        ui.weak(format!("↑↓ move · ↵ open · F2 rename · ⌘⇧V {mode}"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak("modified");
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(8.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 3.0, items::MODIFIED);
        });
    });
}

/// Chips for the levels visited before this one.
fn recent_row(ui: &mut egui::Ui, lane: &mut Lane<'_>) {
    let doc = &*lane.doc;
    let recent: Vec<LevelId> =
        lane.view.recent.iter().copied().filter(|&id| id != doc.level).take(RECENT_SHOWN).collect();
    if recent.is_empty() {
        return;
    }
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.weak("Recent");
        for id in recent {
            let label =
                doc.names.get(id).map_or_else(|| id.to_string(), |name| format!("{id}  {name}"));
            if ui.small_button(label).clicked() {
                lane.actions.push(Action::SelectLevel(id));
            }
        }
    });
    ui.add_space(2.0);
}

/// A collapsible group: its header row, then its rows or cards.
fn group(ui: &mut egui::Ui, lane: &mut Lane<'_>, name: &str, range: &str, ids: &[LevelId]) {
    let id = ui.make_persistent_id(("levels-group", name));
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
    if items::group_header(ui, state.is_open(), name, range, ids.len()).clicked() {
        state.toggle(ui);
    }
    state.show_body_unindented(ui, |ui| group_body(ui, lane, ids));
}

/// The rows or cards of one group.
fn group_body(ui: &mut egui::Ui, lane: &mut Lane<'_>, ids: &[LevelId]) {
    if lane.view.grid {
        cards(ui, lane, ids);
    } else {
        for &id in ids {
            let item = item_for(lane.doc, lane.view, id);
            let drawn = items::row(ui, &item);
            react(ui, lane, &item, drawn);
        }
    }
}

/// How many cards of at least the minimum width fit in `available` points.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn columns_for(available: f32) -> usize {
    let pitch = items::CARD_MIN_WIDTH + items::CARD_GAP;
    let fit = ((available + items::CARD_GAP) / pitch).floor().max(1.0);
    // The float is a small positive integer; its decimal form parses exactly.
    format!("{fit:.0}").parse().unwrap_or(1)
}

/// Cards in as many columns as fit.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn cards(ui: &mut egui::Ui, lane: &mut Lane<'_>, ids: &[LevelId]) {
    let available = ui.available_width();
    let columns = columns_for(available);
    let columns_f = super::to_f32(u32::try_from(columns).unwrap_or(1));
    let width = items::CARD_GAP.mul_add(-(columns_f - 1.0), available) / columns_f;
    ui.spacing_mut().item_spacing = egui::Vec2::splat(items::CARD_GAP);
    for chunk in ids.chunks(columns) {
        ui.horizontal(|ui| {
            for &id in chunk {
                let item = item_for(lane.doc, lane.view, id);
                let drawn = {
                    let picture = picture_for(lane.doc, id);
                    items::card(ui, &item, &picture, width)
                };
                if ui.is_rect_visible(drawn.response.rect)
                    && let Some(thumbs) = lane.doc.thumbs.as_mut()
                {
                    thumbs.request(id);
                }
                react(ui, lane, &item, drawn);
            }
        });
    }
}

fn picture_for(doc: &Document, id: LevelId) -> Picture<'_> {
    match doc.thumbs.as_ref() {
        Some(thumbs) if thumbs.failed(id) => Picture::Failed,
        Some(thumbs) => thumbs.get(id).map_or(Picture::Rendering, Picture::Ready),
        None => Picture::Rendering,
    }
}

/// Clicks open a level; double-clicking the name or F2 renames it.
fn react(ui: &mut egui::Ui, lane: &mut Lane<'_>, item: &Item, drawn: ItemResponse) {
    if lane.view.renaming.as_ref().is_some_and(|r| r.id == item.id) {
        rename_box(ui, lane.doc, lane.view, drawn.name_rect);
        return;
    }
    if drawn.response.double_clicked() {
        lane.view.renaming = Some(Rename::start(&lane.doc.names, item.id));
    } else if drawn.response.clicked() {
        lane.view.cursor = Some(item.id);
        if !item.current {
            lane.actions.push(Action::SelectLevel(item.id));
        }
    }
    if item.derived && !item.name.is_empty() {
        drawn.response.on_hover_text("Double-click to name this level");
    }
}

/// The inline text box of a rename.
fn rename_box(ui: &mut egui::Ui, doc: &mut Document, view: &mut LevelsView, rect: egui::Rect) {
    let Some(rename) = view.renaming.as_mut() else {
        return;
    };
    let edit = egui::TextEdit::singleline(&mut rename.text)
        .id_salt(("rename", rename.id))
        .font(egui::FontId::proportional(13.0));
    let response = ui.put(rect, edit);
    if !rename.focused {
        response.request_focus();
        rename.focused = true;
        return;
    }
    if response.lost_focus() {
        let commit = ui.input(|i| i.key_pressed(Key::Enter));
        if commit && let Err(err) = doc.names.set(rename.id, &rename.text) {
            doc.status = format!("cannot save level names: {err}");
        }
        view.renaming = None;
    }
}

/// Arrow keys move the cursor, Enter opens it, F2 renames it.
fn keyboard(
    ui: &egui::Ui,
    view: &mut LevelsView,
    doc: &Document,
    shown: &[LevelId],
    actions: &mut Vec<Action>,
) {
    if view.renaming.is_some() {
        return;
    }
    let current = doc.level;
    let search_focused = ui.memory(|m| m.focused() == Some(search_id()));
    if !(ui.ui_contains_pointer() || search_focused) {
        return;
    }
    let (down, up, enter, rename) = ui.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.key_pressed(Key::Enter),
            i.consume_key(Modifiers::NONE, Key::F2),
        )
    });
    let cursor = view.cursor.unwrap_or(current);
    let at = shown.iter().position(|&id| id == cursor);
    if down || up {
        let next = match (at, down) {
            (Some(i), true) => i.saturating_add(1).min(shown.len().saturating_sub(1)),
            (Some(i), false) => i.saturating_sub(1),
            (None, _) => 0,
        };
        view.cursor = shown.get(next).copied();
    }
    if enter && let Some(id) = view.cursor.filter(|&id| id != current) {
        actions.push(Action::SelectLevel(id));
    }
    if rename {
        view.renaming = Some(Rename::start(&doc.names, cursor));
    }
}

/// What the list shows for one slot.
fn item_for(doc: &Document, view: &LevelsView, id: LevelId) -> Item {
    let summary = doc.catalogue.get(id);
    let unused = doc.catalogue.is_unused(id);
    let (name, derived) = doc.names.get(id).map_or_else(
        || (derived_name(&doc.catalogue, id, unused), true),
        |n| (n.to_owned(), false),
    );
    Item {
        id,
        name,
        derived,
        meta: summary.map(meta_line).unwrap_or_default(),
        current: id == doc.level,
        cursor: view.cursor == Some(id),
        modified: id == doc.level && doc.is_dirty(),
        unused,
    }
}

/// `unused slot`, `from 105 scr 07 · 002`, or nothing for an unnamed
/// overworld level.
fn derived_name(catalogue: &Catalogue, id: LevelId, unused: bool) -> String {
    if unused {
        return "unused slot".to_owned();
    }
    let parents = catalogue.parents(id);
    if parents.is_empty() {
        return if id.is_overworld() { String::new() } else { "no incoming exit".to_owned() };
    }
    let mut text = String::from("from ");
    for (n, parent) in parents.iter().take(3).enumerate() {
        if n > 0 {
            text.push_str(" · ");
        }
        text.push_str(&parent.from.to_string());
        if n == 0 {
            let _ = write!(text, " scr {:02X}", parent.screen);
        }
    }
    if parents.len() > 3 {
        text.push_str(" …");
    }
    text
}

/// `20 scr · T7 · 1 exit`.
fn meta_line(level: &LevelSummary) -> String {
    if level.error.is_some() {
        return "unreadable".to_owned();
    }
    let mut text = format!("{} scr · T{}", level.screens, level.tileset);
    if level.vertical {
        text.push_str(" · vert");
    }
    match level.exits.len() {
        0 => {}
        1 => text.push_str(" · 1 exit"),
        n => {
            let _ = write!(text, " · {n} exits");
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use smw_data::LevelId;

    use super::LevelsView;

    #[test]
    fn recent_is_deduplicated_and_bounded() {
        let mut view = LevelsView::default();
        let id = |n| LevelId::new(n).unwrap();
        for n in 0..12u16 {
            view.remember(id(n));
        }
        view.remember(id(3));
        assert_eq!(view.recent.len(), super::RECENT_KEPT);
        assert_eq!(view.recent.front(), Some(&id(3)));
        assert_eq!(view.recent.iter().filter(|&&r| r == id(3)).count(), 1);
    }
}
