//! The processor status register.

/// Status flags `NVMXDIZC` plus the hidden emulation bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    /// Negative.
    pub n: bool,
    /// Overflow.
    pub v: bool,
    /// Accumulator/memory is 8-bit (native mode only; always set in emulation).
    pub m: bool,
    /// Index registers are 8-bit (native mode only; always set in emulation).
    pub x: bool,
    /// Decimal arithmetic.
    pub d: bool,
    /// IRQ disable.
    pub i: bool,
    /// Zero.
    pub z: bool,
    /// Carry.
    pub c: bool,
}

impl Flags {
    /// Bit positions in the packed byte.
    pub const N: u8 = 0x80;
    /// Overflow bit.
    pub const V: u8 = 0x40;
    /// Memory width bit.
    pub const M: u8 = 0x20;
    /// Index width bit.
    pub const X: u8 = 0x10;
    /// Decimal bit.
    pub const D: u8 = 0x08;
    /// IRQ disable bit.
    pub const I: u8 = 0x04;
    /// Zero bit.
    pub const Z: u8 = 0x02;
    /// Carry bit.
    pub const C: u8 = 0x01;

    /// Packs into the `P` register byte.
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        (if self.n { Self::N } else { 0 })
            | (if self.v { Self::V } else { 0 })
            | (if self.m { Self::M } else { 0 })
            | (if self.x { Self::X } else { 0 })
            | (if self.d { Self::D } else { 0 })
            | (if self.i { Self::I } else { 0 })
            | (if self.z { Self::Z } else { 0 })
            | (if self.c { Self::C } else { 0 })
    }

    /// Unpacks from the `P` register byte.
    #[must_use]
    pub const fn from_byte(b: u8) -> Self {
        Self {
            n: b & Self::N != 0,
            v: b & Self::V != 0,
            m: b & Self::M != 0,
            x: b & Self::X != 0,
            d: b & Self::D != 0,
            i: b & Self::I != 0,
            z: b & Self::Z != 0,
            c: b & Self::C != 0,
        }
    }

    /// Sets N and Z from an 8- or 16-bit result.
    pub const fn set_nz(&mut self, value: u16, eight_bit: bool) {
        if eight_bit {
            self.n = value & 0x80 != 0;
            self.z = value.trailing_zeros() >= 8;
        } else {
            self.n = value & 0x8000 != 0;
            self.z = value == 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Flags;

    #[test]
    fn pack_round_trip() {
        for b in 0..=255u8 {
            assert_eq!(Flags::from_byte(b).to_byte(), b);
        }
    }

    #[test]
    fn nz() {
        let mut f = Flags::default();
        f.set_nz(0x0080, true);
        assert!(f.n && !f.z);
        f.set_nz(0x0100, true);
        assert!(!f.n && f.z);
        f.set_nz(0x8000, false);
        assert!(f.n && !f.z);
    }
}
