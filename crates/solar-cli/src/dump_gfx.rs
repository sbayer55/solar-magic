//! `solar dump-gfx`: write GFX files as PNG tile sheets.

use std::fs;
use std::path::Path;

use anyhow::Context as _;
use smw_data::RomLayout;
use smw_data::gfx::{GfxFile, GfxId};
use smw_data::palette::{PaletteSelection, build_level_palette};
use smw_render::{Raster, Rgba8};
use snes_rom::Rom;

use crate::png::{blit_indexed_tile, greyscale_palette, save_png};

/// Tiles per row in the output sheet.
const SHEET_COLUMNS: u32 = 16;

/// Command options.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Options {
    /// Dump only this file; `None` means every file in the table.
    pub(crate) id: Option<u16>,
    /// Pixel scale factor.
    pub(crate) scale: u32,
    /// Palette row to colour with; `None` means greyscale.
    pub(crate) row: Option<usize>,
}

/// Dumps one file or every file in the vanilla table.
pub(crate) fn run(rom_path: &Path, out_dir: &Path, options: Options) -> anyhow::Result<()> {
    let rom = Rom::load(rom_path).with_context(|| format!("loading {}", rom_path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let colors: Option<Vec<Rgba8>> = match options.row {
        Some(row) => {
            let palette = build_level_palette(&rom, &layout, PaletteSelection::default())?;
            Some(palette.row_rgba8(row).to_vec())
        }
        None => None,
    };
    let ids: Vec<GfxId> = match options.id {
        Some(n) => vec![GfxId::new(n)],
        None => (0..u16::try_from(layout.gfx_ptrs.count).unwrap_or(0)).map(GfxId::new).collect(),
    };
    for id in ids {
        let file = GfxFile::load(&rom, &layout, id).with_context(|| format!("decoding {id}"))?;
        let out = out_dir.join(format!("{id}.png"));
        write_sheet(&file, &out, options.scale.max(1), colors.as_deref())?;
        println!("{id}: {:?}, {} tiles -> {}", file.bpp, file.tiles.len(), out.display());
    }
    Ok(())
}

/// Lays the file's tiles out 16 per row and saves them.
fn write_sheet(
    file: &GfxFile,
    out: &Path,
    scale: u32,
    colors: Option<&[Rgba8]>,
) -> anyhow::Result<()> {
    let tiles = u32::try_from(file.tiles.len()).unwrap_or(u32::MAX);
    let rows = tiles.div_ceil(SHEET_COLUMNS).max(1);
    let mut raster = Raster::new(SHEET_COLUMNS.saturating_mul(8), rows.saturating_mul(8));
    let palette: Vec<Rgba8> =
        colors.map_or_else(|| greyscale_palette(file.bpp.colors()), <[Rgba8]>::to_vec);
    for (n, tile) in file.tiles.iter().enumerate() {
        let n = u32::try_from(n).unwrap_or(u32::MAX);
        let x = (n % SHEET_COLUMNS).saturating_mul(8);
        let y = (n / SHEET_COLUMNS).saturating_mul(8);
        blit_indexed_tile(&mut raster, tile, (x, y), &palette);
    }
    save_png(&raster.scaled(scale), out)
}
