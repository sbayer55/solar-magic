//! `solar diff-view`: the pixel oracle. Captures a level, composes the
//! editor's machine picture, and compares the window the emulator showed
//! (sprites hidden, below the status bar) pixel for pixel.

use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use smw_data::level::Level;
use smw_data::{LevelId, RomLayout};
use smw_play::{BootCache, CaptureOptions, capture_level};
use smw_render::assets::{FrameOrigin, LevelAssets, diff_frame, frame_window};
use smw_render::scene::{LevelScene, RenderOptions};
use smw_render::{Raster, Rect};
use snes_rom::Rom;

use crate::png::save_png;

/// Runs the comparison and prints the report.
pub(crate) fn run(rom_path: &Path, number: u16, out: Option<&Path>) -> anyhow::Result<()> {
    let rom = Rom::load(rom_path).with_context(|| format!("loading {}", rom_path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    let id = LevelId::new(number).with_context(|| format!("level {number:#X} is out of range"))?;
    let level = Level::load(&rom, &layout, id)?;
    let mut cache = BootCache::new();
    let assets = capture_level(&mut cache, &rom, &layout, level, CaptureOptions::default())
        .with_context(|| format!("capturing level {id}"))?;
    if assets.play.hdma_enabled != 0 {
        println!(
            "level {id}: HDMA active ({:#04X}); the comparison is not meaningful",
            assets.play.hdma_enabled
        );
    }
    let assets = Arc::new(assets);
    let scene = LevelScene::load(&rom, &layout, id)?.with_assets(Arc::clone(&assets));
    let raster = scene.render(RenderOptions { sprites: false, ..RenderOptions::default() });
    let (origin, rows) = frame_window(&assets);
    let diff = diff_frame(&raster, &assets.frame_no_obj, origin, rows.clone(), 8);
    println!(
        "level {id}: window origin ({}, {}), rows {}..{}: {} differing pixels",
        origin.x, origin.y, rows.start, rows.end, diff.count
    );
    for (x, y) in &diff.first {
        println!(
            "  frame ({x}, {y}) = level ({}, {})",
            origin.x.saturating_add(*x),
            origin.y.saturating_add(*y)
        );
    }
    if let Some(path) = out {
        let marked = mark(&raster, &assets, origin, &rows);
        save_png(&marked, path)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// The frame window of `raster` with differing pixels painted red.
fn mark(
    raster: &Raster,
    assets: &LevelAssets,
    origin: FrameOrigin,
    rows: &core::ops::Range<u32>,
) -> Raster {
    let frame = &assets.frame_no_obj;
    let mut out = Raster::new(frame.width, frame.height);
    for fy in 0..frame.height {
        for fx in 0..frame.width {
            let ours = raster
                .get(origin.x.saturating_add(fx), origin.y.saturating_add(fy))
                .unwrap_or([0, 0, 0, 0xFF]);
            out.put(fx, fy, ours);
        }
    }
    for (x, y) in diff_frame(raster, frame, origin, rows.clone(), usize::MAX).first {
        out.fill_rect(Rect { x, y, w: 1, h: 1 }, [0xFF, 0, 0, 0xFF]);
    }
    out
}
