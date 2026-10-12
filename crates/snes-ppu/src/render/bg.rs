//! Background line rendering for modes 0-6: tilemap lookup, planar
//! character decode, 16×16 tiles, flips, mosaic, the 512-half-pixel
//! hi-res modes (5/6), interlaced rows and offset-per-tile (modes 2/4/6).
//! Follows ares `Background::fetchNameTable`/`fetchOffset`.

use super::pixel::Px;
use crate::regs::BgRegs;
use crate::vram::Vram;

/// Decodes one 8-pixel row of a planar character (`bpp` 2/4/8).
fn decode_row(vram: &Vram, tile_addr: u16, ch: u16, row: u8, bpp: u8) -> [u8; 8] {
    let words_per_char = u16::from(bpp).wrapping_mul(4);
    let base =
        tile_addr.wrapping_add(ch.wrapping_mul(words_per_char)).wrapping_add(u16::from(row & 7));
    let mut out = [0u8; 8];
    let planes = usize::from(bpp / 2);
    for plane_pair in 0..planes {
        let word =
            vram.word(base.wrapping_add(u16::try_from(plane_pair.wrapping_mul(8)).unwrap_or(0)));
        let [p0, p1] = word.to_le_bytes();
        let shift = u8::try_from(plane_pair.wrapping_mul(2)).unwrap_or(0);
        for (x, slot) in out.iter_mut().enumerate() {
            let bit = 7u8.wrapping_sub(u8::try_from(x).unwrap_or(0));
            let b0 = (p0 >> bit) & 1;
            let b1 = (p1 >> bit) & 1;
            *slot |= (b0 | (b1 << 1)) << shift;
        }
    }
    out
}

/// Word address of the tilemap entry at map coordinates (`tx`, `ty`), in
/// entries, for the layer's screen size.
const fn map_entry_addr(bg: &BgRegs, tx: u16, ty: u16) -> u16 {
    let screen = match bg.screen_size {
        0 => 0,
        1 => (tx >> 5) & 1,
        2 => (ty >> 5) & 1,
        _ => ((tx >> 5) & 1) | (((ty >> 5) & 1) << 1),
    };
    bg.screen_addr
        .wrapping_add(screen.wrapping_mul(0x400))
        .wrapping_add((ty & 31).wrapping_mul(32))
        .wrapping_add(tx & 31)
}

/// The offset-per-tile source (BG3's tilemap) for one layer.
pub(crate) struct Opt<'a> {
    /// BG3 registers.
    pub bg3: &'a BgRegs,
    /// Bit of an entry that enables it for this layer (13 BG1, 14 BG2).
    pub valid_bit: u16,
    /// Mode 4: one entry whose bit 15 picks horizontal or vertical.
    pub mode4: bool,
}

impl Opt<'_> {
    /// The entry for output column `col` (≥ 1) at offset row `y_off`
    /// (0 = horizontal entry, 8 = vertical entry).
    fn entry(&self, vram: &Vram, col: u16, y_off: u16, hires: bool) -> u16 {
        let hoffset = col.wrapping_sub(1).wrapping_mul(8).wrapping_add(self.bg3.hofs & !7);
        let voffset = y_off.wrapping_add(self.bg3.vofs);
        let tile_h: u8 = if self.bg3.tile_16 { 4 } else { 3 };
        // ares: BG3's tiles are 8 wide in mode 6.
        let tile_w: u8 = if hires { 3 } else { tile_h };
        vram.word(map_entry_addr(self.bg3, hoffset >> tile_w, voffset >> tile_h))
    }

    /// Returns `(hoffset, voffset)` replacements for column `col`.
    fn apply(&self, vram: &Vram, col: u16, hires: bool) -> (Option<u16>, Option<u16>) {
        if col == 0 {
            return (None, None);
        }
        let h = self.entry(vram, col, 0, hires);
        if self.mode4 {
            if h & self.valid_bit == 0 {
                return (None, None);
            }
            return if h & 0x8000 == 0 { (Some(h), None) } else { (None, Some(h)) };
        }
        let v = self.entry(vram, col, 8, hires);
        ((h & self.valid_bit != 0).then_some(h), (v & self.valid_bit != 0).then_some(v))
    }
}

/// What one BG line needs.
pub(crate) struct BgLine<'a> {
    pub vram: &'a Vram,
    pub bg: &'a BgRegs,
    pub bpp: u8,
    /// CGRAM offset added to every index (mode 0: 32 × layer).
    pub palette_base: u8,
    /// Scanline being rendered (1-based).
    pub v: u16,
    /// Mosaic block size (1 = off).
    pub mosaic: u8,
    /// Mode 5/6: 512 half-pixels, tiles 16 half-pixels wide.
    pub hires: bool,
    /// The field when the frame is interlaced (hi-res modes double the
    /// vertical resolution).
    pub field: Option<bool>,
    /// Offset-per-tile source (modes 2/4/6, BG1 and BG2 only).
    pub opt: Option<Opt<'a>>,
}

/// Geometry shared by every chunk of a line.
struct Geometry {
    width: u16,
    tile_w: u16,
    tile_h: u16,
    /// Scroll in output pixels (doubled in hi-res).
    hscroll: u16,
    /// Scroll remainder within an output column.
    fine: u16,
    /// log2 of the output pixels per column (3, or 4 in hi-res).
    col_shift: u8,
    /// Row of the tilemap in pixels before the layer's scroll.
    y: u16,
}

impl Geometry {
    fn new(line: &BgLine<'_>) -> Self {
        let mosaic = u16::from(line.mosaic.max(1));
        // Mosaic repeats the first line of each block of `mosaic` lines.
        let v = if mosaic > 1 {
            let y0 = line.v.wrapping_sub(1);
            y0.wrapping_sub(y0.checked_rem(mosaic).unwrap_or(0)).wrapping_add(1)
        } else {
            line.v
        };
        let y = match (line.hires, line.field) {
            (true, Some(field)) => (v << 1) | u16::from(field && mosaic == 1),
            _ => v,
        };
        let hscroll = if line.hires { line.bg.hofs << 1 } else { line.bg.hofs };
        let col_shift: u8 = if line.hires { 4 } else { 3 };
        Self {
            width: if line.hires { 512 } else { 256 },
            tile_w: if line.hires || line.bg.tile_16 { 16 } else { 8 },
            tile_h: if line.bg.tile_16 { 16 } else { 8 },
            hscroll,
            fine: hscroll & ((1u16 << col_shift).wrapping_sub(1)),
            col_shift,
            y,
        }
    }
}

/// A decoded tilemap entry.
struct Entry {
    ch: u16,
    palette: u8,
    priority: u8,
    hflip: bool,
    vflip: bool,
}

impl Entry {
    const fn decode(entry: u16) -> Self {
        Self {
            ch: entry & 0x3FF,
            palette: crate::bits::lo((entry >> 10) & 7),
            priority: crate::bits::lo((entry >> 13) & 1),
            hflip: entry & 0x4000 != 0,
            vflip: entry & 0x8000 != 0,
        }
    }
}

/// Renders one line of a background into `out` (`256 << hires` pixels).
pub(crate) fn render_line(line: &BgLine<'_>, out: &mut [Px; 512]) {
    let g = Geometry::new(line);
    let colors_per_palette: u8 = match line.bpp {
        2 => 4,
        4 => 16,
        _ => 0,
    };
    let mut x: u16 = 0;
    while x < g.width {
        // One chunk is the rest of the current 8-pixel character column.
        let in_col = x.wrapping_add(g.fine);
        let run = 8u16.wrapping_sub(in_col & 7).min(g.width.wrapping_sub(x));
        let (mut hx, mut vy) = (g.hscroll.wrapping_add(x), g.y.wrapping_add(line.bg.vofs));
        if let Some(opt) = &line.opt {
            let (h, v) = opt.apply(line.vram, in_col >> g.col_shift, line.hires);
            if let Some(h) = h {
                let coarse = if line.hires { (h & !7) << 1 } else { h & !7 };
                hx = in_col.wrapping_add(coarse);
            }
            if let Some(v) = v {
                vy = g.y.wrapping_add(v);
            }
        }
        let tile_x = hx.checked_div(g.tile_w).unwrap_or(0);
        let tile_y = vy.checked_div(g.tile_h).unwrap_or(0);
        let entry = Entry::decode(line.vram.word(map_entry_addr(line.bg, tile_x, tile_y)));
        let ty = vy.checked_rem(g.tile_h).unwrap_or(0);
        let ty = if entry.vflip { g.tile_h.wrapping_sub(1).wrapping_sub(ty) } else { ty };
        let tx0 = hx.checked_rem(g.tile_w).unwrap_or(0);
        let flip =
            |tx: u16| if entry.hflip { g.tile_w.wrapping_sub(1).wrapping_sub(tx) } else { tx };
        // 16-wide tiles use the next character for their right half, 16-high
        // tiles the character 16 further down.
        let ch16 = entry.ch.wrapping_add(u16::from(flip(tx0) >= 8)).wrapping_add(if ty >= 8 {
            16
        } else {
            0
        });
        let row = decode_row(
            line.vram,
            line.bg.tile_addr,
            ch16 & 0x3FF,
            crate::bits::lo(ty & 7),
            line.bpp,
        );
        for i in 0..run {
            let color = row.get(usize::from(flip(tx0.wrapping_add(i)) & 7)).copied().unwrap_or(0);
            let px = if color == 0 {
                Px::TRANSPARENT
            } else {
                let index = line
                    .palette_base
                    .wrapping_add(entry.palette.wrapping_mul(colors_per_palette))
                    .wrapping_add(color);
                Px::new(index, entry.priority, true, entry.palette)
            };
            if let Some(slot) = out.get_mut(usize::from(x.wrapping_add(i))) {
                *slot = px;
            }
        }
        x = x.wrapping_add(run);
    }
    if line.mosaic > 1 {
        // Mosaic counts full pixels: a block is `mosaic` dots wide.
        let block = usize::from(line.mosaic) << u8::from(line.hires);
        apply_mosaic(out, usize::from(g.width), block);
    }
}

/// Repeats the first pixel of each horizontal block.
fn apply_mosaic(out: &mut [Px; 512], width: usize, block: usize) {
    let Some(line) = out.get_mut(..width) else { return };
    for chunk in line.chunks_mut(block.max(1)) {
        if let Some(&first) = chunk.first() {
            for slot in chunk.iter_mut().skip(1) {
                *slot = first;
            }
        }
    }
}
