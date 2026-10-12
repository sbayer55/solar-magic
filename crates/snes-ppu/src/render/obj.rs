//! Sprite evaluation and rendering for one line: up to 32 sprites and 34
//! 8-pixel tiles, highest-priority sprites dropped first when the tile
//! budget runs out (bsnes `Object::render`). With `SETINI` bit 1 sprites
//! are half height and each field shows alternate rows (ares
//! `Object::fetch`).

use super::pixel::Px;
use crate::oam::Oam;
use crate::regs::Regs;
use crate::vram::Vram;

/// (small, large) sizes by `OBSEL` bits 5-7, as (width, height).
const SIZES: [[(u8, u8); 2]; 8] = [
    [(8, 8), (16, 16)],
    [(8, 8), (32, 32)],
    [(8, 8), (64, 64)],
    [(16, 16), (32, 32)],
    [(16, 16), (64, 64)],
    [(32, 32), (64, 64)],
    [(16, 32), (32, 64)],
    [(16, 32), (32, 32)],
];

#[derive(Debug, Clone, Copy)]
struct Sprite {
    x: i16,
    row: u8,
    width: u8,
    height: u8,
    tile: u8,
    name_table: bool,
    palette: u8,
    priority: u8,
    hflip: bool,
    vflip: bool,
}

fn sprite(oam: &Oam, regs: &Regs, n: u8, v: u16) -> Option<Sprite> {
    let base = u16::from(n).wrapping_mul(4);
    let x_low = oam.byte(base);
    let y = oam.byte(base.wrapping_add(1));
    let tile = oam.byte(base.wrapping_add(2));
    let attr = oam.byte(base.wrapping_add(3));
    let high = oam.byte(0x200u16.wrapping_add(u16::from(n >> 2)));
    let shift = (n & 3).wrapping_mul(2);
    let x_high = (high >> shift) & 1 != 0;
    let large = (high >> shift) & 2 != 0;
    let (width, height) = SIZES
        .get(usize::from(regs.obj_size))
        .and_then(|pair| pair.get(usize::from(large)))
        .copied()
        .unwrap_or((8, 8));
    // Sprites appear one line below their Y: line v shows sprite row v-1-y.
    let row = crate::bits::lo(v.wrapping_sub(1).wrapping_sub(u16::from(y)) & 0xFF);
    let visible_height = if regs.obj_interlace { height >> 1 } else { height };
    if row >= visible_height {
        return None;
    }
    let x = if x_high { i16::from(x_low).wrapping_sub(256) } else { i16::from(x_low) };
    if x <= i16::from(width).wrapping_neg() && x != -256 {
        return None;
    }
    Some(Sprite {
        x,
        row,
        width,
        height,
        tile,
        name_table: attr & 1 != 0,
        palette: (attr >> 1) & 7,
        priority: (attr >> 4) & 3,
        hflip: attr & 0x40 != 0,
        vflip: attr & 0x80 != 0,
    })
}

/// Range-over and time-over flags from one line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ObjFlags {
    pub range_over: bool,
    pub time_over: bool,
}

/// What one sprite line needs.
#[derive(Clone, Copy)]
pub(crate) struct ObjLine<'a> {
    pub vram: &'a Vram,
    pub oam: &'a Oam,
    pub regs: &'a Regs,
    /// Scanline being rendered (1-based).
    pub v: u16,
    /// Interlace field (only matters with `SETINI` bit 1).
    pub field: bool,
}

/// The row of the sprite's character data shown on this line: doubled and
/// offset by the field in OBJ interlace, flipped per 16/32-pixel half for
/// the tall sprite sizes (ares `Object::fetch`).
fn tile_row(s: &Sprite, interlace: bool, field: bool) -> u8 {
    let mut y = if interlace { s.row << 1 } else { s.row };
    if s.vflip {
        y = if s.width == s.height {
            s.height.wrapping_sub(1).wrapping_sub(y)
        } else if y < s.width {
            s.width.wrapping_sub(1).wrapping_sub(y)
        } else {
            s.width.wrapping_add(s.width.wrapping_sub(1)).wrapping_sub(y.wrapping_sub(s.width))
        };
    }
    if interlace {
        let f = u8::from(field);
        y = if s.vflip { y.wrapping_sub(f) } else { y.wrapping_add(f) };
    }
    y
}

/// Renders the sprites visible on the line into `out`.
pub(crate) fn render_line(line: &ObjLine<'_>, out: &mut [Px; 256]) -> ObjFlags {
    let ObjLine { vram, oam, regs, v, field } = *line;
    out.fill(Px::TRANSPARENT);
    let mut flags = ObjFlags::default();
    let mut items: Vec<Sprite> = Vec::with_capacity(32);
    for i in 0..128u8 {
        let n = oam.first_sprite.wrapping_add(i) & 0x7F;
        if let Some(s) = sprite(oam, regs, n, v) {
            if items.len() == 32 {
                flags.range_over = true;
                break;
            }
            items.push(s);
        }
    }
    let mut tiles = 0u8;
    // Reverse order: lowest-priority sprites draw first and get the tile budget.
    for s in items.iter().rev() {
        let columns = s.width / 8;
        let ry = tile_row(s, regs.obj_interlace, field);
        for col in 0..columns {
            if tiles >= 34 {
                flags.time_over = true;
                break;
            }
            tiles = tiles.wrapping_add(1);
            let cx = if s.hflip { columns.wrapping_sub(1).wrapping_sub(col) } else { col };
            draw_tile(vram, regs, s, TilePos { cx, ry, col }, out);
        }
    }
    flags
}

/// Which 8×8 character of a sprite to draw, and where.
#[derive(Clone, Copy)]
struct TilePos {
    /// Character column within the sprite (after flipping).
    cx: u8,
    /// Row within the sprite (after flipping).
    ry: u8,
    /// Screen column within the sprite.
    col: u8,
}

fn draw_tile(vram: &Vram, regs: &Regs, s: &Sprite, pos: TilePos, out: &mut [Px; 256]) {
    let TilePos { cx, ry, col } = pos;
    // Characters advance through the low nibble horizontally and the high
    // nibble vertically, each wrapping within its nibble.
    let ch = ((s.tile & 0xF0).wrapping_add((ry >> 3) << 4) & 0xF0)
        | ((s.tile & 0x0F).wrapping_add(cx) & 0x0F);
    let table = if s.name_table {
        regs.obj_tile_addr.wrapping_add(u16::from(regs.obj_name_select).wrapping_add(1) << 12)
    } else {
        regs.obj_tile_addr
    };
    let base = table.wrapping_add(u16::from(ch).wrapping_mul(16)).wrapping_add(u16::from(ry & 7));
    let [p0, p1] = vram.word(base & 0x7FFF).to_le_bytes();
    let [p2, p3] = vram.word(base.wrapping_add(8) & 0x7FFF).to_le_bytes();
    let x0 = i32::from(s.x).wrapping_add(i32::from(col).wrapping_mul(8));
    for px in 0..8u8 {
        let bit = if s.hflip { px } else { 7u8.wrapping_sub(px) };
        let color = ((p0 >> bit) & 1)
            | (((p1 >> bit) & 1) << 1)
            | (((p2 >> bit) & 1) << 2)
            | (((p3 >> bit) & 1) << 3);
        if color == 0 {
            continue;
        }
        let sx = x0.wrapping_add(i32::from(px));
        if let Ok(ix) = usize::try_from(sx)
            && let Some(slot) = out.get_mut(ix)
        {
            let index = 128u8.wrapping_add(s.palette.wrapping_mul(16)).wrapping_add(color);
            *slot = Px::new(index, s.priority, s.palette >= 4, 0);
        }
    }
}
