//! PNG output and small drawing helpers for the CLI.

use std::path::Path;

use anyhow::Context as _;
use image::{ImageBuffer, Rgba};
use smw_data::gfx::Tile8Pixels;
use smw_render::{Raster, Rgba8};

/// Greyscale ramp for `colors` palette entries; index 0 is drawn as magenta
/// so transparency is visible in dumps.
pub(crate) fn greyscale_palette(colors: u8) -> Vec<Rgba8> {
    let last = u32::from(colors.saturating_sub(1)).max(1);
    (0..colors)
        .map(|i| {
            if i == 0 {
                return [0xFF, 0x00, 0xFF, 0xFF];
            }
            let level =
                u8::try_from(u32::from(i).saturating_mul(255).checked_div(last).unwrap_or(0))
                    .unwrap_or(255);
            [level, level, level, 0xFF]
        })
        .collect()
}

/// Draws an indexed 8×8 tile at `pos` using `colors` directly (no flips).
pub(crate) fn blit_indexed_tile(
    raster: &mut Raster,
    tile: &Tile8Pixels,
    pos: (u32, u32),
    colors: &[Rgba8],
) {
    for ty in 0..8u32 {
        for tx in 0..8u32 {
            let index = tile
                .get(usize::try_from(tx).unwrap_or(0), usize::try_from(ty).unwrap_or(0))
                .unwrap_or(0);
            let color = colors.get(usize::from(index)).copied().unwrap_or([0, 0, 0, 0xFF]);
            raster.put(pos.0.saturating_add(tx), pos.1.saturating_add(ty), color);
        }
    }
}

/// Writes a raster as a PNG file.
pub(crate) fn save_png(raster: &Raster, path: &Path) -> anyhow::Result<()> {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(raster.width(), raster.height(), raster.to_rgba_bytes())
            .context("raster dimensions do not match pixel buffer")?;
    img.save(path).with_context(|| format!("writing {}", path.display()))
}
