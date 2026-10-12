//! The renderer: layer line buffers built at the start of each scanline,
//! composited dot by dot as the raster advances (`Ppu::run_to`), so register
//! writes to the compositor's inputs (brightness, screens, windows, colour
//! math) land on the exact dot while tile fetches use the line-start state.

mod bg;
mod color;
mod mode7;
mod obj;
mod pixel;
mod window;

use color::{BPP, Layer, PRIORITY};
use pixel::Px;
use window::{WIN_COLOR, WIN_OBJ, Windows};

use crate::frame::Frame;
use crate::{Position, Ppu};

/// First dot of the 256 output pixels.
const FIRST_OUTPUT_DOT: u16 = 22;
/// Dots per line.
const DOTS_PER_LINE: u16 = 340;

/// Layer line buffers and raster position.
#[derive(Debug, Clone)]
pub(crate) struct Renderer {
    bg: Box<[[Px; 256]; 4]>,
    obj: Box<[Px; 256]>,
    windows: Windows,
    pub(crate) frame: Frame,
    cur_v: u16,
    cur_dot: u16,
    line_built: bool,
    /// Debug mask: bits 0-3 BG1-4, bit 4 OBJ.
    pub(crate) layer_mask: u8,
    pub(crate) obj_flags: obj::ObjFlags,
}

impl Default for Renderer {
    fn default() -> Self {
        Self {
            bg: Box::new([[Px::TRANSPARENT; 256]; 4]),
            obj: Box::new([Px::TRANSPARENT; 256]),
            windows: Windows::default(),
            frame: Frame::new(),
            cur_v: 0,
            cur_dot: 0,
            line_built: false,
            layer_mask: 0x1F,
            obj_flags: obj::ObjFlags::default(),
        }
    }
}

impl Renderer {
    /// Back to line 0, dot 0.
    pub(crate) const fn restart_frame(&mut self) {
        self.cur_v = 0;
        self.cur_dot = 0;
        self.line_built = false;
        self.obj_flags = obj::ObjFlags { range_over: false, time_over: false };
    }
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
    /// Advances the renderer to `pos`, drawing every output dot passed.
    pub fn run_to(&mut self, pos: Position) {
        loop {
            let (v, dot) = (self.render.cur_v, self.render.cur_dot);
            if pos.v < v || (pos.v == v && pos.h_dot <= dot) {
                return;
            }
            if dot == 0 && !self.render.line_built {
                self.line_start(v, pos.vdisp);
                self.render.line_built = true;
            }
            let end = if v == pos.v { pos.h_dot } else { DOTS_PER_LINE };
            self.render_dots(v, dot, end, pos.vdisp);
            if end >= DOTS_PER_LINE {
                self.render.cur_v = v.wrapping_add(1);
                self.render.cur_dot = 0;
                self.render.line_built = false;
            } else {
                self.render.cur_dot = end;
            }
        }
    }

    /// The last completed picture (rows fill in as the frame progresses).
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.render.frame
    }

    /// Debug: which layers to draw (bits 0-3 BG1-4, bit 4 OBJ).
    pub const fn set_layer_mask(&mut self, mask: u8) {
        self.render.layer_mask = mask;
    }

    fn line_start(&mut self, v: u16, vdisp: u16) {
        self.render.frame.height = usize::from(vdisp.saturating_sub(1));
        if v == 0 || v >= vdisp || self.regs.forced_blank {
            return;
        }
        self.render.windows.rebuild(&self.regs);
        let flags = obj::render_line(&self.vram, &self.oam, &self.regs, v, &mut self.render.obj);
        self.render.obj_flags.range_over |= flags.range_over;
        self.render.obj_flags.time_over |= flags.time_over;
        self.status.range_over |= flags.range_over;
        self.status.time_over |= flags.time_over;
        let mode = usize::from(self.regs.bg_mode);
        if mode == 7 {
            let (bg1, rest) = self.render.bg.split_at_mut(1);
            if let (Some(b1), Some(b2)) = (bg1.first_mut(), rest.first_mut()) {
                mode7::render_line(&self.vram, &self.regs.m7, v, false, b1);
                if self.regs.extbg {
                    mode7::render_line(&self.vram, &self.regs.m7, v, true, b2);
                } else {
                    b2.fill(Px::TRANSPARENT);
                }
            }
            return;
        }
        for n in 0..4 {
            let bpp = BPP.get(mode).and_then(|m| m.get(n)).copied().unwrap_or(0);
            let Some(buf) = self.render.bg.get_mut(n) else { continue };
            if bpp == 0 {
                buf.fill(Px::TRANSPARENT);
                continue;
            }
            let Some(bg) = self.regs.bg.get(n) else { continue };
            let line = bg::BgLine {
                vram: &self.vram,
                bg,
                bpp,
                palette_base: if mode == 0 {
                    u8::try_from(n.wrapping_mul(32)).unwrap_or(0)
                } else {
                    0
                },
                v,
                mosaic: if bg.mosaic { self.regs.mosaic_size.wrapping_add(1) } else { 1 },
            };
            bg::render_line(&line, buf);
        }
    }

    fn render_dots(&mut self, v: u16, from: u16, to: u16, vdisp: u16) {
        if v == 0 || v >= vdisp {
            return;
        }
        let row = usize::from(v.wrapping_sub(1));
        let last = to.min(FIRST_OUTPUT_DOT.wrapping_add(256));
        let mut dot = from.max(FIRST_OUTPUT_DOT);
        while dot < last {
            let x = usize::from(dot.wrapping_sub(FIRST_OUTPUT_DOT));
            let color = if self.regs.forced_blank { 0 } else { self.composite(x) };
            self.render.frame.set_pixel(x, row, color);
            dot = dot.wrapping_add(1);
        }
    }

    fn layer_px(&self, layer: Layer, x: usize) -> Px {
        match layer {
            Layer::Bg(n) => self.render.bg.get(usize::from(n)).and_then(|b| b.get(x)).copied(),
            Layer::Obj => self.render.obj.get(x).copied(),
        }
        .unwrap_or(Px::TRANSPARENT)
    }

    /// Highest-priority opaque pixel of the enabled, unmasked layers.
    fn resolve(&self, x: usize, enabled: u8, masked: u8) -> Option<(Px, Layer)> {
        let mode = usize::from(self.regs.bg_mode);
        let visible = |layer: Layer, px: Px| -> bool {
            let bit = layer_bit(layer);
            enabled & bit != 0
                && self.render.layer_mask & bit != 0
                && px.opaque()
                && !(masked & bit != 0 && self.render.windows.inside(window_index(layer), x))
                && !(mode == 7 && layer == Layer::Bg(1) && !self.regs.extbg)
        };
        if mode == 1 && self.regs.bg3_priority {
            let px = self.layer_px(Layer::Bg(2), x);
            if px.priority() == 1 && visible(Layer::Bg(2), px) {
                return Some((px, Layer::Bg(2)));
            }
        }
        for &(layer, prio) in PRIORITY.get(mode)?.iter().flatten() {
            let px = self.layer_px(layer, x);
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

    fn composite(&self, x: usize) -> u16 {
        let regs = &self.regs;
        let inside = self.render.windows.inside(WIN_COLOR, x);
        let clip = region(regs.cgwsel >> 6, inside);
        let prevent = region(regs.cgwsel >> 4, inside);
        let main = self.resolve(x, regs.main_screen, regs.main_window);
        let (main_color, main_bit, math_ok) = match main {
            Some((px, layer)) => (self.color_of(px, layer), layer_bit(layer), px.math_ok()),
            None => (self.cgram.color(0), 1 << 5, true),
        };
        let mut color = if clip { 0 } else { main_color };
        if regs.cgadsub & main_bit != 0 && math_ok && !prevent {
            let use_sub = regs.cgwsel & 2 != 0;
            let (sub_color, sub_is_fixed) = if use_sub {
                match self.resolve(x, regs.sub_screen, regs.sub_window) {
                    Some((px, layer)) => (self.color_of(px, layer), false),
                    None => (regs.fixed_color, true),
                }
            } else {
                (regs.fixed_color, false)
            };
            let half = regs.cgadsub & 0x40 != 0 && !clip && !(use_sub && sub_is_fixed);
            color = color::blend(color, sub_color, regs.cgadsub & 0x80 != 0, half);
        }
        color::brightness(color, regs.brightness)
    }
}
