//! Where SMW's data tables live in a given ROM.
//!
//! The addresses below are the vanilla USA ROM's. Each constant names the
//! document that records how it was verified. Lunar Magic moves several of
//! these tables; detection of such ROMs is in [`RomLayout::detect`].

use snes_rom::{Rom, SnesAddr};

use crate::LayoutError;

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
    /// Decoded background tilemap low bytes (`$7E:B900`, 0x400 bytes).
    pub ram_bg_tilemap_low: u32,
    /// Decoded background tilemap high bytes (`$7E:BD00`, 0x400 bytes).
    pub ram_bg_tilemap_high: u32,
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
}

/// The vanilla USA ROM layout.
pub const VANILLA: RomLayout = RomLayout {
    kind: LayoutKind::Vanilla,
    layer1_ptrs: SnesAddr::lit(0x05_E000),
    layer2_ptrs: SnesAddr::lit(0x05_E600),
    sprite_ptrs: SnesAddr::lit(0x05_EC00),
    sprite_bank: 0x07,
    sprite_banks: None,
    secondary_header: [
        SnesAddr::lit(0x05_F000),
        SnesAddr::lit(0x05_F200),
        SnesAddr::lit(0x05_F400),
        SnesAddr::lit(0x05_F600),
    ],
    gfx_ptrs: SplitPointerTable {
        low: SnesAddr::lit(0x00_B992),
        high: SnesAddr::lit(0x00_B9C4),
        bank: SnesAddr::lit(0x00_B9F6),
        count: 0x32,
    },
    palettes: PaletteTables {
        back_area_colors: SnesAddr::lit(0x00_B0A0),
        background: SnesAddr::lit(0x00_B0B0),
        status_bar: SnesAddr::lit(0x00_B170),
        foreground: SnesAddr::lit(0x00_B190),
        standard: SnesAddr::lit(0x00_B250),
        player: SnesAddr::lit(0x00_B2C8),
        sprite: SnesAddr::lit(0x00_B318),
        berry: SnesAddr::lit(0x00_B674),
    },
    map16: Map16Tables {
        common: SnesAddr::lit(0x0D_8000),
        bg_tiles: SnesAddr::lit(0x0D_9100),
        tileset_locations: SnesAddr::lit(0x05_8000),
        tileset_bank: 0x0D,
        tileset_bitmask: SnesAddr::lit(0x05_81BB),
        appearance_table: SnesAddr::lit(0x05_8776),
        diagonal_pipes: SnesAddr::lit(0x0D_8A70),
    },
    layer_gfx_list: SnesAddr::lit(0x00_A92B),
    sprite_gfx_list: SnesAddr::lit(0x00_A8C3),
    level_mode_table: SnesAddr::lit(0x05_8417),
    loader: LevelLoader {
        entry: SnesAddr::lit(0x05_801E),
        tile_ram_ptrs_low: SnesAddr::lit(0x00_BEA8),
        tile_ram_ptrs_high: SnesAddr::lit(0x00_BEAC),
        ram_layer1_ptr: 0x00_0065,
        ram_layer2_ptr: 0x00_0068,
        ram_sprite_ptr: 0x00_00CE,
        ram_game_mode: 0x00_0100,
        ram_bg_tilemap_low: 0x7E_B900,
        ram_bg_tilemap_high: 0x7E_BD00,
    },
};

impl RomLayout {
    /// Works out which layout a ROM uses.
    ///
    /// # Errors
    ///
    /// - [`LayoutError::NotSmw`] if the header title is not SMW's.
    /// - [`LayoutError::LunarMagicUnsupported`] if the ROM carries a Lunar
    ///   Magic signature. Reading such ROMs is planned but not implemented.
    pub fn detect(rom: &Rom) -> Result<Self, LayoutError> {
        let title = &rom.header().title;
        if title != SMW_TITLE {
            return Err(LayoutError::NotSmw { title: title.clone() });
        }
        if let Some(signature) = find_lunar_magic_signature(rom.bytes()) {
            return Err(LayoutError::LunarMagicUnsupported { signature });
        }
        Ok(VANILLA)
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
    use super::find_lunar_magic_signature;

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
