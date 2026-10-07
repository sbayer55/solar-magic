//! Running the game's level loader to obtain the Map16 tile grid of each
//! layer. See `docs/formats/level-load-routine.md`.

use smw_data::level::Level;
use smw_data::{LevelId, RomLayout};
use snes_rom::{Rom, RomError, SnesAddr};
use w65c816::{Cpu, CpuError};

use crate::emu::SnesBus;

/// Rows of tiles per screen in a horizontal level.
pub const HORIZONTAL_ROWS: usize = 27;
/// Rows of tiles per screen in a vertical level.
pub const VERTICAL_ROWS: usize = 16;
/// Tiles per screen row in a horizontal level (and per half in a vertical one).
pub const SCREEN_COLS: usize = 16;
/// Tiles per screen row in a vertical level: two 16-wide halves.
pub const VERTICAL_COLS: usize = 32;
/// RAM offset of a vertical screen's right half.
const VERTICAL_RIGHT_HALF: u32 = 0x100;

/// Instruction budget for one level load.
const STEP_BUDGET: u64 = 200_000_000;
/// Address the emulated `JSL` returns to.
const SENTINEL: u32 = 0xFF_FFFF;
/// Game mode value that makes the loader skip its sprite-system follow-up.
const GAME_MODE_SKIP_SPRITES: u8 = 0x22;

/// Which layer's tiles to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// Foreground.
    One,
    /// Background objects (only meaningful for object-based layer 2).
    Two,
}

impl Layer {
    const fn index(self) -> u32 {
        match self {
            Self::One => 0,
            Self::Two => 1,
        }
    }
}

/// Map16 tile numbers for one layer of a level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileGrid {
    width: usize,
    height: usize,
    tiles: Vec<u16>,
}

impl TileGrid {
    /// Width in tiles.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Height in tiles.
    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Tile at `(x, y)`.
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> Option<u16> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.tiles.get(y.saturating_mul(self.width).saturating_add(x)).copied()
    }

    /// All tiles, row-major.
    #[must_use]
    pub fn tiles(&self) -> &[u16] {
        &self.tiles
    }
}

/// Everything the loader produced.
#[derive(Debug)]
pub struct LoadedLevel {
    /// The parsed level data.
    pub level: Level,
    /// Whether the level is vertical.
    pub vertical: bool,
    /// Layer 1 tiles.
    pub layer1: TileGrid,
    /// Layer 2 object tiles, when layer 2 is object-based.
    pub layer2: Option<TileGrid>,
    /// Decoded background tilemap (two screens of BG Map16 numbers), when
    /// layer 2 is a background.
    pub background: Option<Vec<u16>>,
    /// Instructions the loader executed.
    pub steps: u64,
}

/// Errors from running the loader.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoadError {
    /// The level data could not be parsed.
    #[error(transparent)]
    Level(#[from] smw_data::level::LevelError),
    /// The emulated CPU stopped or ran out of budget.
    #[error("level loader did not finish: {0}")]
    Cpu(#[from] CpuError),
    /// A ROM table could not be read.
    #[error(transparent)]
    Rom(#[from] RomError),
    /// A tile RAM pointer landed outside WRAM.
    #[error("tile RAM pointer {0:#08X} for screen {1} is not WRAM")]
    BadTilePointer(u32, usize),
}

/// Loads a level by running the game's own loader in the interpreter.
///
/// # Errors
///
/// See [`LoadError`].
pub fn load_level(rom: &Rom, layout: &RomLayout, id: LevelId) -> Result<LoadedLevel, LoadError> {
    let level = Level::load(rom, layout, id)?;
    let vertical = level.header.level_mode().is_vertical(rom, layout)?;
    let mut bus = SnesBus::new(rom);
    let loader = &layout.loader;
    bus.set_wram_u24(loader.ram_layer1_ptr, level.addresses.layer1.get());
    bus.set_wram_u24(loader.ram_layer2_ptr, level.addresses.layer2_raw.get());
    bus.set_wram_u24(loader.ram_sprite_ptr, level.addresses.sprites.get());
    bus.set_wram(loader.ram_game_mode, GAME_MODE_SKIP_SPRITES);

    let mut cpu = Cpu::new();
    cpu.brk_is_fatal = true;
    cpu.call_long(&mut bus, loader.entry.get(), SENTINEL, STEP_BUDGET)?;

    let screens = usize::from(level.header.screens());
    let mode = level.header.level_mode().get();
    let layer1 =
        read_grid(rom, layout, &bus, GridShape { layer: Layer::One, mode, screens, vertical })?;
    let (layer2, background) = match level.layer2 {
        smw_data::level::Layer2::Objects { .. } => (
            Some(read_grid(
                rom,
                layout,
                &bus,
                GridShape { layer: Layer::Two, mode, screens, vertical },
            )?),
            None,
        ),
        smw_data::level::Layer2::Background { .. } => (None, Some(read_background(layout, &bus))),
    };
    Ok(LoadedLevel { level, vertical, layer1, layer2, background, steps: cpu.steps })
}

/// Geometry of the tile RAM for one layer of one level.
#[derive(Debug, Clone, Copy)]
struct GridShape {
    layer: Layer,
    mode: u8,
    screens: usize,
    vertical: bool,
}

impl GridShape {
    /// Rows and columns per screen.
    const fn screen_dims(self) -> (usize, usize) {
        if self.vertical { (VERTICAL_ROWS, VERTICAL_COLS) } else { (HORIZONTAL_ROWS, SCREEN_COLS) }
    }

    /// Width and height of the whole grid in tiles.
    const fn grid_dims(self) -> (usize, usize) {
        let (rows, cols) = self.screen_dims();
        if self.vertical {
            (cols, rows.saturating_mul(self.screens))
        } else {
            (cols.saturating_mul(self.screens), rows)
        }
    }
}

/// Reads a layer's tiles out of WRAM using the game's per-mode pointer tables.
fn read_grid(
    rom: &Rom,
    layout: &RomLayout,
    bus: &SnesBus<'_>,
    shape: GridShape,
) -> Result<TileGrid, LoadError> {
    let (rows, cols) = shape.screen_dims();
    let (width, height) = shape.grid_dims();
    let mut tiles = vec![0u16; width.saturating_mul(height)];
    let low_ptrs = screen_pointers(rom, layout.loader.tile_ram_ptrs_low, shape)?;
    let high_ptrs = screen_pointers(rom, layout.loader.tile_ram_ptrs_high, shape)?;
    for (screen, (lo_base, hi_base)) in low_ptrs.iter().zip(high_ptrs.iter()).enumerate() {
        for row in 0..rows {
            for col in 0..cols {
                let (x, y, offset) = cell_position(shape.vertical, screen, row, col);
                let lo = bus
                    .wram_at(lo_base.wrapping_add(offset))
                    .ok_or(LoadError::BadTilePointer(*lo_base, screen))?;
                let hi = bus
                    .wram_at(hi_base.wrapping_add(offset))
                    .ok_or(LoadError::BadTilePointer(*hi_base, screen))?;
                if let Some(slot) = tiles.get_mut(y.saturating_mul(width).saturating_add(x)) {
                    *slot = u16::from_le_bytes([lo, hi]);
                }
            }
        }
    }
    Ok(TileGrid { width, height, tiles })
}

/// Grid position and RAM offset (from the screen's base pointer) of one cell.
///
/// Horizontal screens are 16×27 row-major blocks laid out left to right.
/// Vertical screens are 32×16, stored as two 16×16 row-major halves (left at
/// +0, right at +0x100), laid out top to bottom.
fn cell_position(vertical: bool, screen: usize, row: usize, col: usize) -> (usize, usize, u32) {
    let within_half =
        u32::try_from(row.saturating_mul(SCREEN_COLS).saturating_add(col % SCREEN_COLS))
            .unwrap_or(0);
    if vertical {
        let half = if col >= SCREEN_COLS { VERTICAL_RIGHT_HALF } else { 0 };
        (
            col,
            screen.saturating_mul(VERTICAL_ROWS).saturating_add(row),
            half.wrapping_add(within_half),
        )
    } else {
        (screen.saturating_mul(SCREEN_COLS).saturating_add(col), row, within_half)
    }
}

/// The per-screen 24-bit RAM pointers for `layer` in level `mode`.
fn screen_pointers(rom: &Rom, table: SnesAddr, shape: GridShape) -> Result<Vec<u32>, LoadError> {
    let bank = table.bank();
    let layer_table = rom.read_u16_le(offset(table, shape.layer.index().saturating_mul(2))?)?;
    let mode_entry =
        offset(SnesAddr::from_parts(bank, layer_table), u32::from(shape.mode).saturating_mul(2))?;
    let screen_table = SnesAddr::from_parts(bank, rom.read_u16_le(mode_entry)?);
    (0..shape.screens)
        .map(|s| {
            let entry = offset(screen_table, u32::try_from(s).unwrap_or(0).saturating_mul(3))?;
            Ok(rom.read_u24_le(entry)?)
        })
        .collect()
}

/// The decoded background tilemap as BG Map16 numbers.
fn read_background(layout: &RomLayout, bus: &SnesBus<'_>) -> Vec<u16> {
    (0..smw_data::level::BG_TILEMAP_LEN)
        .map(|i| {
            let i = u32::try_from(i).unwrap_or(0);
            let lo = bus.wram_at(layout.loader.ram_bg_tilemap_low.wrapping_add(i)).unwrap_or(0);
            let hi = bus.wram_at(layout.loader.ram_bg_tilemap_high.wrapping_add(i)).unwrap_or(0);
            u16::from_le_bytes([lo, hi])
        })
        .collect()
}

fn offset(base: SnesAddr, by: u32) -> Result<SnesAddr, LoadError> {
    Ok(base.checked_add(by).ok_or(RomError::Unmapped(base))?)
}

#[cfg(test)]
mod tests {
    use super::cell_position;

    #[test]
    fn horizontal_and_vertical_cells() {
        assert_eq!(cell_position(false, 0, 0, 0), (0, 0, 0));
        assert_eq!(cell_position(false, 2, 5, 3), (35, 5, 0x53));
        assert_eq!(cell_position(true, 0, 0, 0), (0, 0, 0));
        assert_eq!(cell_position(true, 1, 2, 19), (19, 18, 0x123));
        assert_eq!(cell_position(true, 2, 0, 0), (0, 32, 0));
    }

    /// Loads Yoshi's Island 2 through the emulated loader when the ROM is available.
    #[test]
    fn real_rom_yoshis_island_2() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = smw_data::RomLayout::detect(&rom).expect("vanilla");
        let loaded =
            super::load_level(&rom, &layout, smw_data::LevelId::new(0x105).expect("valid"))
                .expect("loads");
        assert!(!loaded.vertical);
        assert_eq!(loaded.layer1.width(), 20 * 16);
        assert_eq!(loaded.layer1.height(), 27);
        // The bottom row of the first screen is ground, not blank (0x25).
        let bottom: Vec<u16> = (0..16).filter_map(|x| loaded.layer1.get(x, 26)).collect();
        assert!(bottom.iter().any(|&t| t != 0x25), "expected ground tiles, got {bottom:X?}");
        assert!(loaded.background.is_some());
        eprintln!("steps: {}", loaded.steps);
    }
}
