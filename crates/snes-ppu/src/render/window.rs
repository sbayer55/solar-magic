//! Window masks: for each layer, whether window masking is active at each
//! x on the main and sub screens, and the colour-math window.
//! fullsnes "SNES PPU Window".

use crate::regs::Regs;

/// Per-line window state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Windows {
    /// `[layer][x]`: inside the layer's window (BG1-4, OBJ, colour).
    inside: Box<[[bool; 256]; 6]>,
}

impl Default for Windows {
    fn default() -> Self {
        Self { inside: Box::new([[false; 256]; 6]) }
    }
}

/// Layer indexes into the mask arrays.
pub(crate) const WIN_OBJ: usize = 4;
pub(crate) const WIN_COLOR: usize = 5;

const fn combine(logic: u8, a: bool, b: bool) -> bool {
    match logic & 3 {
        0 => a || b,
        1 => a && b,
        2 => a ^ b,
        _ => !(a ^ b),
    }
}

impl Windows {
    /// Rebuilds the masks from the window registers.
    pub(crate) fn rebuild(&mut self, regs: &Regs) {
        let [w1l, w1r, w2l, w2r] = regs.window_pos;
        for layer in 0..6usize {
            // Four settings bits per layer: w1 invert, w1 enable, w2 invert, w2 enable.
            let sel = match layer {
                0 => regs.window_sel[0],
                1 => regs.window_sel[0] >> 4,
                2 => regs.window_sel[1],
                3 => regs.window_sel[1] >> 4,
                4 => regs.window_sel[2],
                _ => regs.window_sel[2] >> 4,
            };
            let logic = match layer {
                0..=3 => regs.window_bg_logic >> (layer.wrapping_mul(2)),
                4 => regs.window_obj_logic,
                _ => regs.window_obj_logic >> 2,
            };
            let w1_enable = sel & 0x02 != 0;
            let w1_invert = sel & 0x01 != 0;
            let w2_enable = sel & 0x08 != 0;
            let w2_invert = sel & 0x04 != 0;
            let Some(row) = self.inside.get_mut(layer) else { continue };
            for (x, slot) in row.iter_mut().enumerate() {
                let xi = u8::try_from(x).unwrap_or(0);
                let in1 = (xi >= w1l && xi <= w1r) != w1_invert;
                let in2 = (xi >= w2l && xi <= w2r) != w2_invert;
                *slot = match (w1_enable, w2_enable) {
                    (false, false) => false,
                    (true, false) => in1,
                    (false, true) => in2,
                    (true, true) => combine(logic, in1, in2),
                };
            }
        }
    }

    /// Whether `layer` is inside its window at `x`.
    pub(crate) fn inside(&self, layer: usize, x: usize) -> bool {
        self.inside.get(layer).and_then(|row| row.get(x)).copied().unwrap_or(false)
    }
}
