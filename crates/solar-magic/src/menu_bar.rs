//! Menu-bar buttons that behave like a native (macOS-style) menu bar.
//!
//! egui's stock [`egui::Ui::menu_button`] opens each menu only on a click.
//! Here the first click opens a menu, and while any bar menu is open, hovering
//! a sibling button switches to that menu without another click.

use egui::containers::menu::MenuConfig;
use egui::{Id, InnerResponse, Popup, SetOpenCommand, Ui};

/// Popup ids of every menu shown in the bar so far, kept in egui's temp data
/// so each button can tell whether one of its siblings is currently open.
#[derive(Clone, Default)]
struct BarMenus(Vec<Id>);

fn registry_id() -> Id {
    Id::new("solar_magic::menu_bar::registry")
}

/// Show a top-level menu button inside an [`egui::MenuBar`].
///
/// Returns the button response plus the menu contents' result when the menu
/// was open this frame, matching [`egui::Ui::menu_button`].
pub(crate) fn menu_button<R>(
    ui: &mut Ui,
    label: &str,
    content: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<Option<R>> {
    let response = ui.button(label);
    let popup_id = Popup::default_response_id(&response);

    let siblings = ui.data_mut(|data| {
        let menus = data.get_temp_mut_or_default::<BarMenus>(registry_id());
        if !menus.0.contains(&popup_id) {
            menus.0.push(popup_id);
        }
        menus.0.clone()
    });
    let is_open = Popup::is_id_open(ui.ctx(), popup_id);
    let sibling_open =
        !is_open && siblings.iter().any(|id| *id != popup_id && Popup::is_id_open(ui.ctx(), *id));

    let set = if response.clicked() {
        Some(SetOpenCommand::Toggle)
    } else if response.hovered() && sibling_open {
        Some(SetOpenCommand::Bool(true))
    } else {
        None
    };

    let config = MenuConfig::find(ui);
    let inner = Popup::menu(&response)
        .open_memory(set)
        .close_behavior(config.close_behavior)
        .style(config.style)
        .show(content);
    InnerResponse::new(inner.map(|i| i.inner), response)
}

#[cfg(test)]
mod tests {
    use egui::{Context, Event, PointerButton, Pos2, RawInput, Rect, Vec2};

    use super::menu_button;

    /// Which menus rendered their contents this frame, plus the button centers.
    #[derive(Default)]
    struct Frame {
        file_open: bool,
        edit_open: bool,
        file_pos: Pos2,
        edit_pos: Pos2,
    }

    fn run(ctx: &Context, events: Vec<Event>) -> Frame {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            events,
            ..RawInput::default()
        };
        let mut frame = Frame::default();
        let mut output = ctx.run_ui(input, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let file = menu_button(ui, "File", |ui| {
                    ui.label("Open");
                    frame.file_open = true;
                });
                frame.file_pos = file.response.rect.center();
                let edit = menu_button(ui, "Edit", |ui| {
                    ui.label("Undo");
                    frame.edit_open = true;
                });
                frame.edit_pos = edit.response.rect.center();
            });
        });
        output.textures_delta.clear();
        frame
    }

    fn click(pos: Pos2) -> Vec<Event> {
        let button = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        vec![Event::PointerMoved(pos), button(true), button(false)]
    }

    #[test]
    fn hovering_a_sibling_switches_menus_only_while_one_is_open() {
        let ctx = Context::default();
        let layout = run(&ctx, Vec::new());
        let (file, edit) = (layout.file_pos, layout.edit_pos);

        // Hovering with nothing open leaves every menu closed.
        let hover = run(&ctx, vec![Event::PointerMoved(edit)]);
        assert!(!hover.file_open && !hover.edit_open);

        // A click opens the menu under the pointer.
        let opened = run(&ctx, click(file));
        assert!(opened.file_open && !opened.edit_open);

        // Hovering a sibling switches to it; the old menu is gone next frame.
        let switched = run(&ctx, vec![Event::PointerMoved(edit)]);
        assert!(switched.edit_open);
        let settled = run(&ctx, Vec::new());
        assert!(settled.edit_open && !settled.file_open);

        // Hovering back switches again, and Escape closes everything.
        let back = run(&ctx, vec![Event::PointerMoved(file)]);
        assert!(back.file_open);
        let _ = run(
            &ctx,
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let closed = run(&ctx, Vec::new());
        assert!(!closed.file_open && !closed.edit_open);
    }
}
