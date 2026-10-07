//! GFX files: 8×8 tiles in SNES planar formats.
//!
//! See `docs/formats/gfx.md`.

use core::fmt;

use snes_rom::{Rom, RomError, SnesAddr};

use crate::RomLayout;

/// Identifies a graphics file. Vanilla files are `0x00..=0x33`; Lunar Magic's
/// ExGFX continue from `0x80`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GfxId(u16);

impl GfxId {
    /// Number of vanilla GFX files (GFX00–GFX33).
    pub const VANILLA_COUNT: u16 = 0x34;

    /// Creates an id. Any 16-bit value is representable; whether a ROM has
    /// such a file is decided when loading.
    #[must_use]
    pub const fn new(n: u16) -> Self {
        Self(n)
    }

    /// The raw file number.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// Whether this is one of the 50 files reachable through the vanilla
    /// pointer tables (GFX00–GFX31).
    #[must_use]
    pub const fn in_vanilla_table(self) -> bool {
        self.0 < 0x32
    }
}

impl fmt::Debug for GfxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GfxId({self})")
    }
}

impl fmt::Display for GfxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 < 0x80 { write!(f, "GFX{:02X}", self.0) } else { write!(f, "ExGFX{:X}", self.0) }
    }
}

/// Bits per pixel of a planar tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bpp {
    /// 4 colours, 16 bytes per tile. Layer 3 graphics.
    Two,
    /// 8 colours, 24 bytes per tile. Most SMW graphics.
    Three,
    /// 16 colours, 32 bytes per tile. Player and animated graphics.
    Four,
}

impl Bpp {
    /// Bytes occupied by one 8×8 tile.
    #[must_use]
    pub const fn bytes_per_tile(self) -> usize {
        match self {
            Self::Two => 16,
            Self::Three => 24,
            Self::Four => 32,
        }
    }

    /// Number of colours a pixel can select.
    #[must_use]
    pub const fn colors(self) -> u8 {
        match self {
            Self::Two => 4,
            Self::Three => 8,
            Self::Four => 16,
        }
    }

    /// Infers the format from a decompressed file's size the way Lunar Magic
    /// does: `0x400`/`0x800` → 2bpp, `0x600`/`0xC00`/`0x1800` → 3bpp,
    /// `0x1000`/`0x2000` → 4bpp. (`0x400` is vanilla GFX2F, the credits font;
    /// `0x600` is GFX30/31, 64-tile 3bpp files.)
    #[must_use]
    pub const fn guess_from_len(len: usize) -> Option<Self> {
        match len {
            0x400 | 0x800 => Some(Self::Two),
            0x600 | 0xC00 | 0x1800 => Some(Self::Three),
            0x1000 | 0x2000 => Some(Self::Four),
            _ => None,
        }
    }
}

/// One decoded 8×8 tile: 64 palette indices in row-major order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile8Pixels {
    /// Palette index per pixel, `pixels[y * 8 + x]`.
    pub pixels: [u8; 64],
}

impl Tile8Pixels {
    /// A fully transparent tile.
    pub const EMPTY: Self = Self { pixels: [0; 64] };

    /// Palette index at `(x, y)`, or `None` outside the tile.
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> Option<u8> {
        if x >= 8 || y >= 8 {
            return None;
        }
        self.pixels.get(y.saturating_mul(8).saturating_add(x)).copied()
    }

    /// Decodes one planar tile. `raw` must be exactly `bpp.bytes_per_tile()` long.
    #[must_use]
    pub fn decode(bpp: Bpp, raw: &[u8]) -> Option<Self> {
        if raw.len() != bpp.bytes_per_tile() {
            return None;
        }
        let mut pixels = [0u8; 64];
        for (y, row) in pixels.chunks_exact_mut(8).enumerate() {
            let planes = plane_bytes(bpp, raw, y)?;
            for (x, px) in row.iter_mut().enumerate() {
                let shift = u32::try_from(7usize.checked_sub(x)?).ok()?;
                *px = planes
                    .iter()
                    .enumerate()
                    .fold(0u8, |acc, (p, &byte)| acc | (((byte >> shift) & 1) << p));
            }
        }
        Some(Self { pixels })
    }
}

/// The plane bytes for row `y`: up to four, in plane order.
fn plane_bytes(bpp: Bpp, raw: &[u8], y: usize) -> Option<Vec<u8>> {
    let two_y = y.checked_mul(2)?;
    let p0 = *raw.get(two_y)?;
    let p1 = *raw.get(two_y.checked_add(1)?)?;
    Some(match bpp {
        Bpp::Two => vec![p0, p1],
        Bpp::Three => vec![p0, p1, *raw.get(16usize.checked_add(y)?)?],
        Bpp::Four => {
            let base = 16usize.checked_add(two_y)?;
            vec![p0, p1, *raw.get(base)?, *raw.get(base.checked_add(1)?)?]
        }
    })
}

/// A decoded graphics file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GfxFile {
    /// Which file this is.
    pub id: GfxId,
    /// Pixel format.
    pub bpp: Bpp,
    /// Decoded tiles in file order.
    pub tiles: Vec<Tile8Pixels>,
}

impl GfxFile {
    /// Decodes raw (already decompressed) planar data.
    ///
    /// # Errors
    ///
    /// [`GfxError::BadLength`] if `raw.len()` is not a multiple of the tile size.
    pub fn decode(id: GfxId, bpp: Bpp, raw: &[u8]) -> Result<Self, GfxError> {
        let per_tile = bpp.bytes_per_tile();
        if !raw.len().is_multiple_of(per_tile) {
            return Err(GfxError::BadLength { id, len: raw.len() });
        }
        let tiles = raw
            .chunks_exact(per_tile)
            .map(|chunk| {
                Tile8Pixels::decode(bpp, chunk).ok_or(GfxError::BadLength { id, len: raw.len() })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { id, bpp, tiles })
    }

    /// Decodes raw data, inferring the format from its length.
    ///
    /// # Errors
    ///
    /// [`GfxError::UnknownFormat`] if the size is not one Lunar Magic recognises.
    pub fn decode_guessing_bpp(id: GfxId, raw: &[u8]) -> Result<Self, GfxError> {
        let bpp =
            Bpp::guess_from_len(raw.len()).ok_or(GfxError::UnknownFormat { id, len: raw.len() })?;
        Self::decode(id, bpp, raw)
    }

    /// Loads and decompresses a file through the ROM's pointer tables.
    ///
    /// # Errors
    ///
    /// [`GfxError::NotInTable`] for ids the vanilla tables do not cover, plus
    /// ROM read and decompression failures.
    pub fn load(rom: &Rom, layout: &RomLayout, id: GfxId) -> Result<Self, GfxError> {
        let addr = gfx_address(rom, layout, id)?;
        let pc = rom.pc(addr)?;
        let compressed = rom.bytes().get(pc..).ok_or(RomError::Unmapped(addr))?;
        let decompressed =
            lc_lz2::decompress(compressed).map_err(|source| GfxError::Decompress { id, source })?;
        Self::decode_guessing_bpp(id, &decompressed.data)
    }

    /// Tile `n`, or `None` past the end.
    #[must_use]
    pub fn tile(&self, n: usize) -> Option<&Tile8Pixels> {
        self.tiles.get(n)
    }
}

/// Resolves the ROM address of a file's compressed data.
///
/// # Errors
///
/// [`GfxError::NotInTable`] if the id is outside the table; ROM errors otherwise.
pub fn gfx_address(rom: &Rom, layout: &RomLayout, id: GfxId) -> Result<SnesAddr, GfxError> {
    let table = &layout.gfx_ptrs;
    let index = usize::from(id.get());
    if index >= table.count {
        return Err(GfxError::NotInTable(id));
    }
    let offset = u32::try_from(index).map_err(|_| GfxError::NotInTable(id))?;
    let byte_at = |base: SnesAddr| -> Result<u8, GfxError> {
        let addr = base.checked_add(offset).ok_or(RomError::Unmapped(base))?;
        Ok(rom.read_u8(addr)?)
    };
    let low = byte_at(table.low)?;
    let high = byte_at(table.high)?;
    let bank = byte_at(table.bank)?;
    Ok(SnesAddr::from_parts(bank, u16::from_le_bytes([low, high])))
}

/// Errors from loading or decoding graphics.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GfxError {
    /// The id is not reachable through the ROM's pointer tables.
    #[error("{0} is not in this ROM's GFX pointer table")]
    NotInTable(GfxId),

    /// Raw data length is not a whole number of tiles.
    #[error("{id}: {len} bytes is not a whole number of tiles")]
    BadLength {
        /// File in question.
        id: GfxId,
        /// Offending length.
        len: usize,
    },

    /// Raw data length does not match any known format.
    #[error("{id}: cannot infer bit depth from a {len}-byte file")]
    UnknownFormat {
        /// File in question.
        id: GfxId,
        /// Offending length.
        len: usize,
    },

    /// LC_LZ2 decoding failed.
    #[error("{id}: {source}")]
    Decompress {
        /// File in question.
        id: GfxId,
        /// Underlying error.
        #[source]
        source: lc_lz2::Error,
    },

    /// ROM access failed.
    #[error(transparent)]
    Rom(#[from] RomError),
}

#[cfg(test)]
mod tests {
    use super::{Bpp, GfxFile, GfxId, Tile8Pixels};

    #[test]
    fn decodes_2bpp_tile() {
        // Row 0: plane0 = 0b1010_0000, plane1 = 0b1100_0000 => pixels 3,1,2,0,0,0,0,0
        let mut raw = [0u8; 16];
        raw[0] = 0b1010_0000;
        raw[1] = 0b1100_0000;
        let t = Tile8Pixels::decode(Bpp::Two, &raw).expect("16 bytes");
        assert_eq!(&t.pixels[..8], &[3, 2, 1, 0, 0, 0, 0, 0]);
        assert_eq!(t.get(0, 0), Some(3));
        assert_eq!(t.get(8, 0), None);
    }

    #[test]
    fn decodes_3bpp_tile() {
        // Row 7: plane0 bit7 set, plane2 bit7 set => pixel (0,7) = 0b101 = 5
        let mut raw = [0u8; 24];
        raw[14] = 0x80;
        raw[16 + 7] = 0x80;
        let t = Tile8Pixels::decode(Bpp::Three, &raw).expect("24 bytes");
        assert_eq!(t.get(0, 7), Some(5));
        assert_eq!(t.get(1, 7), Some(0));
    }

    #[test]
    fn decodes_4bpp_tile() {
        // Row 1: all four planes have bit 0 set => pixel (7,1) = 15
        let mut raw = [0u8; 32];
        raw[2] = 1;
        raw[3] = 1;
        raw[16 + 2] = 1;
        raw[16 + 3] = 1;
        let t = Tile8Pixels::decode(Bpp::Four, &raw).expect("32 bytes");
        assert_eq!(t.get(7, 1), Some(15));
        assert_eq!(t.get(6, 1), Some(0));
    }

    #[test]
    fn wrong_length_rejected() {
        assert!(Tile8Pixels::decode(Bpp::Two, &[0; 15]).is_none());
        assert!(GfxFile::decode(GfxId::new(0), Bpp::Three, &[0; 25]).is_err());
        assert!(GfxFile::decode_guessing_bpp(GfxId::new(0), &[0; 0x900]).is_err());
    }

    #[test]
    fn file_decode_and_guess() {
        let f = GfxFile::decode_guessing_bpp(GfxId::new(0x28), &[0; 0x800]).expect("2bpp");
        assert_eq!(f.bpp, Bpp::Two);
        assert_eq!(f.tiles.len(), 128);
        let g = GfxFile::decode_guessing_bpp(GfxId::new(0), &[0; 0xC00]).expect("3bpp");
        assert_eq!(g.bpp, Bpp::Three);
        assert_eq!(g.tiles.len(), 128);
    }

    #[test]
    fn id_display() {
        assert_eq!(GfxId::new(0x0A).to_string(), "GFX0A");
        assert_eq!(GfxId::new(0x80).to_string(), "ExGFX80");
        assert_eq!(GfxId::new(0x123).to_string(), "ExGFX123");
        assert!(GfxId::new(0x31).in_vanilla_table());
        assert!(!GfxId::new(0x32).in_vanilla_table());
    }

    /// Decodes every vanilla file when `SMW_ROM` is set.
    #[test]
    fn real_rom_all_files_decode() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = crate::RomLayout::detect(&rom).expect("vanilla");
        for n in 0..0x32u16 {
            let id = GfxId::new(n);
            let file = GfxFile::load(&rom, &layout, id).unwrap_or_else(|e| panic!("{id}: {e}"));
            let (expected_bpp, expected_tiles) = match n {
                0x28..=0x2B => (Bpp::Two, 128),
                0x2F => (Bpp::Two, 64),
                0x30 | 0x31 => (Bpp::Three, 64),
                _ => (Bpp::Three, 128),
            };
            assert_eq!(file.bpp, expected_bpp, "{id}");
            assert_eq!(file.tiles.len(), expected_tiles, "{id}");
        }
        assert_eq!(
            super::gfx_address(&rom, &layout, GfxId::new(0)).expect("in table"),
            snes_rom::SnesAddr::lit(0x08_D9F9)
        );
    }
}
