//! What the player starts the level with, and where.

use smw_data::level::Level;

/// Which character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Player {
    /// Mario.
    #[default]
    Mario,
    /// Luigi.
    Luigi,
}

/// `Powerup` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Powerup {
    /// Small.
    #[default]
    Small,
    /// Super Mushroom.
    Big,
    /// Cape.
    Cape,
    /// Fire flower.
    Fire,
}

impl Powerup {
    /// The `$19` value.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Small => 0,
            Self::Big => 1,
            Self::Cape => 2,
            Self::Fire => 3,
        }
    }
}

/// Where in the level play begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Entrance {
    /// The level's main entrance.
    #[default]
    Main,
    /// The midway entrance (the level's midway screen with the main
    /// entrance's position indices).
    Midway,
    /// Through a secondary exit; the exit's own target level and position
    /// apply, so `level` only supplies the unsaved edits.
    SecondaryExit(u16),
    /// A specific screen and entrance position indices (rewrites the
    /// secondary header in the overlaid copy).
    Position {
        /// Entrance screen (0-31).
        screen: u8,
        /// Entrance X index (0-7).
        x_index: u8,
        /// Entrance Y index (0-15).
        y_index: u8,
    },
}

/// Player state poked in before the level loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarioState {
    /// Character.
    pub player: Player,
    /// Powerup.
    pub powerup: Powerup,
    /// Item box contents (`$0DC2`: 0 none, 1 mushroom, 2 flower, 3 star, 4 feather).
    pub item_box: u8,
    /// Lives.
    pub lives: u8,
    /// Coins.
    pub coins: u8,
    /// Where to start.
    pub entrance: Entrance,
}

impl Default for MarioState {
    fn default() -> Self {
        Self {
            player: Player::Mario,
            powerup: Powerup::Small,
            item_box: 0,
            lives: 5,
            coins: 0,
            entrance: Entrance::Main,
        }
    }
}

/// Everything a launch needs.
#[derive(Debug, Clone)]
pub struct LaunchRequest {
    /// The level to play, with any unsaved edits.
    pub level: Level,
    /// Player state.
    pub mario: MarioState,
}
