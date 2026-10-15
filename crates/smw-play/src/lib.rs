//! Launching the game into a level: boot to the title screen, poke the
//! game mode and the player state, let the game's own level-entry routine
//! run up to the point where the level number is final, substitute ours,
//! and wait for play. Unsaved edits reach the game through bus overlays, as
//! in the viewer. `docs/formats/smw-launch-protocol.md`.

mod boot;
mod capture;
mod launcher;
mod overlays;
mod state;

pub use boot::{BootCache, boot_to_title};
pub use capture::{CaptureError, CaptureOptions, capture_level};
pub use launcher::{LaunchError, Launcher, Phase};
pub use state::{Entrance, LaunchRequest, MarioState, Player, Powerup, entrance_for_tile};

use smw_data::RomLayout;
use snes_core::Console;

/// WRAM bank the launch addresses live in.
const WRAM: u32 = 0x7E_0000;

/// The current game mode byte.
#[must_use]
pub fn game_mode(console: &Console, layout: &RomLayout) -> u8 {
    console.peek(WRAM | u32::from(layout.launch.ram.game_mode))
}

/// The player's level position (`PlayerXPosNext`, `PlayerYPosNext`).
#[must_use]
pub fn player_position(console: &Console, layout: &RomLayout) -> (u16, u16) {
    let ram = &layout.launch.ram;
    (
        console.peek_u16(WRAM | u32::from(ram.player_x)),
        console.peek_u16(WRAM | u32::from(ram.player_y)),
    )
}
