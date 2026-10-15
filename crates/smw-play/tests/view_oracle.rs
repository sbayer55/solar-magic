//! The pixel oracle: the editor's machine-composed picture must equal the
//! emulator's own frame (sprites hidden) over the play area. Needs `SMW_ROM`.

use std::sync::Arc;

use smw_data::level::Level;
use smw_data::{LevelId, RomLayout};
use smw_play::{BootCache, CaptureError, CaptureOptions, capture_level};
use smw_render::assets::{diff_frame, frame_window};
use smw_render::scene::{LevelScene, RenderOptions};
use snes_rom::Rom;

#[allow(clippy::expect_used, clippy::print_stderr, reason = "test harness")]
fn rom_from(var: &str) -> Option<(Rom, RomLayout)> {
    let Some(path) = std::env::var_os(var) else {
        eprintln!("{var} not set; skipping");
        return None;
    };
    let rom = Rom::load(std::path::Path::new(&path)).expect("ROM loads");
    let layout = RomLayout::detect(&rom).expect("SMW layout");
    Some((rom, layout))
}

/// Levels with distinct shapes: Yoshi's Island 2, Yoshi's Island 1, a
/// vertical level, a layer-2-object level, a layer 3 level.
const LEVELS: [u16; 5] = [0x105, 0x001, 0x0C5, 0x0E8, 0x101];

#[test]
#[allow(clippy::expect_used, clippy::print_stderr, reason = "test harness")]
fn machine_picture_matches_the_emulator_frame() {
    let Some((rom, layout)) = rom_from("SMW_ROM") else { return };
    let mut cache = BootCache::new();
    let mut failures = Vec::new();
    for number in LEVELS {
        let id = LevelId::new(number).expect("valid");
        let level = Level::load(&rom, &layout, id).expect("level loads");
        let assets =
            match capture_level(&mut cache, &rom, &layout, level, CaptureOptions::default()) {
                Ok(assets) => Arc::new(assets),
                Err(CaptureError::UnsupportedMode(_)) => continue,
                Err(err) => {
                    failures.push(format!("level {id}: {err}"));
                    continue;
                }
            };
        if assets.play.hdma_enabled != 0 {
            eprintln!("level {id}: HDMA active; skipping the pixel comparison");
            continue;
        }
        let scene =
            LevelScene::load(&rom, &layout, id).expect("scene").with_assets(Arc::clone(&assets));
        let raster = scene.render(RenderOptions { sprites: false, ..RenderOptions::default() });
        let (origin, rows) = frame_window(&assets);
        let diff = diff_frame(&raster, &assets.frame_no_obj, origin, rows, 4);
        if diff.count > 0 {
            failures
                .push(format!("level {id}: {} pixels differ, first {:?}", diff.count, diff.first));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
