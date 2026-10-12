//! The output picture: BGR555 pixels, converted to RGBA on demand.

/// Pixels per output row.
pub const FRAME_WIDTH: usize = 256;
/// Maximum rows (overscan).
pub const FRAME_HEIGHT: usize = 239;

/// One frame of BGR555 pixels.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pixels: Box<[u16]>,
    /// Visible rows this frame (224 or 239).
    pub height: usize,
}

impl core::fmt::Debug for Frame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Frame").field("height", &self.height).finish_non_exhaustive()
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Frame {
    /// A black 256×224 frame (storage for 239 rows).
    #[must_use]
    pub fn new() -> Self {
        Self { pixels: vec![0; FRAME_WIDTH * FRAME_HEIGHT].into_boxed_slice(), height: 224 }
    }

    /// All rows, BGR555, `FRAME_WIDTH` per row.
    #[must_use]
    pub fn pixels(&self) -> &[u16] {
        &self.pixels
    }

    /// One pixel.
    #[must_use]
    pub fn pixel(&self, x: usize, y: usize) -> u16 {
        self.pixels.get(y.saturating_mul(FRAME_WIDTH).saturating_add(x)).copied().unwrap_or(0)
    }

    pub(crate) fn set_pixel(&mut self, x: usize, y: usize, v: u16) {
        if x < FRAME_WIDTH
            && let Some(slot) = self.pixels.get_mut(y.saturating_mul(FRAME_WIDTH).saturating_add(x))
        {
            *slot = v;
        }
    }

    /// Converts BGR555 to 8-bit RGBA (5-bit channels scaled to 0-255).
    #[must_use]
    pub const fn rgba(bgr: u16) -> [u8; 4] {
        let r = crate::bits::lo(bgr & 0x1F);
        let g = crate::bits::lo((bgr >> 5) & 0x1F);
        let b = crate::bits::lo((bgr >> 10) & 0x1F);
        [(r << 3) | (r >> 2), (g << 3) | (g >> 2), (b << 3) | (b >> 2), 0xFF]
    }

    /// The visible rows as tightly packed RGBA bytes.
    #[must_use]
    pub fn to_rgba(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .take(self.height.saturating_mul(FRAME_WIDTH))
            .flat_map(|&p| Self::rgba(p))
            .collect()
    }
}
