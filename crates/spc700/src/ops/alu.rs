//! Byte arithmetic, logic, compares, shifts, increments.

use super::{Alu, P, Reg, Rmw};
use crate::addressing::Mode;
use crate::{ApuBus, Spc700};

impl Spc700 {
    /// Applies `op` to `a` and `b`, setting flags; returns the result (for
    /// `CMP` the left operand, unchanged).
    pub(crate) fn alu(&mut self, op: Alu, a: u8, b: u8) -> u8 {
        match op {
            Alu::Or => {
                let r = a | b;
                self.psw.set_nz(r);
                r
            }
            Alu::And => {
                let r = a & b;
                self.psw.set_nz(r);
                r
            }
            Alu::Eor => {
                let r = a ^ b;
                self.psw.set_nz(r);
                r
            }
            Alu::Cmp => {
                let r = a.wrapping_sub(b);
                self.psw.set_nz(r);
                self.psw.c = a >= b;
                a
            }
            Alu::Adc => self.adc(a, b),
            Alu::Sbc => self.adc(a, !b),
        }
    }

    fn adc(&mut self, a: u8, b: u8) -> u8 {
        let carry = u16::from(self.psw.c);
        let sum = u16::from(a).wrapping_add(u16::from(b)).wrapping_add(carry);
        let r = crate::bits::lo(sum);
        self.psw.c = sum > 0xFF;
        self.psw.h =
            ((a & 0x0F).wrapping_add(b & 0x0F).wrapping_add(crate::bits::lo(carry))) > 0x0F;
        self.psw.v = (!(a ^ b) & (a ^ r)) & 0x80 != 0;
        self.psw.set_nz(r);
        r
    }

    /// Applies a read-modify-write operation, setting flags.
    pub(crate) fn rmw(&mut self, op: Rmw, v: u8) -> u8 {
        let r = match op {
            Rmw::Asl => {
                self.psw.c = v & 0x80 != 0;
                v << 1
            }
            Rmw::Rol => {
                let c = u8::from(self.psw.c);
                self.psw.c = v & 0x80 != 0;
                (v << 1) | c
            }
            Rmw::Lsr => {
                self.psw.c = v & 1 != 0;
                v >> 1
            }
            Rmw::Ror => {
                let c = if self.psw.c { 0x80 } else { 0 };
                self.psw.c = v & 1 != 0;
                (v >> 1) | c
            }
            Rmw::Inc => v.wrapping_add(1),
            Rmw::Dec => v.wrapping_sub(1),
        };
        self.psw.set_nz(r);
        r
    }

    pub(crate) const fn reg(&self, r: Reg) -> u8 {
        match r {
            Reg::A => self.a,
            Reg::X => self.x,
            Reg::Y => self.y,
            Reg::Sp => self.sp,
            Reg::Psw => self.psw.to_byte(),
        }
    }

    pub(crate) const fn set_reg(&mut self, r: Reg, v: u8) {
        match r {
            Reg::A => self.a = v,
            Reg::X => self.x = v,
            Reg::Y => self.y = v,
            Reg::Sp => self.sp = v,
            Reg::Psw => self.psw = crate::Psw::from_byte(v),
        }
    }
}

/// `OP A, operand` for every memory mode.
pub(crate) fn alu_a(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Alu(op, mode) = p else { return };
    let addr = cpu.addr_read(bus, mode);
    let v = bus.read(addr);
    if mode == Mode::IndXInc {
        bus.idle();
    }
    cpu.a = cpu.alu(op, cpu.a, v);
}

/// `OP A, #imm`.
pub(crate) fn alu_a_imm(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::AluMem(op) = p else { return };
    let v = cpu.fetch(bus);
    cpu.a = cpu.alu(op, cpu.a, v);
}

/// `OP (X), (Y)`: read `(Y)`, read `(X)`, write `(X)` (or idle for CMP).
pub(crate) fn alu_ix_iy(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::AluMem(op) = p else { return };
    cpu.dummy(bus);
    let b = bus.read(cpu.dp(cpu.y));
    let x_addr = cpu.dp(cpu.x);
    let a = bus.read(x_addr);
    let r = cpu.alu(op, a, b);
    if op == Alu::Cmp {
        bus.idle();
    } else {
        bus.write(x_addr, r);
    }
}

/// `OP dd, ds`.
pub(crate) fn alu_dp_dp(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::AluMem(op) = p else { return };
    let src = cpu.fetch(bus);
    let b = bus.read(cpu.dp(src));
    let dst = cpu.fetch(bus);
    let dst_addr = cpu.dp(dst);
    let a = bus.read(dst_addr);
    let r = cpu.alu(op, a, b);
    if op == Alu::Cmp {
        bus.idle();
    } else {
        bus.write(dst_addr, r);
    }
}

/// `OP dp, #imm`.
pub(crate) fn alu_dp_imm(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::AluMem(op) = p else { return };
    let imm = cpu.fetch(bus);
    let d = cpu.fetch(bus);
    let addr = cpu.dp(d);
    let a = bus.read(addr);
    let r = cpu.alu(op, a, imm);
    if op == Alu::Cmp {
        bus.idle();
    } else {
        bus.write(addr, r);
    }
}

/// `CMP X/Y, #imm`.
pub(crate) fn cmp_reg_imm(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Reg(r) = p else { return };
    let v = cpu.fetch(bus);
    cpu.alu(Alu::Cmp, cpu.reg(r), v);
}

/// `CMP X/Y, dp` and `CMP X/Y, !abs`.
pub(crate) fn cmp_reg_mem(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Load(r, mode) = p else { return };
    let addr = cpu.addr_read(bus, mode);
    let v = bus.read(addr);
    cpu.alu(Alu::Cmp, cpu.reg(r), v);
}

/// Shifts and increments on memory.
pub(crate) fn rmw_mem(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::Rmw(op, mode) = p else { return };
    let addr = cpu.addr_read(bus, mode);
    let v = bus.read(addr);
    let r = cpu.rmw(op, v);
    bus.write(addr, r);
}

/// Shifts and increments on A, X or Y.
pub(crate) fn rmw_reg(cpu: &mut Spc700, bus: &mut dyn ApuBus, p: P) {
    let P::RmwReg(op, r) = p else { return };
    cpu.dummy(bus);
    let v = cpu.rmw(op, cpu.reg(r));
    cpu.set_reg(r, v);
}
