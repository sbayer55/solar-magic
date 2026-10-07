//! Byte/word helpers. The only place `as` truncation is allowed in this crate.

/// Low byte of a word.
#[must_use]
#[allow(clippy::as_conversions, reason = "masked truncation")]
pub(crate) const fn lo(v: u16) -> u8 {
    (v & 0xFF) as u8
}

/// High byte of a word.
#[must_use]
#[allow(clippy::as_conversions, reason = "shifted value fits in 8 bits")]
pub(crate) const fn hi(v: u16) -> u8 {
    (v >> 8) as u8
}

/// Low 16 bits of a 32-bit value.
#[must_use]
#[allow(clippy::as_conversions, reason = "masked truncation")]
pub(crate) const fn lo16(v: u32) -> u16 {
    (v & 0xFFFF) as u16
}

/// Bank byte of a 24-bit address.
#[must_use]
#[allow(clippy::as_conversions, reason = "masked truncation")]
pub(crate) const fn bank(v: u32) -> u8 {
    ((v >> 16) & 0xFF) as u8
}

/// Joins a bank and a 16-bit offset.
#[must_use]
#[allow(clippy::as_conversions, reason = "lossless widening in a const fn")]
pub(crate) const fn join(bank: u8, offset: u16) -> u32 {
    ((bank as u32) << 16) | offset as u32
}

/// Widens a byte.
#[must_use]
#[allow(clippy::as_conversions, reason = "lossless widening in a const fn")]
pub(crate) const fn w(v: u8) -> u16 {
    v as u16
}

/// Widens a word.
#[must_use]
#[allow(clippy::as_conversions, reason = "lossless widening in a const fn")]
pub(crate) const fn w32(v: u16) -> u32 {
    v as u32
}

/// Masks to 8 or 16 bits.
#[must_use]
pub(crate) const fn mask(v: u16, eight: bool) -> u16 {
    if eight { v & 0xFF } else { v }
}
