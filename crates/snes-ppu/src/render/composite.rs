//! The per-dot compositor: picks the top layer of the main and sub screens,
//! applies windows, colour math and brightness, and in hi-res emits the
//! sub-screen half-pixel to the left of the main-screen one.
//!
//! Colour math follows ares `DAC::above`/`DAC::below`: the sub half-pixel
//! of a hi-res dot is blended with the *previous* dot's main pixel, using
//! that dot's math decisions, and both screens use colour 0 as their
//! backdrop in hi-res.

use super::color::{self, Layer, PRIORITY};
use super::pixel::Px;
use super::window::{WIN_COLOR, WIN_OBJ};
use crate::Ppu;

/// Colour-math state carried from one dot to the next.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MathState {
    /// The main-screen colour (before math and brightness).
    main_color: u16,
    /// Main screen not clipped to black by the colour window.
    main_visible: bool,
    /// Colour math enabled for the main layer and not prevented.
    math_enabled: bool,
    /// Blend with the sub screen (else the fixed colour).
    use_sub: bool,
    /// Halve the result.
    half: bool,
}

/// Which screen to resolve.
#[derive(Debug, Clone, Copy)]
struct Screen {
    /// `TM`/`TS` layer bits.
    layers: u8,
    /// `TMW`/`TSW` window-mask bits.
    masked: u8,
    /// Sub screen (even half-pixel of hi-res backgrounds).
    sub: bool,
    /// Backgrounds hold 512 half-pixels (modes 5/6).
    bg_hires: bool,
}

const fn layer_bit(layer: Layer) -> u8 {
    match layer {
        Layer::Bg(n) => 1 << n,
        Layer::Obj => 1 << 4,
    }
}

fn window_index(layer: Layer) -> usize {
    match layer {
        Layer::Bg(n) => usize::from(n),
        Layer::Obj => WIN_OBJ,
    }
}

/// `CGWSEL` region select: 0 never, 1 outside, 2 inside, 3 always.
const fn region(sel: u8, inside: bool) -> bool {
    match sel & 3 {
        0 => false,
        1 => !inside,
        2 => inside,
        _ => true,
    }
}

impl Ppu {
    /// Resets the carried math state at the start of a line. The first
    /// hi-res sub half-pixel shows its own colour unblended (krom's
    /// hardware screenshots; ares blanks it and notes the power-on state
    /// is unconfirmed).
    pub(crate) fn line_math_reset(&mut self) {
        self.render.math = MathState {
            main_color: self.cgram.color(0),
            main_visible: true,
            ..MathState::default()
        };
    }

    fn layer_px(&self, layer: Layer, x: usize, screen: Screen) -> Px {
        match layer {
            Layer::Bg(n) => {
                let i = if screen.bg_hires {
                    x.wrapping_mul(2).wrapping_add(usize::from(!screen.sub))
                } else {
                    x
                };
                self.render.bg.get(usize::from(n)).and_then(|b| b.get(i)).copied()
            }
            Layer::Obj => self.render.obj.get(x).copied(),
        }
        .unwrap_or(Px::TRANSPARENT)
    }

    /// Highest-priority opaque pixel of the screen's enabled, unmasked layers.
    fn resolve(&self, x: usize, screen: Screen) -> Option<(Px, Layer)> {
        let mode = usize::from(self.regs.bg_mode);
        let visible = |layer: Layer, px: Px| -> bool {
            let bit = layer_bit(layer);
            screen.layers & bit != 0
                && self.render.layer_mask & bit != 0
                && px.opaque()
                && !(screen.masked & bit != 0 && self.render.windows.inside(window_index(layer), x))
                && !(mode == 7 && layer == Layer::Bg(1) && !self.regs.extbg)
        };
        if mode == 1 && self.regs.bg3_priority {
            let px = self.layer_px(Layer::Bg(2), x, screen);
            if px.priority() == 1 && visible(Layer::Bg(2), px) {
                return Some((px, Layer::Bg(2)));
            }
        }
        for &(layer, prio) in PRIORITY.get(mode)?.iter().flatten() {
            let px = self.layer_px(layer, x, screen);
            if px.priority() == prio && visible(layer, px) {
                return Some((px, layer));
            }
        }
        None
    }

    fn color_of(&self, px: Px, layer: Layer) -> u16 {
        let direct = self.regs.cgwsel & 1 != 0
            && layer == Layer::Bg(0)
            && matches!(self.regs.bg_mode, 3 | 4 | 7);
        if direct {
            color::direct_color(px.index(), px.palette())
        } else {
            self.cgram.color(px.index())
        }
    }

    /// The sub screen at `x`: its colour (colour 0 when empty) and whether
    /// it was empty.
    fn sub_pixel(&self, x: usize, bg_hires: bool) -> (u16, bool) {
        let screen = Screen {
            layers: self.regs.sub_screen,
            masked: self.regs.sub_window,
            sub: true,
            bg_hires,
        };
        match self.resolve(x, screen) {
            Some((px, layer)) => (self.color_of(px, layer), false),
            None => (self.cgram.color(0), true),
        }
    }

    /// Composites dot `x`, returning the (left, right) half-pixel colours;
    /// both are the main-screen colour unless `out_hires`.
    pub(crate) fn composite_dot(&mut self, x: usize, out_hires: bool) -> (u16, u16) {
        let regs = &self.regs;
        let bg_hires = matches!(regs.bg_mode, 5 | 6);
        let subtract = regs.cgadsub & 0x80 != 0;
        let fixed = regs.fixed_color;
        let inside = self.render.windows.inside(WIN_COLOR, x);
        let clip = region(regs.cgwsel >> 6, inside);
        let prevent = region(regs.cgwsel >> 4, inside);
        let main =
            Screen { layers: regs.main_screen, masked: regs.main_window, sub: false, bg_hires };
        let (main_color, main_bit, math_ok) = match self.resolve(x, main) {
            Some((px, layer)) => (self.color_of(px, layer), layer_bit(layer), px.math_ok()),
            None => (self.cgram.color(0), 1 << 5, true),
        };
        let math_enabled = regs.cgadsub & main_bit != 0 && math_ok && !prevent;
        let main_visible = !clip;
        let (sub_color, sub_empty) =
            if out_hires || math_enabled { self.sub_pixel(x, bg_hires) } else { (0, true) };
        let use_sub_io = regs.cgwsel & 2 != 0;
        let (use_sub, half) = if use_sub_io && sub_empty {
            (false, false)
        } else {
            (use_sub_io, regs.cgadsub & 0x40 != 0 && main_visible)
        };
        let prev = self.render.math;
        self.render.math = MathState { main_color, main_visible, math_enabled, use_sub, half };
        let shown = if main_visible { main_color } else { 0 };
        let right = if math_enabled {
            color::blend(shown, if use_sub { sub_color } else { fixed }, subtract, half)
        } else {
            shown
        };
        let left = if !out_hires {
            right
        } else if !prev.math_enabled {
            if prev.main_visible { sub_color } else { 0 }
        } else {
            let a = if prev.main_visible { sub_color } else { 0 };
            let b = if prev.use_sub { prev.main_color } else { fixed };
            color::blend(a, b, subtract, prev.half)
        };
        let level = self.regs.brightness;
        (color::brightness(left, level), color::brightness(right, level))
    }
}
