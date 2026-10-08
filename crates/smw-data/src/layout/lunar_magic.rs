//! Tables Lunar Magic 2.5x adds to a ROM. `docs/formats/lunar-magic-ext.md`
//!
//! Every address here was verified against Quickie World 1.12 (Lunar Magic
//! 2.52) and Quickie World 2 (2.53) on 2026-10-06. Other versions may move
//! them, so [`detect`] only accepts a ROM whose routines carry the expected
//! opcode fingerprints.

use snes_rom::{Rom, SnesAddr};

use super::{LayoutKind, RomLayout, VANILLA};
use crate::LayoutError;

/// Lunar Magic's added tables and routines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LunarMagicTables {
    /// `"Lunar Magic Version x.xx ..."` ASCII string.
    pub signature: SnesAddr,
    /// Per-level sprite data bank bytes (0x200 bytes).
    pub sprite_banks: SnesAddr,
    /// Per-level layer 2 flags (0x200 bytes): bit 1 = the layer 2 pointer is
    /// a background tilemap at its real bank, bit 2 = that tilemap is 16-bit
    /// (0x800 decoded bytes, 32 rows per screen), bits 4–7 = BG page or page
    /// set. Read by the loader hook at `$0E:F510` and copied to `$7F:C00B`.
    pub level_flags: SnesAddr,
    /// Per-level 24-bit pointers to custom palettes (0x200 × 3 bytes):
    /// `000000` = none, all `FFFFFF` = the ROM has no custom palettes.
    pub custom_palettes: SnesAddr,
    /// 24-bit pointers to ExGFX 80–FF (0x80 × 3 bytes): `000000` = not inserted.
    pub exgfx_ptrs: SnesAddr,
    /// 24-bit pointer to the ExGFX 100–FFF pointer table (0xF00 × 3 bytes).
    pub exgfx_high_table_ptr: SnesAddr,
    /// 24-bit pointer to the per-level "Super GFX Bypass" table (0x200 × 32 bytes).
    pub gfx_bypass_table_ptr: SnesAddr,
    /// 24-bit pointers to background Map16 page sets (16 pages each).
    pub bg_page_sets: SnesAddr,
    /// `JSL` routine: A = Map16 tile × 2 → A = offset, bank at `$0C`, for
    /// foreground tiles 0x200 and up.
    pub fg_map16_getter: SnesAddr,
    /// Secondary header bytes 4, 5 and 6, one 0x200-byte table each.
    pub secondary_header_ext: [SnesAddr; 3],
    /// WRAM copy of the level's flags byte (`$7F:C00B`).
    pub ram_level_flags: u32,
}

/// Lunar Magic 2.5x's tables.
pub const LM_25X: LunarMagicTables = LunarMagicTables {
    signature: SnesAddr::lit(0x0F_F0A0),
    sprite_banks: SnesAddr::lit(0x0E_F100),
    level_flags: SnesAddr::lit(0x0E_F310),
    custom_palettes: SnesAddr::lit(0x0E_F600),
    exgfx_ptrs: SnesAddr::lit(0x0F_F600),
    exgfx_high_table_ptr: SnesAddr::lit(0x0F_F937),
    gfx_bypass_table_ptr: SnesAddr::lit(0x0F_F7FF),
    bg_page_sets: SnesAddr::lit(0x0E_FD50),
    fg_map16_getter: SnesAddr::lit(0x06_F540),
    secondary_header_ext: [
        SnesAddr::lit(0x05_DE00),
        SnesAddr::lit(0x06_FC00),
        SnesAddr::lit(0x06_FE00),
    ],
    ram_level_flags: 0x7F_C00B,
};

/// Opcode bytes that must be present for [`LM_25X`] to apply.
struct Fingerprint {
    /// What the bytes are, for error messages.
    name: &'static str,
    /// Where they start.
    addr: SnesAddr,
    /// The bytes.
    bytes: &'static [u8],
}

/// Routine prologues and hook sites Lunar Magic 2.5x writes; all `FF` in vanilla.
const FINGERPRINTS: &[Fingerprint] = &[
    Fingerprint {
        name: "FG Map16 getter ($06:F540)",
        addr: SnesAddr::lit(0x06_F540),
        bytes: &[0xC9, 0x00, 0x04, 0x90, 0x74],
    },
    Fingerprint {
        name: "BG Map16 page set getter ($0E:FD00)",
        addr: SnesAddr::lit(0x0E_FD00),
        bytes: &[0x08, 0xC2, 0x30, 0xA2, 0x00, 0x00, 0xA0, 0xB0, 0x01, 0xAF, 0x0B, 0xC0, 0x7F],
    },
    Fingerprint {
        name: "layer 2 flags hook ($0E:F510)",
        addr: SnesAddr::lit(0x0E_F510),
        bytes: &[0xC9, 0xFF, 0xD0, 0x09, 0x1A, 0x8F, 0x0B, 0xC0, 0x7F],
    },
    Fingerprint {
        name: "loader jump to the layer 2 hook ($05:8039)",
        addr: SnesAddr::lit(0x05_8039),
        bytes: &[0xA5, 0x6A, 0x5C, 0x10, 0xF5, 0x0E],
    },
    Fingerprint {
        name: "BG page set call ($05:8DA4)",
        addr: SnesAddr::lit(0x05_8DA4),
        bytes: &[0x22, 0x00, 0xFD, 0x0E, 0xEA],
    },
    Fingerprint {
        name: "direct Map16 object 0x27 dispatch ($0D:A4C7)",
        addr: SnesAddr::lit(0x0D_A4C7),
        bytes: &[0x50, 0xF1, 0x0D],
    },
];

/// Builds the Lunar Magic layout for `rom`, whose signature was found.
///
/// # Errors
///
/// [`LayoutError::LunarMagicUnsupported`] if any fingerprint is missing.
pub(super) fn detect(rom: &Rom, signature: String) -> Result<RomLayout, LayoutError> {
    if let Some(missing) = FINGERPRINTS
        .iter()
        .find(|f| rom.read(f.addr, f.bytes.len()).ok() != Some(f.bytes))
        .map(|f| f.name)
    {
        return Err(LayoutError::LunarMagicUnsupported { signature, missing });
    }
    let (major, minor) = parse_version(&signature).unwrap_or((0, 0));
    Ok(RomLayout {
        kind: LayoutKind::LunarMagic { major, minor },
        sprite_banks: Some(LM_25X.sprite_banks),
        known_free: &[],
        lm: Some(LM_25X),
        ..VANILLA
    })
}

/// Extracts `(major, minor)` from `"Lunar Magic Version 2.53 Public"`.
fn parse_version(signature: &str) -> Option<(u8, u8)> {
    let version = signature.split_whitespace().find(|w| w.contains('.'))?;
    let (major, minor) = version.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("Lunar Magic Version 2.53 Public"), Some((2, 53)));
        assert_eq!(parse_version("Lunar Magic Version 3.40"), Some((3, 40)));
        assert_eq!(parse_version("Lunar Magic Version"), None);
    }
}
