//! Per-level data Lunar Magic keeps outside the level's own byte streams.
//! See `docs/formats/lunar-magic-ext.md`.

use snes_rom::{Rom, RomError, SnesAddr};

use crate::{Layer2FlagsFormat, LevelId, RomLayout};

/// Pointer value meaning "no custom palette for this level".
const NO_PALETTE: u32 = 0;
/// Pointer value Lunar Magic leaves in a never-written table.
const UNWRITTEN: u32 = 0xFF_FFFF;

/// A level's layer 2 flags byte (table `level_flags`, copied to `$7F:C00B`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub struct LmLevelFlags(pub u8);

impl LmLevelFlags {
    /// Bit 1: the layer 2 pointer names a background tilemap at its real bank
    /// (rather than an object stream).
    #[must_use]
    pub const fn relocated_tilemap(self) -> bool {
        self.0 & 0x02 != 0
    }

    /// Bit 3 (3.x only): a vanilla background whose pointer carries the
    /// real bank `$0C` instead of the `$FF` marker.
    #[must_use]
    pub const fn vanilla_bank_tilemap(self) -> bool {
        self.0 & 0x08 != 0
    }

    /// Whether the layer 2 pointer names a background tilemap (of either
    /// kind) under `format`.
    #[must_use]
    pub const fn tilemap(self, format: Layer2FlagsFormat) -> bool {
        match format {
            Layer2FlagsFormat::V25x => self.relocated_tilemap(),
            Layer2FlagsFormat::V3x => self.relocated_tilemap() || self.vanilla_bank_tilemap(),
        }
    }

    /// Bit 2: that tilemap decodes to 0x800 bytes of 16-bit tile numbers
    /// (32 rows per screen) instead of 0x400 low bytes.
    #[must_use]
    pub const fn sixteen_bit(self) -> bool {
        self.0 & 0x04 != 0
    }

    /// Bits 4–7: the BG page (8-bit tilemaps) or page set (16-bit tilemaps).
    #[must_use]
    pub const fn page(self) -> u8 {
        self.0 >> 4
    }
}

/// What Lunar Magic records about a level outside its data streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct LmLevelInfo {
    /// Layer 2 flags.
    pub flags: LmLevelFlags,
    /// How `flags` is laid out (from the ROM's hook variant).
    pub format: Layer2FlagsFormat,
    /// Where the level's custom palette block lives, if it has one.
    pub custom_palette: Option<SnesAddr>,
    /// Secondary header bytes 4, 5 and 6, as stored.
    pub secondary_ext: [u8; 3],
}

impl LmLevelInfo {
    /// Whether the layer 2 pointer names a background tilemap rather than
    /// an object stream (`flags` interpreted under `format`).
    #[must_use]
    pub const fn has_tilemap(&self) -> bool {
        self.flags.tilemap(self.format)
    }

    /// Reads the level's entries from Lunar Magic's tables; `None` for a
    /// vanilla layout.
    ///
    /// # Errors
    ///
    /// ROM read failures.
    pub fn load(rom: &Rom, layout: &RomLayout, id: LevelId) -> Result<Option<Self>, RomError> {
        let Some(lm) = &layout.lm else {
            return Ok(None);
        };
        let index = u32::try_from(id.index()).unwrap_or(0);
        let flags = LmLevelFlags(rom.read_u8(offset(lm.level_flags, index)?)?);
        let raw_ptr = rom.read_u24_le(offset(lm.custom_palettes, index.saturating_mul(3))?)?;
        let custom_palette = match raw_ptr {
            NO_PALETTE | UNWRITTEN => None,
            ptr => SnesAddr::new(ptr),
        };
        let mut secondary_ext = [0u8; 3];
        for (slot, table) in secondary_ext.iter_mut().zip(lm.secondary_header_ext.iter()) {
            *slot = rom.read_u8(offset(*table, index)?)?;
        }
        Ok(Some(Self { flags, format: lm.layer2_flags_format, custom_palette, secondary_ext }))
    }
}

fn offset(base: SnesAddr, by: u32) -> Result<SnesAddr, RomError> {
    base.checked_add(by).ok_or(RomError::Unmapped(base))
}

#[cfg(test)]
mod tests {
    use super::LmLevelFlags;
    use crate::Layer2FlagsFormat;

    #[test]
    fn flag_bits() {
        let f = LmLevelFlags(0x06);
        assert!(f.relocated_tilemap());
        assert!(f.sixteen_bit());
        assert_eq!(f.page(), 0);
        let g = LmLevelFlags(0x12);
        assert!(g.relocated_tilemap());
        assert!(!g.sixteen_bit());
        assert_eq!(g.page(), 1);
        assert!(!LmLevelFlags(0).relocated_tilemap());
        let v3 = LmLevelFlags(0x18);
        assert!(v3.vanilla_bank_tilemap());
        assert!(!v3.relocated_tilemap());
        assert_eq!(v3.page(), 1);
        assert!(v3.tilemap(Layer2FlagsFormat::V3x));
        assert!(!v3.tilemap(Layer2FlagsFormat::V25x));
        assert!(f.tilemap(Layer2FlagsFormat::V25x));
    }
}
