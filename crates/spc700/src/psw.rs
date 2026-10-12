//! The program status word.

/// Flags `NVPBHIZC`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Psw {
    /// Negative.
    pub n: bool,
    /// Overflow.
    pub v: bool,
    /// Direct page is `$0100-$01FF`.
    pub p: bool,
    /// Break.
    pub b: bool,
    /// Half carry.
    pub h: bool,
    /// Interrupt enable (no effect on the S-SMP).
    pub i: bool,
    /// Zero.
    pub z: bool,
    /// Carry.
    pub c: bool,
}

impl Psw {
    /// Power-on: `$02` (Z set).
    #[must_use]
    pub const fn new() -> Self {
        Self { n: false, v: false, p: false, b: false, h: false, i: false, z: true, c: false }
    }

    /// Packs into a byte.
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        (if self.n { 0x80 } else { 0 })
            | (if self.v { 0x40 } else { 0 })
            | (if self.p { 0x20 } else { 0 })
            | (if self.b { 0x10 } else { 0 })
            | (if self.h { 0x08 } else { 0 })
            | (if self.i { 0x04 } else { 0 })
            | (if self.z { 0x02 } else { 0 })
            | (if self.c { 0x01 } else { 0 })
    }

    /// Unpacks from a byte.
    #[must_use]
    pub const fn from_byte(b: u8) -> Self {
        Self {
            n: b & 0x80 != 0,
            v: b & 0x40 != 0,
            p: b & 0x20 != 0,
            b: b & 0x10 != 0,
            h: b & 0x08 != 0,
            i: b & 0x04 != 0,
            z: b & 0x02 != 0,
            c: b & 0x01 != 0,
        }
    }

    /// Sets N and Z from a byte.
    pub const fn set_nz(&mut self, v: u8) {
        self.n = v & 0x80 != 0;
        self.z = v == 0;
    }

    /// Sets N and Z from a word.
    pub const fn set_nz16(&mut self, v: u16) {
        self.n = v & 0x8000 != 0;
        self.z = v == 0;
    }
}
