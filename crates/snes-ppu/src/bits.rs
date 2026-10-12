//! Narrowing helpers; the only place `as` is allowed in this crate.

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

/// Widens a byte.
#[must_use]
#[allow(clippy::as_conversions, reason = "lossless widening in a const fn")]
pub(crate) const fn w(v: u8) -> u16 {
    v as u16
}

/// A 16-bit register as a signed value.
#[must_use]
pub(crate) const fn signed16(v: u16) -> i16 {
    v.cast_signed()
}

/// A 13-bit register as a signed value.
#[must_use]
pub(crate) const fn signed13(v: u16) -> i16 {
    (v << 3).cast_signed() >> 3
}

/// A byte as a signed value.
#[must_use]
pub(crate) const fn signed8(v: u8) -> i8 {
    v.cast_signed()
}

/// Low 24 bits of a product as little-endian bytes.
#[must_use]
pub(crate) const fn product24(v: i32) -> [u8; 3] {
    let [b0, b1, b2, _] = v.cast_unsigned().to_le_bytes();
    [b0, b1, b2]
}
