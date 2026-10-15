//! Running the game's level loader to obtain the Map16 tile grid of each
//! layer. See `docs/formats/level-load-routine.md`.

use smw_data::level::{Layer2, Level};
use smw_data::{LevelId, RomLayout};
use snes_rom::{Rom, RomError, SnesAddr};
use w65c816::{Addr, Cpu, CpuError};

use crate::emu::{Overlay, SnesBus, Trace};
use crate::ownership::{AttributedBus, Ownership};

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

/// Scratch bus addresses the level's bytes are overlaid at. These banks are
/// beyond any ROM the mapper can address, so the overlay shadows nothing.
const OVERLAY_LAYER1: Addr = 0x60_8000;
const OVERLAY_LAYER2: Addr = 0x61_8000;
const OVERLAY_SPRITES: Addr = 0x62_8000;
/// Background tilemaps must sit in bank `$0C`, on the half matching their page.
const OVERLAY_BACKGROUND: [Addr; 2] = [0x0C_8000, 0x0C_E8FE];

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

/// A row-major grid of per-tile values in level tile coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid<T> {
    width: usize,
    height: usize,
    cells: Vec<T>,
}

/// Map16 tile numbers for one layer of a level.
pub type TileGrid = Grid<u16>;

impl<T: Copy> Grid<T> {
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

    /// Value at `(x, y)`.
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> Option<T> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cells.get(y.saturating_mul(self.width).saturating_add(x)).copied()
    }

    /// All cells, row-major.
    #[must_use]
    pub fn cells(&self) -> &[T] {
        &self.cells
    }
}

impl TileGrid {
    /// All tiles, row-major.
    #[must_use]
    pub fn tiles(&self) -> &[u16] {
        &self.cells
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
    /// Rows per screen of `background` (27, or 32 for Lunar Magic 16-bit tilemaps).
    pub background_rows: u8,
    /// Which object stamped each tile.
    pub ownership: Ownership,
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

/// Loads a level from the ROM and runs the game's loader on it.
///
/// # Errors
///
/// See [`LoadError`].
pub fn load_level(rom: &Rom, layout: &RomLayout, id: LevelId) -> Result<LoadedLevel, LoadError> {
    load_level_data(rom, layout, Level::load(rom, layout, id)?)
}

/// Runs the game's own loader on `level`'s current bytes.
///
/// The serialized level is overlaid on the bus at scratch addresses, so
/// edited levels render without touching the ROM, and every tile-RAM write
/// is attributed to the object whose bytes the loader last fetched.
///
/// # Errors
///
/// See [`LoadError`].
pub fn load_level_data(
    rom: &Rom,
    layout: &RomLayout,
    level: Level,
) -> Result<LoadedLevel, LoadError> {
    load_level_with(rom, layout, level, false).map(|t| t.loaded)
}

/// A level load together with what the loader touched.
#[derive(Debug)]
pub struct TracedLoad {
    /// The loaded level.
    pub loaded: LoadedLevel,
    /// Every access the loader made (empty when tracing was off).
    pub trace: Trace,
    /// WRAM after the loader ran (`$7E:0000` is index 0; empty when tracing
    /// was off).
    pub wram: Vec<u8>,
}

/// [`load_level_data`] with the bus recording every access, for reverse
/// engineering what a (patched) loader reads and writes.
///
/// # Errors
///
/// See [`LoadError`].
pub fn load_level_data_traced(
    rom: &Rom,
    layout: &RomLayout,
    level: Level,
) -> Result<TracedLoad, LoadError> {
    load_level_with(rom, layout, level, true)
}

/// Runs the loader; returns the result, the trace (when recording) and the
/// final WRAM image.
fn load_level_with(
    rom: &Rom,
    layout: &RomLayout,
    level: Level,
    record: bool,
) -> Result<TracedLoad, LoadError> {
    let vertical = level.vertical;
    let loader = &layout.loader;
    let mut inner = SnesBus::new(rom);
    if record {
        inner.enable_trace();
    }
    let layer2_addr = seed_bus(&mut inner, layout, &level);
    let mut bus = AttributedBus::new(inner, &level, OVERLAY_LAYER1, layer2_addr);
    let mut cpu = Cpu::new();
    cpu.brk_is_fatal = true;
    cpu.call_long(&mut bus, loader.entry.get(), SENTINEL, STEP_BUDGET)?;
    let trace = bus.inner_mut().take_trace().unwrap_or_default();
    let wram = if record { bus.inner().wram().to_vec() } else { Vec::new() };

    let screens = usize::from(level.header.screens());
    let mode = level.header.level_mode().get();
    let shape1 = GridShape { layer: Layer::One, mode, screens, vertical };
    let shape2 = GridShape { layer: Layer::Two, mode, screens, vertical };
    let layer1 = read_grid(layout, bus.inner(), shape1)?;
    let owners1 =
        read_cells(layout, bus.inner(), shape1, |lo, _| Some(bus.owner_at(Layer::One, lo)))?;
    let (layer2, owners2, background, background_rows) = match level.layer2 {
        Layer2::Objects { .. } => (
            Some(read_grid(layout, bus.inner(), shape2)?),
            Some(read_cells(layout, bus.inner(), shape2, |lo, _| {
                Some(bus.owner_at(Layer::Two, lo))
            })?),
            None,
            0,
        ),
        Layer2::Background { rows_per_screen, .. } => {
            (None, None, Some(read_background(layout, bus.inner())), rows_per_screen)
        }
    };
    let ownership = Ownership::new(owners1, owners2);
    let result = LoadedLevel {
        level,
        vertical,
        layer1,
        layer2,
        background,
        background_rows,
        ownership,
        steps: cpu.steps,
    };
    Ok(TracedLoad { loaded: result, trace, wram })
}

/// Overlays the level's bytes on the bus and seeds the RAM the loader
/// expects. Returns the bus address of the layer 2 data.
fn seed_bus(inner: &mut SnesBus<'_>, layout: &RomLayout, level: &Level) -> Addr {
    let loader = &layout.loader;
    let layer1_bytes = level.layer1_bytes();
    let layer2_bytes = level.layer2_bytes();
    let lm_relocated = level.lm.is_some_and(|info| info.has_tilemap());
    let vanilla_bg_page = match &level.layer2 {
        Layer2::Background { tiles, .. } => {
            tiles.first().map_or(0, |t| usize::from(t.to_le_bytes()[1] & 1))
        }
        Layer2::Objects { .. } => 0,
    };
    let layer2_addr = match &level.layer2 {
        Layer2::Objects { .. } => OVERLAY_LAYER2,
        // Lunar Magic's relocated tilemaps are read from the ROM through its
        // own flags table, so they are not overlaid.
        Layer2::Background { .. } if lm_relocated => level.addresses.layer2_raw.get(),
        Layer2::Background { .. } => {
            OVERLAY_BACKGROUND.get(vanilla_bg_page).copied().unwrap_or(OVERLAY_BACKGROUND[0])
        }
    };
    let layer2_ptr = match &level.layer2 {
        Layer2::Objects { .. } => layer2_addr,
        Layer2::Background { .. } if lm_relocated => layer2_addr,
        // The game recognises vanilla backgrounds by the `$FF` bank byte.
        Layer2::Background { .. } => 0xFF_0000 | (layer2_addr & 0xFFFF),
    };
    inner.add_overlay(Overlay { start: OVERLAY_LAYER1, bytes: layer1_bytes });
    if !lm_relocated {
        inner.add_overlay(Overlay { start: layer2_addr, bytes: layer2_bytes });
    }
    inner.add_overlay(Overlay { start: OVERLAY_SPRITES, bytes: level.sprite_bytes() });
    inner.set_wram_u24(loader.ram_layer1_ptr, OVERLAY_LAYER1);
    inner.set_wram_u24(loader.ram_layer2_ptr, layer2_ptr);
    inner.set_wram_u24(loader.ram_sprite_ptr, OVERLAY_SPRITES);
    inner.set_wram(loader.ram_game_mode, GAME_MODE_SKIP_SPRITES);
    inner.set_wram_u16(loader.ram_level_number, level.id.get());
    inner.set_wram_u16(loader.ram_level_number_copy, level.id.get());
    if let Some(lm) = &layout.lm {
        // What Lunar Magic 3.x's level init would have left in WRAM.
        for seed in lm.wram_seeds {
            for i in 0..seed.len {
                let i = u32::try_from(i).unwrap_or(0);
                let byte = inner.peek(seed.rom.get().wrapping_add(i));
                inner.set_wram(seed.ram.wrapping_add(i), byte);
            }
        }
        if let Some((addr, height)) = lm.level_height {
            inner.set_wram_u16(addr, height);
        }
    }
    layer2_addr
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
    layout: &RomLayout,
    bus: &SnesBus<'_>,
    shape: GridShape,
) -> Result<TileGrid, LoadError> {
    read_cells(layout, bus, shape, |lo, hi| {
        Some(u16::from_le_bytes([bus.wram_at(lo)?, bus.wram_at(hi)?]))
    })
}

/// Builds a grid by asking `source` for the value behind each cell's tile-RAM
/// low and high byte addresses.
fn read_cells<T: Copy + Default>(
    layout: &RomLayout,
    bus: &SnesBus<'_>,
    shape: GridShape,
    source: impl Fn(u32, u32) -> Option<T>,
) -> Result<Grid<T>, LoadError> {
    let (rows, cols) = shape.screen_dims();
    let (width, height) = shape.grid_dims();
    let mut cells = vec![T::default(); width.saturating_mul(height)];
    let low_ptrs = screen_pointers(bus, layout.loader.tile_ram_ptrs_low, shape)?;
    let high_ptrs = screen_pointers(bus, layout.loader.tile_ram_ptrs_high, shape)?;
    for (screen, (lo_base, hi_base)) in low_ptrs.iter().zip(high_ptrs.iter()).enumerate() {
        for row in 0..rows {
            for col in 0..cols {
                let (x, y, offset) = cell_position(shape.vertical, screen, row, col);
                let value = source(lo_base.wrapping_add(offset), hi_base.wrapping_add(offset))
                    .ok_or(LoadError::BadTilePointer(*lo_base, screen))?;
                if let Some(slot) = cells.get_mut(y.saturating_mul(width).saturating_add(x)) {
                    *slot = value;
                }
            }
        }
    }
    Ok(Grid { width, height, cells })
}

/// Byte offset into tile RAM (`$7E:C800` / `$7F:C800`) of a layer's cell.
///
/// For vanilla-height levels: horizontal screens are 16×27 blocks laid out
/// left to right (layer 2 objects after the 32 layer 1 screens), vertical
/// screens are 32×16 as two 16×16 halves laid out top to bottom. Vertical
/// layer 2 objects are not laid out this way.
#[must_use]
pub fn tile_ram_offset(vertical: bool, layer: Layer, x: usize, y: usize) -> Option<u32> {
    let (screen, row, col) = if vertical {
        (y / VERTICAL_ROWS, y % VERTICAL_ROWS, x)
    } else {
        (x / SCREEN_COLS, y, x % SCREEN_COLS)
    };
    let (_, _, within) = cell_position(vertical, screen, row, col);
    let screen = u32::try_from(screen).ok()?;
    let base = match (vertical, layer) {
        (false, Layer::One) => screen.checked_mul(0x1B0)?,
        (false, Layer::Two) => screen.checked_mul(0x1B0)?.checked_add(0x1B00)?,
        (true, Layer::One) => screen.checked_mul(0x200)?,
        (true, Layer::Two) => return None,
    };
    base.checked_add(within)
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

/// The per-screen 24-bit RAM pointers for `layer` in level `mode`. Modes
/// without tile RAM (boss rooms: the game's table holds `$0000`) yield no
/// pointers, so their grids come out blank.
///
/// Reads go through the bus because Lunar Magic 3.x's mode table points at
/// per-screen tables its level init builds in WRAM (`$0BF6`).
fn screen_pointers(
    bus: &SnesBus<'_>,
    table: SnesAddr,
    shape: GridShape,
) -> Result<Vec<u32>, LoadError> {
    let bank = table.bank();
    let layer_table = bus.peek_u16(offset(table, shape.layer.index().saturating_mul(2))?.get());
    let mode_entry =
        offset(SnesAddr::from_parts(bank, layer_table), u32::from(shape.mode).saturating_mul(2))?;
    let screen_table_offset = bus.peek_u16(mode_entry.get());
    if screen_table_offset == 0 {
        return Ok(Vec::new());
    }
    let screen_table = SnesAddr::from_parts(bank, screen_table_offset);
    (0..shape.screens)
        .map(|s| {
            let entry = offset(screen_table, u32::try_from(s).unwrap_or(0).saturating_mul(3))?;
            Ok(bus.peek_u24(entry.get()))
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
    fn tile_ram_offsets() {
        use super::{Layer, tile_ram_offset};
        assert_eq!(tile_ram_offset(false, Layer::One, 0, 0), Some(0));
        assert_eq!(tile_ram_offset(false, Layer::One, 35, 5), Some(2 * 0x1B0 + 0x53));
        assert_eq!(tile_ram_offset(false, Layer::Two, 16, 0), Some(0x1B00 + 0x1B0));
        assert_eq!(tile_ram_offset(true, Layer::One, 19, 18), Some(0x200 + 0x123));
        assert_eq!(tile_ram_offset(true, Layer::Two, 0, 0), None);
    }

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

    /// Loading through the overlay path gives the same grids as the ROM for a
    /// spread of levels (every 13th slot covers all level modes).
    #[test]
    fn real_rom_overlay_path_matches_rom_path() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = smw_data::RomLayout::detect(&rom).expect("vanilla");
        for id in smw_data::LevelId::all().step_by(13) {
            let level = smw_data::level::Level::load(&rom, &layout, id).expect("parses");
            let direct = super::load_level(&rom, &layout, id).expect("loads");
            let via_bytes = super::load_level_data(&rom, &layout, level).expect("loads");
            assert_eq!(direct.layer1, via_bytes.layer1, "level {id} layer 1");
            assert_eq!(direct.layer2, via_bytes.layer2, "level {id} layer 2");
            assert_eq!(direct.background, via_bytes.background, "level {id} background");
        }
    }
}
