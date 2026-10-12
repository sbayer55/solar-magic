//! The output picture: BGR555 pixels, converted to RGBA on demand.
//!
//! Storage is always the PPU's full resolution, 512 half-pixels per line
//! and two rows per line, so that hi-res (modes 5/6, pseudo-hires) and
//! interlaced frames need no reallocation. A lo-res dot writes both
//! half-pixels; a progressive frame writes the even rows only. The
//! accessors present the picture at its natural size: 256 or 512 wide,
//! 224/239 rows, doubled when interlaced.

/// Pixels per lo-res output row.
pub const FRAME_WIDTH: usize = 256;
/// Maximum lo-res rows (overscan).
pub const FRAME_HEIGHT: usize = 239;
/// Half-pixels per stored row (hi-res width).
pub const MAX_FRAME_WIDTH: usize = 512;
/// Stored rows (both interlace fields of an overscan frame).
pub const MAX_FRAME_HEIGHT: usize = 478;

/// One frame of BGR555 pixels.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pixels: Box<[u16]>,
    /// Scanlines of the last completed frame (224 or 239).
    lines: usize,
    /// Whether any dot of the last completed frame was hi-res.
    hires: bool,
    /// Whether the last completed frame was interlaced.
    interlaced: bool,
}

impl core::fmt::Debug for Frame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Frame")
            .field("width", &self.width())
            .field("height", &self.height())
            .finish_non_exhaustive()
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Frame {
    /// A black 256×224 frame.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pixels: vec![0; MAX_FRAME_WIDTH * MAX_FRAME_HEIGHT].into_boxed_slice(),
            lines: 224,
            hires: false,
            interlaced: false,
        }
    }

    /// Output width: 256, or 512 when the frame used a hi-res mode.
    #[must_use]
    pub const fn width(&self) -> usize {
        if self.hires { MAX_FRAME_WIDTH } else { FRAME_WIDTH }
    }

    /// Output rows: the scanlines, doubled when interlaced.
    #[must_use]
    pub const fn height(&self) -> usize {
        if self.interlaced { self.lines.saturating_mul(2) } else { self.lines }
    }

    /// Scanlines (224 or 239).
    #[must_use]
    pub const fn lines(&self) -> usize {
        self.lines
    }

    /// Whether the frame used a hi-res mode (512 pixels wide).
    #[must_use]
    pub const fn hires(&self) -> bool {
        self.hires
    }

    /// Whether the frame was interlaced (rows doubled).
    #[must_use]
    pub const fn interlaced(&self) -> bool {
        self.interlaced
    }

    /// Describes the frame just completed.
    pub(crate) const fn publish(&mut self, lines: usize, hires: bool, interlaced: bool) {
        self.lines = lines;
        self.hires = hires;
        self.interlaced = interlaced;
    }

    /// Storage index of output pixel (`x`, `y`).
    const fn index(&self, x: usize, y: usize) -> usize {
        let row = if self.interlaced { y } else { y.saturating_mul(2) };
        let col = if self.hires { x } else { x.saturating_mul(2) };
        row.saturating_mul(MAX_FRAME_WIDTH).saturating_add(col)
    }

    /// One pixel in output coordinates.
    #[must_use]
    pub fn pixel(&self, x: usize, y: usize) -> u16 {
        if x >= self.width() || y >= self.height() {
            return 0;
        }
        self.pixels.get(self.index(x, y)).copied().unwrap_or(0)
    }

    /// Stores one dot: the two half-pixels of lo-res column `x` on stored
    /// row `row` (a lo-res dot passes the same colour twice).
    pub(crate) fn set_dot(&mut self, x: usize, row: usize, left: u16, right: u16) {
        if x >= FRAME_WIDTH || row >= MAX_FRAME_HEIGHT {
            return;
        }
        let i = row.saturating_mul(MAX_FRAME_WIDTH).saturating_add(x.saturating_mul(2));
        if let Some(pair) = self.pixels.get_mut(i..i.saturating_add(2)) {
            pair.copy_from_slice(&[left, right]);
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

    /// The picture as tightly packed RGBA bytes, `width() × height()`.
    #[must_use]
    pub fn to_rgba(&self) -> Vec<u8> {
        let (width, height) = (self.width(), self.height());
        let mut out = Vec::with_capacity(width.saturating_mul(height).saturating_mul(4));
        for y in 0..height {
            for x in 0..width {
                out.extend_from_slice(&Self::rgba(self.pixel(x, y)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lores_progressive_layout() {
        let mut f = Frame::new();
        f.set_dot(3, 0, 0x1234, 0x1234);
        f.set_dot(3, 2, 0x7FFF, 0x7FFF);
        assert_eq!((f.width(), f.height()), (256, 224));
        assert_eq!(f.pixel(3, 0), 0x1234);
        assert_eq!(f.pixel(3, 1), 0x7FFF);
        assert_eq!(f.pixel(300, 0), 0);
        assert_eq!(f.to_rgba().len(), 256 * 224 * 4);
    }

    #[test]
    fn hires_interlaced_layout() {
        let mut f = Frame::new();
        f.set_dot(0, 0, 1, 2);
        f.set_dot(0, 1, 3, 4);
        f.publish(239, true, true);
        assert_eq!((f.width(), f.height()), (512, 478));
        assert_eq!([f.pixel(0, 0), f.pixel(1, 0), f.pixel(0, 1), f.pixel(1, 1)], [1, 2, 3, 4]);
    }

    #[test]
    fn rgba_scaling() {
        assert_eq!(Frame::rgba(0x7FFF), [255, 255, 255, 255]);
        assert_eq!(Frame::rgba(0x001F), [255, 0, 0, 255]);
    }
}
