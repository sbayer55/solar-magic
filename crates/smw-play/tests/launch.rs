//! Launches real levels when `SMW_ROM` / `LM_ROM` are set.

use smw_data::level::Level;
use smw_data::{LevelId, RomLayout};
use smw_play::{
    Entrance, LaunchRequest, Launcher, MarioState, Powerup, game_mode, player_position,
};
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
    let mut launcher = Launcher::start(&mut console, rom, layout, LaunchRequest { level, mario });
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

/// The game's loader, run by the emulator, must leave the same layer 1
/// tiles in tile RAM as the viewer's untimed run of the same loader.
#[test]
#[allow(clippy::expect_used, clippy::arithmetic_side_effects, reason = "test harness")]
fn tile_ram_matches_the_viewer() {
    let Some((rom, layout)) = rom_from("SMW_ROM") else { return };
    let id = LevelId::new(0x105).expect("valid id");
    let loaded = smw_render::level::load_level(&rom, &layout, id).expect("viewer loads");
    assert!(!loaded.vertical);
    let console = launch(&rom, &layout, 0x105, MarioState::default());
    let grid = &loaded.layer1;
    let mut mismatches = 0;
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            let (screen, col) = (x / 16, x % 16);
            let offset = u32::try_from(screen * 0x1B0 + y * 16 + col).expect("fits");
            let lo = console.peek(0x7E_C800 + offset);
            let hi = console.peek(0x7F_C800 + offset);
            if grid.get(x, y) != Some(u16::from_le_bytes([lo, hi])) {
                mismatches += 1;
            }
        }
    }
    assert_eq!(mismatches, 0, "layer 1 tiles differ from the viewer in {mismatches} cells");
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
