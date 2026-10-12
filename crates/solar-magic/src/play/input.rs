//! Keyboard to controller mapping.

use egui::Key;
use snes_core::{Buttons, Input};

/// Which key drives which button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyMap {
    /// `(key, button)` pairs.
    pub(crate) bindings: Vec<(Key, Buttons)>,
}

impl Default for KeyMap {
    fn default() -> Self {
        Self {
            bindings: vec![
                (Key::ArrowUp, Buttons::UP),
                (Key::ArrowDown, Buttons::DOWN),
                (Key::ArrowLeft, Buttons::LEFT),
                (Key::ArrowRight, Buttons::RIGHT),
                (Key::Z, Buttons::B),
                (Key::X, Buttons::A),
                (Key::A, Buttons::Y),
                (Key::S, Buttons::X),
                (Key::Q, Buttons::L),
                (Key::W, Buttons::R),
                (Key::Enter, Buttons::START),
                (Key::Backspace, Buttons::SELECT),
            ],
        }
    }
}

impl KeyMap {
    /// A readable summary for the panel.
    pub(crate) const fn summary() -> &'static str {
        "arrows: D-pad   Z: B   X: A   A: Y   S: X   Q/W: L/R   Enter: Start   Backspace: Select"
    }
}

/// Reads the held keys into controller 1.
pub(crate) fn read_input(ui: &egui::Ui, keys: &KeyMap) -> Input {
    let mut p1 = Buttons::empty();
    ui.input(|i| {
        for &(key, button) in &keys.bindings {
            if i.key_down(key) {
                p1 |= button;
            }
        }
    });
    Input { p1, p2: Buttons::empty() }
}
