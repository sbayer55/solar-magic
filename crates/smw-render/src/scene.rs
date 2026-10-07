//! Composes a whole level picture: layers, background, sprite markers.
//! Shared by the CLI and the editor.

use smw_data::gfx::{GfxFile, GfxId};
use smw_data::map16::{self, Map16Set};
use smw_data::palette::{Palette, build_level_palette};
use smw_data::{LevelId, RomLayout};
use snes_rom::Rom;

use crate::level::{HORIZONTAL_ROWS, LoadError, LoadedLevel, TileGrid, load_level};
use crate::{CharAtlas, Raster, TileContext, draw_map16_tile};

/// What to include in the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderOptions {
    /// Draw layer 1.
    pub layer1: bool,
    /// Draw layer 2 (objects or background).
    pub layer2: bool,
    /// Draw sprite position markers.
    pub sprites: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { layer1: true, layer2: true, sprites: false }
    }
}

/// Everything needed to draw a level, kept so a view can redraw cheaply.
#[derive(Debug)]
pub struct LevelScene {
    /// The loaded level and its tile grids.
    pub loaded: LoadedLevel,
    /// The level's palette.
    pub palette: Palette,
    /// 8×8 characters for the tileset.
    pub atlas: CharAtlas,
    /// Foreground Map16 pages 0–1 for the tileset.
    pub fg_set: Map16Set,
    /// Background Map16 set.
    pub bg_set: Map16Set,
}

/// Errors from building a scene.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SceneError {
    /// Loading the level failed.
    #[error(transparent)]
    Load(#[from] LoadError),
    /// Graphics could not be decoded.
    #[error(transparent)]
    Gfx(#[from] smw_data::gfx::GfxError),
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
        let loaded = load_level(rom, layout, id)?;
        let header = &loaded.level.header;
        let palette = build_level_palette(rom, layout, header.palette_selection())?;
        let atlas = load_tileset_atlas(rom, layout, header.tileset())?;
        let fg_set = map16::load_fg(rom, layout, header.tileset())?;
        let bg_set = map16::load_bg(rom, layout)?;
        Ok(Self { loaded, palette, atlas, fg_set, bg_set })
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
            draw_grid(&mut raster, &self.loaded.layer1, &self.fg_set, ctx);
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

    fn draw_layer2(&self, raster: &mut Raster, ctx: TileContext<'_>) {
        if let Some(grid) = &self.loaded.layer2 {
            draw_grid(raster, grid, &self.fg_set, ctx);
        }
        let Some(background) = &self.loaded.background else {
            return;
        };
        for y in 0..self.loaded.layer1.height() {
            for x in 0..self.loaded.layer1.width() {
                let Some(tile) =
                    background_tile(background, x, y).and_then(|n| self.bg_set.get(usize::from(n)))
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

/// Draws a grid of Map16 tiles from `set`.
pub fn draw_grid(raster: &mut Raster, grid: &TileGrid, set: &Map16Set, ctx: TileContext<'_>) {
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            let Some(tile) = grid.get(x, y).and_then(|n| set.get(usize::from(n))) else {
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

/// Tile of the two-screen background at level tile `(x, y)`: screens are
/// 16×27 row-major blocks of 0x1B0 bytes, repeating every two screens
/// horizontally and every 27 rows vertically.
#[must_use]
pub fn background_tile(background: &[u16], x: usize, y: usize) -> Option<u16> {
    let screen = (x / 16) % 2;
    background
        .get(
            screen
                .saturating_mul(0x1B0)
                .saturating_add((y % HORIZONTAL_ROWS).saturating_mul(16))
                .saturating_add(x % 16),
        )
        .copied()
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
    let load = |id: GfxId| GfxFile::load(rom, layout, id);
    let files = [load(ids[0])?, load(ids[1])?, load(ids[2])?, load(ids[3])?];
    Ok(CharAtlas::from_layer_files(&files))
}

fn to_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}
