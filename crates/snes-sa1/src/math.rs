//! The arithmetic unit (`$2250-$2254`, results at `$2306-$230B`) and the
//! variable-length bit reader (`$2258-$225B`, `$230C-$230D`).
//! fullsnes "Arithmetic Maths", "Variable-Length Bit Processing"; ares for
//! the division remainder rule and the bit-pointer stepping.

use crate::Chip;
use crate::bits::{set_byte16, signed16};

impl Chip {
    /// `$2250-$2254`: mode, operands; writing `MB`'s high byte starts the
    /// operation (results are available at once; the 5-6 SA-1 cycles the
    /// hardware takes are not modelled).
    pub(crate) fn write_arith(&mut self, reg: u16, v: u8) {
        match reg {
            0x2250 => {
                self.io.md = v & 1 != 0;
                self.io.acm = v & 2 != 0;
                if self.io.acm {
                    self.io.mr = 0;
                }
            }
            0x2251 => self.io.ma = set_byte16(self.io.ma, 0, v),
            0x2252 => self.io.ma = set_byte16(self.io.ma, 1, v),
            0x2253 => self.io.mb = set_byte16(self.io.mb, 0, v),
            0x2254 => {
                self.io.mb = set_byte16(self.io.mb, 1, v);
                self.arith_start();
            }
            _ => {}
        }
    }

    fn arith_start(&mut self) {
        let io = &mut self.io;
        let a = i64::from(signed16(io.ma));
        let product = a.wrapping_mul(i64::from(signed16(io.mb))).cast_unsigned();
        if io.acm {
            // Cumulative sum: 40-bit, overflow flagged beyond that.
            let sum = io.mr.wrapping_add(product);
            io.overflow = sum >> 40 != 0;
            io.mr = sum & 0xFF_FFFF_FFFF;
        } else if io.md {
            // Unsigned divisor, signed dividend; remainder is non-negative.
            let divisor = i64::from(io.mb);
            io.mr = if divisor == 0 {
                0
            } else {
                let remainder = a.rem_euclid(divisor);
                let quotient = a.wrapping_sub(remainder).checked_div(divisor).unwrap_or(0);
                ((remainder.cast_unsigned() & 0xFFFF) << 16) | (quotient.cast_unsigned() & 0xFFFF)
            };
            io.ma = 0;
        } else {
            io.mr = product & 0xFFFF_FFFF;
        }
        io.mb = 0;
    }

    /// `VBD`: bit count (0 = 16) and mode; in fixed mode each write
    /// advances the bit pointer by the previous length.
    pub(crate) fn vbd_write(&mut self, v: u8) {
        let bits = v & 0x0F;
        self.io.vb = if bits == 0 { 16 } else { bits };
        self.io.hl = v & 0x80 != 0;
        if !self.io.hl {
            self.advance_bits();
        }
    }

    fn advance_bits(&mut self) {
        let bits = self.io.vbit.wrapping_add(self.io.vb);
        self.io.va = self.io.va.wrapping_add(u32::from(bits >> 3)) & 0xFF_FFFF;
        self.io.vbit = bits & 7;
    }

    /// `VDP`: the 16 bits at the bit pointer; reading the high byte in
    /// auto-increment mode advances the pointer.
    pub(crate) fn vdp_read(&mut self, high: bool) -> u8 {
        let va = self.io.va;
        let word = u32::from(self.vbr_read(va))
            | (u32::from(self.vbr_read(va.wrapping_add(1))) << 8)
            | (u32::from(self.vbr_read(va.wrapping_add(2))) << 16);
        let shifted = word >> self.io.vbit;
        if high {
            if self.io.hl {
                self.advance_bits();
            }
            crate::bits::lo(shifted >> 8)
        } else {
            crate::bits::lo(shifted)
        }
    }
}
