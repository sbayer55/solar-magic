//! Controller ports: the `$4016/$4017` serial interface and the automatic
//! read into `$4218-$421F` that starts on the first vblank line and takes
//! 4224 master cycles.

use crate::bits::{hi, lo};
use crate::input::{Buttons, Input};

/// Master cycles the auto-read keeps `HVBJOY` bit 0 set.
pub const AUTO_READ_CYCLES: u64 = 4224;

/// Both ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Joypads {
    /// Current controller state.
    pub input: Input,
    /// Serial latch: while high, reads return the first bit.
    strobe: bool,
    /// Bits shifted out so far on each port.
    serial_index: [u8; 2],
    /// Button state captured by the last latch, per port.
    shift: [u16; 2],
    /// Auto-read results.
    auto: [u16; 2],
    /// Master cycle at which the current auto-read finishes.
    busy_until: Option<u64>,
}

impl Joypads {
    /// Idle ports, no buttons.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            input: Input { p1: Buttons::empty(), p2: Buttons::empty() },
            strobe: false,
            serial_index: [0; 2],
            shift: [0; 2],
            auto: [0; 2],
            busy_until: None,
        }
    }

    const fn latch(&mut self) {
        self.shift = [self.input.p1.bits(), self.input.p2.bits()];
        self.serial_index = [0; 2];
    }

    /// `$4016` write: bit 0 is the latch line.
    pub const fn write_strobe(&mut self, v: u8) {
        let strobe = v & 1 != 0;
        if strobe {
            self.latch();
        }
        self.strobe = strobe;
    }

    /// `$4016`/`$4017` read: one bit per read after the latch drops; 1 after
    /// all 16 bits.
    pub fn read_serial(&mut self, port: usize) -> u8 {
        if self.strobe {
            self.latch();
        }
        let index = self.serial_index.get(port).copied().unwrap_or(16);
        let data = self.shift.get(port).copied().unwrap_or(0);
        let bit = if index >= 16 { 1 } else { lo((data >> (15u8.wrapping_sub(index))) & 1) };
        if !self.strobe
            && let Some(i) = self.serial_index.get_mut(port)
        {
            *i = i.saturating_add(1);
        }
        bit
    }

    /// The first vblank line reached with auto-read enabled.
    pub const fn start_auto_read(&mut self, now: u64) {
        self.auto = [self.input.p1.bits(), self.input.p2.bits()];
        self.busy_until = Some(now.saturating_add(AUTO_READ_CYCLES));
        // The hardware strobes the pads itself, which resets the serial shift.
        self.latch();
    }

    /// `HVBJOY` bit 0.
    #[must_use]
    pub fn busy(&self, now: u64) -> bool {
        self.busy_until.is_some_and(|until| now < until)
    }

    /// `$4218-$421F`: `JOY1L, JOY1H, JOY2L, JOY2H`, then zeros for pads 3/4.
    #[must_use]
    pub fn read_auto(&self, offset: u8) -> u8 {
        let port = usize::from((offset >> 1) & 3);
        let word = if port < 2 { self.auto.get(port).copied().unwrap_or(0) } else { 0 };
        if offset & 1 == 0 { lo(word) } else { hi(word) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_shifts_b_first_then_ones() {
        let mut j = Joypads::new();
        j.input.p1 = Buttons::B | Buttons::RIGHT | Buttons::R;
        j.write_strobe(1);
        j.write_strobe(0);
        let bits: Vec<u8> = (0..18).map(|_| j.read_serial(0)).collect();
        assert_eq!(bits, [1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1, 1]);
    }

    #[test]
    fn auto_read_registers() {
        let mut j = Joypads::new();
        j.input.p1 = Buttons::START | Buttons::A;
        j.start_auto_read(1000);
        assert!(j.busy(1000) && !j.busy(1000 + AUTO_READ_CYCLES));
        assert_eq!(j.read_auto(0), 0x80); // JOY1L: A
        assert_eq!(j.read_auto(1), 0x10); // JOY1H: Start
        assert_eq!(j.read_auto(4), 0);
    }
}
