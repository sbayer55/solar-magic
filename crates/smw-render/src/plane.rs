//! Index planes: one background layer drawn as CGRAM indices with its
//! per-pixel priority bit, ready for [`crate::compose`].

use smw_data::gfx::Bpp;
use smw_data::map16::{Map16Tile, Tile8};

use crate::CharAtlas;

/// Bit of a plane cell set when the pixel is opaque.
const OPAQUE: u16 = 0x100;
/// Bit of a plane cell set when the tile has priority.
const PRIORITY: u16 = 0x200;

/// One pixel of a plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cell(u16);

impl Cell {
    /// A transparent pixel.
    pub const TRANSPARENT: Self = Self(0);

    /// An opaque pixel with CGRAM index `index`.
    #[must_use]
    #[allow(clippy::as_conversions, reason = "lossless u8 -> u16 in a const fn")]
    pub const fn opaque(index: u8, priority: bool) -> Self {
        let bits = if priority { OPAQUE | PRIORITY } else { OPAQUE };
        Self(bits | index as u16)
    }

    /// Whether the pixel is drawn.
    #[must_use]
    pub const fn is_opaque(self) -> bool {
        self.0 & OPAQUE != 0
    }

    /// The tile priority bit.
    #[must_use]
    pub const fn priority(self) -> bool {
        self.0 & PRIORITY != 0
    }

    /// CGRAM index (0–255).
    #[must_use]
    #[allow(clippy::as_conversions, clippy::cast_possible_truncation, reason = "masked to 8 bits")]
    pub const fn index(self) -> u8 {
        (self.0 & 0xFF) as u8
    }
}

/// A layer as CGRAM indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexPlane {
    width: u32,
    height: u32,
    cells: Vec<Cell>,
}

impl IndexPlane {
    /// A transparent plane.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let len = usize::try_from(u64::from(width).saturating_mul(u64::from(height))).unwrap_or(0);
        Self { width, height, cells: vec![Cell::TRANSPARENT; len] }
    }

    /// Width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    fn offset(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        usize::try_from(
            u64::from(y).saturating_mul(u64::from(self.width)).saturating_add(u64::from(x)),
        )
        .ok()
    }

    /// The cell at `(x, y)` (transparent outside the plane).
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Cell {
        self.offset(x, y).and_then(|i| self.cells.get(i).copied()).unwrap_or_default()
    }

    /// Sets the cell at `(x, y)`; outside the plane nothing happens.
    pub fn put(&mut self, x: u32, y: u32, cell: Cell) {
        if let Some(slot) = self.offset(x, y).and_then(|i| self.cells.get_mut(i)) {
            *slot = cell;
        }
    }
}

/// Characters and their depth for drawing into a plane.
#[derive(Debug, Clone, Copy)]
pub struct PlaneContext<'a> {
    /// Character graphics.
    pub atlas: &'a CharAtlas,
    /// Bits per pixel: decides how many CGRAM colours a tilemap palette spans.
    pub bpp: Bpp,
}

/// Draws one tilemap entry with its top-left corner at `pos`; only opaque
/// pixels are written.
pub fn draw_tile8_indexed(
    plane: &mut IndexPlane,
    pos: (u32, u32),
    tile: Tile8,
    ctx: PlaneContext<'_>,
) {
    let Some(chr) = ctx.atlas.get(usize::from(tile.num)) else {
        return;
    };
    let colors = ctx.bpp.colors();
    let base = tile.palette.wrapping_mul(colors);
    for py in 0..8u32 {
        for px in 0..8u32 {
            let sx = if tile.xflip { 7u32.saturating_sub(px) } else { px };
            let sy = if tile.yflip { 7u32.saturating_sub(py) } else { py };
            let index = chr
                .get(usize::try_from(sx).unwrap_or(0), usize::try_from(sy).unwrap_or(0))
                .unwrap_or(0);
            if index == 0 {
                continue;
            }
            let cell = Cell::opaque(base.wrapping_add(index), tile.priority);
            plane.put(pos.0.saturating_add(px), pos.1.saturating_add(py), cell);
        }
    }
}

/// Draws a Map16 tile with its top-left corner at `pos`.
pub fn draw_map16_indexed(
    plane: &mut IndexPlane,
    pos: (u32, u32),
    tile: &Map16Tile,
    ctx: PlaneContext<'_>,
) {
    for (dx, dy, quadrant) in tile.quadrants() {
        let at = (pos.0.saturating_add(u32::from(dx)), pos.1.saturating_add(u32::from(dy)));
        draw_tile8_indexed(plane, at, quadrant, ctx);
    }
}

#[cfg(test)]
mod tests {
    use smw_data::gfx::{Bpp, Tile8Pixels};
    use smw_data::map16::Tile8;

    use super::{Cell, CharAtlas, IndexPlane, PlaneContext, draw_tile8_indexed};

    #[test]
    fn indices_fold_in_the_palette_and_priority() {
        let mut atlas = CharAtlas::new();
        let mut px = [0u8; 64];
        px[0] = 3;
        atlas.set(7, Tile8Pixels { pixels: px });
        let mut plane = IndexPlane::new(8, 8);
        let tile = Tile8 { num: 7, palette: 2, priority: true, xflip: true, yflip: false };
        draw_tile8_indexed(
            &mut plane,
            (0, 0),
            tile,
            PlaneContext { atlas: &atlas, bpp: Bpp::Four },
        );
        let cell = plane.get(7, 0);
        assert!(cell.is_opaque() && cell.priority());
        assert_eq!(cell.index(), 2 * 16 + 3);
        assert_eq!(plane.get(0, 0), Cell::TRANSPARENT);
        let mut plane2 = IndexPlane::new(8, 8);
        let tile2 = Tile8 { priority: false, xflip: false, ..tile };
        draw_tile8_indexed(
            &mut plane2,
            (0, 0),
            tile2,
            PlaneContext { atlas: &atlas, bpp: Bpp::Two },
        );
        assert_eq!(plane2.get(0, 0).index(), 2 * 4 + 3);
        assert!(!plane2.get(0, 0).priority());
        assert_eq!(plane2.get(9, 9), Cell::TRANSPARENT, "outside the plane");
    }
}
