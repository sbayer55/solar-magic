//! Loads and stores.

use crate::addressing::{Access, Mode};
use crate::bus::Bus;
use crate::cpu::{Cpu, CpuError};

type R = Result<(), CpuError>;

pub(crate) fn lda(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.m8();
    let loc = cpu.resolve(bus, mode, eight, Access::Read)?;
    let v = cpu.read_loc(bus, loc, eight);
    cpu.p.set_nz(v, eight);
    cpu.set_a(v);
    Ok(())
}

pub(crate) fn ldx(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.x8();
    let loc = cpu.resolve(bus, mode, eight, Access::Read)?;
    let v = cpu.read_loc(bus, loc, eight);
    cpu.p.set_nz(v, eight);
    cpu.set_x(v);
    Ok(())
}

pub(crate) fn ldy(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.x8();
    let loc = cpu.resolve(bus, mode, eight, Access::Read)?;
    let v = cpu.read_loc(bus, loc, eight);
    cpu.p.set_nz(v, eight);
    cpu.set_y(v);
    Ok(())
}

pub(crate) fn sta(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.m8();
    let loc = cpu.resolve(bus, mode, eight, Access::Write)?;
    let a = cpu.a;
    cpu.write_loc(bus, loc, eight, a);
    Ok(())
}

pub(crate) fn stx(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.x8();
    let loc = cpu.resolve(bus, mode, eight, Access::Write)?;
    let x = cpu.x;
    cpu.write_loc(bus, loc, eight, x);
    Ok(())
}

pub(crate) fn sty(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.x8();
    let loc = cpu.resolve(bus, mode, eight, Access::Write)?;
    let y = cpu.y;
    cpu.write_loc(bus, loc, eight, y);
    Ok(())
}

pub(crate) fn stz(cpu: &mut Cpu, bus: &mut dyn Bus, mode: Mode) -> R {
    let eight = cpu.m8();
    let loc = cpu.resolve(bus, mode, eight, Access::Write)?;
    cpu.write_loc(bus, loc, eight, 0);
    Ok(())
}
