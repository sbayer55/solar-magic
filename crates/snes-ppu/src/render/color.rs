//! Layer priority order per mode, colour math and brightness.
//! fullsnes "Background Priority Chart", "SNES PPU Color-Math".

/// A layer source in the priority chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layer {
    /// Background `n` (0-3).
    Bg(u8),
    /// Sprites.
    Obj,
}

/// One rung of the chart: layer plus the priority value it needs.
pub(crate) type Rung = (Layer, u8);

const B1: Layer = Layer::Bg(0);
const B2: Layer = Layer::Bg(1);
const B3: Layer = Layer::Bg(2);
const B4: Layer = Layer::Bg(3);
const O: Layer = Layer::Obj;

/// Priority order from top to bottom for each BG mode; the `Option` pads
/// shorter modes. Mode 1's BG3 "a" rung (`BGMODE` bit 3) is handled by
/// the compositor, which promotes BG3 priority-1 pixels to the top.
#[rustfmt::skip]
pub(crate) const PRIORITY: [[Option<Rung>; 12]; 8] = [
    // Mode 0
    [Some((O,3)), Some((B1,1)), Some((B2,1)), Some((O,2)), Some((B1,0)), Some((B2,0)), Some((O,1)), Some((B3,1)), Some((B4,1)), Some((O,0)), Some((B3,0)), Some((B4,0))],
    // Mode 1 (BG3.1a promoted separately when BGMODE bit 3 is set)
    [Some((O,3)), Some((B1,1)), Some((B2,1)), Some((O,2)), Some((B1,0)), Some((B2,0)), Some((O,1)), Some((B3,1)), Some((O,0)), Some((B3,0)), None, None],
    // Mode 2
    [Some((O,3)), Some((B1,1)), Some((O,2)), Some((B2,1)), Some((O,1)), Some((B1,0)), Some((O,0)), Some((B2,0)), None, None, None, None],
    // Mode 3
    [Some((O,3)), Some((B1,1)), Some((O,2)), Some((B2,1)), Some((O,1)), Some((B1,0)), Some((O,0)), Some((B2,0)), None, None, None, None],
    // Mode 4
    [Some((O,3)), Some((B1,1)), Some((O,2)), Some((B2,1)), Some((O,1)), Some((B1,0)), Some((O,0)), Some((B2,0)), None, None, None, None],
    // Mode 5
    [Some((O,3)), Some((B1,1)), Some((O,2)), Some((B2,1)), Some((O,1)), Some((B1,0)), Some((O,0)), Some((B2,0)), None, None, None, None],
    // Mode 6
    [Some((O,3)), Some((B1,1)), Some((O,2)), Some((O,1)), Some((B1,0)), Some((O,0)), None, None, None, None, None, None],
    // Mode 7 (BG2 = EXTBG with its per-pixel priority)
    [Some((O,3)), Some((O,2)), Some((B2,1)), Some((O,1)), Some((B1,0)), Some((O,0)), Some((B2,0)), None, None, None, None, None],
];

/// Bits per pixel of each BG in each mode (0 = layer absent).
pub(crate) const BPP: [[u8; 4]; 8] = [
    [2, 2, 2, 2],
    [4, 4, 2, 0],
    [4, 4, 0, 0],
    [8, 4, 0, 0],
    [8, 2, 0, 0],
    [4, 2, 0, 0],
    [4, 0, 0, 0],
    [8, 7, 0, 0],
];

/// Channel shifts of a BGR555 colour.
const CHANNELS: [u8; 3] = [0, 5, 10];

/// Adds or subtracts two BGR555 colours channel by channel.
pub(crate) fn blend(main: u16, sub: u16, subtract: bool, half: bool) -> u16 {
    let mut out = 0u16;
    for shift in CHANNELS {
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
pub(crate) fn brightness(color: u16, level: u8) -> u16 {
    if level == 0 {
        return 0;
    }
    if level >= 15 {
        return color & 0x7FFF;
    }
    let scale = u16::from(level).wrapping_add(1);
    let mut out = 0u16;
    for shift in CHANNELS {
        let c = (color >> shift) & 0x1F;
        out |= (c.wrapping_mul(scale) >> 4) << shift;
    }
    out
}

/// Direct colour: `bbgggrrr` palette index plus the tilemap palette bits
/// as the low bits of each channel (`B = bb0 p, G = ggg p, R = rrr p`).
pub(crate) fn direct_color(index: u8, palette: u8) -> u16 {
    let r = ((index & 0x07) << 2) | ((palette & 1) << 1);
    let g = ((index & 0x38) >> 1) | (palette & 2);
    let b = ((index & 0xC0) >> 3) | (palette & 4);
    u16::from(r) | (u16::from(g) << 5) | (u16::from(b) << 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_saturates_and_halves() {
        assert_eq!(blend(0x7FFF, 0x7FFF, false, false), 0x7FFF);
        assert_eq!(blend(0x0010, 0x0010, false, true), 0x0010);
        assert_eq!(blend(0x0005, 0x0007, true, false), 0);
    }

    #[test]
    fn brightness_scales() {
        assert_eq!(brightness(0x7FFF, 15), 0x7FFF);
        assert_eq!(brightness(0x7FFF, 7), 0x3DEF);
        assert_eq!(brightness(0x7FFF, 0), 0);
    }
}
