//! Register transfers, flag manipulation, block moves, and odds and ends.
//! Implied instructions spend one internal cycle after the opcode fetch.

use crate::addressing::Mode;
use crate::bits::join;
use crate::bus::Bus;
use crate::cpu::{Cpu, CpuError, StopReason};
use crate::flags::Flags;

type R = Result<(), CpuError>;

pub(crate) fn tax(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    let eight = cpu.x8();
    cpu.set_x(cpu.a);
    cpu.p.set_nz(cpu.x, eight);
    Ok(())
}

pub(crate) fn tay(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    let eight = cpu.x8();
    cpu.set_y(cpu.a);
    cpu.p.set_nz(cpu.y, eight);
    Ok(())
}

pub(crate) fn txa(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    let eight = cpu.m8();
    cpu.set_a(cpu.x);
    cpu.p.set_nz(crate::bits::mask(cpu.a, eight), eight);
    Ok(())
}

pub(crate) fn tya(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    let eight = cpu.m8();
    cpu.set_a(cpu.y);
    cpu.p.set_nz(crate::bits::mask(cpu.a, eight), eight);
    Ok(())
}

pub(crate) fn tsx(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    let eight = cpu.x8();
    cpu.set_x(cpu.s);
    cpu.p.set_nz(cpu.x, eight);
    Ok(())
}

pub(crate) fn txs(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    cpu.s = if cpu.e {
        0x0100 | (cpu.x & 0xFF)
    } else if cpu.x8() {
        cpu.x & 0xFF
    } else {
        cpu.x
    };
    Ok(())
}

pub(crate) fn txy(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    let eight = cpu.x8();
    cpu.set_y(cpu.x);
    cpu.p.set_nz(cpu.y, eight);
    Ok(())
}

pub(crate) fn tyx(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    let eight = cpu.x8();
    cpu.set_x(cpu.y);
    cpu.p.set_nz(cpu.x, eight);
    Ok(())
}

pub(crate) fn tcd(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    cpu.d = cpu.a;
    cpu.p.set_nz(cpu.d, false);
    Ok(())
}

pub(crate) fn tdc(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    cpu.a = cpu.d;
    cpu.p.set_nz(cpu.a, false);
    Ok(())
}

pub(crate) fn tcs(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    cpu.s = if cpu.e { 0x0100 | (cpu.a & 0xFF) } else { cpu.a };
    Ok(())
}

pub(crate) fn tsc(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    cpu.a = cpu.s;
    cpu.p.set_nz(cpu.a, false);
    Ok(())
}

pub(crate) fn xba(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    bus.idle();
    cpu.a = cpu.a.rotate_left(8);
    cpu.p.set_nz(cpu.a & 0xFF, true);
    Ok(())
}

pub(crate) fn xce(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    core::mem::swap(&mut cpu.p.c, &mut cpu.e);
    if cpu.e {
        cpu.p.m = true;
        cpu.p.x = true;
        cpu.s = 0x0100 | (cpu.s & 0xFF);
    }
    cpu.apply_flag_widths();
    Ok(())
}

pub(crate) fn sep(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let bits = cpu.fetch8(bus);
    bus.idle();
    cpu.p = Flags::from_byte(cpu.p.to_byte() | bits);
    if cpu.e {
        cpu.p.m = true;
        cpu.p.x = true;
    }
    cpu.apply_flag_widths();
    Ok(())
}

pub(crate) fn rep(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let bits = cpu.fetch8(bus);
    bus.idle();
    cpu.p = Flags::from_byte(cpu.p.to_byte() & !bits);
    if cpu.e {
        cpu.p.m = true;
        cpu.p.x = true;
    }
    cpu.apply_flag_widths();
    Ok(())
}

pub(crate) fn clc(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    cpu.p.c = false;
    Ok(())
}
pub(crate) fn sec(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    cpu.p.c = true;
    Ok(())
}
pub(crate) fn cli(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    cpu.p.i = false;
    Ok(())
}
pub(crate) fn sei(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    cpu.p.i = true;
    Ok(())
}
pub(crate) fn cld(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    cpu.p.d = false;
    Ok(())
}
pub(crate) fn sed(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    cpu.p.d = true;
    Ok(())
}
pub(crate) fn clv(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    cpu.p.v = false;
    Ok(())
}

pub(crate) fn nop(_: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    Ok(())
}

/// Reserved two-byte opcode: the signature byte is skipped during an
/// internal cycle rather than read.
pub(crate) fn wdm(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    cpu.pc = cpu.pc.wrapping_add(1);
    bus.idle();
    Ok(())
}

/// Stops the clock. Only a reset restarts the processor.
pub(crate) fn stp(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    bus.idle();
    cpu.stopped = true;
    Err(StopReason::Stp.into())
}

/// Waits for an interrupt: later steps do nothing until `nmi`/`irq`.
pub(crate) fn wai(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    bus.idle();
    cpu.waiting = true;
    Ok(())
}

/// One byte of a block move (7 cycles); the instruction repeats until the
/// 16-bit accumulator wraps to `$FFFF`.
fn block_move(cpu: &mut Cpu, bus: &mut dyn Bus, step: u16) -> R {
    let dst_bank = cpu.fetch8(bus);
    let src_bank = cpu.fetch8(bus);
    let v = bus.read(join(src_bank, cpu.x));
    bus.write(join(dst_bank, cpu.y), v);
    bus.idle();
    bus.idle();
    cpu.db = dst_bank;
    cpu.set_x(cpu.x.wrapping_add(step));
    cpu.set_y(cpu.y.wrapping_add(step));
    cpu.a = cpu.a.wrapping_sub(1);
    if cpu.a != 0xFFFF {
        cpu.pc = cpu.pc.wrapping_sub(3);
    }
    Ok(())
}

pub(crate) fn mvn(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    block_move(cpu, bus, 1)
}

pub(crate) fn mvp(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    block_move(cpu, bus, 0xFFFF)
}

pub(crate) fn inx(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    let eight = cpu.x8();
    cpu.set_x(cpu.x.wrapping_add(1));
    cpu.p.set_nz(cpu.x, eight);
    Ok(())
}

pub(crate) fn iny(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    let eight = cpu.x8();
    cpu.set_y(cpu.y.wrapping_add(1));
    cpu.p.set_nz(cpu.y, eight);
    Ok(())
}

pub(crate) fn dex(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    let eight = cpu.x8();
    cpu.set_x(cpu.x.wrapping_sub(1));
    cpu.p.set_nz(cpu.x, eight);
    Ok(())
}

pub(crate) fn dey(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    bus.idle();
    let eight = cpu.x8();
    cpu.set_y(cpu.y.wrapping_sub(1));
    cpu.p.set_nz(cpu.y, eight);
    Ok(())
}
