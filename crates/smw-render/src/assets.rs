//! What the running machine shows for a level, captured as plain data.
//!
//! `smw-play` fills a [`LevelAssets`] from the in-house emulator after
//! launching the level (`docs/formats/level-view.md`); this crate only
//! consumes it, so nothing here depends on the emulator.

use core::fmt;

use smw_data::gfx::{Bpp, Tile8Pixels};
use smw_data::map16::Map16Tile;
use smw_data::palette::{Bgr555, Palette};

/// VRAM size in 16-bit words.
pub const VRAM_WORDS: usize = 0x8000;
/// 8×8 characters a 4bpp background character base covers (the whole
/// 0x4000-word half of VRAM SMW keeps them in).
pub const BG_CHARS: usize = 0x400;
/// Characters in the sprite area (`$6000`–`$7FFF`, 4bpp).
pub const SPRITE_CHARS: usize = 0x200;
/// Bytes of tile RAM per layer (`$7E:C800..`, `$7F:C800..`).
pub const TILE_RAM_LEN: usize = 0x3800;
/// First and last Map16 tile the game animates through its pointer table.
pub const ANIMATED_TILES: core::ops::RangeInclusive<u16> = 0x133..=0x13A;

/// One background layer's bases and scroll, as the PPU registers hold them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BgBase {
    /// Tilemap base, VRAM word address.
    pub tilemap: u16,
    /// `BGnSC` bits 0-1: 0 = 32×32, 1 = 64×32, 2 = 32×64, 3 = 64×64 tiles.
    pub tilemap_size: u8,
    /// Character base, VRAM word address.
    pub chars: u16,
    /// Horizontal scroll.
    pub hofs: u16,
    /// Vertical scroll.
    pub vofs: u16,
}

/// The PPU registers that decide how the level's layers compose.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PpuSummary {
    /// `BGMODE` bits 0-2.
    pub bg_mode: u8,
    /// `BGMODE` bit 3: BG3 priority tiles above everything.
    pub bg3_priority: bool,
    /// `TM`.
    pub main_screen: u8,
    /// `TS`.
    pub sub_screen: u8,
    /// `CGADSUB`.
    pub cgadsub: u8,
    /// `CGWSEL`.
    pub cgwsel: u8,
    /// `COLDATA` as a BGR555 colour.
    pub fixed_color: u16,
    /// `INIDISP` brightness (0-15).
    pub brightness: u8,
    /// Sprite character base, VRAM word address.
    pub obj_chars: u16,
    /// Per-layer bases and scroll.
    pub bg: [BgBase; 4],
    /// `VTIME`: the scanline the game's IRQ fires on (the status bar's end).
    pub irq_line: u16,
    /// `HDMAEN` at capture time.
    pub hdma_enabled: u8,
}

/// A rendered frame.
#[derive(Clone, PartialEq, Eq)]
pub struct CapturedFrame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA8, row-major.
    pub rgba: Vec<u8>,
}

impl fmt::Debug for CapturedFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CapturedFrame({}x{})", self.width, self.height)
    }
}

/// The game's tile RAM for both layers.
#[derive(Clone, PartialEq, Eq)]
pub struct TileRam {
    /// Low bytes (`$7E:C800..`).
    pub low: Vec<u8>,
    /// High bytes (`$7F:C800..`).
    pub high: Vec<u8>,
}

impl fmt::Debug for TileRam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TileRam({} + {} bytes)", self.low.len(), self.high.len())
    }
}

impl TileRam {
    /// The Map16 number at a byte offset into both tables.
    #[must_use]
    pub fn tile(&self, offset: u32) -> Option<u16> {
        let i = usize::try_from(offset).ok()?;
        Some(u16::from_le_bytes([*self.low.get(i)?, *self.high.get(i)?]))
    }
}

/// Everything the editor's picture takes from the machine.
#[derive(Clone, PartialEq, Eq)]
pub struct LevelAssets {
    /// VRAM as captured (32K words).
    pub vram: Vec<u16>,
    /// CGRAM as a palette (row 0 colour 0 = backdrop).
    pub palette: Palette,
    /// OAM low table (512 bytes) followed by the high table (32 bytes).
    pub oam: Vec<u8>,
    /// Background characters: 4bpp decode of [`BG_CHARS`] characters from
    /// BG1's character base.
    pub chars: Vec<Tile8Pixels>,
    /// Layer 3 characters: 2bpp decode of [`BG_CHARS`] characters from
    /// BG3's character base.
    pub layer3_chars: Vec<Tile8Pixels>,
    /// Sprite characters: 4bpp decode from the OBJ character base.
    pub sprite_chars: Vec<Tile8Pixels>,
    /// Registers right after the frame's vblank processing (status bar
    /// scroll and friends).
    pub top: PpuSummary,
    /// Registers below the status bar IRQ (the play area).
    pub play: PpuSummary,
    /// The animated Map16 tiles as their pointer table resolved them at
    /// capture time.
    pub animated: Vec<(u16, Map16Tile)>,
    /// Tile RAM, the oracle for the untimed loader.
    pub tile_ram: TileRam,
    /// The frame rendered from this state.
    pub frame: CapturedFrame,
    /// The same frame with sprites hidden.
    pub frame_no_obj: CapturedFrame,
    /// Emulator frame counter at capture.
    pub frame_number: u64,
}

impl fmt::Debug for LevelAssets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LevelAssets")
            .field("frame_number", &self.frame_number)
            .field("play", &self.play)
            .field("animated", &self.animated.len())
            .finish_non_exhaustive()
    }
}

/// Where the machine's frame sits inside the level picture: the level pixel
/// at the frame's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameOrigin {
    /// Level pixel x of frame column 0.
    pub x: u32,
    /// Level pixel y of frame line 0.
    pub y: u32,
}

/// The PPU shows tilemap row `vofs + line + 1` on frame line `line`
/// (`docs/emulation/ppu.md`), so frame line 0 is level row `vofs + 1`.
/// Verified 2026-10-14: level 105's play area matches pixel for pixel.
pub const LINE_OFFSET: u32 = 1;

/// Where the captured frame sits in the level picture (from BG1's play-area
/// scroll) and which frame rows show the level (below the status bar IRQ).
#[must_use]
pub fn frame_window(assets: &LevelAssets) -> (FrameOrigin, core::ops::Range<u32>) {
    let cam = assets.play.bg[0];
    let origin =
        FrameOrigin { x: u32::from(cam.hofs), y: u32::from(cam.vofs).saturating_add(LINE_OFFSET) };
    let rows = u32::from(assets.play.irq_line).saturating_add(1)..assets.frame_no_obj.height;
    (origin, rows)
}

/// The result of [`diff_frame`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FrameDiff {
    /// Differing pixels in the compared rows.
    pub count: usize,
    /// The first few differing pixels, as frame coordinates.
    pub first: Vec<(u32, u32)>,
}

/// Compares `frame` (within `rows`) against the level `raster` at `origin`,
/// keeping at most `limit` differing coordinates.
#[must_use]
pub fn diff_frame(
    raster: &crate::Raster,
    frame: &CapturedFrame,
    origin: FrameOrigin,
    rows: core::ops::Range<u32>,
    limit: usize,
) -> FrameDiff {
    let mut count = 0usize;
    let mut first = Vec::new();
    for fy in rows {
        for fx in 0..frame.width {
            let i = usize::try_from(fy.saturating_mul(frame.width).saturating_add(fx)).unwrap_or(0);
            let want = frame.rgba.get(i.saturating_mul(4)..i.saturating_mul(4).saturating_add(4));
            let got = raster.get(origin.x.saturating_add(fx), origin.y.saturating_add(fy));
            if want != got.as_ref().map(<[u8; 4]>::as_slice) {
                count = count.saturating_add(1);
                if first.len() < limit {
                    first.push((fx, fy));
                }
            }
        }
    }
    FrameDiff { count, first }
}

/// Decodes `count` characters of `bpp` starting at VRAM word `base`.
/// Characters past the end of VRAM come out empty.
#[must_use]
pub fn decode_chars(vram: &[u16], base: u16, count: usize, bpp: Bpp) -> Vec<Tile8Pixels> {
    let words_per_char = bpp.bytes_per_tile() / 2;
    (0..count)
        .map(|n| {
            let start = usize::from(base).saturating_add(n.saturating_mul(words_per_char));
            let bytes: Vec<u8> = vram
                .get(start..start.saturating_add(words_per_char))
                .map(|words| words.iter().flat_map(|w| w.to_le_bytes()).collect())
                .unwrap_or_default();
            Tile8Pixels::decode(bpp, &bytes).unwrap_or(Tile8Pixels::EMPTY)
        })
        .collect()
}

/// A [`Palette`] from the 256 CGRAM colours.
#[must_use]
pub fn palette_from_cgram(cgram: &[u16; 256]) -> Palette {
    let mut palette = Palette::black();
    for (index, color) in cgram.iter().enumerate() {
        palette.set(index / 16, index % 16, Bgr555::new(*color));
    }
    palette
}

#[cfg(test)]
mod tests {
    use smw_data::gfx::Bpp;
    use smw_data::palette::Bgr555;

    use super::{decode_chars, palette_from_cgram};

    #[test]
    fn decodes_vram_words_as_characters() {
        // One 2bpp character: plane 0 = 0xFF on row 0, plane 1 = 0x01 on row 0.
        let mut vram = vec![0u16; 64];
        vram[0] = u16::from_le_bytes([0xFF, 0x01]);
        let chars = decode_chars(&vram, 0, 2, Bpp::Two);
        assert_eq!(chars.len(), 2);
        assert_eq!(chars[0].get(0, 0), Some(1));
        assert_eq!(chars[0].get(7, 0), Some(3));
        assert_eq!(chars[0].get(0, 1), Some(0));
        assert_eq!(chars[1].pixels, [0; 64]);
        // Past the end of VRAM: empty, not a panic.
        assert_eq!(decode_chars(&vram, 0x7FF0, 2, Bpp::Four)[1].pixels, [0; 64]);
    }

    #[test]
    fn cgram_maps_to_rows_and_columns() {
        let mut cgram = [0u16; 256];
        cgram[0] = 0x1234;
        cgram[17] = 0x7FFF;
        let palette = palette_from_cgram(&cgram);
        assert_eq!(palette.get(0, 0).map(Bgr555::raw), Some(0x1234));
        assert_eq!(palette.get(1, 1).map(Bgr555::raw), Some(0x7FFF));
    }
}
