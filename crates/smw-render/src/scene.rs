//! Composes a whole level picture: layers, background, sprite markers.
//! Shared by the CLI and the editor.

use std::collections::HashMap;

use smw_data::gfx::{GfxError, GfxFile, GfxId};
use smw_data::level::Level;
use smw_data::map16::{self, Map16Set, Map16Tile};
use smw_data::palette::{Palette, level_palette};
use smw_data::tileset::{LevelGfxSlots, level_gfx_slots};
use smw_data::{LevelId, RomLayout};
use snes_rom::Rom;

use crate::level::{LoadError, LoadedLevel, TileGrid, load_level_data};
use crate::map16_ext::{FIRST_EXT_TILE, FgPageResolver};
use crate::{CharAtlas, Raster, Rect, TileContext, draw_map16_tile};

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
    /// The level's palette.
    pub palette: Palette,
    /// Which GFX files were uploaded (after any Lunar Magic bypass).
    pub gfx_slots: LevelGfxSlots,
    /// 8×8 characters for the tileset.
    pub atlas: CharAtlas,
    /// Foreground Map16 pages 0–1 for the tileset.
    pub fg_set: Map16Set,
    /// Foreground tiles 0x200+ the level uses (Lunar Magic pages 2+).
    pub ext_tiles: HashMap<u16, Map16Tile>,
    /// Background Map16 set (the page set the level's layer 2 selects).
    pub bg_set: Map16Set,
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
        let id = level.id;
        let loaded = load_level_data(rom, layout, level)?;
        let header = &loaded.level.header;
        let palette = level_palette(rom, layout, &loaded.level)?;
        let gfx_slots = level_gfx_slots(rom, layout, id, *header)?;
        let atlas = load_slot_atlas(rom, layout, &gfx_slots)?;
        let fg_set = map16::load_fg(rom, layout, header.tileset())?;
        let ext_tiles = resolve_ext_tiles(rom, layout, &loaded)?;
        let page_set = match loaded.level.layer2 {
            smw_data::level::Layer2::Background { page_set, .. } => page_set,
            smw_data::level::Layer2::Objects { .. } => 0,
        };
        let bg_set = map16::load_bg_set(rom, layout, page_set)?;
        Ok(Self { loaded, palette, gfx_slots, atlas, fg_set, ext_tiles, bg_set })
    }

    /// Picture size in pixels.
    #[must_use]
    pub fn pixel_size(&self) -> (u32, u32) {
        (
            to_u32(self.loaded.layer1.width()).saturating_mul(16),
            to_u32(self.loaded.layer1.height()).saturating_mul(16),
        )
    }

    /// Draws the level at 1:1 scale.
    #[must_use]
    pub fn render(&self, options: RenderOptions) -> Raster {
        let (w, h) = self.pixel_size();
        let backdrop = self.palette.get(0, 0).unwrap_or_default().to_rgba8();
        let mut raster = Raster::filled(w, h, backdrop);
        let ctx = TileContext { atlas: &self.atlas, palette: &self.palette, opaque_zero: false };
        if options.layer2 {
            self.draw_layer2(&mut raster, ctx);
        }
        if options.layer1 {
            draw_grid(&mut raster, &self.loaded.layer1, self.map16(), ctx);
        }
        if options.owners {
            self.tint_owners(&mut raster);
        }
        if options.sprites {
            crate::sprites::draw_sprite_markers(
                &mut raster,
                &self.loaded.level.sprites.sprites,
                self.loaded.vertical,
            );
        }
        raster
    }

    /// Blends a per-object colour over every owned layer 1 tile.
    fn tint_owners(&self, raster: &mut Raster) {
        let owners = &self.loaded.ownership.layer1;
        for y in 0..owners.height() {
            for x in 0..owners.width() {
                let Some(index) = owners.get(x, y).flatten() else {
                    continue;
                };
                let rect = Rect {
                    x: to_u32(x).saturating_mul(16),
                    y: to_u32(y).saturating_mul(16),
                    w: 16,
                    h: 16,
                };
                raster.blend_rect(rect, owner_color(index));
            }
        }
    }

    /// The foreground Map16 lookup for this level.
    #[must_use]
    pub const fn map16(&self) -> Map16Lookup<'_> {
        Map16Lookup { pages01: &self.fg_set, ext: &self.ext_tiles }
    }

    fn draw_layer2(&self, raster: &mut Raster, ctx: TileContext<'_>) {
        if let Some(grid) = &self.loaded.layer2 {
            draw_grid(raster, grid, self.map16(), ctx);
        }
        let Some(background) = &self.loaded.background else {
            return;
        };
        for y in 0..self.loaded.layer1.height() {
            for x in 0..self.loaded.layer1.width() {
                let Some(tile) = background_tile(background, self.loaded.background_rows, x, y)
                    .and_then(|n| self.bg_set.get(usize::from(n)))
                else {
                    continue;
                };
                draw_map16_tile(
                    raster,
                    (to_u32(x).saturating_mul(16), to_u32(y).saturating_mul(16)),
                    tile,
                    ctx,
                );
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
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            let Some(tile) = grid.get(x, y).and_then(|n| lookup.get(n)) else {
                continue;
            };
            draw_map16_tile(
                raster,
                (to_u32(x).saturating_mul(16), to_u32(y).saturating_mul(16)),
                tile,
                ctx,
            );
        }
    }
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

/// Resolves every foreground tile 0x200+ the level's grids reference.
fn resolve_ext_tiles(
    rom: &Rom,
    layout: &RomLayout,
    loaded: &LoadedLevel,
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
            if let Some(tile) = resolver.tile(n)? {
                out.insert(n, tile);
            }
        }
    }
    Ok(out)
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
