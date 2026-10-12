//! Single-bit instructions: `SET1`/`CLR1`, `BBS`/`BBC`, the carry-bit
//! operations on `mem.bit`, `NOT1`, `TSET1`/`TCLR1`.

use super::P;
use crate::{ApuBus, Spc700};

/// Splits the 16-bit `mem.bit` operand into address (13 bits) and bit.
fn mem_bit(cpu: &mut Spc700, bus: &mut dyn ApuBus) -> (u16, u8) {
    let operand = cpu.fetch16(bus);
    (operand & 0x1FFF, crate::bits::lo(operand >> 13))
}

/// `SET1 dp.b` (p = bit, set) and `CLR1 dp.b`.
pub(crate) fn set1_clr1(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Num(n) = p else { return };
    let (bit, set) = (n & 7, n & 8 != 0);
    let d = cpu.fetch(bus);
    let addr = cpu.dp(d);
    let v = bus.read(addr);
    let mask = 1u8 << bit;
    bus.write(addr, if set { v | mask } else { v & !mask });
}

/// `BBS dp.b, rel` (p = bit | 8) and `BBC dp.b, rel`.
pub(crate) fn bbs_bbc(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Num(n) = p else { return };
    let (bit, want_set) = (n & 7, n & 8 != 0);
    let d = cpu.fetch(bus);
    let v = bus.read(cpu.dp(d));
    bus.idle();
    let rel = cpu.fetch(bus);
    if (v & (1u8 << bit) != 0) == want_set {
        cpu.branch(bus, rel);
    }
}

/// `OR1`/`AND1`/`EOR1` with `C` and `mem.bit` (`p`: 0 OR, 1 OR not, 2 AND,
/// 3 AND not, 4 EOR). `AND1` finishes a cycle earlier than the others.
pub(crate) fn carry_logic(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Num(kind) = p else { return };
    let (addr, bit) = mem_bit(cpu, bus);
    let v = bus.read(addr);
    if !matches!(kind, 2 | 3) {
        bus.idle();
    }
    let b = v & (1u8 << bit) != 0;
    cpu.psw.c = match kind {
        0 => cpu.psw.c || b,
        1 => cpu.psw.c || !b,
        2 => cpu.psw.c && b,
        3 => cpu.psw.c && !b,
        _ => cpu.psw.c ^ b,
    };
}

/// `MOV1 C, mem.bit`.
pub(crate) fn mov1_c_mem(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    let (addr, bit) = mem_bit(cpu, bus);
    let v = bus.read(addr);
    cpu.psw.c = v & (1u8 << bit) != 0;
}

/// `MOV1 mem.bit, C`.
pub(crate) fn mov1_mem_c(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    let (addr, bit) = mem_bit(cpu, bus);
    let v = bus.read(addr);
    bus.idle();
    let mask = 1u8 << bit;
    bus.write(addr, if cpu.psw.c { v | mask } else { v & !mask });
}

/// `NOT1 mem.bit`.
pub(crate) fn not1(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    let (addr, bit) = mem_bit(cpu, bus);
    let v = bus.read(addr);
    bus.write(addr, v ^ (1u8 << bit));
}

/// `TSET1 !abs` (p = 1) and `TCLR1 !abs` (p = 0): flags from `A - mem`.
pub(crate) fn tset1_tclr1(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Num(set) = p else { return };
    let addr = cpu.fetch16(bus);
    let v = bus.read(addr);
    bus.read(addr);
    cpu.psw.set_nz(cpu.a.wrapping_sub(v));
    bus.write(addr, if set == 1 { v | cpu.a } else { v & !cpu.a });
}
