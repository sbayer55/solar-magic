//! The parts of a level picture that depend only on the ROM and the level's
//! headers: palette, GFX slots, character atlas and Map16 sets.
//!
//! Moving, inserting or deleting objects and sprites changes none of them,
//! so the editor keeps one [`SceneAssets`] alive across such edits and only
//! re-runs the level loader. [`AssetKey`] says when they must be rebuilt.

use smw_data::gfx::{GfxError, GfxFile, GfxId};
use smw_data::level::{Layer2, Level, LmLevelInfo};
use smw_data::map16::{self, Map16Set};
use smw_data::palette::{Palette, level_palette};
use smw_data::tileset::{LevelGfxSlots, level_gfx_slots};
use smw_data::{LevelId, RomLayout};
use snes_rom::Rom;

use crate::CharAtlas;
use crate::scene::SceneError;

/// Everything the assets are a function of, besides the ROM itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssetKey {
    /// The level (Lunar Magic's bypass and custom palette tables are per level).
    pub id: LevelId,
    /// Primary header bytes: tileset, palette fields, sprite set.
    pub header: [u8; 5],
    /// Secondary header bytes.
    pub secondary: [u8; 4],
    /// The background Map16 page set layer 2 selects (0 for object layers).
    pub page_set: u8,
    /// Lunar Magic's per-level information (custom palette block and so on).
    pub lm: Option<LmLevelInfo>,
}

impl AssetKey {
    /// The key for `level` as it currently is.
    #[must_use]
    pub const fn of(level: &Level) -> Self {
        let page_set = match level.layer2 {
            Layer2::Background { page_set, .. } => page_set,
            Layer2::Objects { .. } => 0,
        };
        Self {
            id: level.id,
            header: level.header.raw,
            secondary: level.secondary.raw,
            page_set,
            lm: level.lm,
        }
    }
}

/// Palette, graphics and Map16 definitions for one level.
#[derive(Debug, Clone)]
pub struct SceneAssets {
    /// What these assets were built for.
    pub key: AssetKey,
    /// The level's palette.
    pub palette: Palette,
    /// Which GFX files were uploaded (after any Lunar Magic bypass).
    pub gfx_slots: LevelGfxSlots,
    /// 8×8 characters for the tileset.
    pub atlas: CharAtlas,
    /// Foreground Map16 pages 0–1 for the tileset.
    pub fg_set: Map16Set,
    /// Background Map16 set (the page set the level's layer 2 selects).
    pub bg_set: Map16Set,
}

impl SceneAssets {
    /// Reads everything for `level` from the ROM.
    ///
    /// # Errors
    ///
    /// See [`SceneError`].
    pub fn build(rom: &Rom, layout: &RomLayout, level: &Level) -> Result<Self, SceneError> {
        let key = AssetKey::of(level);
        let header = &level.header;
        let palette = level_palette(rom, layout, level)?;
        let gfx_slots = level_gfx_slots(rom, layout, level.id, *header)?;
        let atlas = load_slot_atlas(rom, layout, &gfx_slots)?;
        let fg_set = map16::load_fg(rom, layout, header.tileset())?;
        let bg_set = map16::load_bg_set(rom, layout, key.page_set)?;
        Ok(Self { key, palette, gfx_slots, atlas, fg_set, bg_set })
    }

    /// Whether these assets still fit `level`.
    #[must_use]
    pub fn fits(&self, level: &Level) -> bool {
        self.key == AssetKey::of(level)
    }
}

/// Loads the four layer GFX files a tileset uses into a character atlas.
///
/// # Errors
///
/// Tileset table or GFX decoding failures.
pub fn load_tileset_atlas(
    rom: &Rom,
    layout: &RomLayout,
    tileset: u8,
) -> Result<CharAtlas, SceneError> {
    let ids = smw_data::tileset::layer_gfx_files(rom, layout, tileset)?;
    let slots = LevelGfxSlots { layer: ids.map(Some), sprites: [None; 4], bypassed: false };
    load_slot_atlas(rom, layout, &slots)
}

/// Loads a level's layer slots into an atlas. Empty slots and ExGFX that
/// were never inserted stay blank; other failures are errors.
///
/// # Errors
///
/// GFX decoding failures.
pub fn load_slot_atlas(
    rom: &Rom,
    layout: &RomLayout,
    slots: &LevelGfxSlots,
) -> Result<CharAtlas, SceneError> {
    let load = |id: Option<GfxId>| -> Result<Option<GfxFile>, GfxError> {
        match id.map(|id| GfxFile::load(rom, layout, id)) {
            None | Some(Err(GfxError::NotInserted(_))) => Ok(None),
            Some(Ok(file)) => Ok(Some(file)),
            Some(Err(e)) => Err(e),
        }
    };
    let files = [
        load(slots.layer[0])?,
        load(slots.layer[1])?,
        load(slots.layer[2])?,
        load(slots.layer[3])?,
    ];
    Ok(CharAtlas::from_slots([
        files[0].as_ref(),
        files[1].as_ref(),
        files[2].as_ref(),
        files[3].as_ref(),
    ]))
}

#[cfg(test)]
mod tests {
    use smw_data::level::{Level, ObjectSpec, TilePos};
    use smw_data::{LevelId, RomLayout};

    use super::{AssetKey, SceneAssets};

    /// Object edits keep the key; header edits change it.
    #[test]
    fn key_follows_the_headers_only() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = RomLayout::detect(&rom).expect("layout");
        let id = LevelId::new(0x105).expect("id");
        let mut level = Level::load(&rom, &layout, id).expect("level");
        let assets = SceneAssets::build(&rom, &layout, &level).expect("assets");
        let before = AssetKey::of(&level);
        let spec = ObjectSpec { number: 1, third: 0x11, pos: TilePos::new(4, 4) };
        level.insert_object(smw_data::level::Layer::One, 0, spec).expect("insert");
        assert_eq!(AssetKey::of(&level), before);
        assert!(assets.fits(&level));
        level.header.raw[4] ^= 0x0F;
        assert_ne!(AssetKey::of(&level), before);
        assert!(!assets.fits(&level));
    }
}
