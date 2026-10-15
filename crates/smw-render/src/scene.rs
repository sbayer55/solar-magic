//! Composes a whole level picture: layers, background, sprite markers.
//! Shared by the CLI and the editor.

use std::collections::HashMap;
use std::sync::Arc;

use smw_data::gfx::GfxError;
use smw_data::level::Level;
use smw_data::map16::{self, Map16Set, Map16Tile};
use smw_data::{LevelId, RomLayout};
use snes_rom::Rom;

use crate::level::{LoadError, LoadedLevel, TileGrid, load_level_data};
use crate::map16_ext::{FIRST_EXT_TILE, FgPageResolver};
use crate::ownership::TileRect;
pub use crate::scene_assets::{SceneAssets, load_slot_atlas, load_tileset_atlas};
use crate::scene_patch::marker_cell;
use crate::{Raster, Rect, TileContext, draw_map16_tile};

/// What to include in the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderOptions {
    /// Draw layer 1.
    pub layer1: bool,
    /// Draw layer 2 (objects or background).
    pub layer2: bool,
    /// Draw sprite position markers.
    pub sprites: bool,
    /// Tint every layer 1 tile by the object that owns it (debug view).
    pub owners: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { layer1: true, layer2: true, sprites: false, owners: false }
    }
}

/// Everything needed to draw a level, kept so a view can redraw cheaply.
#[derive(Debug)]
pub struct LevelScene {
    /// The loaded level and its tile grids.
    pub loaded: LoadedLevel,
    /// Palette, graphics and Map16 sets; shared with the scene an edit was
    /// built from when the headers did not change.
    pub assets: Arc<SceneAssets>,
    /// Foreground tiles 0x200+ the level uses (Lunar Magic pages 2+).
    pub ext_tiles: HashMap<u16, Map16Tile>,
}

/// Where to find a foreground tile's definition: pages 0–1 from the tileset
/// set, higher pages from the resolved extension tiles.
#[derive(Debug, Clone, Copy)]
pub struct Map16Lookup<'a> {
    /// Pages 0–1.
    pub pages01: &'a Map16Set,
    /// Tiles 0x200 and up.
    pub ext: &'a HashMap<u16, Map16Tile>,
}

impl Map16Lookup<'_> {
    /// Tile `n`, if defined.
    #[must_use]
    pub fn get(&self, n: u16) -> Option<&Map16Tile> {
        if n < FIRST_EXT_TILE { self.pages01.get(usize::from(n)) } else { self.ext.get(&n) }
    }
}

/// Errors from building a scene.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SceneError {
    /// Loading the level failed.
    #[error(transparent)]
    Load(#[from] LoadError),
    /// Parsing the level failed.
    #[error(transparent)]
    Level(#[from] smw_data::level::LevelError),
    /// Graphics could not be decoded.
    #[error(transparent)]
    Gfx(#[from] GfxError),
    /// Map16 data could not be read.
    #[error(transparent)]
    Map16(#[from] map16::Map16Error),
    /// Tileset tables could not be read.
    #[error(transparent)]
    Tileset(#[from] smw_data::tileset::TilesetError),
    /// ROM access failed.
    #[error(transparent)]
    Rom(#[from] snes_rom::RomError),
}

impl LevelScene {
    /// Loads the level and everything needed to draw it.
    ///
    /// # Errors
    ///
    /// See [`SceneError`].
    pub fn load(rom: &Rom, layout: &RomLayout, id: LevelId) -> Result<Self, SceneError> {
        Self::from_level(rom, layout, Level::load(rom, layout, id)?)
    }

    /// Builds the scene for a level value, which need not match the ROM:
    /// this is how edits are previewed.
    ///
    /// # Errors
    ///
    /// See [`SceneError`].
    pub fn from_level(rom: &Rom, layout: &RomLayout, level: Level) -> Result<Self, SceneError> {
        Self::from_level_reusing(rom, layout, level, None)
    }

    /// Like [`LevelScene::from_level`], but shares `previous`'s assets (and
    /// already resolved extension tiles) when the level's headers still
    /// match them, so an object or sprite edit only re-runs the loader.
    ///
    /// # Errors
    ///
    /// See [`SceneError`].
    pub fn from_level_reusing(
        rom: &Rom,
        layout: &RomLayout,
        level: Level,
        previous: Option<&Self>,
    ) -> Result<Self, SceneError> {
        let previous = previous.filter(|p| p.assets.fits(&level));
        let loaded = load_level_data(rom, layout, level)?;
        let assets = match previous {
            Some(p) => Arc::clone(&p.assets),
            None => Arc::new(SceneAssets::build(rom, layout, &loaded.level)?),
        };
        let ext_tiles = resolve_ext_tiles(rom, layout, &loaded, previous.map(|p| &p.ext_tiles))?;
        Ok(Self { loaded, assets, ext_tiles })
    }

    /// Picture size in pixels.
    #[must_use]
    pub fn pixel_size(&self) -> (u32, u32) {
        (
            to_u32(self.loaded.layer1.width()).saturating_mul(16),
            to_u32(self.loaded.layer1.height()).saturating_mul(16),
        )
    }

    /// The whole level as a tile rectangle.
    #[must_use]
    pub const fn tile_bounds(&self) -> TileRect {
        TileRect { x: 0, y: 0, w: self.loaded.layer1.width(), h: self.loaded.layer1.height() }
    }

    /// Draws the level at 1:1 scale.
    #[must_use]
    pub fn render(&self, options: RenderOptions) -> Raster {
        let (w, h) = self.pixel_size();
        let mut raster = Raster::new(w, h);
        self.render_region(&mut raster, options, self.tile_bounds());
        raster
    }

    /// Redraws the tiles of `rect` into `raster`, which must be a picture of
    /// this level at 1:1 scale: the backdrop, layer 2, layer 1, the owner
    /// tint and the sprite markers inside the rectangle, in that order.
    pub fn render_region(&self, raster: &mut Raster, options: RenderOptions, rect: TileRect) {
        let assets = &*self.assets;
        let backdrop = assets.palette.get(0, 0).unwrap_or_default().to_rgba8();
        raster.fill_rect(pixel_rect(rect), backdrop);
        let ctx =
            TileContext { atlas: &assets.atlas, palette: &assets.palette, opaque_zero: false };
        if options.layer2 {
            self.draw_layer2(raster, ctx, rect);
        }
        if options.layer1 {
            draw_grid_region(raster, &self.loaded.layer1, self.map16(), ctx, rect);
        }
        if options.owners {
            self.tint_owners(raster, rect);
        }
        if options.sprites {
            let vertical = self.loaded.vertical;
            let inside: Vec<_> = self
                .loaded
                .level
                .sprites
                .sprites
                .iter()
                .copied()
                .filter(|s| marker_cell(*s, vertical).intersects(rect))
                .collect();
            crate::sprites::draw_sprite_markers(raster, &inside, vertical);
        }
    }

    /// Blends a per-object colour over every owned layer 1 tile in `rect`.
    fn tint_owners(&self, raster: &mut Raster, rect: TileRect) {
        let owners = &self.loaded.ownership.layer1;
        for y in rows(rect, owners.height()) {
            for x in cols(rect, owners.width()) {
                let Some(index) = owners.get(x, y).flatten() else {
                    continue;
                };
                raster.blend_rect(tile_pixels(x, y), owner_color(index));
            }
        }
    }

    /// The foreground Map16 lookup for this level.
    #[must_use]
    pub fn map16(&self) -> Map16Lookup<'_> {
        Map16Lookup { pages01: &self.assets.fg_set, ext: &self.ext_tiles }
    }

    fn draw_layer2(&self, raster: &mut Raster, ctx: TileContext<'_>, rect: TileRect) {
        if let Some(grid) = &self.loaded.layer2 {
            draw_grid_region(raster, grid, self.map16(), ctx, rect);
        }
        let Some(background) = &self.loaded.background else {
            return;
        };
        let bg_set = &self.assets.bg_set;
        for y in rows(rect, self.loaded.layer1.height()) {
            for x in cols(rect, self.loaded.layer1.width()) {
                let Some(tile) = background_tile(background, self.loaded.background_rows, x, y)
                    .and_then(|n| bg_set.get(usize::from(n)))
                else {
                    continue;
                };
                draw_map16_tile(raster, tile_origin(x, y), tile, ctx);
            }
        }
    }

    /// Map16 number under level tile `(x, y)` on layer 1.
    #[must_use]
    pub fn layer1_tile(&self, x: usize, y: usize) -> Option<u16> {
        self.loaded.layer1.get(x, y)
    }
}

/// Draws a grid of Map16 tiles through `lookup`.
pub fn draw_grid(
    raster: &mut Raster,
    grid: &TileGrid,
    lookup: Map16Lookup<'_>,
    ctx: TileContext<'_>,
) {
    let all = TileRect { x: 0, y: 0, w: grid.width(), h: grid.height() };
    draw_grid_region(raster, grid, lookup, ctx, all);
}

/// Draws the Map16 tiles of `grid` inside `rect` through `lookup`.
pub fn draw_grid_region(
    raster: &mut Raster,
    grid: &TileGrid,
    lookup: Map16Lookup<'_>,
    ctx: TileContext<'_>,
    rect: TileRect,
) {
    for y in rows(rect, grid.height()) {
        for x in cols(rect, grid.width()) {
            let Some(tile) = grid.get(x, y).and_then(|n| lookup.get(n)) else {
                continue;
            };
            draw_map16_tile(raster, tile_origin(x, y), tile, ctx);
        }
    }
}

/// The rows of `rect` that exist in a grid `height` tall.
fn rows(rect: TileRect, height: usize) -> core::ops::Range<usize> {
    rect.y..rect.y.saturating_add(rect.h).min(height)
}

/// The columns of `rect` that exist in a grid `width` wide.
fn cols(rect: TileRect, width: usize) -> core::ops::Range<usize> {
    rect.x..rect.x.saturating_add(rect.w).min(width)
}

/// Top-left pixel of a tile.
fn tile_origin(x: usize, y: usize) -> (u32, u32) {
    (to_u32(x).saturating_mul(16), to_u32(y).saturating_mul(16))
}

/// The 16×16 pixels of a tile.
fn tile_pixels(x: usize, y: usize) -> Rect {
    let (px, py) = tile_origin(x, y);
    Rect { x: px, y: py, w: 16, h: 16 }
}

/// The pixels a tile rectangle covers.
#[must_use]
pub fn pixel_rect(rect: TileRect) -> Rect {
    let (x, y) = tile_origin(rect.x, rect.y);
    Rect { x, y, w: to_u32(rect.w).saturating_mul(16), h: to_u32(rect.h).saturating_mul(16) }
}

/// Tile of the two-screen background at level tile `(x, y)`.
///
/// Screens are 16×`rows` row-major blocks (27 rows = 0x1B0 entries in
/// vanilla, 32 rows = 0x200 in Lunar Magic 16-bit tilemaps), repeating every
/// two screens horizontally and every `rows` rows vertically.
#[must_use]
pub fn background_tile(background: &[u16], rows: u8, x: usize, y: usize) -> Option<u16> {
    let rows = usize::from(rows.max(1));
    let screen = (x / 16) % 2;
    let row = y.checked_rem(rows).unwrap_or(0);
    background
        .get(
            screen
                .saturating_mul(rows.saturating_mul(16))
                .saturating_add(row.saturating_mul(16))
                .saturating_add(x % 16),
        )
        .copied()
}

/// Resolves every foreground tile 0x200+ the level's grids reference,
/// taking already known definitions from `known` instead of the emulator.
fn resolve_ext_tiles(
    rom: &Rom,
    layout: &RomLayout,
    loaded: &LoadedLevel,
    known: Option<&HashMap<u16, Map16Tile>>,
) -> Result<HashMap<u16, Map16Tile>, LoadError> {
    let mut out = HashMap::new();
    let Some(mut resolver) = FgPageResolver::new(rom, layout) else {
        return Ok(out);
    };
    let grids = [Some(&loaded.layer1), loaded.layer2.as_ref()];
    for grid in grids.into_iter().flatten() {
        for &n in grid.tiles().iter().filter(|&&n| n >= FIRST_EXT_TILE) {
            if out.contains_key(&n) {
                continue;
            }
            if let Some(tile) = known.and_then(|k| k.get(&n)) {
                out.insert(n, *tile);
            } else if let Some(tile) = resolver.tile(n)? {
                out.insert(n, tile);
            }
        }
    }
    Ok(out)
}

/// A stable, distinct-looking colour per object index (half transparent).
#[must_use]
pub const fn owner_color(index: u16) -> crate::Rgba8 {
    let hue = index.wrapping_mul(0x9E37);
    let [r, g] = hue.to_le_bytes();
    [r | 0x40, g | 0x40, index.wrapping_mul(53).to_le_bytes()[0] | 0x40, 0x80]
}

fn to_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use smw_data::level::{Layer, Level, TilePos};
    use smw_data::{LevelId, RomLayout};

    use super::{LevelScene, RenderOptions};
    use crate::scene_patch::{Dirty, dirty_between};

    /// A Lunar Magic 3.x save that touched no level renders every probed
    /// level exactly like the vanilla ROM it came from: the hijacks, the
    /// rewritten layer 2 bank bytes and the new flag bits change nothing.
    #[test]
    fn lm3_unchanged_save_renders_like_vanilla() {
        let (Some(vanilla), Some(lm3)) = (std::env::var_os("SMW_ROM"), std::env::var_os("LM3_ROM"))
        else {
            eprintln!("SMW_ROM or LM3_ROM not set; skipping");
            return;
        };
        let vanilla = snes_rom::Rom::load(std::path::Path::new(&vanilla)).expect("loads");
        let lm3 = snes_rom::Rom::load(std::path::Path::new(&lm3)).expect("loads");
        let layout_v = RomLayout::detect(&vanilla).expect("vanilla");
        let layout_3 = RomLayout::detect(&lm3).expect("lunar magic 3");
        assert!(layout_3.is_lunar_magic());
        let options = RenderOptions { sprites: true, ..RenderOptions::default() };
        for number in [0x105u16, 0x001, 0x0C5, 0x0E8, 0x1F4] {
            let id = LevelId::new(number).expect("valid");
            let a = LevelScene::load(&vanilla, &layout_v, id).expect("vanilla scene");
            let b = LevelScene::load(&lm3, &layout_3, id).expect("lm3 scene");
            assert_eq!(a.render(options), b.render(options), "level {id} differs");
        }
    }

    /// Moving an object and a sprite shares the assets, dirties exactly the
    /// tiles that changed, and repainting that rectangle into the old
    /// picture gives the new picture pixel for pixel.
    #[test]
    fn edits_reuse_assets_and_patch_the_picture() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = RomLayout::detect(&rom).expect("layout");
        let id = LevelId::new(0x105).expect("id");
        let level = Level::load(&rom, &layout, id).expect("level");
        let options = RenderOptions { sprites: true, ..RenderOptions::default() };
        let old = LevelScene::from_level(&rom, &layout, level.clone()).expect("scene");
        let mut picture = old.render(options);

        let mut edited = level;
        let index = edited.layer1.objects.iter().position(|o| o.position(false).is_some()).unwrap();
        let from = edited.layer1.objects[index].position(false).unwrap();
        edited.move_object(Layer::One, index, TilePos::new(from.x + 3, from.y + 1)).unwrap();
        let sprite_from = edited.sprites.sprites[0].position(false);
        edited.move_sprite(0, TilePos::new(sprite_from.x + 2, sprite_from.y)).unwrap();

        let new = LevelScene::from_level_reusing(&rom, &layout, edited, Some(&old)).expect("scene");
        assert!(Arc::ptr_eq(&old.assets, &new.assets), "headers unchanged: assets shared");
        let Dirty::Rect(rect) = dirty_between(&old, &new, options) else {
            panic!("a move dirties a rectangle");
        };
        let old_bounds = old.loaded.ownership.bounds1[index].unwrap();
        let new_bounds = new.loaded.ownership.bounds1[index].unwrap();
        assert!(rect.intersects(old_bounds) && rect.intersects(new_bounds));
        assert!(rect.contains(usize::from(sprite_from.x), usize::from(sprite_from.y)));
        assert!(rect.contains(usize::from(sprite_from.x) + 2, usize::from(sprite_from.y)));
        assert!(rect.w < old.loaded.layer1.width(), "a move must not dirty the whole level");

        new.render_region(&mut picture, options, rect);
        assert_eq!(picture, new.render(options));
        assert_eq!(dirty_between(&new, &new, options), Dirty::Nothing);

        let mut other = new.loaded.level.clone();
        other.header.raw[0] ^= 0x01;
        let rebuilt =
            LevelScene::from_level_reusing(&rom, &layout, other, Some(&new)).expect("scene");
        assert!(!Arc::ptr_eq(&new.assets, &rebuilt.assets), "a header edit rebuilds the assets");
        assert_eq!(dirty_between(&new, &rebuilt, options), Dirty::All);
    }
}
