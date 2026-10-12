//! 16-bit `YA` operations, multiply and divide.

use super::P;
use crate::{ApuBus, Spc700};

fn dp_word_addrs(cpu: &mut Spc700, bus: &mut dyn ApuBus) -> (u16, u16) {
    let d = cpu.fetch(bus);
    (cpu.dp(d), cpu.dp(d.wrapping_add(1)))
}

/// `MOVW YA, dp`: read low, idle, read high.
pub(crate) fn movw_ya_dp(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    let (lo_addr, hi_addr) = dp_word_addrs(cpu, bus);
    let lo = bus.read(lo_addr);
    bus.idle();
    let hi = bus.read(hi_addr);
    let v = u16::from_le_bytes([lo, hi]);
    cpu.set_ya(v);
    cpu.psw.set_nz16(v);
}

/// `MOVW dp, YA`: dummy read of the low byte, then two writes.
pub(crate) fn movw_dp_ya(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    let (lo_addr, hi_addr) = dp_word_addrs(cpu, bus);
    bus.read(lo_addr);
    bus.write(lo_addr, cpu.a);
    bus.write(hi_addr, cpu.y);
}

/// `INCW dp` / `DECW dp`: byte by byte, each read then written.
pub(crate) fn incw_decw(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Num(dec) = p else { return };
    let (lo_addr, hi_addr) = dp_word_addrs(cpu, bus);
    let lo = bus.read(lo_addr);
    let (new_lo, carry) = if dec == 1 { lo.overflowing_sub(1) } else { lo.overflowing_add(1) };
    bus.write(lo_addr, new_lo);
    let hi = bus.read(hi_addr);
    let new_hi =
        if carry { if dec == 1 { hi.wrapping_sub(1) } else { hi.wrapping_add(1) } } else { hi };
    bus.write(hi_addr, new_hi);
    cpu.psw.set_nz16(u16::from_le_bytes([new_lo, new_hi]));
}

/// `ADDW YA, dp` / `SUBW YA, dp`.
pub(crate) fn addw_subw(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Num(sub) = p else { return };
    let (lo_addr, hi_addr) = dp_word_addrs(cpu, bus);
    let lo = bus.read(lo_addr);
    bus.idle();
    let hi = bus.read(hi_addr);
    let operand = u16::from_le_bytes([lo, hi]);
    let ya = cpu.ya();
    let b = if sub == 1 { !operand } else { operand };
    let carry_in = u32::from(sub == 1);
    let sum = u32::from(ya).wrapping_add(u32::from(b)).wrapping_add(carry_in);
    let r = crate::bits::lo16(sum);
    cpu.psw.c = sum > 0xFFFF;
    // Half carry out of bit 11 (the high byte's low nibble).
    let half = (ya & 0x0FFF).wrapping_add(b & 0x0FFF).wrapping_add(crate::bits::lo16(carry_in));
    cpu.psw.h = half > 0x0FFF;
    cpu.psw.v = (!(ya ^ b) & (ya ^ r)) & 0x8000 != 0;
    cpu.psw.set_nz16(r);
    cpu.set_ya(r);
}

/// `CMPW YA, dp`.
pub(crate) fn cmpw(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    let (lo_addr, hi_addr) = dp_word_addrs(cpu, bus);
    let lo = bus.read(lo_addr);
    let hi = bus.read(hi_addr);
    let operand = u16::from_le_bytes([lo, hi]);
    let ya = cpu.ya();
    cpu.psw.c = ya >= operand;
    cpu.psw.set_nz16(ya.wrapping_sub(operand));
}

/// `MUL YA`: `YA = Y × A`, flags from Y.
pub(crate) fn mul(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    cpu.dummy(bus);
    for _ in 0..7 {
        bus.idle();
    }
    let product = u16::from(cpu.y).wrapping_mul(u16::from(cpu.a));
    cpu.set_ya(product);
    cpu.psw.set_nz(cpu.y);
}

/// `DIV YA, X`: `A = YA / X`, `Y = YA % X`, with the hardware's behaviour
/// when the quotient does not fit (ares `algorithmDIV`).
pub(crate) fn div(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    cpu.dummy(bus);
    for _ in 0..10 {
        bus.idle();
    }
    let ya = u32::from(cpu.ya());
    let x = u32::from(cpu.x);
    cpu.psw.h = (cpu.y & 0x0F) >= (cpu.x & 0x0F);
    cpu.psw.v = cpu.y >= cpu.x;
    let (a, y) = if u32::from(cpu.y) < (x << 1) {
        (ya.checked_div(x).unwrap_or(0xFF), ya.checked_rem(x).unwrap_or(0))
    } else {
        let rest = ya.wrapping_sub(x << 9);
        let divisor = 256u32.wrapping_sub(x);
        (
            255u32.wrapping_sub(rest.checked_div(divisor).unwrap_or(0)),
            x.wrapping_add(rest.checked_rem(divisor).unwrap_or(0)),
        )
    };
    cpu.a = crate::bits::lo(crate::bits::lo16(a));
    cpu.y = crate::bits::lo(crate::bits::lo16(y));
    cpu.psw.set_nz(cpu.a);
}
