//! Reusing the parts of a scene that an object, sprite or exit edit cannot
//! change. A [`SceneKey`] names what those parts (and a machine capture)
//! depend on; a [`SceneCache`] keeps the last set built for a key.

use std::sync::Arc;

use smw_data::level::{Layer2, Level, LmLevelInfo, PrimaryHeader, SecondaryHeader};
use smw_data::map16::{self, Map16Set};
use smw_data::palette::{Palette, level_palette};
use smw_data::tileset::{LevelGfxSlots, level_gfx_slots};
use smw_data::{LevelId, RomLayout};
use snes_rom::Rom;

use super::{SceneError, load_slot_atlas};
use crate::CharAtlas;
use crate::assets::LevelAssets;

/// Everything the shared scene parts and the machine capture depend on.
///
/// The headers (graphics, palettes, mode, scroll, entrance), the layer 2
/// page set and Lunar Magic's per-level records. Objects, sprites and
/// exits are deliberately absent.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SceneKey {
    /// The level (its bypass row is read from the ROM by number).
    pub id: LevelId,
    /// Primary header.
    pub header: PrimaryHeader,
    /// Secondary header.
    pub secondary: SecondaryHeader,
    /// Background page set, or `None` when layer 2 holds objects.
    pub page_set: Option<u8>,
    /// Lunar Magic's records (custom palette, flags).
    pub lm: Option<LmLevelInfo>,
}

impl SceneKey {
    /// The key for a level value.
    #[must_use]
    pub const fn of(level: &Level) -> Self {
        let page_set = match &level.layer2 {
            Layer2::Background { page_set, .. } => Some(*page_set),
            Layer2::Objects { .. } => None,
        };
        Self {
            id: level.id,
            header: level.header,
            secondary: level.secondary,
            page_set,
            lm: level.lm,
        }
    }
}

/// The parts of a scene every edit under one key shares.
#[derive(Debug, Clone)]
pub struct SceneShared {
    /// What these parts were built for.
    pub key: SceneKey,
    /// The level's palette.
    pub palette: Palette,
    /// Which GFX files were uploaded.
    pub gfx_slots: LevelGfxSlots,
    /// 8×8 characters for the tileset.
    pub atlas: CharAtlas,
    /// Foreground Map16 pages 0–1.
    pub fg_set: Map16Set,
    /// Background Map16 set.
    pub bg_set: Map16Set,
}

impl SceneShared {
    /// Builds the shared parts for `level`.
    ///
    /// # Errors
    ///
    /// See [`SceneError`].
    pub fn build(rom: &Rom, layout: &RomLayout, level: &Level) -> Result<Self, SceneError> {
        let key = SceneKey::of(level);
        let header = level.header;
        let palette = level_palette(rom, layout, level)?;
        let gfx_slots = level_gfx_slots(rom, layout, level.id, header)?;
        let atlas = load_slot_atlas(rom, layout, &gfx_slots)?;
        let fg_set = map16::load_fg(rom, layout, header.tileset())?;
        let bg_set = map16::load_bg_set(rom, layout, key.page_set.unwrap_or(0))?;
        Ok(Self { key, palette, gfx_slots, atlas, fg_set, bg_set })
    }
}

/// The last shared parts and machine assets, each with the key they belong to.
#[derive(Debug, Clone, Default)]
pub struct SceneCache {
    shared: Option<Arc<SceneShared>>,
    assets: Option<(SceneKey, Arc<LevelAssets>)>,
}

impl SceneCache {
    /// An empty cache.
    #[must_use]
    pub const fn new() -> Self {
        Self { shared: None, assets: None }
    }

    /// The cached shared parts.
    #[must_use]
    pub const fn shared(&self) -> Option<&Arc<SceneShared>> {
        self.shared.as_ref()
    }

    /// The key of the cached shared parts.
    #[must_use]
    pub fn key(&self) -> Option<&SceneKey> {
        self.shared.as_deref().map(|s| &s.key)
    }

    /// The machine assets captured for `key`, if any.
    #[must_use]
    pub fn assets_for(&self, key: &SceneKey) -> Option<Arc<LevelAssets>> {
        self.assets.as_ref().filter(|(k, _)| k == key).map(|(_, a)| Arc::clone(a))
    }

    /// Keeps machine assets for `key` (replacing any other).
    pub fn set_assets(&mut self, key: SceneKey, assets: Arc<LevelAssets>) {
        self.assets = Some((key, assets));
    }

    /// Forgets everything.
    pub fn clear(&mut self) {
        self.shared = None;
        self.assets = None;
    }

    /// The shared parts for `level`, built when the cached ones are for
    /// another key.
    ///
    /// # Errors
    ///
    /// See [`SceneError`].
    pub fn shared_for(
        &mut self,
        rom: &Rom,
        layout: &RomLayout,
        level: &Level,
    ) -> Result<Arc<SceneShared>, SceneError> {
        let key = SceneKey::of(level);
        if let Some(shared) = self.shared.as_ref().filter(|s| s.key == key) {
            return Ok(Arc::clone(shared));
        }
        let shared = Arc::new(SceneShared::build(rom, layout, level)?);
        self.shared = Some(Arc::clone(&shared));
        Ok(shared)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use smw_data::level::fields::primary;
    use smw_data::level::{Level, SpriteSpec, TilePos};
    use smw_data::{LevelId, RomLayout};

    use super::SceneCache;
    use crate::scene::{LevelScene, RenderOptions};

    /// Sprite edits reuse the shared parts; a tileset change rebuilds them.
    /// Either way the cached scene draws exactly what a fresh one does.
    #[test]
    #[allow(clippy::expect_used, clippy::print_stderr, reason = "test harness")]
    fn cached_scene_reuses_shared_parts_until_the_key_changes() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = RomLayout::detect(&rom).expect("layout");
        let id = LevelId::new(0x105).expect("valid");
        let level = Level::load(&rom, &layout, id).expect("level");
        let mut cache = SceneCache::new();
        let options = RenderOptions::default();

        let first =
            LevelScene::from_level_cached(&rom, &layout, level.clone(), &mut cache).expect("scene");
        let shared = Arc::clone(cache.shared().expect("built"));
        assert_eq!(
            first.render(options),
            LevelScene::from_level(&rom, &layout, level.clone()).expect("plain").render(options)
        );

        let mut edited = level.clone();
        let spec = SpriteSpec { number: 0x0F, extra_bits: 0, pos: TilePos::new(5, 20) };
        edited.insert_sprite(usize::MAX, spec).expect("inserts");
        LevelScene::from_level_cached(&rom, &layout, edited, &mut cache).expect("scene");
        assert!(Arc::ptr_eq(&shared, cache.shared().expect("kept")));

        let mut retiled = level;
        retiled.set_header_field(primary::TILESET, 3).expect("sets");
        let third = LevelScene::from_level_cached(&rom, &layout, retiled.clone(), &mut cache)
            .expect("scene");
        assert!(!Arc::ptr_eq(&shared, cache.shared().expect("rebuilt")));
        assert_eq!(
            third.render(options),
            LevelScene::from_level(&rom, &layout, retiled).expect("plain").render(options)
        );
    }
}
