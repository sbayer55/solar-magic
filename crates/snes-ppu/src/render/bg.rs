//! Background line rendering for modes 0-6: tilemap lookup, planar
//! character decode, 16×16 tiles, flips, mosaic.

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
}

/// Renders one line of a background into `out`.
pub(crate) fn render_line(line: &BgLine<'_>, out: &mut [Px; 256]) {
    let mosaic = u16::from(line.mosaic.max(1));
    // Mosaic repeats the first line of each block of `mosaic` lines.
    let v = if mosaic > 1 {
        let y0 = line.v.wrapping_sub(1);
        y0.wrapping_sub(y0.checked_rem(mosaic).unwrap_or(0)).wrapping_add(1)
    } else {
        line.v
    };
    let tile_px: u16 = if line.bg.tile_16 { 16 } else { 8 };
    let tile_shift: u8 = if line.bg.tile_16 { 4 } else { 3 };
    let vy = line.bg.vofs.wrapping_add(v);
    let colors_per_palette: u8 = match line.bpp {
        2 => 4,
        4 => 16,
        _ => 0,
    };
    let mut x: u16 = 0;
    while x < 256 {
        let hx = line.bg.hofs.wrapping_add(x);
        let within = hx.checked_rem(tile_px).unwrap_or(0);
        let run = tile_px.wrapping_sub(within).min(256u16.wrapping_sub(x));
        let entry = line.vram.word(map_entry_addr(line.bg, hx >> tile_shift, vy >> tile_shift));
        let (ch, palette, priority, hflip, vflip) = (
            entry & 0x3FF,
            crate::bits::lo((entry >> 10) & 7),
            crate::bits::lo((entry >> 13) & 1),
            entry & 0x4000 != 0,
            entry & 0x8000 != 0,
        );
        let ty = vy.checked_rem(tile_px).unwrap_or(0);
        let ty = if vflip { tile_px.wrapping_sub(1).wrapping_sub(ty) } else { ty };
        for i in 0..run {
            let px_x = within.wrapping_add(i);
            let tx = if hflip { tile_px.wrapping_sub(1).wrapping_sub(px_x) } else { px_x };
            // 16×16 tiles use four consecutive characters: +1 right, +16 down.
            let ch16 =
                ch.wrapping_add(u16::from(tx >= 8)).wrapping_add(if ty >= 8 { 16 } else { 0 });
            let row = decode_row(
                line.vram,
                line.bg.tile_addr,
                ch16 & 0x3FF,
                crate::bits::lo(ty & 7),
                line.bpp,
            );
            let color = row.get(usize::from(tx & 7)).copied().unwrap_or(0);
            let px = if color == 0 {
                Px::TRANSPARENT
            } else {
                let index = line
                    .palette_base
                    .wrapping_add(palette.wrapping_mul(colors_per_palette))
                    .wrapping_add(color);
                Px::new(index, priority, true, palette)
            };
            if let Some(slot) = out.get_mut(usize::from(x.wrapping_add(i))) {
                *slot = px;
            }
        }
        x = x.wrapping_add(run);
    }
    if mosaic > 1 {
        apply_mosaic(out, mosaic);
    }
}

/// Repeats the first pixel of each horizontal block.
fn apply_mosaic(out: &mut [Px; 256], size: u16) {
    let size = usize::from(size);
    for block in out.chunks_mut(size) {
        if let Some(&first) = block.first() {
            for slot in block.iter_mut().skip(1) {
                *slot = first;
            }
        }
    }
}
