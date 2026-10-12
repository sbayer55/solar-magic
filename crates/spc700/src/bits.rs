//! Narrowing helpers; the only place `as` is allowed in this crate.

/// Low byte of a word.
#[must_use]
#[allow(clippy::as_conversions, reason = "masked truncation")]
pub(crate) const fn lo(v: u16) -> u8 {
    (v & 0xFF) as u8
}

/// Low 16 bits of a 32-bit value.
#[must_use]
#[allow(clippy::as_conversions, reason = "masked truncation")]
pub(crate) const fn lo16(v: u32) -> u16 {
    (v & 0xFFFF) as u16
}
