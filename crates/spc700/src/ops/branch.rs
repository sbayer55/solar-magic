//! Branches, jumps, calls, returns, `BRK`.

use super::{Cond, P};
use crate::addressing::Mode;
use crate::{ApuBus, Spc700};

impl Spc700 {
    const fn condition(&self, c: Cond) -> bool {
        match c {
            Cond::Pl => !self.psw.n,
            Cond::Mi => self.psw.n,
            Cond::Vc => !self.psw.v,
            Cond::Vs => self.psw.v,
            Cond::Cc => !self.psw.c,
            Cond::Cs => self.psw.c,
            Cond::Ne => !self.psw.z,
            Cond::Eq => self.psw.z,
            Cond::Always => true,
        }
    }
}

/// Conditional relative branches and `BRA`.
pub(crate) fn branch_cond(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Cond(c) = p else { return };
    let rel = cpu.fetch(bus);
    if cpu.condition(c) {
        cpu.branch(bus, rel);
    }
}

/// `CBNE dp, rel` / `CBNE dp+X, rel`: branch when A differs from memory.
pub(crate) fn cbne(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Load(_, mode) = p else { return };
    let addr = cpu.addr_read(bus, mode);
    let v = bus.read(addr);
    bus.idle();
    let rel = cpu.fetch(bus);
    if cpu.a != v {
        cpu.branch(bus, rel);
    }
}

/// `DBNZ dp, rel`: decrement memory, branch if nonzero.
pub(crate) fn dbnz_dp(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    let d = cpu.fetch(bus);
    let addr = cpu.dp(d);
    let v = bus.read(addr).wrapping_sub(1);
    bus.write(addr, v);
    let rel = cpu.fetch(bus);
    if v != 0 {
        cpu.branch(bus, rel);
    }
}

/// `DBNZ Y, rel`.
pub(crate) fn dbnz_y(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    cpu.dummy(bus);
    bus.idle();
    let rel = cpu.fetch(bus);
    cpu.y = cpu.y.wrapping_sub(1);
    if cpu.y != 0 {
        cpu.branch(bus, rel);
    }
}

/// `JMP !abs`.
pub(crate) fn jmp_abs(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    cpu.pc = cpu.fetch16(bus);
}

/// `JMP [!abs+X]`.
pub(crate) fn jmp_iabsx(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    let base = cpu.fetch16(bus);
    bus.idle();
    let ptr = base.wrapping_add(u16::from(cpu.x));
    let lo = bus.read(ptr);
    let hi = bus.read(ptr.wrapping_add(1));
    cpu.pc = u16::from_le_bytes([lo, hi]);
}

fn push_pc(cpu: &mut Spc700, bus: &mut dyn ApuBus) {
    let [lo, hi] = cpu.pc.to_le_bytes();
    cpu.push(bus, hi);
    cpu.push(bus, lo);
}

/// `CALL !abs`.
pub(crate) fn call(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    let target = cpu.fetch16(bus);
    bus.idle();
    push_pc(cpu, bus);
    bus.idle();
    bus.idle();
    cpu.pc = target;
}

/// `PCALL up`: call into `$FF00 + up`.
pub(crate) fn pcall(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    let up = cpu.fetch(bus);
    bus.idle();
    push_pc(cpu, bus);
    bus.idle();
    cpu.pc = 0xFF00 | u16::from(up);
}

/// `TCALL n`: call through the vector at `$FFDE - 2n`.
pub(crate) fn tcall(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Num(n) = p else { return };
    cpu.dummy(bus);
    bus.idle();
    push_pc(cpu, bus);
    bus.idle();
    let vector = 0xFFDEu16.wrapping_sub(u16::from(n & 15) << 1);
    let lo = bus.read(vector);
    let hi = bus.read(vector.wrapping_add(1));
    cpu.pc = u16::from_le_bytes([lo, hi]);
}

/// `BRK`: push PC and PSW, set B, clear I, vector `$FFDE`.
pub(crate) fn brk(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    cpu.dummy(bus);
    push_pc(cpu, bus);
    let psw = cpu.psw.to_byte();
    cpu.push(bus, psw);
    bus.idle();
    cpu.psw.b = true;
    cpu.psw.i = false;
    let lo = bus.read(0xFFDE);
    let hi = bus.read(0xFFDF);
    cpu.pc = u16::from_le_bytes([lo, hi]);
}

/// `RET`.
pub(crate) fn ret(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    cpu.dummy(bus);
    bus.idle();
    let lo = cpu.pop(bus);
    let hi = cpu.pop(bus);
    cpu.pc = u16::from_le_bytes([lo, hi]);
}

/// `RETI`.
pub(crate) fn reti(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    cpu.dummy(bus);
    bus.idle();
    let psw = cpu.pop(bus);
    cpu.psw = crate::Psw::from_byte(psw);
    let lo = cpu.pop(bus);
    let hi = cpu.pop(bus);
    cpu.pc = u16::from_le_bytes([lo, hi]);
}

/// Keeps the `Mode` import meaningful for `cbne`'s table entries.
#[allow(dead_code, reason = "documents the modes cbne accepts")]
const CBNE_MODES: [Mode; 2] = [Mode::Dp, Mode::DpX];
