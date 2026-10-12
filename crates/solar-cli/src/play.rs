//! `solar play`: launch a level in the in-house emulator and write a frame.

use std::path::Path;

use anyhow::Context as _;
use smw_data::level::Level;
use smw_data::{LevelId, RomLayout};
use smw_play::{Entrance, LaunchRequest, Launcher, MarioState, Powerup};
use snes_core::{Console, Input, Region};
use snes_rom::Rom;

/// Command options.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Options {
    /// Level number.
    pub(crate) level: u16,
    /// Frames to play after the level starts.
    pub(crate) frames: u32,
    /// Start at the midway entrance.
    pub(crate) midway: bool,
    /// Enter through a secondary exit.
    pub(crate) exit: Option<u16>,
    /// Powerup code 0-3.
    pub(crate) powerup: u8,
}

/// Launches the level and optionally writes the picture.
pub(crate) fn run(rom_path: &Path, out: Option<&Path>, options: Options) -> anyhow::Result<()> {
    let rom = Rom::load(rom_path).with_context(|| format!("loading {}", rom_path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    let id = LevelId::new(options.level)
        .with_context(|| format!("level {:#X} is out of range", options.level))?;
    let level = Level::load(&rom, &layout, id)?;
    let entrance = match (options.midway, options.exit) {
        (_, Some(exit)) => Entrance::SecondaryExit(exit),
        (true, None) => Entrance::Midway,
        (false, None) => Entrance::Main,
    };
    let powerup = match options.powerup {
        1 => Powerup::Big,
        2 => Powerup::Cape,
        3 => Powerup::Fire,
        _ => Powerup::Small,
    };
    let mario = MarioState { powerup, entrance, ..MarioState::default() };
    let mut console = Console::new(&rom, Region::Ntsc);
    let mut launcher = Launcher::start(&mut console, &rom, &layout, LaunchRequest { level, mario });
    launcher.run_to_play(&mut console, &layout).context("launching the level")?;
    let launched_at = console.bus().clock.frame;
    for _ in 0..options.frames {
        console.run_frame(Input::default());
    }
    let (x, y) = smw_play::player_position(&console, &layout);
    println!(
        "level {id}: playing since frame {launched_at}; now frame {}, game mode {:02X}, player at ({x}, {y})",
        console.bus().clock.frame,
        smw_play::game_mode(&console, &layout)
    );
    if let Some(path) = out {
        crate::emu_boot::write_frame(&console, path)?;
    }
    Ok(())
}
