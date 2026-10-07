//! Drawing 8×8 characters and Map16 tiles.

use smw_data::map16::{Map16Tile, Tile8};
use smw_data::palette::Palette;

use crate::{CharAtlas, Raster};

/// Everything a tile draw needs besides the tile and its position.
#[derive(Debug, Clone, Copy)]
pub struct TileContext<'a> {
    /// Character graphics.
    pub atlas: &'a CharAtlas,
    /// Colours.
    pub palette: &'a Palette,
    /// Draw palette index 0 as colour 0 instead of leaving it transparent
    /// (used for the bottom layer).
    pub opaque_zero: bool,
}

/// Draws one tilemap entry with its top-left corner at `pos`.
pub fn draw_tile8(raster: &mut Raster, pos: (u32, u32), tile: Tile8, ctx: TileContext<'_>) {
    let Some(chr) = ctx.atlas.get(usize::from(tile.num)) else {
        return;
    };
    let row = ctx.palette.row_rgba8(usize::from(tile.palette));
    for py in 0..8u32 {
        for px in 0..8u32 {
            let sx = if tile.xflip { 7u32.saturating_sub(px) } else { px };
            let sy = if tile.yflip { 7u32.saturating_sub(py) } else { py };
            let index = chr
                .get(usize::try_from(sx).unwrap_or(0), usize::try_from(sy).unwrap_or(0))
                .unwrap_or(0);
            if index == 0 && !ctx.opaque_zero {
                continue;
            }
            let color = row.get(usize::from(index)).copied().unwrap_or([0, 0, 0, 0xFF]);
            raster.put(pos.0.saturating_add(px), pos.1.saturating_add(py), color);
        }
    }
}

/// Draws a Map16 tile with its top-left corner at `pos`.
pub fn draw_map16_tile(
    raster: &mut Raster,
    pos: (u32, u32),
    tile: &Map16Tile,
    ctx: TileContext<'_>,
) {
    for (dx, dy, quadrant) in tile.quadrants() {
        let at = (pos.0.saturating_add(u32::from(dx)), pos.1.saturating_add(u32::from(dy)));
        draw_tile8(raster, at, quadrant, ctx);
    }
}

#[cfg(test)]
mod tests {
    use smw_data::gfx::Tile8Pixels;
    use smw_data::map16::Tile8;
    use smw_data::palette::{Bgr555, Palette};

    use super::{CharAtlas, Raster, TileContext, draw_tile8};

    #[test]
    fn flips_and_transparency() {
        let mut atlas = CharAtlas::new();
        let mut px = [0u8; 64];
        px[0] = 1; // top-left pixel only
        atlas.set(5, Tile8Pixels { pixels: px });
        let mut pal = Palette::black();
        pal.set(3, 1, Bgr555::WHITE);

        let mut r = Raster::new(8, 8);
        let ctx = TileContext { atlas: &atlas, palette: &pal, opaque_zero: false };
        draw_tile8(&mut r, (0, 0), Tile8 { num: 5, palette: 3, ..Tile8::default() }, ctx);
        assert_eq!(r.get(0, 0), Some([255, 255, 255, 255]));
        assert_eq!(r.get(1, 0), Some([0, 0, 0, 0]), "index 0 stays transparent");

        let mut r2 = Raster::new(8, 8);
        let flipped = Tile8 { num: 5, palette: 3, xflip: true, yflip: true, priority: false };
        let opaque = TileContext { opaque_zero: true, ..ctx };
        draw_tile8(&mut r2, (0, 0), flipped, opaque);
        assert_eq!(r2.get(7, 7), Some([255, 255, 255, 255]));
        assert_eq!(r2.get(0, 0), Some([0, 0, 0, 255]), "opaque zero draws colour 0 (black)");
    }
}
