//! The open ROM and the level currently shown.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use smw_data::{LevelId, RomLayout};
use smw_render::scene::{LevelScene, RenderOptions};
use snes_rom::Rom;

/// An open ROM with one level loaded.
pub(crate) struct Document {
    /// Where the ROM came from.
    pub(crate) path: PathBuf,
    /// The ROM.
    pub(crate) rom: Rom,
    /// Table locations.
    pub(crate) layout: RomLayout,
    /// Selected level.
    pub(crate) level: LevelId,
    /// The loaded level, or the error that prevented loading it.
    pub(crate) scene: Result<LevelScene, String>,
    /// Level texture (recreated when the scene or options change).
    pub(crate) texture: Option<egui::TextureHandle>,
    /// Map16 sheet texture for the level's tileset.
    pub map16_texture: Option<egui::TextureHandle>,
    /// Options the current texture was rendered with.
    pub(crate) rendered_with: Option<RenderOptions>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("path", &self.path)
            .field("level", &self.level)
            .finish_non_exhaustive()
    }
}

impl Document {
    /// Opens a ROM and loads `level`.
    ///
    /// # Errors
    ///
    /// ROM load, detection, or layout failures.
    pub(crate) fn open(path: &Path, level: LevelId) -> anyhow::Result<Self> {
        let rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
        let layout = RomLayout::detect(&rom)?;
        let mut doc = Self {
            path: path.to_owned(),
            rom,
            layout,
            level,
            scene: Err(String::new()),
            texture: None,
            map16_texture: None,
            rendered_with: None,
        };
        doc.select(level);
        Ok(doc)
    }

    /// Loads another level.
    pub(crate) fn select(&mut self, level: LevelId) {
        self.level = level;
        self.scene = LevelScene::load(&self.rom, &self.layout, level).map_err(|e| e.to_string());
        self.texture = None;
        self.map16_texture = None;
        self.rendered_with = None;
    }

    /// Makes sure the level texture matches `options`.
    pub(crate) fn ensure_texture(&mut self, ctx: &egui::Context, options: RenderOptions) {
        if self.texture.is_some() && self.rendered_with == Some(options) {
            return;
        }
        let Ok(scene) = &self.scene else {
            return;
        };
        let raster = scene.render(options);
        self.texture = Some(upload(ctx, "level", &raster));
        self.rendered_with = Some(options);
    }

    /// Makes sure the Map16 sheet texture exists.
    pub(crate) fn ensure_map16_texture(&mut self, ctx: &egui::Context) {
        if self.map16_texture.is_some() {
            return;
        }
        let Ok(scene) = &self.scene else {
            return;
        };
        let backdrop = scene.palette.get(0, 0).unwrap_or_default().to_rgba8();
        let mut raster = smw_render::Raster::filled(16 * 16, 32 * 16, backdrop);
        let tile_ctx = smw_render::TileContext {
            atlas: &scene.atlas,
            palette: &scene.palette,
            opaque_zero: false,
        };
        for (n, tile) in scene.fg_set.tiles().iter().enumerate() {
            let n = u32::try_from(n).unwrap_or(0);
            smw_render::draw_map16_tile(
                &mut raster,
                ((n % 16).saturating_mul(16), (n / 16).saturating_mul(16)),
                tile,
                tile_ctx,
            );
        }
        self.map16_texture = Some(upload(ctx, "map16", &raster));
    }
}

/// Uploads a raster as a nearest-filtered texture.
fn upload(ctx: &egui::Context, name: &str, raster: &smw_render::Raster) -> egui::TextureHandle {
    let size = [
        usize::try_from(raster.width()).unwrap_or(0),
        usize::try_from(raster.height()).unwrap_or(0),
    ];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, &raster.to_rgba_bytes());
    ctx.load_texture(name, image, egui::TextureOptions::NEAREST)
}
