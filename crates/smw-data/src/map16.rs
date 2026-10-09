//! Map16: the 16×16 tiles levels are built from.
//!
//! Each Map16 tile is four 8×8 SNES tilemap entries (2 bytes each) in the
//! order top-left, bottom-left, top-right, bottom-right. The game builds a
//! 512-entry pointer table at `$7E:0FBE` on level load, mixing a common table
//! with a tileset-specific one; this module reproduces the result. See
//! `docs/formats/map16.md`.

use core::fmt;

use snes_rom::{Rom, RomError, SnesAddr};

use crate::RomLayout;

/// Number of tiles in one Map16 set (pages 0 and 1).
pub const TILES_PER_SET: usize = 0x200;

/// Bytes per Map16 tile in ROM.
const TILE_BYTES: u32 = 8;

/// Number of tilesets the vanilla game defines.
pub const TILESET_COUNT: u8 = 15;

/// One 8×8 tilemap entry: `YXPCCCTT TTTTTTTT`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Tile8 {
    /// 10-bit character number.
    pub num: u16,
    /// Palette row, 0–7.
    pub palette: u8,
    /// Priority bit.
    pub priority: bool,
    /// Horizontal flip.
    pub xflip: bool,
    /// Vertical flip.
    pub yflip: bool,
}

impl Tile8 {
    /// Decodes a little-endian tilemap word.
    #[must_use]
    #[allow(clippy::as_conversions, reason = "value is masked to 3 bits before the cast")]
    pub const fn from_word(word: u16) -> Self {
        Self {
            num: word & 0x03FF,
            palette: ((word >> 10) & 0x07) as u8,
            priority: word & 0x2000 != 0,
            xflip: word & 0x4000 != 0,
            yflip: word & 0x8000 != 0,
        }
    }

    /// Decodes from the two bytes as stored in ROM (low byte first).
    #[must_use]
    pub const fn from_bytes(low: u8, high: u8) -> Self {
        Self::from_word(u16::from_le_bytes([low, high]))
    }

    /// Re-encodes as a tilemap word.
    #[must_use]
    #[allow(clippy::as_conversions, reason = "lossless widening in a const fn")]
    pub const fn to_word(self) -> u16 {
        (self.num & 0x03FF)
            | ((self.palette as u16 & 0x07) << 10)
            | if self.priority { 0x2000 } else { 0 }
            | if self.xflip { 0x4000 } else { 0 }
            | if self.yflip { 0x8000 } else { 0 }
    }
}

impl fmt::Debug for Tile8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Tile8({:03X} pal{}{}{}{})",
            self.num,
            self.palette,
            if self.priority { " P" } else { "" },
            if self.xflip { " X" } else { "" },
            if self.yflip { " Y" } else { "" },
        )
    }
}

/// A 16×16 tile made of four 8×8 entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Map16Tile {
    /// Top-left quadrant.
    pub top_left: Tile8,
    /// Bottom-left quadrant.
    pub bottom_left: Tile8,
    /// Top-right quadrant.
    pub top_right: Tile8,
    /// Bottom-right quadrant.
    pub bottom_right: Tile8,
}

impl Map16Tile {
    /// Decodes the 8-byte ROM representation.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let b = |i: usize| bytes.get(i).copied();
        Some(Self {
            top_left: Tile8::from_bytes(b(0)?, b(1)?),
            bottom_left: Tile8::from_bytes(b(2)?, b(3)?),
            top_right: Tile8::from_bytes(b(4)?, b(5)?),
            bottom_right: Tile8::from_bytes(b(6)?, b(7)?),
        })
    }

    /// The quadrants in drawing order: `(x, y, tile)` with `x`, `y` in `{0, 8}`.
    #[must_use]
    pub const fn quadrants(&self) -> [(u8, u8, Tile8); 4] {
        [
            (0, 0, self.top_left),
            (0, 8, self.bottom_left),
            (8, 0, self.top_right),
            (8, 8, self.bottom_right),
        ]
    }
}

/// 512 Map16 tiles: either the foreground pages 0–1 for a tileset, or the
/// background set.
#[derive(Clone, PartialEq, Eq)]
pub struct Map16Set {
    tiles: Vec<Map16Tile>,
}

impl Map16Set {
    /// Wraps a list of tiles.
    #[must_use]
    pub const fn from_tiles(tiles: Vec<Map16Tile>) -> Self {
        Self { tiles }
    }

    /// Tile `n`, or `None` past the end.
    #[must_use]
    pub fn get(&self, n: usize) -> Option<&Map16Tile> {
        self.tiles.get(n)
    }

    /// All tiles in order.
    #[must_use]
    pub fn tiles(&self) -> &[Map16Tile] {
        &self.tiles
    }

    /// Number of tiles.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.tiles.len()
    }

    /// Whether the set is empty (never, for a successfully loaded set).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}

impl fmt::Debug for Map16Set {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Map16Set").field("len", &self.tiles.len()).finish()
    }
}

/// Errors from loading Map16 data.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Map16Error {
    /// Tileset number outside `0..15`.
    #[error("tileset {0} does not exist (vanilla has 15)")]
    NoSuchTileset(u8),

    /// Background page set with no pointer in Lunar Magic's table.
    #[error("background Map16 page set {0} does not exist")]
    NoSuchPageSet(u8),

    /// ROM access failed.
    #[error(transparent)]
    Rom(#[from] RomError),
}

/// Loads the foreground Map16 pages 0–1 as the game assembles them for
/// `tileset`, with the animated tiles 0x133–0x13A on their first frame.
///
/// # Errors
///
/// [`Map16Error::NoSuchTileset`] or ROM read failures.
pub fn load_fg(rom: &Rom, layout: &RomLayout, tileset: u8) -> Result<Map16Set, Map16Error> {
    if tileset >= TILESET_COUNT {
        return Err(Map16Error::NoSuchTileset(tileset));
    }
    let t = &layout.map16;
    let specific_offset =
        rom.read_u16_le(advance(t.tileset_locations, u32::from(tileset).saturating_mul(2))?)?;
    let mut specific = SnesAddr::from_parts(t.tileset_bank, specific_offset);
    let mut common = t.common;
    let bitmask = rom.read(t.tileset_bitmask, TILES_PER_SET / 8)?;

    let mut tiles = Vec::with_capacity(TILES_PER_SET);
    for n in 0..TILES_PER_SET {
        let bit = bitmask.get(n / 8).is_some_and(|byte| byte & (0x80 >> (n % 8)) != 0);
        let source = if bit { &mut common } else { &mut specific };
        tiles.push(read_tile(rom, *source)?);
        *source = advance(*source, TILE_BYTES)?;
    }

    // Tilesets 0 and 7 swap in diagonal pipe tiles.
    if matches!(tileset, 0 | 7) {
        let mut src = t.diagonal_pipes;
        for n in (0x1C4..=0x1C7).chain(0x1EC..=0x1EF) {
            if let Some(slot) = tiles.get_mut(n) {
                *slot = read_tile(rom, src)?;
            }
            src = advance(src, TILE_BYTES)?;
        }
    }

    // Tiles 0x133-0x13A come from the appearance (animation frame) table.
    let frame_offset = rom.read_u16_le(t.appearance_table)?;
    let mut src = SnesAddr::from_parts(t.tileset_bank, frame_offset);
    for n in 0x133..=0x13A {
        if let Some(slot) = tiles.get_mut(n) {
            *slot = read_tile(rom, src)?;
        }
        src = advance(src, TILE_BYTES)?;
    }

    Ok(Map16Set { tiles })
}

/// Loads the 512 background Map16 tiles.
///
/// # Errors
///
/// ROM read failures.
pub fn load_bg(rom: &Rom, layout: &RomLayout) -> Result<Map16Set, Map16Error> {
    let mut src = layout.map16.bg_tiles;
    let mut tiles = Vec::with_capacity(TILES_PER_SET);
    for _ in 0..TILES_PER_SET {
        tiles.push(read_tile(rom, src)?);
        src = advance(src, TILE_BYTES)?;
    }
    Ok(Map16Set { tiles })
}

/// Tiles per Lunar Magic background page set (16 pages).
pub const PAGE_SET_TILES: usize = 0x1000;

/// Loads background page set `set`.
///
/// Set 0 of a vanilla ROM is the vanilla BG tiles; Lunar Magic ROMs have
/// 16-page sets listed at `$0E:FD50` (`docs/formats/map16.md`). Lunar Magic
/// sizes each set's block to the last tile it stored, so the result may hold
/// fewer than 0x1000 tiles.
///
/// # Errors
///
/// [`Map16Error::NoSuchPageSet`] or ROM read failures.
pub fn load_bg_set(rom: &Rom, layout: &RomLayout, set: u8) -> Result<Map16Set, Map16Error> {
    let lm = layout.lm.filter(|lm| lm.features.contains(crate::LmFeatures::BG_PAGE_SETS));
    let Some(lm) = lm else {
        return if set == 0 { load_bg(rom, layout) } else { Err(Map16Error::NoSuchPageSet(set)) };
    };
    let entry = advance(lm.bg_page_sets, u32::from(set).saturating_mul(3))?;
    let ptr = rom.read_u24_le(entry)?;
    let base = SnesAddr::new(ptr)
        .filter(|_| ptr != 0 && ptr != 0xFF_FFFF)
        .ok_or(Map16Error::NoSuchPageSet(set))?;
    // Lunar Magic 3.x points set 0 at the vanilla table until a BG page is
    // edited; that table is untagged and exactly 512 tiles.
    let count = if base == layout.map16.bg_tiles {
        TILES_PER_SET
    } else {
        rats_bounded_count(rom, base).min(PAGE_SET_TILES)
    };
    let mut src = base;
    let mut tiles = Vec::with_capacity(count);
    for _ in 0..count {
        tiles.push(read_tile(rom, src)?);
        src = advance(src, TILE_BYTES)?;
    }
    Ok(Map16Set { tiles })
}

/// How many 8-byte tiles fit in the RATS block starting at `data`, or up
/// to the end of its bank if the block is untagged.
fn rats_bounded_count(rom: &Rom, data: SnesAddr) -> usize {
    let Ok(pc) = rom.pc(data) else {
        return 0;
    };
    let bank_remaining = usize::from(u16::MAX.wrapping_sub(data.offset())).saturating_add(1);
    let len = pc
        .checked_sub(snes_rom::rats::TAG_LEN)
        .and_then(|tag_pc| snes_rom::rats::RatsTag::parse_at(rom.bytes(), tag_pc))
        .map_or_else(|| rom.len().saturating_sub(pc).min(bank_remaining), |tag| tag.data_len);
    len / TILE_BYTES_USIZE
}

/// [`TILE_BYTES`] as a `usize`.
const TILE_BYTES_USIZE: usize = 8;

fn read_tile(rom: &Rom, addr: SnesAddr) -> Result<Map16Tile, RomError> {
    let bytes = rom.read(addr, 8)?;
    Map16Tile::from_bytes(bytes).ok_or(RomError::Unmapped(addr))
}

fn advance(addr: SnesAddr, by: u32) -> Result<SnesAddr, RomError> {
    addr.checked_add(by).ok_or(RomError::Unmapped(addr))
}

#[cfg(test)]
mod tests {
    use super::{Map16Tile, Tile8};

    /// Lunar Magic's background page set 0 starts with the vanilla BG tiles.
    #[test]
    fn lm_rom_bg_page_set() {
        let Some(path) = std::env::var_os("LM_ROM") else {
            eprintln!("LM_ROM not set; skipping Lunar Magic ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = crate::RomLayout::detect(&rom).expect("lunar magic");
        let set = super::load_bg_set(&rom, &layout, 0).expect("set 0");
        assert!(set.len() >= 0x200 && set.len() <= super::PAGE_SET_TILES, "{}", set.len());
        let first = set.get(0).expect("tile 0");
        assert_eq!(first.top_left, Tile8::from_bytes(0xF8, 0x00));
        assert_eq!(first.bottom_right, Tile8::from_bytes(0xF8, 0x00));
        assert!(super::load_bg_set(&rom, &layout, 15).is_err());
    }

    #[test]
    fn tile8_round_trip() {
        let t = Tile8::from_bytes(0x70, 0x1C);
        assert_eq!(
            t,
            Tile8 { num: 0x070, palette: 7, priority: false, xflip: false, yflip: false }
        );
        let high_tile = Tile8::from_bytes(0xC5, 0x09);
        assert_eq!(high_tile.num, 0x1C5);
        assert_eq!(high_tile.palette, 2);
        let flipped = Tile8::from_word(0xE3FF);
        assert!(flipped.priority && flipped.xflip && flipped.yflip);
        assert_eq!(flipped.num, 0x3FF);
        assert_eq!(flipped.palette, 0);
        assert_eq!(flipped.to_word(), 0xE3FF);
        assert_eq!(Tile8::from_bytes(0x88, 0x54).to_word(), 0x5488);
    }

    #[test]
    fn map16_tile_quadrant_order() {
        let t = Map16Tile::from_bytes(&[0x70, 0x1C, 0x72, 0x1C, 0x71, 0x1C, 0x73, 0x1C])
            .expect("8 bytes");
        assert_eq!(t.top_left.num, 0x70);
        assert_eq!(t.bottom_left.num, 0x72);
        assert_eq!(t.top_right.num, 0x71);
        assert_eq!(t.bottom_right.num, 0x73);
        assert!(Map16Tile::from_bytes(&[0; 7]).is_none());
    }

    #[test]
    fn real_rom_fg_and_bg_sets() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = crate::RomLayout::detect(&rom).expect("vanilla");
        let fg0 = super::load_fg(&rom, &layout, 0).expect("tileset 0");
        assert_eq!(fg0.len(), 0x200);
        // Tile 0 is the first common tile: 70/72/71/73, palette 7.
        let t0 = fg0.get(0).expect("tile 0");
        assert_eq!(t0.top_left, Tile8::from_bytes(0x70, 0x1C));
        assert_eq!(t0.bottom_right, Tile8::from_bytes(0x73, 0x1C));
        // Tile 0x73 is the first tileset-specific tile: C5 15 for tileset 0.
        assert_eq!(fg0.get(0x73).expect("tile").top_left, Tile8::from_bytes(0xC5, 0x15));
        // Tileset 1 differs there but not in the common range.
        let fg1 = super::load_fg(&rom, &layout, 1).expect("tileset 1");
        assert_eq!(fg0.get(0x100), fg1.get(0x100));
        assert_ne!(fg0.get(0x73), fg1.get(0x73));
        // Tileset 0 has diagonal pipes at 1C4; tileset 1 does not.
        assert_ne!(fg0.get(0x1C4), fg1.get(0x1C4));
        assert!(super::load_fg(&rom, &layout, 15).is_err());
        // Background tile 0 is four copies of F8 00.
        let bg = super::load_bg(&rom, &layout).expect("bg");
        assert_eq!(bg.get(0).expect("tile").top_left, Tile8::from_bytes(0xF8, 0x00));
        assert_eq!(bg.get(1).expect("tile").top_left, Tile8::from_bytes(0x53, 0x01));
    }
}
