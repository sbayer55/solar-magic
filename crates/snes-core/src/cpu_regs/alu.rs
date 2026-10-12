//! The `$4202-$4206` multiplier/divider, modelled bit-serially so that
//! results read before completion are the hardware's intermediate values
//! (8 CPU cycles for a multiply, 16 for a divide; bsnes `aluEdge`).

/// Multiply/divide unit state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alu {
    /// `WRMPYA`.
    pub mpya: u8,
    /// `WRMPYB`.
    pub mpyb: u8,
    /// `WRDIV`.
    pub dividend: u16,
    /// `WRDIVB`.
    pub divisor: u8,
    /// `RDDIV`: quotient (also the multiplier shift register).
    pub rddiv: u16,
    /// `RDMPY`: product or remainder.
    pub rdmpy: u16,
    mpy_counter: u8,
    div_counter: u8,
    shift: u32,
}

impl Default for Alu {
    fn default() -> Self {
        Self::new()
    }
}

impl Alu {
    /// Power-on values (`FF` operands, zero results).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            mpya: 0xFF,
            mpyb: 0xFF,
            dividend: 0xFFFF,
            divisor: 0xFF,
            rddiv: 0,
            rdmpy: 0,
            mpy_counter: 0,
            div_counter: 0,
            shift: 0,
        }
    }

    /// `$4203 WRMPYB`: starts an 8-cycle multiply unless one is running.
    pub fn start_multiply(&mut self, b: u8) {
        self.rdmpy = 0;
        if self.mpy_counter != 0 || self.div_counter != 0 {
            return;
        }
        self.mpyb = b;
        self.rddiv = u16::from_be_bytes([b, self.mpya]);
        self.mpy_counter = 8;
        self.shift = u32::from(b);
    }

    /// `$4206 WRDIVB`: starts a 16-cycle divide unless one is running.
    pub fn start_divide(&mut self, b: u8) {
        self.rdmpy = self.dividend;
        if self.mpy_counter != 0 || self.div_counter != 0 {
            return;
        }
        self.divisor = b;
        self.div_counter = 16;
        self.shift = u32::from(b) << 16;
    }

    /// One CPU cycle of the bit-serial engine.
    pub fn edge(&mut self) {
        if self.mpy_counter != 0 {
            self.mpy_counter = self.mpy_counter.wrapping_sub(1);
            if self.rddiv & 1 != 0 {
                self.rdmpy = self.rdmpy.wrapping_add(crate::bits::lo16(self.shift));
            }
            self.rddiv >>= 1;
            self.shift <<= 1;
        }
        if self.div_counter != 0 {
            self.div_counter = self.div_counter.wrapping_sub(1);
            self.rddiv <<= 1;
            self.shift >>= 1;
            let remainder = u32::from(self.rdmpy);
            if remainder >= self.shift {
                self.rdmpy = crate::bits::lo16(remainder.wrapping_sub(self.shift));
                self.rddiv |= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Alu;

    #[test]
    fn multiply_completes_in_eight_cycles() {
        let mut alu = Alu::new();
        alu.mpya = 0xC3;
        alu.start_multiply(0x7B);
        for _ in 0..8 {
            alu.edge();
        }
        assert_eq!(alu.rdmpy, 0xC3 * 0x7B);
        assert_eq!(alu.rddiv, 0);
    }

    #[test]
    fn divide_completes_in_sixteen_cycles() {
        let mut alu = Alu::new();
        alu.dividend = 0xBEEF;
        alu.start_divide(0x1F);
        for _ in 0..16 {
            alu.edge();
        }
        assert_eq!(alu.rddiv, 0xBEEF / 0x1F);
        assert_eq!(alu.rdmpy, 0xBEEF % 0x1F);
    }

    #[test]
    fn divide_by_zero() {
        let mut alu = Alu::new();
        alu.dividend = 0x1234;
        alu.start_divide(0);
        for _ in 0..16 {
            alu.edge();
        }
        assert_eq!((alu.rddiv, alu.rdmpy), (0xFFFF, 0x1234));
    }
}
