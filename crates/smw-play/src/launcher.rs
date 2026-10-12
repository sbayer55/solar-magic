//! The launch state machine.

use smw_data::RomLayout;
use snes_core::{Console, Hit, Input};
use snes_rom::Rom;

use crate::state::{Entrance, LaunchRequest, Player};
use crate::{WRAM, game_mode, overlays};

/// Frames allowed per phase.
const BOOT_BUDGET: u32 = 900;
const LOADER_BUDGET: u32 = 240;
const LEVEL_BUDGET: u32 = 600;

/// Why a launch failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LaunchError {
    /// The game never reached the title screen or the overworld.
    #[error("the game did not reach its title screen within {0} frames")]
    NoTitle(u32),
    /// The level-entry routine does not carry the expected code.
    #[error("the level-entry routine at {0:#08X} does not match the expected code")]
    EntryFingerprint(u32),
    /// The level-entry breakpoint was never reached.
    #[error("the level-entry routine was not reached within {0} frames")]
    NoEntry(u32),
    /// The level never started.
    #[error("the level did not start within {0} frames")]
    NoLevel(u32),
    /// The CPU stopped.
    #[error("the CPU stopped: {0:?}")]
    Stopped(Hit),
}

/// Where the launcher is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Booting to the title screen (or overworld).
    Boot,
    /// Game mode set; waiting for the level-entry routine.
    WaitEntry,
    /// Level number substituted; waiting for play.
    WaitLevel,
    /// The level is running.
    Playing,
    /// Gave up.
    Failed(LaunchError),
}

/// Drives a [`Console`] into a level one frame at a time.
#[derive(Debug, Clone)]
pub struct Launcher {
    request: LaunchRequest,
    phase: Phase,
    frames_in_phase: u32,
    entry_pc: u32,
    use_breakpoint: bool,
}

impl Launcher {
    /// Resets the console, installs the overlays and the entry breakpoint.
    pub fn start(
        console: &mut Console,
        rom: &Rom,
        layout: &RomLayout,
        mut request: LaunchRequest,
    ) -> Self {
        if let Entrance::Position { screen, x_index, y_index } = request.mario.entrance {
            let raw = &mut request.level.secondary.raw;
            raw[0] = (raw[0] & 0xF0) | (y_index & 0x0F);
            raw[1] = (raw[1] & !0x07) | (x_index & 0x07);
            raw[3] = (raw[3] & !0x1F) | (screen & 0x1F);
        }
        console.reset();
        overlays::install(console, layout, &request.level);
        let hooks = &layout.launch;
        let entry_pc = hooks.level_select_pc.get();
        let fingerprint_ok =
            rom.read(hooks.level_select_pc, hooks.level_select_fingerprint.len()).ok()
                == Some(hooks.level_select_fingerprint);
        let use_breakpoint =
            fingerprint_ok && !matches!(request.mario.entrance, Entrance::SecondaryExit(_));
        if use_breakpoint {
            console.add_breakpoint(entry_pc);
        }
        let phase = if fingerprint_ok {
            Phase::Boot
        } else {
            Phase::Failed(LaunchError::EntryFingerprint(entry_pc))
        };
        Self { request, phase, frames_in_phase: 0, entry_pc, use_breakpoint }
    }

    /// Current phase.
    #[must_use]
    pub const fn phase(&self) -> Phase {
        self.phase
    }

    /// The request being launched.
    #[must_use]
    pub const fn request(&self) -> &LaunchRequest {
        &self.request
    }

    /// Runs at most one frame and advances the state machine.
    pub fn step(&mut self, console: &mut Console, layout: &RomLayout) -> Phase {
        match self.phase {
            Phase::Boot => self.boot(console, layout),
            Phase::WaitEntry => self.wait_entry(console, layout),
            Phase::WaitLevel => self.wait_level(console, layout),
            Phase::Playing | Phase::Failed(_) => {}
        }
        self.phase
    }

    /// Steps until the level plays or the launch fails.
    ///
    /// # Errors
    ///
    /// The failure, if any.
    pub fn run_to_play(
        &mut self,
        console: &mut Console,
        layout: &RomLayout,
    ) -> Result<(), LaunchError> {
        loop {
            match self.step(console, layout) {
                Phase::Playing => return Ok(()),
                Phase::Failed(e) => return Err(e),
                Phase::Boot | Phase::WaitEntry | Phase::WaitLevel => {}
            }
        }
    }

    const fn fail(&mut self, e: LaunchError) {
        self.phase = Phase::Failed(e);
    }

    const fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.frames_in_phase = 0;
    }

    fn run_frame(&mut self, console: &mut Console) -> Option<Hit> {
        self.frames_in_phase = self.frames_in_phase.saturating_add(1);
        console.run_frame(Input::default()).hit
    }

    fn boot(&mut self, console: &mut Console, layout: &RomLayout) {
        let mode = game_mode(console, layout);
        let modes = &layout.launch.modes;
        if mode == modes.title || mode == modes.overworld {
            self.poke_state(console, layout);
            console.poke(WRAM | u32::from(layout.launch.ram.game_mode), modes.fade_to_level);
            self.enter(Phase::WaitEntry);
            return;
        }
        match self.run_frame(console) {
            Some(Hit::Breakpoint(_)) => {} // the title demo entering its level: ignore
            Some(hit) => self.fail(LaunchError::Stopped(hit)),
            None if self.frames_in_phase >= BOOT_BUDGET => {
                self.fail(LaunchError::NoTitle(BOOT_BUDGET));
            }
            None => {}
        }
    }

    /// Player state, entrance selection and the "load a level now" bytes.
    fn poke_state(&self, console: &mut Console, layout: &RomLayout) {
        let ram = &layout.launch.ram;
        let mario = self.request.mario;
        let level = self.request.level.id.get();
        let p = |a: u16| WRAM | u32::from(a);
        console.poke(p(ram.powerup), mario.powerup.code());
        // `PlayerLives` holds lives − 1.
        console.poke(p(ram.lives), mario.lives.saturating_sub(1));
        console.poke(p(ram.coins), mario.coins);
        console.poke(p(ram.item_box), mario.item_box);
        console.poke(p(ram.yoshi_carried), 0);
        console.poke(p(ram.riding_yoshi), 0);
        console.poke(p(ram.current_player), if mario.player == Player::Luigi { 4 } else { 0 });
        let submap = u8::from(level >= 0x100);
        console.poke(p(ram.player_submap), submap);
        console.poke(p(ram.player_submap.wrapping_add(1)), submap);
        console.poke(p(ram.translevel), 0);
        console.poke(p(ram.midway_flag), 0);
        console.poke(p(ram.sublevel_count), 0);
        console.poke(p(ram.use_secondary_exit), 0);
        // Nonzero: the entry routine skips the overworld position lookup.
        console.poke(p(ram.overworld_override), 1);
        match mario.entrance {
            Entrance::Midway => {
                // The overworld tile's bit 6 selects the midway screen on entry.
                let tile = p(layout.launch.ram.ow_level_tile_settings);
                console.poke(tile, console.peek(tile) | 0x40);
            }
            Entrance::SecondaryExit(exit) => {
                let [lo, _] = exit.to_le_bytes();
                console.poke(p(ram.sublevel_count), 1);
                console.poke(p(ram.use_secondary_exit), 1);
                console.poke(p(ram.player_x.wrapping_add(1)), 0);
                console.poke(p(ram.exit_table_low), lo);
                console.poke(p(ram.exit_table_high), 0);
                let high = u8::from(exit >= 0x100);
                console.poke(p(ram.player_submap), high);
                console.poke(p(ram.player_submap.wrapping_add(1)), high);
            }
            Entrance::Main | Entrance::Position { .. } => {}
        }
    }

    fn wait_entry(&mut self, console: &mut Console, layout: &RomLayout) {
        if !self.use_breakpoint {
            self.enter(Phase::WaitLevel);
            return;
        }
        match self.run_frame(console) {
            Some(Hit::Breakpoint(pc)) if pc & 0x7F_FFFF == self.entry_pc & 0x7F_FFFF => {
                let ram = &layout.launch.ram;
                let level = self.request.level.id.get();
                let [lo, _] = level.to_le_bytes();
                console.poke_u16(WRAM | u32::from(ram.level_number), level);
                console.poke(WRAM | u32::from(ram.loading_level_number), lo);
                console.poke_u16(WRAM | u32::from(ram.level_number_copy), level);
                console.remove_breakpoint(self.entry_pc);
                self.enter(Phase::WaitLevel);
            }
            Some(hit) => self.fail(LaunchError::Stopped(hit)),
            None if self.frames_in_phase >= LOADER_BUDGET => {
                self.fail(LaunchError::NoEntry(LOADER_BUDGET));
            }
            None => {}
        }
    }

    fn wait_level(&mut self, console: &mut Console, layout: &RomLayout) {
        if game_mode(console, layout) == layout.launch.modes.level {
            self.enter(Phase::Playing);
            return;
        }
        match self.run_frame(console) {
            Some(hit) => self.fail(LaunchError::Stopped(hit)),
            None if self.frames_in_phase >= LEVEL_BUDGET => {
                self.fail(LaunchError::NoLevel(LEVEL_BUDGET));
            }
            None => {}
        }
    }
}
