//! Arithmetic, logic, compare, shift, and read-modify-write instructions.

use crate::addressing::{Access, Loc, Mode};
use crate::bits::{lo16, mask, w32};
use crate::bus::Bus;
use crate::cpu::{Cpu, CpuError};

type R = Result<(), CpuError>;

/// Width-dependent constants.
const fn limits(eight: bool) -> (u32, u16) {
    if eight { (0x100, 0x80) } else { (0x1_0000, 0x8000) }
}

fn operand(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode, eight: bool) -> Result<u16, CpuError> {
    let loc = cpu.resolve(bus, mode, eight, Access::Read)?;
    Ok(cpu.read_loc(bus, loc, eight))
}

pub(crate) fn adc(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.m8();
    let v = operand(cpu, bus, mode, eight)?;
    let result = if cpu.p.d { adc_decimal(cpu, v, eight) } else { adc_binary(cpu, v, eight) };
    cpu.set_a(result);
    Ok(())
}

fn adc_binary(cpu: &mut Cpu, v: u16, eight: bool) -> u16 {
    let a = mask(cpu.a, eight);
    let (modulus, sign) = limits(eight);
    let sum = w32(a).wrapping_add(w32(v)).wrapping_add(u32::from(cpu.p.c));
    cpu.p.c = sum >= modulus;
    let result = lo16(sum) & lo16(modulus.wrapping_sub(1));
    cpu.p.v = (!(a ^ v) & (a ^ result) & sign) != 0;
    cpu.p.set_nz(result, eight);
    result
}

/// BCD addition, nibble by nibble, with the 65c816's flag behaviour
/// (overflow computed before the final digit adjustment).
fn adc_decimal(cpu: &mut Cpu, v: u16, eight: bool) -> u16 {
    let a = mask(cpu.a, eight);
    let (modulus, sign) = limits(eight);
    let nibbles = if eight { 2u32 } else { 4 };
    let mut result = 0u32;
    let mut unadjusted = 0u32;
    let mut carry = u32::from(cpu.p.c);
    for i in 0..nibbles {
        let shift = i.wrapping_mul(4);
        let mut partial =
            ((w32(a) >> shift) & 0xF).wrapping_add((w32(v) >> shift) & 0xF).wrapping_add(carry);
        unadjusted = (partial << shift) | (result & ((1u32 << shift).wrapping_sub(1)));
        if partial > 9 {
            partial = partial.wrapping_add(6);
        }
        carry = u32::from(partial > 0xF);
        result |= (partial & 0xF) << shift;
    }
    cpu.p.c = carry != 0;
    let unadjusted16 = lo16(unadjusted) & lo16(modulus.wrapping_sub(1));
    cpu.p.v = (!(a ^ v) & (a ^ unadjusted16) & sign) != 0;
    let result16 = lo16(result);
    cpu.p.set_nz(result16, eight);
    result16
}

pub(crate) fn sbc(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.m8();
    let v = operand(cpu, bus, mode, eight)?;
    let inverted = mask(!v, eight);
    let result =
        if cpu.p.d { sbc_decimal(cpu, inverted, eight) } else { adc_binary(cpu, inverted, eight) };
    cpu.set_a(result);
    Ok(())
}

/// BCD subtraction on the already-inverted operand (signed nibble math so
/// that under-9 digits borrow the way the hardware does).
fn sbc_decimal(cpu: &mut Cpu, inverted: u16, eight: bool) -> u16 {
    let a = mask(cpu.a, eight);
    let (modulus, sign) = limits(eight);
    let nibbles = if eight { 2u32 } else { 4 };
    let mut result = 0i32;
    let mut unadjusted = 0i32;
    let mut carry = i32::from(cpu.p.c);
    for i in 0..nibbles {
        let shift = i.wrapping_mul(4);
        let a_nib = i32::from((a >> shift) & 0xF);
        let v_nib = i32::from((inverted >> shift) & 0xF);
        let mut partial = a_nib.wrapping_add(v_nib).wrapping_add(carry);
        unadjusted = (partial << shift) | (result & ((1i32 << shift).wrapping_sub(1)));
        if partial <= 0xF {
            partial = partial.wrapping_sub(6);
        }
        carry = i32::from(partial > 0xF);
        result |= (partial & 0xF) << shift;
    }
    cpu.p.c = carry != 0;
    let unadjusted16 =
        lo16(u32::from_ne_bytes(unadjusted.to_ne_bytes())) & lo16(modulus.wrapping_sub(1));
    cpu.p.v = (!(a ^ inverted) & (a ^ unadjusted16) & sign) != 0;
    let result16 = lo16(u32::from_ne_bytes(result.to_ne_bytes()));
    cpu.p.set_nz(result16, eight);
    result16
}

const fn compare(cpu: &mut Cpu, reg: u16, v: u16, eight: bool) {
    let r = mask(reg, eight);
    let result = mask(r.wrapping_sub(v), eight);
    cpu.p.c = r >= v;
    cpu.p.set_nz(result, eight);
}

pub(crate) fn cmp(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.m8();
    let v = operand(cpu, bus, mode, eight)?;
    compare(cpu, cpu.a, v, eight);
    Ok(())
}

pub(crate) fn cpx(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.x8();
    let v = operand(cpu, bus, mode, eight)?;
    compare(cpu, cpu.x, v, eight);
    Ok(())
}

pub(crate) fn cpy(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.x8();
    let v = operand(cpu, bus, mode, eight)?;
    compare(cpu, cpu.y, v, eight);
    Ok(())
}

fn logic(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode, f: fn(u16, u16) -> u16) -> R {
    let eight = cpu.m8();
    let v = operand(cpu, bus, mode, eight)?;
    let result = mask(f(cpu.a, v), eight);
    cpu.p.set_nz(result, eight);
    cpu.set_a(result);
    Ok(())
}

pub(crate) fn and(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    logic(cpu, bus, mode, |a, v| a & v)
}

pub(crate) fn ora(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    logic(cpu, bus, mode, |a, v| a | v)
}

pub(crate) fn eor(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    logic(cpu, bus, mode, |a, v| a ^ v)
}

pub(crate) fn bit(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.m8();
    let loc = cpu.resolve(bus, mode, eight, Access::Read)?;
    let v = cpu.read_loc(bus, loc, eight);
    cpu.p.z = mask(cpu.a, eight) & v == 0;
    if !matches!(loc, Loc::Imm(_)) {
        let (_, sign) = limits(eight);
        cpu.p.n = v & sign != 0;
        cpu.p.v = v & (sign >> 1) != 0;
    }
    Ok(())
}

/// Read-modify-write helper for memory or accumulator operands: one internal
/// cycle between the read and the (high-byte-first) write.
fn rmw(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode, f: fn(&mut Cpu, u16, bool) -> u16) -> R {
    let eight = cpu.m8();
    let loc = cpu.resolve(bus, mode, eight, Access::Rmw)?;
    let v = cpu.read_loc(bus, loc, eight);
    bus.idle();
    let result = f(cpu, v, eight);
    cpu.write_loc_rmw(bus, loc, eight, result);
    Ok(())
}

pub(crate) fn inc(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    rmw(cpu, bus, mode, |cpu, v, eight| {
        let r = mask(v.wrapping_add(1), eight);
        cpu.p.set_nz(r, eight);
        r
    })
}

pub(crate) fn dec(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    rmw(cpu, bus, mode, |cpu, v, eight| {
        let r = mask(v.wrapping_sub(1), eight);
        cpu.p.set_nz(r, eight);
        r
    })
}

pub(crate) fn asl(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    rmw(cpu, bus, mode, |cpu, v, eight| {
        let (_, sign) = limits(eight);
        cpu.p.c = v & sign != 0;
        let r = mask(v << 1, eight);
        cpu.p.set_nz(r, eight);
        r
    })
}

pub(crate) fn lsr(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    rmw(cpu, bus, mode, |cpu, v, eight| {
        cpu.p.c = v & 1 != 0;
        let r = mask(v, eight) >> 1;
        cpu.p.set_nz(r, eight);
        r
    })
}

pub(crate) fn rol(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    rmw(cpu, bus, mode, |cpu, v, eight| {
        let (_, sign) = limits(eight);
        let carry_in = u16::from(cpu.p.c);
        cpu.p.c = v & sign != 0;
        let r = mask((v << 1) | carry_in, eight);
        cpu.p.set_nz(r, eight);
        r
    })
}

pub(crate) fn ror(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    rmw(cpu, bus, mode, |cpu, v, eight| {
        let (_, sign) = limits(eight);
        let carry_in = if cpu.p.c { sign } else { 0 };
        cpu.p.c = v & 1 != 0;
        let r = (mask(v, eight) >> 1) | carry_in;
        cpu.p.set_nz(r, eight);
        r
    })
}

pub(crate) fn tsb(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    rmw(cpu, bus, mode, |cpu, v, eight| {
        let a = mask(cpu.a, eight);
        cpu.p.z = a & v == 0;
        v | a
    })
}

pub(crate) fn trb(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    rmw(cpu, bus, mode, |cpu, v, eight| {
        let a = mask(cpu.a, eight);
        cpu.p.z = a & v == 0;
        v & !a
    })
}
