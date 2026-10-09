//! Lunar Magic's per-level "Super GFX Bypass" table.
//!
//! It names which GFX / ExGFX file each upload slot takes, overriding the
//! tileset and sprite set lists. See `docs/formats/lunar-magic-ext.md`
//! (verified 2026-10-06).

use snes_rom::{Rom, RomError, SnesAddr};

use crate::gfx::GfxId;
use crate::{LevelId, RomLayout};

/// Bytes per level in the table.
pub const ROW_LEN: usize = 32;
/// Words per level.
pub const SLOT_COUNT: usize = 16;
/// Slot value meaning "no file".
pub const NO_FILE: u16 = 0x7F;
/// Slot value Lunar Magic leaves in rows it never wrote.
pub const UNSET: u16 = 0xFFFF;
/// Word 0 bit that enables the bypass for the level.
const ENABLED: u16 = 0x8000;
/// Bits of a slot word that hold the file number.
const FILE_MASK: u16 = 0x0FFF;

/// Word index of each named slot. The table stores the FG/BG and sprite
/// lists in reverse order.
const FG1: usize = 7;
const FG2: usize = 6;
const BG1: usize = 5;
const FG3: usize = 4;
const SP1: usize = 11;
const SP2: usize = 10;
const SP3: usize = 9;
const SP4: usize = 8;
/// Words 0–3: animation slot 2, layer 3 tiles, BG3, BG2 (from the `.mwl`
/// format description, `docs/formats/mwl.md`; to be confirmed against a
/// ROM with those slots set).
const AN2: usize = 0;
const LT3: usize = 1;
const BG3: usize = 2;
const BG2: usize = 3;
/// Words 12–15: layer 3 GFX files LG4..LG1 (vanilla GFX2B, 2A, 29, 28).
const LG4: usize = 12;

/// The row Lunar Magic 3.71 exports for a level that has no bypass row:
/// every slot "no file" except an unset SP4 and the vanilla layer 3 files.
const DEFAULT_ROW: [u16; SLOT_COUNT] = [
    NO_FILE, NO_FILE, NO_FILE, NO_FILE, NO_FILE, NO_FILE, NO_FILE, NO_FILE, UNSET, NO_FILE,
    NO_FILE, NO_FILE, 0x2B, 0x2A, 0x29, 0x28,
];

/// One level's 16 bypass words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GfxBypass {
    /// The words as stored: low 12 bits = file number, high bits = flags.
    pub words: [u16; SLOT_COUNT],
}

impl GfxBypass {
    /// Reads the level's row. `None` for vanilla layouts or when the ROM has
    /// no bypass table.
    ///
    /// # Errors
    ///
    /// ROM read failures.
    pub fn load(rom: &Rom, layout: &RomLayout, id: LevelId) -> Result<Option<Self>, RomError> {
        let Some(lm) = &layout.lm else {
            return Ok(None);
        };
        let table = rom.read_u24_le(lm.gfx_bypass_table_ptr)?;
        if table == 0 || table == 0xFF_FFFF {
            return Ok(None);
        }
        let table = SnesAddr::new(table).ok_or(RomError::Unmapped(lm.gfx_bypass_table_ptr))?;
        let row_offset = u32::try_from(id.index()).unwrap_or(0).saturating_mul(32);
        let row = table.checked_add(row_offset).ok_or(RomError::Unmapped(table))?;
        let bytes = rom.read(row, ROW_LEN)?;
        let mut words = [0u16; SLOT_COUNT];
        for (word, pair) in words.iter_mut().zip(bytes.chunks_exact(2)) {
            *word = u16::from_le_bytes([
                pair.first().copied().unwrap_or(0),
                pair.get(1).copied().unwrap_or(0),
            ]);
        }
        Ok(Some(Self { words }))
    }

    /// Builds from the words directly.
    #[must_use]
    pub const fn from_words(words: [u16; SLOT_COUNT]) -> Self {
        Self { words }
    }

    /// Whether the level uses these slots instead of its tileset lists.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.words[0] != UNSET && self.words[0] & ENABLED != 0
    }

    /// The file in slot `n`, or `None` for "no file" / unset.
    #[must_use]
    pub fn slot(&self, n: usize) -> Option<GfxId> {
        let word = *self.words.get(n)?;
        if word == UNSET {
            return None;
        }
        let file = word & FILE_MASK;
        (file != NO_FILE).then_some(GfxId::new(file))
    }

    /// `FG1`, `FG2`, `BG1`, `FG3`, in upload order.
    #[must_use]
    pub fn layer_files(&self) -> [Option<GfxId>; 4] {
        [self.slot(FG1), self.slot(FG2), self.slot(BG1), self.slot(FG3)]
    }

    /// `SP1`–`SP4`.
    #[must_use]
    pub fn sprite_files(&self) -> [Option<GfxId>; 4] {
        [self.slot(SP1), self.slot(SP2), self.slot(SP3), self.slot(SP4)]
    }

    /// Words 0–3, whose slots are not decoded yet.
    #[must_use]
    pub const fn extra_words(&self) -> [u16; 4] {
        [self.words[0], self.words[1], self.words[2], self.words[3]]
    }

    /// `AN2`, `LT3`, `BG3`, `BG2` (slot names per the `.mwl` format).
    #[must_use]
    pub fn extra_files(&self) -> [Option<GfxId>; 4] {
        [self.slot(AN2), self.slot(LT3), self.slot(BG3), self.slot(BG2)]
    }

    /// `LG4`..`LG1`, the layer 3 GFX files.
    #[must_use]
    pub fn layer3_files(&self) -> [Option<GfxId>; 4] {
        [self.slot(LG4), self.slot(LG4 + 1), self.slot(LG4 + 2), self.slot(LG4 + 3)]
    }

    /// What Lunar Magic writes for a level without a bypass row.
    #[must_use]
    pub const fn default_row() -> Self {
        Self { words: DEFAULT_ROW }
    }
}

#[cfg(test)]
mod tests {
    use super::GfxBypass;
    use crate::gfx::GfxId;

    /// Quickie World 2, level 105 (verified 2026-10-06).
    const QW2_105: [u16; 16] = [
        0x807F, 0x007F, 0x00F8, 0x007F, 0x0015, 0x00F7, 0x0017, 0x0014, 0x000F, 0x0013, 0x0001,
        0x0000, 0x002B, 0x002A, 0x0029, 0x0028,
    ];

    #[test]
    fn slot_order_and_sentinels() {
        let b = GfxBypass::from_words(QW2_105);
        assert!(b.enabled());
        let id = |n: u16| Some(GfxId::new(n));
        assert_eq!(b.layer_files(), [id(0x14), id(0x17), id(0xF7), id(0x15)]);
        assert_eq!(b.sprite_files(), [id(0x00), id(0x01), id(0x13), id(0x0F)]);
        assert_eq!(b.slot(0), None, "0x7F is no file");
        assert_eq!(b.slot(2), id(0xF8));
        assert_eq!(b.slot(16), None);
        let unset = GfxBypass::from_words([0xFFFF; 16]);
        assert!(!unset.enabled());
        assert_eq!(unset.slot(7), None);
    }
}
