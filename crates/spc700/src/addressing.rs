//! Operand addressing with the exact cycle shapes of each mode.

use crate::{ApuBus, Spc700};

/// Where a memory operand lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// `dp`
    Dp,
    /// `dp+X`
    DpX,
    /// `dp+Y`
    DpY,
    /// `!abs`
    Abs,
    /// `!abs+X`
    AbsX,
    /// `!abs+Y`
    AbsY,
    /// `(X)`
    IndX,
    /// `(X)+` (post-increment)
    IndXInc,
    /// `[dp+X]`
    IndDpX,
    /// `[dp]+Y`
    IndDpY,
}

impl Spc700 {
    /// Reads the byte at `PC` and advances.
    pub(crate) fn fetch(&mut self, bus: &mut dyn ApuBus) -> u8 {
        let v = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }

    /// The dummy read of the next byte that implied-operand instructions make.
    pub(crate) fn dummy(&self, bus: &mut dyn ApuBus) {
        bus.read(self.pc);
    }

    /// Direct page base (`$0000` or `$0100`).
    pub(crate) const fn dp_base(&self) -> u16 {
        if self.psw.p { 0x0100 } else { 0x0000 }
    }

    /// Direct page address of `offset`, wrapping within the page.
    pub(crate) fn dp(&self, offset: u8) -> u16 {
        self.dp_base() | u16::from(offset)
    }

    pub(crate) fn push(&mut self, bus: &mut dyn ApuBus, v: u8) {
        bus.write(0x0100 | u16::from(self.sp), v);
        self.sp = self.sp.wrapping_sub(1);
    }

    pub(crate) fn pop(&mut self, bus: &mut dyn ApuBus) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        bus.read(0x0100 | u16::from(self.sp))
    }

    /// Reads a 16-bit pointer from the direct page (wrapping within it).
    fn dp_pointer(&self, bus: &mut dyn ApuBus, offset: u8) -> u16 {
        let lo = bus.read(self.dp(offset));
        let hi = bus.read(self.dp(offset.wrapping_add(1)));
        u16::from_le_bytes([lo, hi])
    }

    /// Resolves an operand address for a load (the cycles before the data read).
    pub(crate) fn addr_read(&mut self, bus: &mut dyn ApuBus, mode: Mode) -> u16 {
        match mode {
            Mode::Dp => {
                let d = self.fetch(bus);
                self.dp(d)
            }
            Mode::DpX | Mode::DpY => {
                let d = self.fetch(bus);
                bus.idle();
                let index = if mode == Mode::DpX { self.x } else { self.y };
                self.dp(d.wrapping_add(index))
            }
            Mode::Abs => self.fetch16(bus),
            Mode::AbsX | Mode::AbsY => {
                let base = self.fetch16(bus);
                bus.idle();
                let index = if mode == Mode::AbsX { self.x } else { self.y };
                base.wrapping_add(u16::from(index))
            }
            Mode::IndX | Mode::IndXInc => {
                self.dummy(bus);
                self.dp(self.x)
            }
            Mode::IndDpX => {
                let d = self.fetch(bus);
                bus.idle();
                self.dp_pointer(bus, d.wrapping_add(self.x))
            }
            Mode::IndDpY => {
                let d = self.fetch(bus);
                bus.idle();
                self.dp_pointer(bus, d).wrapping_add(u16::from(self.y))
            }
        }
    }

    /// Resolves an operand address for a store, including the dummy read of
    /// the destination the hardware performs; the caller then writes.
    pub(crate) fn addr_write(&mut self, bus: &mut dyn ApuBus, mode: Mode) -> u16 {
        let addr = match mode {
            Mode::IndDpY => {
                // The store form reads the pointer first and idles afterwards.
                let d = self.fetch(bus);
                let ptr = self.dp_pointer(bus, d);
                bus.idle();
                ptr.wrapping_add(u16::from(self.y))
            }
            Mode::IndXInc => {
                self.dummy(bus);
                bus.idle();
                return self.dp(self.x);
            }
            Mode::Dp
            | Mode::DpX
            | Mode::DpY
            | Mode::Abs
            | Mode::AbsX
            | Mode::AbsY
            | Mode::IndX
            | Mode::IndDpX => self.addr_read(bus, mode),
        };
        bus.read(addr);
        addr
    }

    pub(crate) fn fetch16(&mut self, bus: &mut dyn ApuBus) -> u16 {
        let lo = self.fetch(bus);
        let hi = self.fetch(bus);
        u16::from_le_bytes([lo, hi])
    }

    /// Takes a relative branch: two internal cycles, then the jump.
    pub(crate) fn branch(&mut self, bus: &mut dyn ApuBus, offset: u8) {
        bus.idle();
        bus.idle();
        let delta = u16::from_le_bytes([offset, if offset & 0x80 != 0 { 0xFF } else { 0 }]);
        self.pc = self.pc.wrapping_add(delta);
    }
}
