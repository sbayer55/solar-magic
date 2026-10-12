//! Narrowing helpers; the only place `as` is allowed in this crate.

/// Low byte.
#[must_use]
#[allow(clippy::as_conversions, reason = "masked truncation")]
pub(crate) const fn lo(v: u32) -> u8 {
    (v & 0xFF) as u8
}

/// Low 16 bits.
#[must_use]
#[allow(clippy::as_conversions, reason = "masked truncation")]
pub(crate) const fn lo16(v: u32) -> u16 {
    (v & 0xFFFF) as u16
}

/// Byte `n` (0 = low) of a 64-bit value.
#[must_use]
#[allow(clippy::as_conversions, reason = "masked truncation")]
pub(crate) const fn byte64(v: u64, n: u8) -> u8 {
    ((v >> n.wrapping_mul(8)) & 0xFF) as u8
}

/// Replaces byte `n` (0 = low) of a 16-bit value.
#[must_use]
#[allow(clippy::as_conversions, reason = "lossless widening in a const fn")]
pub(crate) const fn set_byte16(v: u16, n: u8, b: u8) -> u16 {
    let shift = if n == 0 { 0 } else { 8 };
    (v & !(0xFF << shift)) | ((b as u16) << shift)
}

/// Replaces byte `n` (0 = low) of a 24-bit value.
#[must_use]
#[allow(clippy::as_conversions, reason = "lossless widening in a const fn")]
pub(crate) const fn set_byte24(v: u32, n: u8, b: u8) -> u32 {
    let shift = match n {
        0 => 0,
        1 => 8,
        _ => 16,
    };
    ((v & !(0xFF << shift)) | ((b as u32) << shift)) & 0xFF_FFFF
}

/// A 16-bit register as a signed value.
#[must_use]
pub(crate) const fn signed16(v: u16) -> i16 {
    v.cast_signed()
}
