//! Launches real levels when `SMW_ROM` / `LM_ROM` / `LM3_ROM` are set.

use smw_data::level::{Layer2, Level};
use smw_data::{LevelId, RomLayout};
use smw_play::{
    BootCache, CaptureError, CaptureOptions, Entrance, LaunchRequest, Launcher, MarioState, Phase,
    Powerup, capture_level, game_mode, player_position,
};
use smw_render::assets::LevelAssets;
use smw_render::level::{Layer, LoadedLevel, tile_ram_offset};
use snes_core::{Console, Input, Region};
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

#[allow(clippy::expect_used, reason = "test harness")]
fn launch(rom: &Rom, layout: &RomLayout, id: u16, mario: MarioState) -> Console {
    let level = Level::load(rom, layout, LevelId::new(id).expect("valid id")).expect("level loads");
    let mut console = Console::new(rom, Region::Ntsc);
    let mut launcher =
        Launcher::start(&mut console, rom, layout, LaunchRequest { level, mario, pixel: None });
    launcher.run_to_play(&mut console, layout).expect("level launches");
    console
}

#[test]
fn level_105_plays_from_its_main_entrance() {
    let Some((rom, layout)) = rom_from("SMW_ROM") else { return };
    let mut console = launch(&rom, &layout, 0x105, MarioState::default());
    assert_eq!(game_mode(&console, &layout), 0x14);
    let (x, _) = player_position(&console, &layout);
    assert_eq!(x >> 8, 0, "main entrance is on screen 0");
    for _ in 0..60 {
        console.run_frame(Input::default());
    }
    assert_eq!(game_mode(&console, &layout), 0x14);
    assert!(console.frame().to_rgba().chunks(4).any(|p| p != [0, 0, 0, 0xFF]));
}

/// Levels worth checking against the machine: fixed anchors plus the first
/// level of each shape (vertical, layer 2 objects, background, layer 3).
#[allow(clippy::expect_used, reason = "test harness")]
fn representative_levels(rom: &Rom, layout: &RomLayout) -> Vec<LevelId> {
    let anchors = [0x105u16, 0x001, 0x0C5, 0x0E8, 0x1F4];
    let mut picked: Vec<LevelId> =
        anchors.iter().map(|&n| LevelId::new(n).expect("valid")).collect();
    type Shape = fn(&Level) -> bool;
    let predicates: [Shape; 4] = [
        |l| l.vertical,
        |l| matches!(l.layer2, Layer2::Objects { .. }),
        |l| matches!(l.layer2, Layer2::Background { .. }),
        |l| l.secondary.layer3_setting() != 0,
    ];
    for id in LevelId::all() {
        if picked.len() >= 16 {
            break;
        }
        let Ok(level) = Level::load(rom, layout, id) else { continue };
        if level.header.level_mode().loads_no_objects() {
            continue;
        }
        for pred in predicates {
            if pred(&level) && !picked.contains(&id) {
                picked.push(id);
                break;
            }
        }
    }
    picked
}

/// Cells where the machine's tile RAM differs from the viewer's grid.
fn grid_mismatches(assets: &LevelAssets, loaded: &LoadedLevel) -> usize {
    let mut mismatches = 0usize;
    let grids = [(Layer::One, Some(&loaded.layer1)), (Layer::Two, loaded.layer2.as_ref())];
    for (layer, grid) in grids {
        let Some(grid) = grid else { continue };
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                let Some(offset) = tile_ram_offset(loaded.vertical, layer, x, y) else { continue };
                if assets.tile_ram.tile(offset) != grid.get(x, y) {
                    mismatches = mismatches.saturating_add(1);
                }
            }
        }
    }
    mismatches
}

/// The game's loader, run by the emulator, must leave the same tiles in
/// tile RAM as the viewer's untimed run of the same loader. Covers the
/// representative set, or every level with `SMW_FULL_ORACLE=1`.
#[test]
#[allow(clippy::expect_used, clippy::print_stderr, reason = "test harness")]
fn tile_ram_matches_the_viewer() {
    let Some((rom, layout)) = rom_from("SMW_ROM") else { return };
    let full = std::env::var_os("SMW_FULL_ORACLE").is_some();
    let levels: Vec<LevelId> =
        if full { LevelId::all().collect() } else { representative_levels(&rom, &layout) };
    let mut cache = BootCache::new();
    let mut failures = Vec::new();
    for id in levels {
        let level = Level::load(&rom, &layout, id).expect("level loads");
        if level.header.level_mode().loads_no_objects() {
            continue;
        }
        let loaded = smw_render::level::load_level(&rom, &layout, id).expect("viewer loads");
        let assets =
            match capture_level(&mut cache, &rom, &layout, level, CaptureOptions::default()) {
                Ok(assets) => assets,
                Err(CaptureError::UnsupportedMode(_)) => continue,
                Err(err) => {
                    failures.push(format!("level {id}: {err}"));
                    continue;
                }
            };
        let mismatches = grid_mismatches(&assets, &loaded);
        if mismatches > 0 {
            failures.push(format!("level {id}: {mismatches} cells differ"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A console from the boot cache skips the boot phase entirely.
#[test]
#[allow(clippy::expect_used, reason = "test harness")]
fn booted_console_launches_without_booting_again() {
    let Some((rom, layout)) = rom_from("SMW_ROM") else { return };
    let mut cache = BootCache::new();
    let mut console = cache.booted(&rom, &layout).expect("boots");
    assert!(cache.is_booted());
    let booted_at = console.bus().clock.frame;
    let level = Level::load(&rom, &layout, LevelId::new(0x105).expect("valid id")).expect("level");
    let request = LaunchRequest { level, mario: MarioState::default(), pixel: None };
    let mut launcher = Launcher::start_booted(&mut console, &rom, &layout, request);
    assert_ne!(launcher.step(&mut console, &layout), Phase::Boot, "boot phase ends at once");
    launcher.run_to_play(&mut console, &layout).expect("launches");
    let frames = console.bus().clock.frame.saturating_sub(booted_at);
    assert!(frames < 400, "launch from the title took {frames} frames");
    assert_eq!(game_mode(&console, &layout), layout.launch.modes.level);
}

#[test]
#[allow(clippy::expect_used, reason = "test harness")]
fn midway_entrance_starts_on_the_midway_screen() {
    let Some((rom, layout)) = rom_from("SMW_ROM") else { return };
    let level = Level::load(&rom, &layout, LevelId::new(0x105).expect("valid id")).expect("level");
    let midway = u16::from(level.secondary.midway_screen());
    let mario =
        MarioState { entrance: Entrance::Midway, powerup: Powerup::Cape, ..MarioState::default() };
    let console = launch(&rom, &layout, 0x105, mario);
    let (x, _) = player_position(&console, &layout);
    assert_eq!(x >> 8, midway);
    assert_eq!(console.peek(0x7E_0019), 2, "cape powerup");
}

#[test]
fn edited_entrance_screen_is_honoured() {
    let Some((rom, layout)) = rom_from("SMW_ROM") else { return };
    let entrance = Entrance::Position { screen: 3, x_index: 0, y_index: 5 };
    let console = launch(&rom, &layout, 0x105, MarioState { entrance, ..MarioState::default() });
    let (x, _) = player_position(&console, &layout);
    assert_eq!(x >> 8, 3);
}

#[test]
fn lunar_magic_rom_launches() {
    let Some((rom, layout)) = rom_from("LM_ROM") else { return };
    let console = launch(&rom, &layout, 0x105, MarioState::default());
    assert_eq!(game_mode(&console, &layout), 0x14);
}

/// Lunar Magic 3.71 keeps vanilla's level-entry routine, so the layout
/// must hand the launcher the vanilla hooks.
#[test]
fn lunar_magic_3_rom_launches_with_vanilla_hooks() {
    let Some((rom, layout)) = rom_from("LM3_ROM") else { return };
    assert_eq!(layout.launch.level_select_pc, smw_data::layout::VANILLA.launch.level_select_pc);
    let console = launch(&rom, &layout, 0x105, MarioState::default());
    assert_eq!(game_mode(&console, &layout), 0x14);
}

/// The level's music plays: a second of sound after the launch is far from
/// silent.
#[test]
fn launched_level_plays_music() {
    let Some((rom, layout)) = rom_from("SMW_ROM") else { return };
    let mut console = launch(&rom, &layout, 0x105, MarioState::default());
    let mut discard = Vec::new();
    console.take_audio(&mut discard);
    let mut audio = Vec::new();
    for _ in 0..60 {
        console.run_frame(Input::default());
        console.take_audio(&mut audio);
    }
    assert!(audio.len() > 60_000, "{} samples", audio.len());
    let energy: u64 = audio.iter().map(|&s| u64::from(s.unsigned_abs())).sum();
    let mean = energy / u64::try_from(audio.len().max(1)).unwrap_or(1);
    assert!(mean > 100, "mean absolute sample {mean}");
}

/// Play-from-here: the entrance nearest to a tile on screen 3 lands the
/// player on screen 3.
#[test]
#[allow(clippy::expect_used, reason = "test harness")]
fn entrance_for_tile_lands_on_that_screen() {
    let Some((rom, layout)) = rom_from("SMW_ROM") else { return };
    let entrance =
        smw_play::entrance_for_tile(&rom, &layout, false, (3 * 16 + 4, 20)).expect("tables");
    assert!(matches!(entrance, Entrance::Position { screen: 3, .. }), "{entrance:?}");
    let console = launch(&rom, &layout, 0x105, MarioState { entrance, ..MarioState::default() });
    let (x, y) = player_position(&console, &layout);
    assert_eq!(x >> 8, 3);
    assert!((256..432).contains(&y), "y {y}");
}
