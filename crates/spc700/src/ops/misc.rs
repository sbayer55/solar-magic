//! Flag manipulation, stack, decimal adjust, `XCN`, `SLEEP`/`STOP`, `NOP`.

use super::{P, Reg};
use crate::{ApuBus, Spc700};

/// `NOP`.
pub(crate) fn nop(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    cpu.dummy(bus);
}

/// Two-cycle flag instructions (`p`: 0 CLRP, 1 SETP, 2 CLRC, 3 SETC, 4 CLRV).
pub(crate) fn flag2(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Num(kind) = p else { return };
    cpu.dummy(bus);
    match kind {
        0 => cpu.psw.p = false,
        1 => cpu.psw.p = true,
        2 => cpu.psw.c = false,
        3 => cpu.psw.c = true,
        _ => {
            cpu.psw.v = false;
            cpu.psw.h = false;
        }
    }
}

/// Three-cycle flag instructions (`p`: 0 EI, 1 DI, 2 NOTC).
pub(crate) fn flag3(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Num(kind) = p else { return };
    cpu.dummy(bus);
    bus.idle();
    match kind {
        0 => cpu.psw.i = true,
        1 => cpu.psw.i = false,
        _ => cpu.psw.c = !cpu.psw.c,
    }
}

/// `PUSH reg`.
pub(crate) fn push(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Reg(r) = p else { return };
    cpu.dummy(bus);
    let v = cpu.reg(r);
    cpu.push(bus, v);
    bus.idle();
}

/// `POP reg` (no flags, except that popping PSW replaces them).
pub(crate) fn pop(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Reg(r) = p else { return };
    cpu.dummy(bus);
    bus.idle();
    let v = cpu.pop(bus);
    cpu.set_reg(r, v);
}

/// `XCN A`: swap nibbles.
pub(crate) fn xcn(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    cpu.dummy(bus);
    bus.idle();
    bus.idle();
    bus.idle();
    cpu.a = cpu.a.rotate_left(4);
    cpu.psw.set_nz(cpu.a);
}

/// `DAA A`.
pub(crate) fn daa(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    cpu.dummy(bus);
    bus.idle();
    if cpu.psw.c || cpu.a > 0x99 {
        cpu.a = cpu.a.wrapping_add(0x60);
        cpu.psw.c = true;
    }
    if cpu.psw.h || (cpu.a & 0x0F) > 9 {
        cpu.a = cpu.a.wrapping_add(0x06);
    }
    cpu.psw.set_nz(cpu.a);
}

/// `DAS A`.
pub(crate) fn das(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    cpu.dummy(bus);
    bus.idle();
    if !cpu.psw.c || cpu.a > 0x99 {
        cpu.a = cpu.a.wrapping_sub(0x60);
        cpu.psw.c = false;
    }
    if !cpu.psw.h || (cpu.a & 0x0F) > 9 {
        cpu.a = cpu.a.wrapping_sub(0x06);
    }
    cpu.psw.set_nz(cpu.a);
}

/// `SLEEP` / `STOP`: the core halts (no interrupt sources exist to wake it).
pub(crate) fn halt(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    for _ in 0..3 {
        cpu.dummy(bus);
        bus.idle();
    }
    cpu.stopped = true;
}

/// Keeps `Reg` referenced for the table's push/pop entries.
#[allow(dead_code, reason = "documents the registers push/pop accept")]
const STACK_REGS: [Reg; 4] = [Reg::A, Reg::X, Reg::Y, Reg::Psw];
