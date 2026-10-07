//! Sprite position markers: an outlined 16×16 box with the sprite number,
//! drawn where the sprite would spawn. Real sprite graphics come later.

use smw_data::level::Sprite;

use crate::{Raster, Rect, Rgba8};

/// Marker fill colour (translucent-looking magenta, drawn opaque).
pub const MARKER_FILL: Rgba8 = [0xC0, 0x20, 0xC0, 0xFF];
/// Marker outline and digit colour.
pub const MARKER_INK: Rgba8 = [0xFF, 0xFF, 0xFF, 0xFF];
/// Colour for scroll commands (sprite numbers `$E7` and up).
pub const COMMAND_FILL: Rgba8 = [0x20, 0x80, 0xC0, 0xFF];

/// 3×5 glyphs for the hex digits, one bit per pixel, rows top to bottom.
const DIGITS: [[u8; 5]; 16] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b001, 0b001], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
    [0b111, 0b101, 0b111, 0b101, 0b101], // A
    [0b110, 0b101, 0b110, 0b101, 0b110], // B
    [0b111, 0b100, 0b100, 0b100, 0b111], // C
    [0b110, 0b101, 0b101, 0b101, 0b110], // D
    [0b111, 0b100, 0b111, 0b100, 0b111], // E
    [0b111, 0b100, 0b111, 0b100, 0b100], // F
];

/// Draws one hex digit (0–15) with its top-left corner at `(x, y)`.
pub fn draw_hex_digit(raster: &mut Raster, (x, y): (u32, u32), digit: u8, color: Rgba8) {
    let Some(glyph) = DIGITS.get(usize::from(digit & 0x0F)) else {
        return;
    };
    for (row, bits) in glyph.iter().enumerate() {
        for col in 0..3u32 {
            if bits & (0b100 >> col) != 0 {
                raster.put(
                    x.saturating_add(col),
                    y.saturating_add(u32::try_from(row).unwrap_or(0)),
                    color,
                );
            }
        }
    }
}

/// Draws a two-digit hex number with its top-left corner at `(x, y)`.
pub fn draw_hex_byte(raster: &mut Raster, (x, y): (u32, u32), value: u8, color: Rgba8) {
    draw_hex_digit(raster, (x, y), value >> 4, color);
    draw_hex_digit(raster, (x.saturating_add(4), y), value & 0x0F, color);
}

/// Level tile position of a sprite. Vertical levels count screens downward
/// and use the X field as the column within the 32-wide level's left/right
/// half selected by the screen's low bit.
#[must_use]
pub fn sprite_tile_position(sprite: Sprite, vertical: bool) -> (u32, u32) {
    let screen = u32::from(sprite.screen());
    let (x, y) = (u32::from(sprite.x()), u32::from(sprite.y()));
    if vertical {
        // In vertical levels the Y field is the row within the screen and the
        // X field spans the two halves via bit 4 of the (swapped) nibbles.
        (
            x.saturating_add((screen & 1).saturating_mul(16)),
            (screen >> 1).saturating_mul(16).saturating_add(y),
        )
    } else {
        (screen.saturating_mul(16).saturating_add(x), y)
    }
}

/// Draws a marker for every sprite in the list.
pub fn draw_sprite_markers(raster: &mut Raster, sprites: &[Sprite], vertical: bool) {
    for sprite in sprites {
        let (tx, ty) = sprite_tile_position(*sprite, vertical);
        let (px, py) = (tx.saturating_mul(16), ty.saturating_mul(16));
        let fill = if sprite.is_scroll_command() { COMMAND_FILL } else { MARKER_FILL };
        raster.fill_rect(Rect { x: px, y: py, w: 16, h: 16 }, fill);
        outline(raster, Rect { x: px, y: py, w: 16, h: 16 }, MARKER_INK);
        draw_hex_byte(
            raster,
            (px.saturating_add(4), py.saturating_add(5)),
            sprite.number(),
            MARKER_INK,
        );
    }
}

fn outline(raster: &mut Raster, r: Rect, color: Rgba8) {
    raster.fill_rect(Rect { x: r.x, y: r.y, w: r.w, h: 1 }, color);
    raster.fill_rect(
        Rect { x: r.x, y: r.y.saturating_add(r.h.saturating_sub(1)), w: r.w, h: 1 },
        color,
    );
    raster.fill_rect(Rect { x: r.x, y: r.y, w: 1, h: r.h }, color);
    raster.fill_rect(
        Rect { x: r.x.saturating_add(r.w.saturating_sub(1)), y: r.y, w: 1, h: r.h },
        color,
    );
}

#[cfg(test)]
mod tests {
    use smw_data::level::Sprite;

    use super::{MARKER_INK, draw_hex_digit, draw_sprite_markers, sprite_tile_position};
    use crate::Raster;

    #[test]
    fn digit_glyph_pixels() {
        let mut r = Raster::new(4, 6);
        draw_hex_digit(&mut r, (0, 0), 1, MARKER_INK);
        assert_eq!(r.get(1, 0), Some(MARKER_INK));
        assert_eq!(r.get(0, 0), Some([0, 0, 0, 0]));
        assert_eq!(r.get(2, 4), Some(MARKER_INK));
    }

    #[test]
    fn positions_and_marker() {
        // screen 0x13, x 9, y 0x16
        let s = Sprite { bytes: [0x60 | 0x08 | 0x02 | 0x01, 0x90 | 0x03, 0x1E] };
        assert_eq!(sprite_tile_position(s, false), (0x13 * 16 + 9, 0x16));
        assert_eq!(sprite_tile_position(s, true), (9 + 16, 9 * 16 + 0x16));
        let mut r = Raster::new(0x140 * 16, 27 * 16);
        draw_sprite_markers(&mut r, &[s], false);
        assert_eq!(r.get((0x13 * 16 + 9) * 16, 0x16 * 16), Some(MARKER_INK));
    }
}
