//! Mode 7: a 128×128 tilemap of 8-bit characters sampled through an affine
//! transform. ares `Background::renderMode7` / fullsnes "Rotation/Scaling".

use super::pixel::Px;
use crate::regs::Mode7Regs;
use crate::vram::Vram;

/// 13-bit sign-extended clamp used for the scroll-minus-centre terms.
const fn clip(v: i32) -> i32 {
    if v & 0x2000 != 0 { v | !0x3FF } else { v & 0x3FF }
}

/// Renders one mode 7 line. With `extbg`, pixels carry bit 7 as priority
/// and the colour from the low 7 bits (BG2); otherwise all 8 bits are the
/// colour (BG1, priority 0).
#[allow(clippy::many_single_char_names, reason = "the matrix registers are named A-D")]
pub(crate) fn render_line(vram: &Vram, m7: &Mode7Regs, v: u16, extbg: bool, out: &mut [Px; 512]) {
    let y = i32::from(if m7.flip_v { 255u16.wrapping_sub(v & 0xFF) } else { v & 0xFF });
    let (a, b, c, d) = (i32::from(m7.a), i32::from(m7.b), i32::from(m7.c), i32::from(m7.d));
    let (cx, cy) = (i32::from(m7.x), i32::from(m7.y));
    let hofs = clip(i32::from(m7.hofs).wrapping_sub(cx));
    let vofs = clip(i32::from(m7.vofs).wrapping_sub(cy));
    let origin_x = (a.wrapping_mul(hofs) & !63)
        .wrapping_add(b.wrapping_mul(y) & !63)
        .wrapping_add(b.wrapping_mul(vofs) & !63)
        .wrapping_add(cx << 8);
    let origin_y = (c.wrapping_mul(hofs) & !63)
        .wrapping_add(d.wrapping_mul(y) & !63)
        .wrapping_add(d.wrapping_mul(vofs) & !63)
        .wrapping_add(cy << 8);
    for (sx, slot) in out.iter_mut().take(256).enumerate() {
        let x = i32::try_from(if m7.flip_h { 255usize.wrapping_sub(sx) } else { sx }).unwrap_or(0);
        let px = origin_x.wrapping_add(a.wrapping_mul(x)) >> 8;
        let py = origin_y.wrapping_add(c.wrapping_mul(x)) >> 8;
        let outside = !(0..=1023).contains(&px) || !(0..=1023).contains(&py);
        let (tx, ty) = (px & 1023, py & 1023);
        let tile = match (outside, m7.repeat) {
            (true, 2) => {
                *slot = Px::TRANSPARENT;
                continue;
            }
            (true, 3) => 0,
            _ => {
                let map_addr = u16::try_from(((ty >> 3) << 7) | (tx >> 3)).unwrap_or(0);
                let [lo, _] = vram.word(map_addr & 0x7FFF).to_le_bytes();
                u16::from(lo)
            }
        };
        let char_addr = (tile << 6) | u16::try_from(((ty & 7) << 3) | (tx & 7)).unwrap_or(0);
        let [_, color] = vram.word(char_addr & 0x7FFF).to_le_bytes();
        *slot = if extbg {
            let index = color & 0x7F;
            if index == 0 { Px::TRANSPARENT } else { Px::new(index, color >> 7, true, 0) }
        } else if color == 0 {
            Px::TRANSPARENT
        } else {
            Px::new(color, 0, true, 0)
        };
    }
}
