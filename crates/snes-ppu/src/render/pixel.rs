//! A layer pixel packed into 16 bits.

/// `index` (0-7) is the CGRAM index or direct-colour source, bit 8 marks an
/// opaque pixel, bits 9-10 hold the priority (0-3), bit 11 whether colour
/// math may apply (sprites: palettes 4-7 only), bits 13-15 the tilemap
/// palette for direct colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Px(pub u16);

impl Px {
    pub(crate) const TRANSPARENT: Self = Self(0);

    #[allow(clippy::as_conversions, reason = "lossless widening in a const fn")]
    pub(crate) const fn new(index: u8, priority: u8, math_ok: bool, palette: u8) -> Self {
        Self(
            (index as u16)
                | 0x0100
                | (((priority & 3) as u16) << 9)
                | (if math_ok { 0x0800 } else { 0 })
                | (((palette & 7) as u16) << 13),
        )
    }

    pub(crate) const fn opaque(self) -> bool {
        self.0 & 0x0100 != 0
    }

    #[allow(clippy::as_conversions, reason = "masked truncation in a const fn")]
    pub(crate) const fn index(self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    #[allow(clippy::as_conversions, reason = "masked truncation in a const fn")]
    pub(crate) const fn priority(self) -> u8 {
        ((self.0 >> 9) & 3) as u8
    }

    pub(crate) const fn math_ok(self) -> bool {
        self.0 & 0x0800 != 0
    }

    #[allow(clippy::as_conversions, reason = "masked truncation in a const fn")]
    pub(crate) const fn palette(self) -> u8 {
        ((self.0 >> 13) & 7) as u8
    }
}
