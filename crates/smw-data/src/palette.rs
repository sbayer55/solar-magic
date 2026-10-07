//! Level palettes.
//!
//! SMW keeps a 256-colour CGRAM image at `$7E:0703` (16 rows × 16 colours)
//! and fills it from several small ROM tables, selecting the background,
//! foreground and sprite sets from the level header. This module reproduces
//! that assembly. See `docs/formats/palettes.md`.

use core::fmt;

use snes_rom::{Rom, RomError, SnesAddr};

use crate::RomLayout;

/// A 15-bit SNES colour: `0bbbbbgggggrrrrr`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Bgr555(u16);

impl Bgr555 {
    /// Pure black.
    pub const BLACK: Self = Self(0);
    /// Pure white.
    pub const WHITE: Self = Self(0x7FFF);

    /// Wraps a raw word; bit 15 is ignored.
    #[must_use]
    pub const fn new(raw: u16) -> Self {
        Self(raw & 0x7FFF)
    }

    /// Builds a colour from 5-bit components (values above 31 are clamped).
    #[must_use]
    #[allow(
        clippy::as_conversions,
        reason = "lossless widening of clamped 5-bit values in a const fn"
    )]
    pub const fn from_rgb5(r: u8, g: u8, b: u8) -> Self {
        Self(((clamp5(b) as u16) << 10) | ((clamp5(g) as u16) << 5) | (clamp5(r) as u16))
    }

    /// The raw 15-bit word.
    #[must_use]
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// 5-bit red component.
    #[must_use]
    #[allow(clippy::as_conversions, reason = "value is masked to 5 bits before the cast")]
    pub const fn r5(self) -> u8 {
        (self.0 & 0x1F) as u8
    }

    /// 5-bit green component.
    #[must_use]
    #[allow(clippy::as_conversions, reason = "value is masked to 5 bits before the cast")]
    pub const fn g5(self) -> u8 {
        ((self.0 >> 5) & 0x1F) as u8
    }

    /// 5-bit blue component.
    #[must_use]
    #[allow(clippy::as_conversions, reason = "value is masked to 5 bits before the cast")]
    pub const fn b5(self) -> u8 {
        ((self.0 >> 10) & 0x1F) as u8
    }

    /// Expands to 8-bit RGBA with full alpha, replicating the top bits into
    /// the low bits so that 31 maps to 255.
    #[must_use]
    pub const fn to_rgba8(self) -> [u8; 4] {
        [expand5(self.r5()), expand5(self.g5()), expand5(self.b5()), 0xFF]
    }
}

/// Scales a 5-bit value to 8 bits.
const fn expand5(v: u8) -> u8 {
    (v << 3) | (v >> 2)
}

/// Clamps to the 5-bit range.
const fn clamp5(v: u8) -> u8 {
    if v > 31 { 31 } else { v }
}

impl fmt::Debug for Bgr555 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bgr555(${:04X})", self.0)
    }
}

/// A full 16×16 palette.
#[derive(Clone, PartialEq, Eq)]
pub struct Palette {
    colors: [Bgr555; 256],
}

impl Palette {
    /// Number of rows.
    pub const ROWS: usize = 16;
    /// Colours per row.
    pub const COLS: usize = 16;

    /// An all-black palette.
    #[must_use]
    pub const fn black() -> Self {
        Self { colors: [Bgr555::BLACK; 256] }
    }

    /// Colour at `(row, col)`, or `None` out of range.
    #[must_use]
    pub fn get(&self, row: usize, col: usize) -> Option<Bgr555> {
        if row >= Self::ROWS || col >= Self::COLS {
            return None;
        }
        self.colors.get(row.saturating_mul(Self::COLS).saturating_add(col)).copied()
    }

    /// Sets the colour at `(row, col)`; out-of-range writes are ignored.
    pub fn set(&mut self, row: usize, col: usize, color: Bgr555) {
        if row >= Self::ROWS || col >= Self::COLS {
            return;
        }
        if let Some(slot) = self.colors.get_mut(row.saturating_mul(Self::COLS).saturating_add(col))
        {
            *slot = color;
        }
    }

    /// One row as a 16-entry RGBA8 slice, handy for rendering.
    #[must_use]
    pub fn row_rgba8(&self, row: usize) -> [[u8; 4]; 16] {
        let mut out = [[0u8; 4]; 16];
        for (col, slot) in out.iter_mut().enumerate() {
            *slot = self.get(row, col).unwrap_or_default().to_rgba8();
        }
        out
    }

    /// All 256 colours, row-major.
    #[must_use]
    pub const fn colors(&self) -> &[Bgr555; 256] {
        &self.colors
    }
}

impl fmt::Debug for Palette {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Palette").field("rows", &Self::ROWS).finish_non_exhaustive()
    }
}

/// Which player palette to load into row 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerPalette {
    /// Small/big Mario.
    #[default]
    Mario,
    /// Small/big Luigi.
    Luigi,
    /// Fire Mario.
    FireMario,
    /// Fire Luigi.
    FireLuigi,
}

impl PlayerPalette {
    const fn index(self) -> u32 {
        match self {
            Self::Mario => 0,
            Self::Luigi => 1,
            Self::FireMario => 2,
            Self::FireLuigi => 3,
        }
    }
}

/// The header-driven choices that select a level's palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PaletteSelection {
    /// Background palette set, 0–7.
    pub background: u8,
    /// Foreground palette set, 0–7.
    pub foreground: u8,
    /// Sprite palette set, 0–7.
    pub sprite: u8,
    /// Back area colour, 0–7.
    pub back_area_color: u8,
    /// Player palette for row 8.
    pub player: PlayerPalette,
}

/// Bytes between one palette set and the next in the FG/BG/sprite tables.
const SET_STRIDE: u32 = 24;

/// Builds a level palette exactly as the game's `LoadPalette` routine does.
///
/// Colour 0 of row 0 receives the back area colour; colour 0 of every other
/// row is left black (it is transparent in hardware anyway).
///
/// # Errors
///
/// ROM read failures.
pub fn build_level_palette(
    rom: &Rom,
    layout: &RomLayout,
    sel: PaletteSelection,
) -> Result<Palette, RomError> {
    let t = &layout.palettes;
    let mut pal = Palette::black();
    let set = |base: SnesAddr, n: u8| offset(base, u32::from(n & 0x07).saturating_mul(SET_STRIDE));

    // Colour 1 of rows 0-7 is $7FDD, of rows 8-15 is $7FFF.
    for row in 0..8 {
        pal.set(row, 1, Bgr555::new(0x7FDD));
        pal.set(row.saturating_add(8), 1, Bgr555::WHITE);
    }
    load_block(rom, &mut pal, t.status_bar, Block { row: 0, col: 8, cols: 8, rows: 2 })?;
    load_block(rom, &mut pal, t.standard, Block { row: 4, col: 2, cols: 6, rows: 10 })?;
    load_block(
        rom,
        &mut pal,
        set(t.foreground, sel.foreground)?,
        Block { row: 2, col: 2, cols: 6, rows: 2 },
    )?;
    load_block(
        rom,
        &mut pal,
        set(t.sprite, sel.sprite)?,
        Block { row: 14, col: 2, cols: 6, rows: 2 },
    )?;
    load_block(
        rom,
        &mut pal,
        set(t.background, sel.background)?,
        Block { row: 0, col: 2, cols: 6, rows: 2 },
    )?;
    load_block(rom, &mut pal, t.berry, Block { row: 2, col: 9, cols: 7, rows: 3 })?;
    load_block(rom, &mut pal, t.berry, Block { row: 9, col: 9, cols: 7, rows: 3 })?;
    let player = offset(t.player, sel.player.index().saturating_mul(20))?;
    load_block(rom, &mut pal, player, Block { row: 8, col: 6, cols: 10, rows: 1 })?;

    let back = offset(t.back_area_colors, u32::from(sel.back_area_color & 0x07).saturating_mul(2))?;
    pal.set(0, 0, Bgr555::new(rom.read_u16_le(back)?));
    Ok(pal)
}

/// A rectangular region of the palette filled from consecutive ROM words.
#[derive(Clone, Copy)]
struct Block {
    row: usize,
    col: usize,
    cols: usize,
    rows: usize,
}

/// Copies `rows × cols` consecutive colours from `src` into the block.
fn load_block(rom: &Rom, pal: &mut Palette, src: SnesAddr, block: Block) -> Result<(), RomError> {
    let mut addr = src;
    for r in 0..block.rows {
        for c in 0..block.cols {
            let color = Bgr555::new(rom.read_u16_le(addr)?);
            pal.set(block.row.saturating_add(r), block.col.saturating_add(c), color);
            addr = offset(addr, 2)?;
        }
    }
    Ok(())
}

fn offset(base: SnesAddr, by: u32) -> Result<SnesAddr, RomError> {
    base.checked_add(by).ok_or(RomError::Unmapped(base))
}

#[cfg(test)]
mod tests {
    use super::{Bgr555, Palette, PaletteSelection, build_level_palette};

    #[test]
    fn color_components_and_expansion() {
        let c = Bgr555::from_rgb5(31, 0, 16);
        assert_eq!(c.r5(), 31);
        assert_eq!(c.g5(), 0);
        assert_eq!(c.b5(), 16);
        assert_eq!(c.to_rgba8(), [255, 0, 132, 255]);
        assert_eq!(Bgr555::new(0xFFFF).raw(), 0x7FFF);
        assert_eq!(Bgr555::WHITE.to_rgba8(), [255, 255, 255, 255]);
    }

    #[test]
    fn palette_indexing() {
        let mut p = Palette::black();
        p.set(3, 5, Bgr555::WHITE);
        assert_eq!(p.get(3, 5), Some(Bgr555::WHITE));
        assert_eq!(p.get(3, 4), Some(Bgr555::BLACK));
        assert_eq!(p.get(16, 0), None);
        assert_eq!(p.row_rgba8(3)[5], [255, 255, 255, 255]);
    }

    /// Checks the structural facts of `LoadPalette` against the real ROM.
    #[test]
    fn real_rom_level_palette() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = crate::RomLayout::detect(&rom).expect("vanilla");
        let pal = build_level_palette(&rom, &layout, PaletteSelection::default()).expect("reads");
        // Fixed colour-1 values.
        assert_eq!(pal.get(0, 1), Some(Bgr555::new(0x7FDD)));
        assert_eq!(pal.get(8, 1), Some(Bgr555::WHITE));
        // Colour 0 of rows other than 0 stays black; row 0 gets the back area colour.
        assert_eq!(pal.get(5, 0), Some(Bgr555::BLACK));
        // Every loaded block should contain at least one non-black colour.
        for (row, col) in [(0, 2), (2, 2), (4, 2), (8, 6), (14, 2), (0, 8), (2, 9), (9, 9)] {
            let any = (col..16).any(|c| pal.get(row, c).is_some_and(|k| k != Bgr555::BLACK));
            assert!(any, "row {row} from col {col} is all black");
        }
        // Different FG set selections must produce different FG rows.
        let other = build_level_palette(
            &rom,
            &layout,
            PaletteSelection { foreground: 3, ..PaletteSelection::default() },
        )
        .expect("reads");
        assert_ne!(pal.row_rgba8(2), other.row_rgba8(2));
        assert_eq!(pal.row_rgba8(4), other.row_rgba8(4));
    }
}
