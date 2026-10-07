//! Push and pull instructions.

use crate::addressing::Mode;
use crate::bits::{lo, w};
use crate::bus::Bus;
use crate::cpu::{Cpu, CpuError};
use crate::flags::Flags;

type R = Result<(), CpuError>;

pub(crate) fn pha(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    if cpu.m8() {
        cpu.push8(bus, lo(cpu.a));
    } else {
        cpu.push16(bus, cpu.a);
    }
    Ok(())
}

pub(crate) fn phx(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    if cpu.x8() {
        cpu.push8(bus, lo(cpu.x));
    } else {
        cpu.push16(bus, cpu.x);
    }
    Ok(())
}

pub(crate) fn phy(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    if cpu.x8() {
        cpu.push8(bus, lo(cpu.y));
    } else {
        cpu.push16(bus, cpu.y);
    }
    Ok(())
}

pub(crate) fn php(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let mut p = cpu.p.to_byte();
    if cpu.e {
        p |= 0x30;
    }
    cpu.push8(bus, p);
    Ok(())
}

pub(crate) fn phb(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let db = cpu.db;
    cpu.push8(bus, db);
    Ok(())
}

pub(crate) fn phk(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let pb = cpu.pb;
    cpu.push8(bus, pb);
    Ok(())
}

pub(crate) fn phd(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let d = cpu.d;
    cpu.push16_nowrap(bus, d);
    Ok(())
}

pub(crate) fn pea(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let v = cpu.fetch16(bus);
    cpu.push16_nowrap(bus, v);
    Ok(())
}

pub(crate) fn pei(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let loc = cpu.resolve(bus, Mode::Dp, false)?;
    let v = cpu.read_loc(bus, loc, false);
    cpu.push16_nowrap(bus, v);
    Ok(())
}

pub(crate) fn per(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let off = cpu.fetch16(bus);
    let v = cpu.pc.wrapping_add(off);
    cpu.push16_nowrap(bus, v);
    Ok(())
}

pub(crate) fn pla(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let eight = cpu.m8();
    let v = if eight { w(cpu.pull8(bus)) } else { cpu.pull16(bus) };
    cpu.p.set_nz(v, eight);
    cpu.set_a(v);
    Ok(())
}

pub(crate) fn plx(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let eight = cpu.x8();
    let v = if eight { w(cpu.pull8(bus)) } else { cpu.pull16(bus) };
    cpu.p.set_nz(v, eight);
    cpu.set_x(v);
    Ok(())
}

pub(crate) fn ply(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let eight = cpu.x8();
    let v = if eight { w(cpu.pull8(bus)) } else { cpu.pull16(bus) };
    cpu.p.set_nz(v, eight);
    cpu.set_y(v);
    Ok(())
}

pub(crate) fn plp(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let p = cpu.pull8(bus);
    cpu.p = Flags::from_byte(p);
    if cpu.e {
        cpu.p.m = true;
        cpu.p.x = true;
    }
    cpu.apply_flag_widths();
    Ok(())
}

pub(crate) fn plb(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    // PLB ignores the page-1 wrap.
    cpu.s = cpu.s.wrapping_add(1);
    let v = bus.read(crate::bits::w32(cpu.s));
    cpu.fix_emulation_stack();
    cpu.db = v;
    cpu.p.set_nz(w(v), true);
    Ok(())
}

pub(crate) fn pld(cpu: &mut Cpu, bus: &mut dyn Bus, _: Mode) -> R {
    let v = cpu.pull16_nowrap(bus);
    cpu.d = v;
    cpu.p.set_nz(v, false);
    Ok(())
}
