//! `solar render`: draw a level to a PNG by running the game's loader.

use std::path::Path;

use anyhow::Context as _;
use smw_data::{LevelId, RomLayout};
use smw_render::scene::{LevelScene, RenderOptions};
use snes_rom::Rom;

use crate::png::save_png;

/// Which layers to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Layers {
    /// Layer 1 only.
    One,
    /// Layer 2 only (objects or background).
    Two,
    /// Layer 2 under layer 1.
    All,
}

/// Command options.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Options {
    /// Level number.
    pub(crate) level: u16,
    /// Layers to draw.
    pub(crate) layers: Layers,
    /// Pixel scale factor.
    pub(crate) scale: u32,
    /// Draw sprite position markers.
    pub(crate) sprites: bool,
}

/// Renders the level and writes `out`.
pub(crate) fn run(rom_path: &Path, out: &Path, options: Options) -> anyhow::Result<()> {
    let rom = Rom::load(rom_path).with_context(|| format!("loading {}", rom_path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    let id = LevelId::new(options.level)
        .with_context(|| format!("level {:#X} is out of range", options.level))?;
    let scene = LevelScene::load(&rom, &layout, id)?;
    let render_options = RenderOptions {
        layer1: matches!(options.layers, Layers::One | Layers::All),
        layer2: matches!(options.layers, Layers::Two | Layers::All),
        sprites: options.sprites,
    };
    let raster = scene.render(render_options);
    save_png(&raster.scaled(options.scale.max(1)), out)?;
    let loaded = &scene.loaded;
    println!(
        "level {id}: {}x{} tiles, {} ({} instructions) -> {}",
        loaded.layer1.width(),
        loaded.layer1.height(),
        if loaded.vertical { "vertical" } else { "horizontal" },
        loaded.steps,
        out.display()
    );
    Ok(())
}
