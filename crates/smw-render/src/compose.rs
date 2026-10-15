//! A per-pixel compositor for the editor's static picture.
//!
//! The SNES mode 1 layer order, main/sub screens and colour math as the PPU
//! applies them (`crates/snes-ppu/src/render/composite.rs`), without sprites
//! or windows. `docs/formats/level-view.md`

use smw_data::palette::{Bgr555, Palette};

use crate::Raster;
use crate::plane::{Cell, IndexPlane};

/// Which background a rank refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bg {
    /// BG1 (layer 1).
    Bg1 = 0,
    /// BG2 (layer 2).
    Bg2 = 1,
    /// BG3 (layer 3).
    Bg3 = 2,
}

impl Bg {
    const fn index(self) -> usize {
        match self {
            Self::Bg1 => 0,
            Self::Bg2 => 1,
            Self::Bg3 => 2,
        }
    }

    const fn screen_bit(self) -> u8 {
        1 << self.index()
    }
}

/// Mode 1 from top to bottom, sprite rungs omitted: (layer, priority bit).
/// BG3's priority tiles are promoted above everything when `BGMODE` bit 3
/// is set, which [`compose`] handles before this table.
pub const MODE1_RANKS: [(Bg, bool); 6] = [
    (Bg::Bg1, true),
    (Bg::Bg2, true),
    (Bg::Bg1, false),
    (Bg::Bg2, false),
    (Bg::Bg3, true),
    (Bg::Bg3, false),
];

/// `CGADSUB` bit for the backdrop.
const BACKDROP_MATH: u8 = 1 << 5;

/// The registers that decide the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ComposeRegs {
    /// `BGMODE` bit 3.
    pub bg3_priority: bool,
    /// `TM`.
    pub main_screen: u8,
    /// `TS`.
    pub sub_screen: u8,
    /// `CGADSUB`.
    pub cgadsub: u8,
    /// `CGWSEL`.
    pub cgwsel: u8,
    /// `COLDATA` as a colour.
    pub fixed_color: u16,
    /// `INIDISP` brightness.
    pub brightness: u8,
}

/// The three background planes (absent = transparent).
#[derive(Debug, Clone, Copy, Default)]
pub struct Planes<'a> {
    /// BG1, BG2, BG3.
    pub bg: [Option<&'a IndexPlane>; 3],
}

impl Planes<'_> {
    fn cell(&self, bg: Bg, x: u32, y: u32) -> Cell {
        self.bg.get(bg.index()).copied().flatten().map_or(Cell::TRANSPARENT, |p| p.get(x, y))
    }

    /// Highest-priority opaque pixel of the enabled layers.
    fn resolve(&self, screen: u8, regs: ComposeRegs, x: u32, y: u32) -> Option<(Cell, Bg)> {
        let visible = |bg: Bg, cell: Cell| screen & bg.screen_bit() != 0 && cell.is_opaque();
        if regs.bg3_priority {
            let cell = self.cell(Bg::Bg3, x, y);
            if cell.priority() && visible(Bg::Bg3, cell) {
                return Some((cell, Bg::Bg3));
            }
        }
        MODE1_RANKS.iter().find_map(|&(bg, prio)| {
            let cell = self.cell(bg, x, y);
            (cell.priority() == prio && visible(bg, cell)).then_some((cell, bg))
        })
    }
}

/// `CGWSEL` region select with no colour window active: 0 never, 1
/// outside (always, since nothing is inside), 2 inside (never), 3 always.
const fn region(sel: u8) -> bool {
    matches!(sel & 3, 1 | 3)
}

/// Colour math as the PPU does it: per channel add or subtract, optional
/// halving, clipped to 5 bits.
#[must_use]
pub fn blend(main: u16, sub: u16, subtract: bool, half: bool) -> u16 {
    let mut out = 0u16;
    for shift in [0u16, 5, 10] {
        let a = (main >> shift) & 0x1F;
        let b = (sub >> shift) & 0x1F;
        let mut c = if subtract { a.saturating_sub(b) } else { a.saturating_add(b) };
        if half {
            c >>= 1;
        }
        out |= c.min(0x1F) << shift;
    }
    out
}

/// Master brightness: each channel scaled by `(level + 1) / 16`.
#[must_use]
pub fn brightness(color: u16, level: u8) -> u16 {
    if level == 0 {
        return 0;
    }
    if level >= 15 {
        return color & 0x7FFF;
    }
    let scale = u16::from(level).wrapping_add(1);
    let mut out = 0u16;
    for shift in [0u16, 5, 10] {
        let c = (color >> shift) & 0x1F;
        out |= (c.wrapping_mul(scale) >> 4) << shift;
    }
    out
}

/// One output pixel (BGR555, before conversion).
fn composite_pixel(planes: Planes<'_>, regs: ComposeRegs, cgram: &Palette, x: u32, y: u32) -> u16 {
    let color_of = |cell: Cell| {
        let index = usize::from(cell.index());
        cgram.get(index / 16, index % 16).unwrap_or_default().raw()
    };
    let backdrop = cgram.get(0, 0).unwrap_or_default().raw();
    let subtract = regs.cgadsub & 0x80 != 0;
    let clip = region(regs.cgwsel >> 6);
    let prevent = region(regs.cgwsel >> 4);
    let (main_color, main_bit) = match planes.resolve(regs.main_screen, regs, x, y) {
        Some((cell, bg)) => (color_of(cell), bg.screen_bit()),
        None => (backdrop, BACKDROP_MATH),
    };
    let math_enabled = regs.cgadsub & main_bit != 0 && !prevent;
    let main_visible = !clip;
    let (sub_color, sub_empty) = if math_enabled {
        match planes.resolve(regs.sub_screen, regs, x, y) {
            Some((cell, _)) => (color_of(cell), false),
            None => (backdrop, true),
        }
    } else {
        (0, true)
    };
    let use_sub_io = regs.cgwsel & 2 != 0;
    let (use_sub, half) = if use_sub_io && sub_empty {
        (false, false)
    } else {
        (use_sub_io, regs.cgadsub & 0x40 != 0 && main_visible)
    };
    let shown = if main_visible { main_color } else { 0 };
    let out = if math_enabled {
        blend(shown, if use_sub { sub_color } else { regs.fixed_color }, subtract, half)
    } else {
        shown
    };
    brightness(out, regs.brightness)
}

/// Composites the planes into an RGBA raster of `size`.
#[must_use]
pub fn compose(planes: Planes<'_>, regs: ComposeRegs, cgram: &Palette, size: (u32, u32)) -> Raster {
    let (width, height) = size;
    let mut raster = Raster::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let color = composite_pixel(planes, regs, cgram, x, y);
            raster.put(x, y, Bgr555::new(color).to_rgba8());
        }
    }
    raster
}

#[cfg(test)]
mod tests {
    use smw_data::palette::{Bgr555, Palette};

    use super::{Bg, ComposeRegs, Planes, blend, brightness, compose};
    use crate::plane::{Cell, IndexPlane};

    #[test]
    fn blend_and_brightness_match_the_ppu() {
        assert_eq!(blend(0x7FFF, 0x7FFF, false, false), 0x7FFF);
        assert_eq!(blend(0x0010, 0x0010, false, true), 0x0010);
        assert_eq!(blend(0x0005, 0x0007, true, false), 0);
        assert_eq!(brightness(0x7FFF, 15), 0x7FFF);
        assert_eq!(brightness(0x7FFF, 7), 0x3DEF);
        assert_eq!(brightness(0x7FFF, 0), 0);
    }

    #[test]
    fn priority_and_backdrop_math() {
        let mut cgram = Palette::black();
        cgram.set(0, 0, Bgr555::new(0x0001)); // backdrop: a little red
        cgram.set(0, 1, Bgr555::new(0x7FFF)); // index 1: white
        cgram.set(0, 2, Bgr555::new(0x03E0)); // index 2: green
        cgram.set(0, 3, Bgr555::new(0x001F)); // index 3: red
        let mut bg1 = IndexPlane::new(4, 1);
        let mut bg2 = IndexPlane::new(4, 1);
        let mut bg3 = IndexPlane::new(4, 1);
        // x0: BG1 low over BG2 high -> BG2 wins; x1: BG1 only; x2: BG3 priority
        // promoted above BG1; x3: nothing on main, BG2 on sub added to backdrop.
        bg1.put(0, 0, Cell::opaque(1, false));
        bg2.put(0, 0, Cell::opaque(2, true));
        bg1.put(1, 0, Cell::opaque(1, true));
        bg1.put(2, 0, Cell::opaque(1, true));
        bg3.put(2, 0, Cell::opaque(3, true));
        bg2.put(3, 0, Cell::opaque(2, false));
        let planes = Planes { bg: [Some(&bg1), Some(&bg2), Some(&bg3)] };
        // Everything on the main screen, no colour math: pure layer order.
        let plain = ComposeRegs { main_screen: 0b111, brightness: 15, ..ComposeRegs::default() };
        let out = compose(planes, plain, &cgram, (4, 1));
        assert_eq!(out.get(0, 0), Some(Bgr555::new(0x03E0).to_rgba8()), "BG2 high beats BG1 low");
        assert_eq!(out.get(1, 0), Some(Bgr555::WHITE.to_rgba8()));
        assert_eq!(out.get(2, 0), Some(Bgr555::WHITE.to_rgba8()), "BG1 over BG3 without promotion");
        assert_eq!(out.get(3, 0), Some(Bgr555::new(0x03E0).to_rgba8()));
        // SMW's layout: BG2 on the sub screen only, added onto the backdrop,
        // BG3's priority tiles promoted above everything.
        let smw = ComposeRegs {
            bg3_priority: true,
            main_screen: 0b101,
            sub_screen: 0b010,
            cgadsub: 0x20,
            cgwsel: 0x02,
            fixed_color: 0,
            brightness: 15,
        };
        let smw_out = compose(planes, smw, &cgram, (4, 1));
        assert_eq!(smw_out.get(0, 0), Some(Bgr555::WHITE.to_rgba8()), "BG2 not on the main screen");
        assert_eq!(smw_out.get(2, 0), Some(Bgr555::new(0x001F).to_rgba8()), "BG3 priority on top");
        assert_eq!(
            smw_out.get(3, 0),
            Some(Bgr555::new(0x03E1).to_rgba8()),
            "backdrop + sub screen"
        );
        assert_eq!(Bg::Bg2.screen_bit(), 2);
    }
}
