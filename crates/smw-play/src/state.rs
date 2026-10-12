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
    /// Exact player position (level pixels) set once the level has loaded;
    /// the entrance still chooses the screen and camera.
    pub pixel: Option<(u16, u16)>,
}

/// The entrance that puts the player nearest to level tile (`x`, `y`).
///
/// The screen comes from the tile; the X/Y indices are the entries of the
/// entrance position tables (`EntranceTables`, `docs/formats/level-header.md`)
/// whose pixel values are closest.
///
/// # Errors
///
/// ROM read failures.
pub fn entrance_for_tile(
    rom: &snes_rom::Rom,
    layout: &smw_data::RomLayout,
    vertical: bool,
    tile: (u16, u16),
) -> Result<Entrance, snes_rom::RomError> {
    let (x, y) = tile;
    let px = u32::from(x).wrapping_mul(16);
    let py = u32::from(y).wrapping_mul(16);
    let (screen, within_x, within_y) = if vertical {
        (y / 16, px, py.checked_rem(256).unwrap_or(0))
    } else {
        (x / 16, px.checked_rem(256).unwrap_or(0), py)
    };
    let tables = &layout.entrance;
    let nearest = |low: snes_rom::SnesAddr, high: snes_rom::SnesAddr, count: u32, target: u32| {
        let mut best = (0u8, u32::MAX);
        for i in 0..count {
            let lo = rom.read_u8(low.checked_add(i).ok_or(snes_rom::RomError::Unmapped(low))?)?;
            let hi = rom.read_u8(high.checked_add(i).ok_or(snes_rom::RomError::Unmapped(high))?)?;
            let value = u32::from(u16::from_le_bytes([lo, hi]));
            let distance = value.abs_diff(target);
            if distance < best.1 {
                best = (u8::try_from(i).unwrap_or(0), distance);
            }
        }
        Ok::<u8, snes_rom::RomError>(best.0)
    };
    let x_index = nearest(tables.x_low, tables.x_high, 8, within_x)?;
    let y_index = nearest(tables.y_low, tables.y_high, 16, within_y)?;
    Ok(Entrance::Position { screen: u8::try_from(screen).unwrap_or(0) & 0x1F, x_index, y_index })
}
