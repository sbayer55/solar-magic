//! Addressing modes and operand resolution.

use crate::bits::{join, lo, lo16, w, w32};
use crate::bus::Bus;
use crate::cpu::{Cpu, CpuError};

/// How an instruction finds its operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// No operand (or operand implied by the opcode).
    Implied,
    /// Operates on the accumulator.
    Acc,
    /// Immediate; width decided by the instruction (M or X flag, or always 8).
    Imm,
    /// `dp`
    Dp,
    /// `dp,X`
    DpX,
    /// `dp,Y`
    DpY,
    /// `(dp)`
    DpInd,
    /// `(dp,X)`
    DpIndX,
    /// `(dp),Y`
    DpIndY,
    /// `[dp]`
    DpIndLong,
    /// `[dp],Y`
    DpIndLongY,
    /// `abs`
    Abs,
    /// `abs,X`
    AbsX,
    /// `abs,Y`
    AbsY,
    /// `long`
    AbsLong,
    /// `long,X`
    AbsLongX,
    /// `sr,S`
    StackRel,
    /// `(sr,S),Y`
    StackRelIndY,
    /// 8-bit PC-relative (branches).
    Rel8,
    /// 16-bit PC-relative (BRL, PER).
    Rel16,
    /// `(abs)` for JMP.
    AbsInd,
    /// `(abs,X)` for JMP/JSR.
    AbsIndX,
    /// `[abs]` for JML.
    AbsIndLong,
    /// Two bank bytes for MVN/MVP.
    BlockMove,
}

/// How a 16-bit access past the operand address wraps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wrap {
    /// Address increments linearly through the 24-bit space.
    Linear,
    /// Address stays in bank 0 and wraps at `$FFFF` (direct page, stack).
    Bank0,
    /// Address wraps within its 256-byte page (emulation-mode direct page quirk).
    Page,
}

/// A resolved operand location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Loc {
    /// An immediate value already fetched.
    Imm(u16),
    /// The accumulator.
    Acc,
    /// A memory address with its wrap rule.
    Mem(u32, Wrap),
}

impl Cpu {
    /// Fetches the operand bytes for `mode` and resolves the location.
    /// `eight` is the data width used to size an immediate operand.
    pub(crate) fn resolve(
        &mut self,
        bus: &mut dyn Bus,
        mode: Mode,
        eight: bool,
    ) -> Result<Loc, CpuError> {
        Ok(match mode {
            Mode::Implied | Mode::Rel8 | Mode::Rel16 | Mode::BlockMove => {
                return Err(CpuError::NotAnOperand);
            }
            Mode::Acc => Loc::Acc,
            Mode::Imm => Loc::Imm(if eight { w(self.fetch8(bus)) } else { self.fetch16(bus) }),
            Mode::Dp => self.dp_loc(bus, 0),
            Mode::DpX => {
                let x = self.x;
                self.dp_loc(bus, x)
            }
            Mode::DpY => {
                let y = self.y;
                self.dp_loc(bus, y)
            }
            Mode::DpInd => {
                let ptr = self.dp_pointer16(bus, 0)?;
                Loc::Mem(join(self.db, ptr), Wrap::Linear)
            }
            Mode::DpIndX => {
                let x = self.x;
                let ptr = self.dp_pointer16(bus, x)?;
                Loc::Mem(join(self.db, ptr), Wrap::Linear)
            }
            Mode::DpIndY => {
                let ptr = self.dp_pointer16(bus, 0)?;
                Loc::Mem(crate::addr24(join(self.db, ptr).wrapping_add(w32(self.y))), Wrap::Linear)
            }
            Mode::DpIndLong => {
                let ptr = self.dp_pointer24(bus)?;
                Loc::Mem(ptr, Wrap::Linear)
            }
            Mode::DpIndLongY => {
                let ptr = self.dp_pointer24(bus)?;
                Loc::Mem(crate::addr24(ptr.wrapping_add(w32(self.y))), Wrap::Linear)
            }
            Mode::Abs => {
                let off = self.fetch16(bus);
                Loc::Mem(join(self.db, off), Wrap::Linear)
            }
            Mode::AbsX => {
                let off = self.fetch16(bus);
                Loc::Mem(crate::addr24(join(self.db, off).wrapping_add(w32(self.x))), Wrap::Linear)
            }
            Mode::AbsY => {
                let off = self.fetch16(bus);
                Loc::Mem(crate::addr24(join(self.db, off).wrapping_add(w32(self.y))), Wrap::Linear)
            }
            Mode::AbsLong => Loc::Mem(self.fetch24(bus), Wrap::Linear),
            Mode::AbsLongX => {
                let base = self.fetch24(bus);
                Loc::Mem(crate::addr24(base.wrapping_add(w32(self.x))), Wrap::Linear)
            }
            Mode::StackRel => {
                let off = self.fetch8(bus);
                Loc::Mem(w32(self.s.wrapping_add(w(off))), Wrap::Bank0)
            }
            Mode::StackRelIndY => {
                let off = self.fetch8(bus);
                let ptr = Self::read16_wrapped(bus, w32(self.s.wrapping_add(w(off))), Wrap::Bank0);
                Loc::Mem(crate::addr24(join(self.db, ptr).wrapping_add(w32(self.y))), Wrap::Linear)
            }
            Mode::AbsInd | Mode::AbsIndX | Mode::AbsIndLong => return Err(CpuError::NotAnOperand),
        })
    }

    /// Direct page address `D + op8 + index`, with the emulation-mode page
    /// wrap when the direct page register's low byte is zero.
    fn dp_loc(&mut self, bus: &mut dyn Bus, index: u16) -> Loc {
        let off = self.fetch8(bus);
        if self.e && lo(self.d) == 0 {
            let addr = (self.d & 0xFF00) | w(off.wrapping_add(lo(index)));
            Loc::Mem(w32(addr), Wrap::Page)
        } else {
            Loc::Mem(w32(self.d.wrapping_add(w(off)).wrapping_add(index)), Wrap::Bank0)
        }
    }

    /// Reads the 16-bit pointer stored at a direct page location.
    fn dp_pointer16(&mut self, bus: &mut dyn Bus, index: u16) -> Result<u16, CpuError> {
        match self.dp_loc(bus, index) {
            Loc::Mem(addr, wrap) => Ok(Self::read16_wrapped(bus, addr, wrap)),
            Loc::Imm(_) | Loc::Acc => Err(CpuError::NotAnOperand),
        }
    }

    /// Reads the 24-bit pointer stored at a direct page location.
    fn dp_pointer24(&mut self, bus: &mut dyn Bus) -> Result<u32, CpuError> {
        match self.dp_loc(bus, 0) {
            Loc::Mem(addr, _) => {
                // Long pointers always wrap within bank 0 regardless of the page quirk.
                let lo = bus.read(addr);
                let mid = bus.read(w32(lo16(addr).wrapping_add(1)));
                let hi = bus.read(w32(lo16(addr).wrapping_add(2)));
                Ok(u32::from_le_bytes([lo, mid, hi, 0]))
            }
            Loc::Imm(_) | Loc::Acc => Err(CpuError::NotAnOperand),
        }
    }

    /// Address one past `addr` under the given wrap rule.
    pub(crate) const fn next_addr(addr: u32, wrap: Wrap) -> u32 {
        match wrap {
            Wrap::Linear => crate::addr24(addr.wrapping_add(1)),
            Wrap::Bank0 => w32(lo16(addr).wrapping_add(1)),
            Wrap::Page => (addr & 0xFF_FF00) | ((addr.wrapping_add(1)) & 0xFF),
        }
    }

    /// Reads a 16-bit value honouring the wrap rule.
    pub(crate) fn read16_wrapped(bus: &mut dyn Bus, addr: u32, wrap: Wrap) -> u16 {
        let lo = bus.read(addr);
        let hi = bus.read(Self::next_addr(addr, wrap));
        u16::from_le_bytes([lo, hi])
    }

    /// Writes a 16-bit value honouring the wrap rule.
    pub(crate) fn write16_wrapped(bus: &mut dyn Bus, addr: u32, wrap: Wrap, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        bus.write(addr, lo);
        bus.write(Self::next_addr(addr, wrap), hi);
    }

    /// Reads the operand at `loc` with the given width.
    pub(crate) fn read_loc(&self, bus: &mut dyn Bus, loc: Loc, eight: bool) -> u16 {
        match loc {
            Loc::Imm(v) => v,
            Loc::Acc => crate::bits::mask(self.a, eight),
            Loc::Mem(addr, wrap) => {
                if eight {
                    w(bus.read(addr))
                } else {
                    Self::read16_wrapped(bus, addr, wrap)
                }
            }
        }
    }

    /// Writes `value` to `loc` with the given width.
    pub(crate) fn write_loc(&mut self, bus: &mut dyn Bus, loc: Loc, eight: bool, value: u16) {
        match loc {
            Loc::Imm(_) => {}
            Loc::Acc => {
                if eight {
                    self.a = (self.a & 0xFF00) | (value & 0xFF);
                } else {
                    self.a = value;
                }
            }
            Loc::Mem(addr, wrap) => {
                if eight {
                    bus.write(addr, lo(value));
                } else {
                    Self::write16_wrapped(bus, addr, wrap, value);
                }
            }
        }
    }
}
