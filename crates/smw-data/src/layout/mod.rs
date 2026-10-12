//! Where SMW's data tables live in a given ROM.
//!
//! The vanilla USA ROM's addresses are in [`VANILLA`]; each field names the
//! document that records how it was verified. Lunar Magic keeps those tables
//! but adds its own, described by [`LunarMagicTables`]; [`RomLayout::detect`]
//! picks the layout.

mod known;
mod lunar_magic;
mod markers;
mod vanilla;

use core::fmt;

use snes_rom::{Rom, SnesAddr};

pub use known::{KnownRange, label_address};
pub use lunar_magic::{
    HijackScan, LM_25X, Layer2FlagsFormat, LmFeatures, LunarMagicTables, V3X_LEVEL_HEIGHT,
    V3X_SCREEN_TABLE_SEEDS, WramSeed, scan_hijacks,
};
pub use markers::{LmMarker, MARKER_LEN, MarkerPlacement, scan_marker_bytes, scan_markers};
pub use vanilla::{VANILLA, VANILLA_FREE_RANGES};

use crate::LayoutError;
use crate::level::ObjectFormat;

/// The ASCII prefix Lunar Magic writes into ROMs it saves.
///
/// Searched for anywhere in the image rather than at a fixed offset so the
/// check survives Lunar Magic moving it between versions. Verification of the
/// exact location is tracked in `docs/formats/lunar-magic-ext.md`.
const LUNAR_MAGIC_SIGNATURE: &[u8] = b"Lunar Magic Version";

/// Internal-header title of every SMW release Solar Magic targets.
const SMW_TITLE: &str = "SUPER MARIOWORLD";

/// Which family of layout a ROM uses.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LayoutKind {
    /// An unmodified retail ROM.
    Vanilla,
    /// A ROM saved by Lunar Magic; the version is taken from its signature.
    LunarMagic {
        /// Major version (the `2` in `2.53`).
        major: u8,
        /// Minor version (the `53` in `2.53`).
        minor: u8,
    },
}

impl fmt::Display for LayoutKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vanilla => f.write_str("Vanilla"),
            Self::LunarMagic { major, minor } => write!(f, "Lunar Magic {major}.{minor:02}"),
        }
    }
}

/// The three parallel byte tables that make up a set of 24-bit pointers.
///
/// SMW stores some pointer tables split into separate low, high and bank byte
/// arrays rather than as packed 3-byte entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitPointerTable {
    /// Table of low bytes.
    pub low: SnesAddr,
    /// Table of high bytes.
    pub high: SnesAddr,
    /// Table of bank bytes.
    pub bank: SnesAddr,
    /// Number of entries.
    pub count: usize,
}

/// The colour tables `LoadPalette` ($00:ABED in vanilla) assembles a level
/// palette from. All hold little-endian BGR555 words. `docs/formats/palettes.md`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteTables {
    /// 8 back area colours (CGRAM colour 0), selected by header bits.
    pub back_area_colors: SnesAddr,
    /// 8 background palette sets × 12 colours (rows 0–1, colours 2–7).
    pub background: SnesAddr,
    /// 16 colours: rows 0–1, colours 8–15 (layer 3 / status bar).
    pub status_bar: SnesAddr,
    /// 8 foreground palette sets × 12 colours (rows 2–3, colours 2–7).
    pub foreground: SnesAddr,
    /// 60 colours: rows 4–13, colours 2–7 (shared object and sprite colours).
    pub standard: SnesAddr,
    /// 4 player palettes × 10 colours (row 8, colours 6–15): Mario, Luigi,
    /// fire Mario, fire Luigi.
    pub player: SnesAddr,
    /// 8 sprite palette sets × 12 colours (rows 14–15, colours 2–7).
    pub sprite: SnesAddr,
    /// 21 berry colours, loaded twice (rows 2–4 and 9–11, colours 9–15).
    pub berry: SnesAddr,
}

/// Where the Map16 tile definitions live. `docs/formats/map16.md`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Map16Tables {
    /// Tiles shared by every tileset (366 × 8 bytes), consumed in order for
    /// each set bit of `tileset_bitmask`.
    pub common: SnesAddr,
    /// The 512 background tiles (page 0x10/0x11 in LM's numbering).
    pub bg_tiles: SnesAddr,
    /// 15 words: the bank-`$0D` offset of each tileset's specific tiles.
    pub tileset_locations: SnesAddr,
    /// Bank the `tileset_locations` words point into.
    pub tileset_bank: u8,
    /// 64 bytes = 512 bits, MSB first: 1 = common tile, 0 = tileset-specific.
    pub tileset_bitmask: SnesAddr,
    /// 4 words (bank `$0D`): animation frames for tiles 0x133–0x13A.
    pub appearance_table: SnesAddr,
    /// 8 tiles patched into 0x1C4–0x1C7 and 0x1EC–0x1EF for tilesets 0 and 7.
    pub diagonal_pipes: SnesAddr,
}

/// Where the game's level loader lives and the RAM it reads and writes.
/// `docs/formats/level-load-routine.md`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelLoader {
    /// `CODE_05801E`: clears tile RAM, decodes the background, runs every
    /// object routine for both layers. Called with `JSL`.
    pub entry: SnesAddr,
    /// `LoadBlkPtrs`: 2 words (layer 1, layer 2) → per-level-mode tables of
    /// per-screen 24-bit pointers to the tile *low* bytes in RAM.
    pub tile_ram_ptrs_low: SnesAddr,
    /// `LoadBlkTable2`: same shape, for the tile *high* bytes.
    pub tile_ram_ptrs_high: SnesAddr,
    /// Direct-page address of the 24-bit layer 1 data pointer (`$65`).
    pub ram_layer1_ptr: u32,
    /// Direct-page address of the 24-bit layer 2 data pointer (`$68`).
    pub ram_layer2_ptr: u32,
    /// Direct-page address of the 24-bit sprite data pointer (`$CE`).
    pub ram_sprite_ptr: u32,
    /// Game mode byte (`$0100`); set to `$22` so post-load sprite init is skipped.
    pub ram_game_mode: u32,
    /// 16-bit level number (`$0E`). Lunar Magic's layer 2 hook indexes its
    /// per-level flag table with it, so the loader must see it.
    pub ram_level_number: u32,
    /// Lunar Magic's own copy of the level number (`$010B`).
    pub ram_level_number_copy: u32,
    /// Decoded background tilemap low bytes (`$7E:B900`, 0x400 bytes).
    pub ram_bg_tilemap_low: u32,
    /// Decoded background tilemap high bytes (`$7E:BD00`, 0x400 bytes).
    pub ram_bg_tilemap_high: u32,
}

/// Where the game decides which level to load, and the RAM the editor
/// pokes to send it into a level. `docs/formats/smw-launch-protocol.md`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchHooks {
    /// First instruction after the level number (`$0E/$0F`) is final in
    /// the level-entry routine (`CODE_05D8B7`): the launch breakpoint.
    pub level_select_pc: SnesAddr,
    /// Opcode bytes expected at `level_select_pc`; the launcher refuses to
    /// launch when they are missing.
    pub level_select_fingerprint: &'static [u8],
    /// RAM the launcher reads and writes.
    pub ram: LaunchRam,
    /// Game mode values the launcher waits for and sets.
    pub modes: GameModes,
}

/// RAM addresses (bank `$7E`, given as 16-bit offsets) used to launch a level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchRam {
    /// `GameMode` (`$0100`).
    pub game_mode: u16,
    /// `OverworldOverride` (`$0109`): nonzero makes the level-entry routine
    /// skip the overworld position lookup.
    pub overworld_override: u16,
    /// Loader-side level number low/high bytes (`$0E`/`$0F`).
    pub level_number: u16,
    /// `LoadingLevelNumber` (`$17BB`).
    pub loading_level_number: u16,
    /// Lunar Magic's 16-bit copy of the level number (`$010B`).
    pub level_number_copy: u16,
    /// `TranslevelNo` (`$13BF`).
    pub translevel: u16,
    /// `OWPlayerSubmap` (`$1F11`): nonzero gives the level number's high byte.
    pub player_submap: u16,
    /// `SublevelCount` (`$141A`): zero means entering from the overworld.
    pub sublevel_count: u16,
    /// `UseSecondaryExit` (`$1B93`).
    pub use_secondary_exit: u16,
    /// `ExitTableLow` (`$19B8`), 32 per-screen exit numbers.
    pub exit_table_low: u16,
    /// `ExitTableHigh` (`$19D8`), 32 per-screen exit flags.
    pub exit_table_high: u16,
    /// `PlayerXPosNext` (`$94`), 16-bit; its high byte is the screen.
    pub player_x: u16,
    /// `PlayerYPosNext` (`$96`), 16-bit.
    pub player_y: u16,
    /// `MidwayFlag` (`$13CE`).
    pub midway_flag: u16,
    /// `Powerup` (`$19`).
    pub powerup: u16,
    /// `PlayerLives` (`$0DBE`), lives − 1.
    pub lives: u16,
    /// `PlayerCoins` (`$0DBF`).
    pub coins: u16,
    /// `PlayerItembox` (`$0DC2`).
    pub item_box: u16,
    /// `CarryYoshiThruLvls` (`$0DC1`): Yoshi colour carried into the level
    /// (0 = none).
    pub yoshi_carried: u16,
    /// `PlayerRidingYoshi` (`$187A`).
    pub riding_yoshi: u16,
    /// `YoshiColor` (`$13C7`).
    pub yoshi_color: u16,
    /// `PlayerTurnOW` (`$0DD6`): current player × 4 (0 = Mario).
    pub current_player: u16,
    /// `OWLevelTileSettings` (`$1EA2`), 96 bytes indexed by translevel; bit 6
    /// means "midway reached", which makes the entry routine use the midway
    /// screen.
    pub ow_level_tile_settings: u16,
}

/// Game mode values (`$0100`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameModes {
    /// Title screen waiting for input.
    pub title: u8,
    /// Overworld waiting for input.
    pub overworld: u8,
    /// Fade out before loading a level.
    pub fade_to_level: u8,
    /// Playing a level.
    pub level: u8,
}

/// The player entrance position tables (`docs/formats/level-header.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntranceTables {
    /// 16 low bytes of the entrance Y pixel, indexed by the Y position index.
    pub y_low: SnesAddr,
    /// 16 high bytes of the entrance Y pixel.
    pub y_high: SnesAddr,
    /// 8 low bytes of the entrance X pixel, indexed by the X position index.
    pub x_low: SnesAddr,
    /// 8 high bytes of the entrance X pixel.
    pub x_high: SnesAddr,
}

/// A range of file offsets known to hold nothing (all `0xFF` in vanilla).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreeRange {
    /// First unused file offset.
    pub pc: usize,
    /// Number of unused bytes.
    pub len: usize,
}

impl FreeRange {
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "const table constructor; every range below has end >= start"
    )]
    pub(super) const fn new(start: usize, end_inclusive: usize) -> Self {
        Self { pc: start, len: end_inclusive - start + 1 }
    }

    /// File offset one past the range.
    #[must_use]
    pub const fn end_pc(self) -> usize {
        self.pc.saturating_add(self.len)
    }

    /// LoROM bank the range starts in.
    #[must_use]
    #[allow(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "masked to 8 bits before the cast"
    )]
    pub const fn bank(self) -> u8 {
        ((self.pc >> 15) & 0xFF) as u8
    }
}

/// Resolved table locations for one ROM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomLayout {
    /// Which family of layout this is.
    pub kind: LayoutKind,

    /// 512 packed 3-byte pointers to layer 1 data (header + object stream).
    /// `docs/formats/level-header.md`
    pub layer1_ptrs: SnesAddr,

    /// 512 packed 3-byte pointers to layer 2 data (object stream or BG tilemap).
    /// `docs/formats/layer2.md`
    pub layer2_ptrs: SnesAddr,

    /// 512 packed 2-byte pointers to sprite data; the bank is implied by
    /// [`RomLayout::sprite_bank`] unless [`RomLayout::sprite_banks`] is set.
    /// `docs/formats/sprites.md`
    pub sprite_ptrs: SnesAddr,

    /// Bank byte for every sprite pointer in vanilla ROMs.
    pub sprite_bank: u8,

    /// Lunar Magic's per-level sprite bank byte table, if the ROM has one.
    pub sprite_banks: Option<SnesAddr>,

    /// Four 512-entry byte tables holding the secondary level header.
    /// `docs/formats/level-header.md`
    pub secondary_header: [SnesAddr; 4],

    /// Four 512-entry byte tables holding the secondary exits.
    /// `docs/formats/level-header.md`
    pub secondary_exits: [SnesAddr; 4],

    /// Pointers to the LC_LZ2-compressed GFX00-GFX31 files (three parallel
    /// 50-byte tables). `docs/formats/gfx.md`
    pub gfx_ptrs: SplitPointerTable,

    /// Level palette source tables. `docs/formats/palettes.md`
    pub palettes: PaletteTables,

    /// Map16 tile definition tables. `docs/formats/map16.md`
    pub map16: Map16Tables,

    /// `OBJECTGFXLIST`: 4 GFX file numbers (FG1, FG2, BG1, FG3) per FG/BG
    /// tileset. `docs/formats/gfx.md`
    pub layer_gfx_list: SnesAddr,

    /// `SPRITEGFXLIST`: 4 GFX file numbers (SP1–SP4) per sprite set.
    pub sprite_gfx_list: SnesAddr,

    /// `VerticalTable`: one byte per level mode; bit 0 set means a vertical
    /// level, bit 7 marks layer-2-vertical modes. `docs/formats/level-header.md`
    pub level_mode_table: SnesAddr,

    /// The level loader and its RAM. `docs/formats/level-load-routine.md`
    pub loader: LevelLoader,

    /// Player entrance position tables.
    pub entrance: EntranceTables,

    /// How to send the running game into a level.
    pub launch: LaunchHooks,

    /// First file offset of expanded space: everything from here up is free
    /// unless a RATS tag claims it. `docs/formats/rats.md`
    pub expanded_start_pc: usize,

    /// Byte ranges inside the original image known to be unused. These are
    /// the only places a vanilla-safe save may allocate below
    /// `expanded_start_pc` (sprite data must stay in bank `$07`). Empty for
    /// Lunar Magic ROMs, which may have used them.
    pub known_free: &'static [FreeRange],

    /// Lunar Magic's added tables, when the ROM was saved by a supported
    /// Lunar Magic version. `docs/formats/lunar-magic-ext.md`
    pub lm: Option<LunarMagicTables>,
}

impl RomLayout {
    /// Works out which layout a ROM uses.
    ///
    /// # Errors
    ///
    /// - [`LayoutError::NotSmw`] if the header title is not SMW's.
    /// - [`LayoutError::LunarMagicUnsupported`] if the ROM carries a Lunar
    ///   Magic signature but the Lunar Magic 2.5x routines the layout relies
    ///   on are not where they should be (another version, or not a level
    ///   ROM at all).
    pub fn detect(rom: &Rom) -> Result<Self, LayoutError> {
        let title = &rom.header().title;
        if title != SMW_TITLE {
            return Err(LayoutError::NotSmw { title: title.clone() });
        }
        find_lunar_magic_signature(rom.bytes())
            .map_or(Ok(VANILLA), |signature| lunar_magic::detect(rom, signature))
    }

    /// How level object streams are encoded in this ROM.
    #[must_use]
    pub const fn object_format(&self) -> ObjectFormat {
        if self.lm.is_some() { ObjectFormat::LunarMagic } else { ObjectFormat::Vanilla }
    }

    /// Whether this is a Lunar Magic layout.
    #[must_use]
    pub const fn is_lunar_magic(&self) -> bool {
        self.lm.is_some()
    }

    /// Returns the Lunar Magic version string if the ROM carries one.
    #[must_use]
    pub fn lunar_magic_signature(rom: &Rom) -> Option<String> {
        find_lunar_magic_signature(rom.bytes())
    }
}

/// Finds the first Lunar Magic signature and returns it up to the first
/// non-printable byte (the string is typically `"Lunar Magic Version 3.xx"`).
fn find_lunar_magic_signature(bytes: &[u8]) -> Option<String> {
    let start = bytes
        .windows(LUNAR_MAGIC_SIGNATURE.len())
        .position(|window| window == LUNAR_MAGIC_SIGNATURE)?;
    let tail = bytes.get(start..)?;
    let text: String = tail
        .iter()
        .take_while(|&&b| b.is_ascii_graphic() || b == b' ')
        .map(|&b| char::from(b))
        .collect();
    Some(text.trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::{LayoutKind, RomLayout, find_lunar_magic_signature};

    #[test]
    fn lm_rom_detects_as_lunar_magic() {
        let Some(path) = std::env::var_os("LM_ROM") else {
            eprintln!("LM_ROM not set; skipping Lunar Magic ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = RomLayout::detect(&rom).expect("supported");
        assert!(matches!(layout.kind, LayoutKind::LunarMagic { major: 2, .. }), "{}", layout.kind);
        assert!(layout.lm.is_some());
        assert_eq!(layout.sprite_banks, layout.lm.map(|lm| lm.sprite_banks));
        assert_eq!(layout.object_format(), crate::level::ObjectFormat::LunarMagic);
    }

    #[test]
    fn signature_extraction() {
        let mut buf = vec![0u8; 64];
        let sig = b"Lunar Magic Version 3.40 Public\0junk";
        buf.extend_from_slice(sig);
        assert_eq!(
            find_lunar_magic_signature(&buf).as_deref(),
            Some("Lunar Magic Version 3.40 Public")
        );
        assert_eq!(find_lunar_magic_signature(&[0u8; 64]), None);
    }
}
