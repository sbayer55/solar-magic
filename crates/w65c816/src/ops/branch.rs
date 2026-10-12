//! Branches, jumps, calls, returns, and software interrupts, with the
//! internal cycles each spends.

use crate::addressing::{Mode, Wrap};
use crate::bits::{join, lo16, w, w32};
use crate::bus::Bus;
use crate::cpu::{Cpu, CpuError, StopReason};
use crate::flags::Flags;

type R = Result<(), CpuError>;

/// Native-mode interrupt vectors.
const VEC_COP_NATIVE: u32 = 0xFFE4;
const VEC_BRK_NATIVE: u32 = 0xFFE6;
/// Emulation-mode vectors (BRK shares IRQ's).
const VEC_COP_EMU: u32 = 0xFFF4;
const VEC_BRK_EMU: u32 = 0xFFFE;

/// A taken branch costs one internal cycle, and one more in emulation mode
/// when it crosses a page.
fn branch_if(cpu: &mut Cpu, bus: &mut dyn Bus, cond: bool) -> R {
    let off = cpu.fetch8(bus);
    if cond {
        bus.idle();
        let delta = if off & 0x80 != 0 { w(off) | 0xFF00 } else { w(off) };
        let target = cpu.pc.wrapping_add(delta);
        if cpu.e && (cpu.pc ^ target) & 0xFF00 != 0 {
            bus.idle();
        }
        cpu.pc = target;
    }
    Ok(())
}

pub(crate) fn bpl(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    branch_if(cpu, bus, !cpu.p.n)
}
pub(crate) fn bmi(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    branch_if(cpu, bus, cpu.p.n)
}
pub(crate) fn bvc(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    branch_if(cpu, bus, !cpu.p.v)
}
pub(crate) fn bvs(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    branch_if(cpu, bus, cpu.p.v)
}
pub(crate) fn bcc(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    branch_if(cpu, bus, !cpu.p.c)
}
pub(crate) fn bcs(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    branch_if(cpu, bus, cpu.p.c)
}
pub(crate) fn bne(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    branch_if(cpu, bus, !cpu.p.z)
}
pub(crate) fn beq(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    branch_if(cpu, bus, cpu.p.z)
}
pub(crate) fn bra(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    branch_if(cpu, bus, true)
}

pub(crate) fn brl(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let off = cpu.fetch16(bus);
    bus.idle();
    cpu.pc = cpu.pc.wrapping_add(off);
    Ok(())
}

#[allow(clippy::wildcard_enum_match_arm, reason = "every other mode is an internal table error")]
pub(crate) fn jmp(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    match mode {
        Mode::Abs => cpu.pc = cpu.fetch16(bus),
        Mode::AbsInd => {
            let ptr = cpu.fetch16(bus);
            cpu.pc = Cpu::read16_wrapped(bus, w32(ptr), Wrap::Bank0);
        }
        Mode::AbsIndX => {
            let ptr = cpu.fetch16(bus).wrapping_add(cpu.x);
            bus.idle();
            cpu.pc = read16_in_bank(bus, cpu.pb, ptr);
        }
        Mode::AbsLong => {
            let target = cpu.fetch24(bus);
            cpu.pb = crate::bits::bank(target);
            cpu.pc = lo16(target);
        }
        Mode::AbsIndLong => {
            let ptr = cpu.fetch16(bus);
            let lo = bus.read(w32(ptr));
            let mid = bus.read(w32(ptr.wrapping_add(1)));
            let hi = bus.read(w32(ptr.wrapping_add(2)));
            cpu.pc = u16::from_le_bytes([lo, mid]);
            cpu.pb = hi;
        }
        _ => return Err(CpuError::NotAnOperand),
    }
    Ok(())
}

/// Reads a 16-bit word that wraps within `bank`.
fn read16_in_bank(bus: &mut dyn Bus, bank: u8, offset: u16) -> u16 {
    let lo = bus.read(join(bank, offset));
    let hi = bus.read(join(bank, offset.wrapping_add(1)));
    u16::from_le_bytes([lo, hi])
}

#[allow(clippy::wildcard_enum_match_arm, reason = "every other mode is an internal table error")]
pub(crate) fn jsr(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    match mode {
        Mode::Abs => {
            let target = cpu.fetch16(bus);
            bus.idle();
            let ret = cpu.pc.wrapping_sub(1);
            cpu.push16(bus, ret);
            cpu.pc = target;
        }
        Mode::AbsIndX => {
            // JSR (abs,X): the return address (the high operand byte) is pushed
            // between the two operand fetches.
            let lo = cpu.fetch8(bus);
            let ret = cpu.pc;
            cpu.push16(bus, ret);
            let hi = cpu.fetch8(bus);
            bus.idle();
            let operand = u16::from_le_bytes([lo, hi]);
            cpu.pc = read16_in_bank(bus, cpu.pb, operand.wrapping_add(cpu.x));
        }
        _ => return Err(CpuError::NotAnOperand),
    }
    Ok(())
}

pub(crate) fn jsl(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let lo = cpu.fetch8(bus);
    let hi = cpu.fetch8(bus);
    // JSL pushes 3 bytes without the page-1 wrap: PB, then (after an internal
    // cycle and the bank fetch) the address of the instruction's last byte.
    let pb = cpu.pb;
    bus.write(w32(cpu.s), pb);
    cpu.s = cpu.s.wrapping_sub(1);
    bus.idle();
    let bank = cpu.fetch8(bus);
    let ret = cpu.pc.wrapping_sub(1);
    cpu.push16_nowrap(bus, ret);
    cpu.pb = bank;
    cpu.pc = u16::from_le_bytes([lo, hi]);
    Ok(())
}

pub(crate) fn rts(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    bus.idle();
    cpu.pc = cpu.pull16(bus).wrapping_add(1);
    bus.idle();
    Ok(())
}

pub(crate) fn rtl(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    bus.idle();
    // RTL ignores the page-1 wrap for all three pulls; the stack pointer is
    // only forced back into page 1 afterwards.
    cpu.s = cpu.s.wrapping_add(1);
    let lo = bus.read(w32(cpu.s));
    cpu.s = cpu.s.wrapping_add(1);
    let hi = bus.read(w32(cpu.s));
    cpu.s = cpu.s.wrapping_add(1);
    let pb = bus.read(w32(cpu.s));
    cpu.fix_emulation_stack();
    cpu.pc = u16::from_le_bytes([lo, hi]).wrapping_add(1);
    cpu.pb = pb;
    Ok(())
}

pub(crate) fn rti(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    bus.idle();
    let p = cpu.pull8(bus);
    cpu.p = Flags::from_byte(p);
    if cpu.e {
        cpu.p.m = true;
        cpu.p.x = true;
    }
    cpu.apply_flag_widths();
    cpu.pc = cpu.pull16(bus);
    if !cpu.e {
        cpu.pb = cpu.pull8(bus);
    }
    Ok(())
}

fn software_interrupt(
    cpu: &mut Cpu,
    bus: &mut dyn Bus,
    native_vec: u32,
    emu_vec: u32,
    is_brk: bool,
) -> R {
    // The signature byte is fetched and ignored.
    cpu.fetch8(bus);
    if cpu.e {
        cpu.push16(bus, cpu.pc);
        let mut p = cpu.p.to_byte() | 0x20;
        if is_brk {
            p |= 0x10;
        }
        cpu.push8(bus, p);
    } else {
        let pb = cpu.pb;
        cpu.push8(bus, pb);
        cpu.push16(bus, cpu.pc);
        cpu.push8(bus, cpu.p.to_byte());
    }
    cpu.p.i = true;
    cpu.p.d = false;
    cpu.pb = 0;
    let vec = if cpu.e { emu_vec } else { native_vec };
    cpu.pc = bus.read_u16(vec);
    Ok(())
}

pub(crate) fn brk(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    if cpu.brk_is_fatal {
        return Err(StopReason::Brk { pb: cpu.pb, pc: cpu.pc.wrapping_sub(1) }.into());
    }
    software_interrupt(cpu, bus, VEC_BRK_NATIVE, VEC_BRK_EMU, true)
}

pub(crate) fn cop(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    if cpu.brk_is_fatal {
        return Err(StopReason::Brk { pb: cpu.pb, pc: cpu.pc.wrapping_sub(1) }.into());
    }
    software_interrupt(cpu, bus, VEC_COP_NATIVE, VEC_COP_EMU, false)
}
