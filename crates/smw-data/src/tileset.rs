//! Which GFX files a level's tileset and sprite set load.
//!
//! See `docs/formats/gfx.md`.

use snes_rom::{Rom, RomError, SnesAddr};

use crate::gfx::GfxId;
use crate::gfx_bypass::GfxBypass;
use crate::level::PrimaryHeader;
use crate::map16::TILESET_COUNT;
use crate::{LevelId, RomLayout};

/// Number of sprite sets in the vanilla table.
pub const SPRITE_SET_COUNT: u8 = 16;

/// The four layer files (`FG1`, `FG2`, `BG1`, `FG3`) for an FG/BG tileset.
///
/// # Errors
///
/// [`TilesetError::NoSuchTileset`] or ROM read failures.
pub fn layer_gfx_files(
    rom: &Rom,
    layout: &RomLayout,
    tileset: u8,
) -> Result<[GfxId; 4], TilesetError> {
    if tileset >= TILESET_COUNT {
        return Err(TilesetError::NoSuchTileset(tileset));
    }
    read_four(rom, layout.layer_gfx_list, tileset)
}

/// The four sprite files (`SP1`–`SP4`) for a sprite set.
///
/// # Errors
///
/// [`TilesetError::NoSuchSpriteSet`] or ROM read failures.
pub fn sprite_gfx_files(
    rom: &Rom,
    layout: &RomLayout,
    sprite_set: u8,
) -> Result<[GfxId; 4], TilesetError> {
    if sprite_set >= SPRITE_SET_COUNT {
        return Err(TilesetError::NoSuchSpriteSet(sprite_set));
    }
    read_four(rom, layout.sprite_gfx_list, sprite_set)
}

/// The GFX files a level uploads, after Lunar Magic's per-level bypass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelGfxSlots {
    /// `FG1`, `FG2`, `BG1`, `FG3`; `None` leaves the slot empty.
    pub layer: [Option<GfxId>; 4],
    /// `SP1`–`SP4`.
    pub sprites: [Option<GfxId>; 4],
    /// Whether a Lunar Magic bypass row was applied.
    pub bypassed: bool,
}

/// The files a level loads: the tileset and sprite set lists, overridden
/// slot by slot by the level's Lunar Magic bypass row when it is enabled.
///
/// # Errors
///
/// Table lookup or ROM read failures.
pub fn level_gfx_slots(
    rom: &Rom,
    layout: &RomLayout,
    id: LevelId,
    header: PrimaryHeader,
) -> Result<LevelGfxSlots, TilesetError> {
    let layer = layer_gfx_files(rom, layout, header.tileset())?.map(Some);
    let sprites = sprite_gfx_files(rom, layout, header.sprite_set())?.map(Some);
    let mut slots = LevelGfxSlots { layer, sprites, bypassed: false };
    if let Some(bypass) = GfxBypass::load(rom, layout, id)?
        && bypass.enabled()
    {
        slots.bypassed = true;
        for (slot, file) in slots.layer.iter_mut().zip(bypass.layer_files()) {
            *slot = file.or(*slot);
        }
        for (slot, file) in slots.sprites.iter_mut().zip(bypass.sprite_files()) {
            *slot = file.or(*slot);
        }
    }
    Ok(slots)
}

fn read_four(rom: &Rom, table: SnesAddr, index: u8) -> Result<[GfxId; 4], TilesetError> {
    let start =
        table.checked_add(u32::from(index).saturating_mul(4)).ok_or(RomError::Unmapped(table))?;
    let bytes = rom.read(start, 4)?;
    let id = |i: usize| GfxId::new(u16::from(bytes.get(i).copied().unwrap_or(0)));
    Ok([id(0), id(1), id(2), id(3)])
}

/// Errors from the tileset tables.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TilesetError {
    /// Tileset number outside `0..15`.
    #[error("tileset {0} does not exist (vanilla has 15)")]
    NoSuchTileset(u8),

    /// Sprite set number outside `0..16`.
    #[error("sprite set {0} does not exist (vanilla has 16)")]
    NoSuchSpriteSet(u8),

    /// ROM access failed.
    #[error(transparent)]
    Rom(#[from] RomError),
}

#[cfg(test)]
mod tests {
    use crate::gfx::GfxId;

    #[test]
    fn real_rom_tables() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = crate::RomLayout::detect(&rom).expect("vanilla");
        let normal1 = super::layer_gfx_files(&rom, &layout, 0).expect("tileset 0");
        assert_eq!(
            normal1,
            [GfxId::new(0x14), GfxId::new(0x17), GfxId::new(0x19), GfxId::new(0x15)]
        );
        let castle = super::layer_gfx_files(&rom, &layout, 1).expect("tileset 1");
        assert_eq!(castle[3], GfxId::new(0x18));
        let forest = super::sprite_gfx_files(&rom, &layout, 0).expect("sprite set 0");
        assert_eq!(
            forest,
            [GfxId::new(0x00), GfxId::new(0x01), GfxId::new(0x13), GfxId::new(0x02)]
        );
        assert!(super::layer_gfx_files(&rom, &layout, 15).is_err());
        assert!(super::sprite_gfx_files(&rom, &layout, 16).is_err());
    }
}
