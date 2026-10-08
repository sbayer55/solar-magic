//! `solar dump-map16`: render a Map16 set as a tile sheet.

use std::path::Path;

use anyhow::{Context as _, bail};
use smw_data::RomLayout;
use smw_data::map16::{self, Map16Set, Map16Tile};
use smw_data::palette::{PaletteSelection, build_level_palette};
use smw_render::map16_ext::FgPageResolver;
use smw_render::scene::load_tileset_atlas;
use smw_render::{Raster, TileContext, draw_map16_tile};
use snes_rom::Rom;

use crate::png::save_png;

/// Tiles per row in the sheet (one Map16 page row).
const SHEET_COLUMNS: u32 = 16;

/// Command options.
#[derive(Debug, Clone)]
pub(crate) struct Options {
    /// FG/BG tileset number.
    pub(crate) tileset: u8,
    /// Render the background set instead of the foreground pages.
    pub(crate) bg: bool,
    /// Which Lunar Magic background page set (16 pages) to render.
    pub(crate) bg_set: u8,
    /// Render one foreground page 2+ (Lunar Magic) instead of pages 0-1.
    pub(crate) page: Option<u8>,
    /// `fg,bg,sprite,back` palette set selections.
    pub(crate) palettes: String,
    /// Pixel scale factor.
    pub(crate) scale: u32,
}

/// Renders the requested set to `out`.
pub(crate) fn run(rom_path: &Path, out: &Path, options: &Options) -> anyhow::Result<()> {
    let rom = Rom::load(rom_path).with_context(|| format!("loading {}", rom_path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    let selection = parse_selection(&options.palettes)?;
    let palette = build_level_palette(&rom, &layout, selection)?;
    let atlas = load_tileset_atlas(&rom, &layout, options.tileset)?;
    let set: Map16Set = if options.bg {
        map16::load_bg_set(&rom, &layout, options.bg_set)?
    } else if let Some(page) = options.page {
        let mut resolver = FgPageResolver::new(&rom, &layout)
            .context("--page needs a Lunar Magic ROM (pages 0-1 are the default)")?;
        let tiles: Vec<Map16Tile> =
            resolver.page(page)?.into_iter().map(Option::unwrap_or_default).collect();
        Map16Set::from_tiles(tiles)
    } else {
        map16::load_fg(&rom, &layout, options.tileset)?
    };

    let rows = u32::try_from(set.len()).unwrap_or(0).div_ceil(SHEET_COLUMNS);
    let backdrop = palette.get(0, 0).unwrap_or_default().to_rgba8();
    let mut raster =
        Raster::filled(SHEET_COLUMNS.saturating_mul(16), rows.saturating_mul(16), backdrop);
    let ctx = TileContext { atlas: &atlas, palette: &palette, opaque_zero: false };
    for (n, tile) in set.tiles().iter().enumerate() {
        let n = u32::try_from(n).unwrap_or(u32::MAX);
        let pos = ((n % SHEET_COLUMNS).saturating_mul(16), (n / SHEET_COLUMNS).saturating_mul(16));
        draw_map16_tile(&mut raster, pos, tile, ctx);
    }
    save_png(&raster.scaled(options.scale.max(1)), out)?;
    println!(
        "{} set, tileset {}, {} tiles -> {}",
        if options.bg { "background" } else { "foreground" },
        options.tileset,
        set.len(),
        out.display()
    );
    Ok(())
}

/// Parses `fg,bg,sprite,back`.
fn parse_selection(text: &str) -> anyhow::Result<PaletteSelection> {
    let parts: Vec<u8> = text
        .split(',')
        .map(|p| p.trim().parse::<u8>().with_context(|| format!("bad palette number {p:?}")))
        .collect::<Result<_, _>>()?;
    let [foreground, background, sprite, back_area_color] = parts.as_slice() else {
        bail!("--palettes wants four comma-separated numbers, got {text:?}");
    };
    Ok(PaletteSelection {
        foreground: *foreground,
        background: *background,
        sprite: *sprite,
        back_area_color: *back_area_color,
        ..PaletteSelection::default()
    })
}
