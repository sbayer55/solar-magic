//! Booting once and reusing the booted machine.
//!
//! Booting to the title screen costs hundreds of frames; `Console` is
//! `Clone`, so a [`BootCache`] boots a ROM once and hands out copies.

use smw_data::RomLayout;
use snes_core::{Console, Hit, Input, Region};
use snes_rom::Rom;

use crate::game_mode;
use crate::launcher::{BOOT_BUDGET, LaunchError};

/// Boots a fresh console until the game waits on its title screen (or the
/// overworld).
///
/// # Errors
///
/// [`LaunchError::NoTitle`] after the boot budget (900 frames),
/// [`LaunchError::Stopped`] if the CPU stops.
pub fn boot_to_title(rom: &Rom, layout: &RomLayout) -> Result<Console, LaunchError> {
    let mut console = Console::new(rom, Region::Ntsc);
    let modes = &layout.launch.modes;
    for _ in 0..BOOT_BUDGET {
        let mode = game_mode(&console, layout);
        if mode == modes.title || mode == modes.overworld {
            return Ok(console);
        }
        match console.run_frame(Input::default()).hit {
            None | Some(Hit::Breakpoint(_)) => {}
            Some(hit) => return Err(LaunchError::Stopped(hit)),
        }
    }
    Err(LaunchError::NoTitle(BOOT_BUDGET))
}

/// A booted console kept for cloning.
#[derive(Debug, Clone, Default)]
pub struct BootCache {
    console: Option<Console>,
}

impl BootCache {
    /// An empty cache.
    #[must_use]
    pub const fn new() -> Self {
        Self { console: None }
    }

    /// A copy of the booted console, booting on first use.
    ///
    /// # Errors
    ///
    /// See [`boot_to_title`].
    pub fn booted(&mut self, rom: &Rom, layout: &RomLayout) -> Result<Console, LaunchError> {
        if self.console.is_none() {
            self.console = Some(boot_to_title(rom, layout)?);
        }
        self.console.clone().ok_or(LaunchError::NoTitle(0))
    }

    /// Forgets the booted console (call when the ROM changes).
    pub fn invalidate(&mut self) {
        self.console = None;
    }

    /// Whether a booted console is cached.
    #[must_use]
    pub const fn is_booted(&self) -> bool {
        self.console.is_some()
    }
}
