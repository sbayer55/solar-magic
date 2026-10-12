//! Loads, stores and register moves.

use super::{P, Reg};
use crate::addressing::Mode;
use crate::{ApuBus, Spc700};

/// `MOV reg, #imm`.
pub(crate) fn mov_reg_imm(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Reg(r) = p else { return };
    let v = cpu.fetch(bus);
    cpu.set_reg(r, v);
    cpu.psw.set_nz(v);
}

/// `MOV reg, mem`.
pub(crate) fn mov_load(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Load(r, mode) = p else { return };
    let addr = cpu.addr_read(bus, mode);
    let v = bus.read(addr);
    if mode == Mode::IndXInc {
        bus.idle();
        cpu.x = cpu.x.wrapping_add(1);
    }
    cpu.set_reg(r, v);
    cpu.psw.set_nz(v);
}

/// `MOV mem, reg`.
pub(crate) fn mov_store(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Store(r, mode) = p else { return };
    let addr = cpu.addr_write(bus, mode);
    bus.write(addr, cpu.reg(r));
    if mode == Mode::IndXInc {
        cpu.x = cpu.x.wrapping_add(1);
    }
}

/// `MOV dst, src` between registers (flags unless the destination is SP).
pub(crate) fn mov_reg_reg(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Reg2(dst, src) = p else { return };
    cpu.dummy(bus);
    let v = cpu.reg(src);
    cpu.set_reg(dst, v);
    if dst != Reg::Sp {
        cpu.psw.set_nz(v);
    }
}

/// `MOV dp, #imm`.
pub(crate) fn mov_dp_imm(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    let imm = cpu.fetch(bus);
    let d = cpu.fetch(bus);
    let addr = cpu.dp(d);
    bus.read(addr);
    bus.write(addr, imm);
}

/// `MOV dd, ds`.
pub(crate) fn mov_dp_dp(cpu: &mut Spc700, bus: &mut dyn ApuBus, _: P) {
    let src = cpu.fetch(bus);
    let v = bus.read(cpu.dp(src));
    let dst = cpu.fetch(bus);
    bus.write(cpu.dp(dst), v);
}
