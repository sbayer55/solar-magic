//! 24-bit SNES bus addresses.

use core::fmt;

/// A 24-bit SNES bus address, conventionally written `$BB:HHLL`.
///
/// The value is always `<= 0xFF_FFFF`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnesAddr(u32);

impl SnesAddr {
    /// Largest representable address.
    pub const MAX: u32 = 0x00FF_FFFF;

    /// Creates an address, returning `None` if `addr` does not fit in 24 bits.
    #[must_use]
    pub const fn new(addr: u32) -> Option<Self> {
        if addr > Self::MAX { None } else { Some(Self(addr)) }
    }

    /// Creates an address from a literal that is known to fit in 24 bits.
    ///
    /// Intended for `const` tables of well-known addresses.
    ///
    /// # Panics
    ///
    /// Panics (at compile time when used in a `const` context) if `addr` exceeds 24 bits.
    #[must_use]
    pub const fn lit(addr: u32) -> Self {
        assert!(addr <= Self::MAX, "SNES address does not fit in 24 bits");
        Self(addr)
    }

    /// Builds an address from a bank byte and a 16-bit offset within the bank.
    #[must_use]
    #[allow(clippy::as_conversions, reason = "lossless widening in a const fn")]
    pub const fn from_parts(bank: u8, offset: u16) -> Self {
        Self(((bank as u32) << 16) | (offset as u32))
    }

    /// The raw 24-bit value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The bank byte (`$BB` in `$BB:HHLL`).
    #[must_use]
    #[allow(clippy::as_conversions, reason = "value is masked to 8 bits before the cast")]
    pub const fn bank(self) -> u8 {
        ((self.0 >> 16) & 0xFF) as u8
    }

    /// The 16-bit offset within the bank (`$HHLL` in `$BB:HHLL`).
    #[must_use]
    #[allow(clippy::as_conversions, reason = "value is masked to 16 bits before the cast")]
    pub const fn offset(self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    /// Adds `n` to the address, returning `None` on overflow past 24 bits.
    ///
    /// Note that this is plain linear addition: it will happily cross from
    /// `$05:FFFF` into `$06:0000`, which is not ROM in LoROM. Callers that
    /// walk ROM tables should prefer PC offsets from [`crate::Rom`].
    #[must_use]
    pub const fn checked_add(self, n: u32) -> Option<Self> {
        match self.0.checked_add(n) {
            Some(v) => Self::new(v),
            None => None,
        }
    }
}

impl fmt::Debug for SnesAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SnesAddr(${:02X}:{:04X})", self.bank(), self.offset())
    }
}

impl fmt::Display for SnesAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${:02X}:{:04X}", self.bank(), self.offset())
    }
}

impl TryFrom<u32> for SnesAddr {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(value)
    }
}

impl From<SnesAddr> for u32 {
    fn from(addr: SnesAddr) -> Self {
        addr.0
    }
}

#[cfg(test)]
mod tests {
    use super::SnesAddr;

    #[test]
    fn parts_round_trip() {
        let a = SnesAddr::from_parts(0x05, 0xE000);
        assert_eq!(a.get(), 0x05_E000);
        assert_eq!(a.bank(), 0x05);
        assert_eq!(a.offset(), 0xE000);
        assert_eq!(a.to_string(), "$05:E000");
    }

    #[test]
    fn rejects_out_of_range() {
        assert!(SnesAddr::new(0x0100_0000).is_none());
        assert_eq!(SnesAddr::try_from(0x0100_0000u32), Err(0x0100_0000));
        assert!(SnesAddr::new(SnesAddr::MAX).is_some());
    }

    #[test]
    fn checked_add_overflows_to_none() {
        assert!(SnesAddr::lit(SnesAddr::MAX).checked_add(1).is_none());
        assert_eq!(SnesAddr::lit(0x05_FFFF).checked_add(1), Some(SnesAddr::lit(0x06_0000)));
    }
}
