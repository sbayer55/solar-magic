//! Tables and hijacks Lunar Magic adds to a ROM. `docs/formats/lunar-magic-ext.md`
//!
//! Lunar Magic installs each feature as a hijack at a fixed site in the
//! original banks. Detection checks every known site independently, so a
//! ROM saved by any version loads with exactly the features it has; only
//! the layer 2 hook (which every Lunar Magic save installs) is required.
//! Verified 2026-10-08 against Quickie World 1.12 (2.52), Quickie World 2
//! (2.53) and a 3.71 save of the vanilla ROM.

use snes_rom::{Rom, RomError, SnesAddr};

use super::{LaunchHooks, LayoutKind, RomLayout, VANILLA};
use crate::level::ExitId;
use crate::{LayoutError, LevelId};

bitflags::bitflags! {
    /// Which Lunar Magic hijacks a ROM carries.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct LmFeatures: u16 {
        /// The loader's layer 2 hook at `$0E:F510` (reads the flags table).
        const LAYER2_HOOK = 1 << 0;
        /// The `JML` from `$05:8039` into that hook.
        const LAYER2_HOOK_CALL = 1 << 1;
        /// Per-level sprite data bank bytes (`$0E:F100`, call at `$05:D8F5`).
        const SPRITE_BANKS = 1 << 2;
        /// The foreground Map16 getter for pages 2+ (`$06:F540`).
        const FG_MAP16_GETTER = 1 << 3;
        /// The background page set getter (`$0E:FD00`).
        const BG_PAGE_SETS = 1 << 4;
        /// The `JSL` into that getter at `$05:8DA4`.
        const BG_PAGE_SET_CALL = 1 << 5;
        /// Direct Map16 objects 0x27/0x29 (dispatch entry at `$0D:A4C7`).
        const DIRECT_MAP16 = 1 << 6;
        /// The 5-byte screen exit, extended object 02 (`$0D:A115`).
        const EXT_OBJECT_02 = 1 << 7;
        /// Lunar Magic 3.x extended objects 01 and 03 (`$0D:A112`, `$0D:A118`).
        const EXT_OBJECTS_3X = 1 << 8;
        /// The Map16 "acts like" getter (`$06:F600`).
        const ACTS_LIKE_GETTER = 1 << 9;
        /// 3.x custom level dimensions: `JSL` at `$05:D9A1` whose target
        /// holds a pointer to the per-level dimension table.
        const LEVEL_DIMENSIONS = 1 << 10;
        /// 3.x secondary exit extra bytes: `LDA long,X` getters at `$05:DC85`.
        const SECONDARY_EXIT_EXT = 1 << 11;
    }
}

impl LmFeatures {
    /// Features every Lunar Magic save must have for the layout to apply.
    pub const REQUIRED: Self = Self::LAYER2_HOOK.union(Self::LAYER2_HOOK_CALL);

    /// The feature names, for display.
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        self.iter_names().map(|(name, _)| name).collect()
    }
}

/// How the per-level layer 2 flags byte (`$0E:F310`) is laid out.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum Layer2FlagsFormat {
    /// 2.5x: bit 1 = tilemap at the pointer's real bank, bit 2 = 16-bit,
    /// bits 4–7 = page / page set. Vanilla backgrounds keep the `$FF` bank.
    #[default]
    V25x,
    /// 3.x: as 2.5x, plus bit 3 = a vanilla background whose pointer now
    /// carries the real bank `$0C` (the `$FF` marker is gone), its page in
    /// bits 4–7.
    V3x,
}

/// ROM bytes Lunar Magic 3.x's level initialisation copies into WRAM
/// before the loader runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WramSeed {
    /// Source bytes in ROM.
    pub rom: SnesAddr,
    /// Destination WRAM address.
    pub ram: u32,
    /// Byte count.
    pub len: usize,
}

/// What 3.x's hijacked level init leaves in WRAM for a vanilla-height level.
///
/// Verified 2026-10-08 by running the game's init (`JSL` at `$00:9708`) on
/// an LM 3.71 ROM: the per-screen tile-RAM pointer tables that 3.x's mode
/// table (`$00:BDA8`) now addresses at `$0BF6` (low bytes, 32 screens; layer
/// 2 starts at screen 16) and `$0C56` (high bytes) are byte copies of
/// vanilla's `$00:BAD8` and `$00:BC40`, and the level height at `$13D7` is
/// `0x1B0`. Levels with custom heights need the real init (planned, LM6).
pub const V3X_SCREEN_TABLE_SEEDS: &[WramSeed] = &[
    WramSeed { rom: SnesAddr::lit(0x00_BAD8), ram: 0x00_0BF6, len: 0x60 },
    WramSeed { rom: SnesAddr::lit(0x00_BC40), ram: 0x00_0C56, len: 0x60 },
];

/// 3.x's level height word (`$13D7`) for a vanilla-height level.
pub const V3X_LEVEL_HEIGHT: (u32, u16) = (0x00_13D7, 0x01B0);

/// Lunar Magic's added tables and routines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LunarMagicTables {
    /// Which hijacks the ROM carries.
    pub features: LmFeatures,
    /// Layout of the flags byte in [`LunarMagicTables::level_flags`].
    pub layer2_flags_format: Layer2FlagsFormat,
    /// WRAM the loader expects 3.x's level init to have filled.
    pub wram_seeds: &'static [WramSeed],
    /// WRAM word holding the level height, with its vanilla-height value.
    pub level_height: Option<(u32, u16)>,
    /// `"Lunar Magic Version x.xx ..."` ASCII string.
    pub signature: SnesAddr,
    /// Per-level sprite data bank bytes (0x200 bytes).
    pub sprite_banks: SnesAddr,
    /// Per-level layer 2 flags (0x200 bytes), see [`Layer2FlagsFormat`].
    /// Read by the loader hook at `$0E:F510` and copied to `$7F:C00B`.
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
    /// 3.x per-level byte table at `$06:FA00` (`0x20` for untouched levels).
    pub level_ext_fa00: SnesAddr,
    /// Per-level `ExAnimation` flag bytes (`$03:FE00`, `FF` in vanilla).
    pub exanim_flags: SnesAddr,
    /// The `JSL` opcode of 3.x's level dimensions hook; its operand points
    /// at a routine whose byte `0x70` holds a pointer to the per-level
    /// dimension table.
    pub dimensions_hook: SnesAddr,
    /// Operands of the two `LDA long,X` getters for 3.x's secondary exit
    /// bytes 4 and 5 (`$05:DC86`, `$05:DC8B`).
    pub secondary_exit_ext_ops: [SnesAddr; 2],
    /// WRAM copy of the level's flags byte (`$7F:C00B`).
    pub ram_level_flags: u32,
}

/// Byte offset inside the dimensions routine of the table pointer.
const DIMENSIONS_TABLE_PTR: u32 = 0x70;

impl LunarMagicTables {
    /// Lunar Magic 3.x's per-level dimension byte, when the ROM has the
    /// feature. `docs/formats/mwl.md`
    ///
    /// # Errors
    ///
    /// ROM read failures.
    pub fn level_dimensions(&self, rom: &Rom, id: LevelId) -> Result<Option<u8>, RomError> {
        if !self.features.contains(LmFeatures::LEVEL_DIMENSIONS) {
            return Ok(None);
        }
        let routine = rom.read_ptr(offset(self.dimensions_hook, 1)?)?;
        let table = rom.read_ptr(offset(routine, DIMENSIONS_TABLE_PTR)?)?;
        Ok(Some(rom.read_u8(offset(table, u32::from(id.get()))?)?))
    }

    /// Lunar Magic 3.x's extra two bytes of a secondary exit.
    ///
    /// # Errors
    ///
    /// ROM read failures.
    pub fn secondary_exit_ext(&self, rom: &Rom, exit: ExitId) -> Result<Option<[u8; 2]>, RomError> {
        if !self.features.contains(LmFeatures::SECONDARY_EXIT_EXT) {
            return Ok(None);
        }
        let mut out = [0u8; 2];
        for (slot, operand) in out.iter_mut().zip(self.secondary_exit_ext_ops) {
            let table = rom.read_ptr(operand)?;
            *slot = rom.read_u8(offset(table, u32::from(exit.get()))?)?;
        }
        Ok(Some(out))
    }

    /// The level's `ExAnimation` flag byte (0 when the table was never written).
    ///
    /// # Errors
    ///
    /// ROM read failures.
    pub fn exanim_flag(&self, rom: &Rom, id: LevelId) -> Result<u8, RomError> {
        let byte = rom.read_u8(offset(self.exanim_flags, u32::from(id.get()))?)?;
        Ok(if byte == 0xFF { 0 } else { byte })
    }
}

fn offset(base: SnesAddr, by: u32) -> Result<SnesAddr, RomError> {
    base.checked_add(by).ok_or(RomError::Unmapped(base))
}

/// Lunar Magic's table addresses (identical in 2.52, 2.53 and 3.71).
/// `features` is empty here; detection fills it in.
pub const LM_25X: LunarMagicTables = LunarMagicTables {
    features: LmFeatures::empty(),
    layer2_flags_format: Layer2FlagsFormat::V25x,
    wram_seeds: &[],
    level_height: None,
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
    level_ext_fa00: SnesAddr::lit(0x06_FA00),
    exanim_flags: SnesAddr::lit(0x03_FE00),
    dimensions_hook: SnesAddr::lit(0x05_D9A1),
    secondary_exit_ext_ops: [SnesAddr::lit(0x05_DC86), SnesAddr::lit(0x05_DC8B)],
    ram_level_flags: 0x7F_C00B,
};

/// Lunar Magic 2.5x rewrites the end of the level-entry routine: after its
/// `JSL $05:DCD0` high-byte hook, `$05:D8BC` holds
/// `REP #$30 / LDA $0E / STA $010B / ASL / CLC / ADC $0E / TAY`, so the
/// level number is final there and the routine itself copies it to `$010B`.
/// Verified against Quickie World 2 (LM 2.53), 2026-10-11. Lunar Magic 3.71
/// leaves this routine as in vanilla (`$05:D8B7`, checked on an LM 3.71
/// ROM 2026-10-12), so the hooks are chosen per ROM in [`detect`].
const LM_LEVEL_SELECT_PC: SnesAddr = SnesAddr::lit(0x05_D8BC);
const LM_LEVEL_SELECT_FINGERPRINT: &[u8] =
    &[0xC2, 0x30, 0xA5, 0x0E, 0x8D, 0x0B, 0x01, 0x0A, 0x18, 0x65, 0x0E, 0xA8];

/// Bytes that must be present at an address.
struct Check {
    addr: SnesAddr,
    bytes: &'static [u8],
}

/// One way a hijack may look (versions differ in the code they install).
struct Variant {
    checks: &'static [Check],
    /// What matching this variant says about the flags byte layout.
    flags_format: Option<Layer2FlagsFormat>,
}

/// A hijack site and the byte patterns that identify it.
struct Hijack {
    feature: LmFeatures,
    name: &'static str,
    variants: &'static [Variant],
}

const fn check(addr: u32, bytes: &'static [u8]) -> Check {
    Check { addr: SnesAddr::lit(addr), bytes }
}

const fn variant(checks: &'static [Check]) -> Variant {
    Variant { checks, flags_format: None }
}

/// Every hijack Solar Magic knows; all sites are `FF` (or vanilla code) in
/// an unmodified ROM.
const HIJACKS: &[Hijack] = &[
    Hijack {
        feature: LmFeatures::LAYER2_HOOK,
        name: "layer 2 flags hook ($0E:F510)",
        variants: &[
            Variant {
                checks: &[check(
                    0x0E_F510,
                    &[0xC9, 0xFF, 0xD0, 0x09, 0x1A, 0x8F, 0x0B, 0xC0, 0x7F],
                )],
                flags_format: Some(Layer2FlagsFormat::V25x),
            },
            Variant {
                checks: &[check(
                    0x0E_F510,
                    &[0x38, 0xE9, 0x7F, 0xD0, 0x08, 0x8F, 0x0B, 0xC0, 0x7F],
                )],
                flags_format: Some(Layer2FlagsFormat::V3x),
            },
        ],
    },
    Hijack {
        feature: LmFeatures::LAYER2_HOOK_CALL,
        name: "loader jump to the layer 2 hook ($05:8039)",
        variants: &[variant(&[check(0x05_8039, &[0xA5, 0x6A, 0x5C, 0x10, 0xF5, 0x0E])])],
    },
    Hijack {
        feature: LmFeatures::SPRITE_BANKS,
        name: "sprite bank table call ($05:D8F5)",
        variants: &[variant(&[check(0x05_D8F5, &[0x22, 0x00, 0xF3, 0x0E])])],
    },
    Hijack {
        feature: LmFeatures::FG_MAP16_GETTER,
        name: "FG Map16 getter ($06:F540)",
        variants: &[variant(&[check(0x06_F540, &[0xC9, 0x00, 0x04, 0x90, 0x74])])],
    },
    Hijack {
        feature: LmFeatures::BG_PAGE_SETS,
        name: "BG Map16 page set getter ($0E:FD00)",
        variants: &[variant(&[check(
            0x0E_FD00,
            &[0x08, 0xC2, 0x30, 0xA2, 0x00, 0x00, 0xA0, 0xB0, 0x01, 0xAF, 0x0B, 0xC0, 0x7F],
        )])],
    },
    Hijack {
        feature: LmFeatures::BG_PAGE_SET_CALL,
        name: "BG page set call ($05:8DA4)",
        variants: &[variant(&[check(0x05_8DA4, &[0x22, 0x00, 0xFD, 0x0E, 0xEA])])],
    },
    Hijack {
        feature: LmFeatures::DIRECT_MAP16,
        name: "direct Map16 object 0x27 dispatch ($0D:A4C7)",
        variants: &[variant(&[check(0x0D_A4C7, &[0x50, 0xF1, 0x0D])])],
    },
    Hijack {
        feature: LmFeatures::EXT_OBJECT_02,
        name: "5-byte screen exit dispatch ($0D:A115)",
        variants: &[variant(&[check(0x0D_A115, &[0xB0, 0xE1, 0x0D])])],
    },
    Hijack {
        feature: LmFeatures::EXT_OBJECTS_3X,
        name: "extended objects 01/03 dispatch ($0D:A112, $0D:A118)",
        variants: &[variant(&[
            check(0x0D_A112, &[0xD0, 0xE1, 0x0D]),
            check(0x0D_A118, &[0xE0, 0xE1, 0x0D]),
        ])],
    },
    Hijack {
        feature: LmFeatures::ACTS_LIKE_GETTER,
        name: "Map16 acts-like getter ($06:F600)",
        variants: &[variant(&[check(0x06_F600, &[0xEA, 0xEA, 0x98, 0x5C, 0x45, 0xF5, 0x00])])],
    },
    Hijack {
        feature: LmFeatures::LEVEL_DIMENSIONS,
        name: "level dimensions hook ($05:D9A1)",
        // Vanilla: `LDA $5B; AND #$01`; 3.x: `JSL` into its bank.
        variants: &[variant(&[check(0x05_D9A1, &[0x22])])],
    },
    Hijack {
        feature: LmFeatures::SECONDARY_EXIT_EXT,
        name: "secondary exit extra byte getters ($05:DC85)",
        // Two `LDA long,X; RTL` stubs.
        variants: &[variant(&[
            check(0x05_D9A1, &[0x22]),
            check(0x05_DC85, &[0xBF]),
            check(0x05_DC89, &[0x6B, 0xBF]),
            check(0x05_DC8E, &[0x6B]),
        ])],
    },
];

/// What the hijack scan found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HijackScan {
    /// Features whose site matched one of its variants.
    pub features: LmFeatures,
    /// Flags layout, from the layer 2 hook variant that matched.
    pub layer2_flags_format: Option<Layer2FlagsFormat>,
}

/// Checks every known hijack site of `rom`.
#[must_use]
pub fn scan_hijacks(rom: &Rom) -> HijackScan {
    let matches = |v: &Variant| {
        v.checks.iter().all(|c| rom.read(c.addr, c.bytes.len()).ok() == Some(c.bytes))
    };
    let mut scan = HijackScan { features: LmFeatures::empty(), layer2_flags_format: None };
    for hijack in HIJACKS {
        if let Some(found) = hijack.variants.iter().find(|v| matches(v)) {
            scan.features |= hijack.feature;
            if found.flags_format.is_some() {
                scan.layer2_flags_format = found.flags_format;
            }
        }
    }
    scan
}

/// Builds the Lunar Magic layout for `rom`, whose signature was found.
///
/// # Errors
///
/// [`LayoutError::LunarMagicUnsupported`] if a required hijack is missing.
pub(super) fn detect(rom: &Rom, signature: String) -> Result<RomLayout, LayoutError> {
    let scan = scan_hijacks(rom);
    let (Some(flags_format), true) =
        (scan.layer2_flags_format, scan.features.contains(LmFeatures::REQUIRED))
    else {
        let missing = HIJACKS
            .iter()
            .find(|h| {
                LmFeatures::REQUIRED.contains(h.feature) && !scan.features.contains(h.feature)
            })
            .map_or("layer 2 hook", |h| h.name);
        return Err(LayoutError::LunarMagicUnsupported { signature, missing });
    };
    let (major, minor) = parse_version(&signature).unwrap_or((0, 0));
    let v3x = flags_format == Layer2FlagsFormat::V3x;
    let lm = LunarMagicTables {
        features: scan.features,
        layer2_flags_format: flags_format,
        wram_seeds: if v3x { V3X_SCREEN_TABLE_SEEDS } else { &[] },
        level_height: v3x.then_some(V3X_LEVEL_HEIGHT),
        ..LM_25X
    };
    Ok(RomLayout {
        kind: LayoutKind::LunarMagic { major, minor },
        sprite_banks: scan.features.contains(LmFeatures::SPRITE_BANKS).then_some(lm.sprite_banks),
        known_free: &[],
        lm: Some(lm),
        launch: launch_hooks(rom),
        ..VANILLA
    })
}

/// The level-entry hooks for this ROM: 2.5x's rewritten routine when its
/// bytes are present, else vanilla's (3.71 keeps the vanilla code).
fn launch_hooks(rom: &Rom) -> LaunchHooks {
    let rewritten = rom
        .read(LM_LEVEL_SELECT_PC, LM_LEVEL_SELECT_FINGERPRINT.len())
        .is_ok_and(|bytes| bytes == LM_LEVEL_SELECT_FINGERPRINT);
    if rewritten {
        LaunchHooks {
            level_select_pc: LM_LEVEL_SELECT_PC,
            level_select_fingerprint: LM_LEVEL_SELECT_FINGERPRINT,
            ..VANILLA.launch
        }
    } else {
        VANILLA.launch
    }
}

/// Extracts `(major, minor)` from `"Lunar Magic Version 2.53 Public"`.
fn parse_version(signature: &str) -> Option<(u8, u8)> {
    let version = signature.split_whitespace().find(|w| w.contains('.'))?;
    let (major, minor) = version.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::{Layer2FlagsFormat, LmFeatures, parse_version, scan_hijacks};
    use crate::layout::{LayoutKind, RomLayout};

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("Lunar Magic Version 2.53 Public"), Some((2, 53)));
        assert_eq!(parse_version("Lunar Magic Version 3.40"), Some((3, 40)));
        assert_eq!(parse_version("Lunar Magic Version"), None);
    }

    #[test]
    fn feature_names() {
        let names = (LmFeatures::LAYER2_HOOK | LmFeatures::DIRECT_MAP16).names();
        assert_eq!(names, vec!["LAYER2_HOOK", "DIRECT_MAP16"]);
    }

    fn load(var: &str) -> Option<snes_rom::Rom> {
        let Some(path) = std::env::var_os(var) else {
            eprintln!("{var} not set; skipping Lunar Magic ROM test");
            return None;
        };
        Some(snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads"))
    }

    #[test]
    fn vanilla_has_no_hijacks() {
        let Some(rom) = load("SMW_ROM") else { return };
        let scan = scan_hijacks(&rom);
        assert!(scan.features.is_empty(), "{:?}", scan.features);
        assert_eq!(scan.layer2_flags_format, None);
    }

    #[test]
    fn lm_rom_detects_v25x() {
        let Some(rom) = load("LM_ROM") else { return };
        let layout = RomLayout::detect(&rom).expect("supported");
        let lm = layout.lm.expect("lunar magic");
        assert!(matches!(layout.kind, LayoutKind::LunarMagic { major: 2, .. }), "{}", layout.kind);
        assert_eq!(lm.layer2_flags_format, Layer2FlagsFormat::V25x);
        let v3_only = LmFeatures::EXT_OBJECTS_3X
            | LmFeatures::LEVEL_DIMENSIONS
            | LmFeatures::SECONDARY_EXIT_EXT;
        assert_eq!(lm.features, LmFeatures::all().difference(v3_only));
        assert_eq!(layout.sprite_banks, Some(lm.sprite_banks));
    }

    #[test]
    fn lm3_rom_detects_all_features() {
        let Some(rom) = load("LM3_ROM") else { return };
        let layout = RomLayout::detect(&rom).expect("supported");
        let lm = layout.lm.expect("lunar magic");
        assert!(matches!(layout.kind, LayoutKind::LunarMagic { major: 3, .. }), "{}", layout.kind);
        assert_eq!(lm.layer2_flags_format, Layer2FlagsFormat::V3x);
        assert_eq!(lm.features, LmFeatures::all());
        assert_eq!(layout.sprite_banks, Some(lm.sprite_banks));
        let id = crate::LevelId::new(0x105).expect("valid");
        assert_eq!(lm.level_dimensions(&rom, id).expect("reads"), Some(0));
        let exit = crate::level::ExitId::new(0x1CB).expect("valid");
        assert_eq!(lm.secondary_exit_ext(&rom, exit).expect("reads"), Some([0, 0]));
        assert_eq!(lm.exanim_flag(&rom, id).expect("reads"), 0);
    }
}
