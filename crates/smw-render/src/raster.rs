//! RGBA8 pixel buffer.

/// One RGBA8 pixel.
pub type Rgba8 = [u8; 4];

/// Fully transparent black.
pub const TRANSPARENT: Rgba8 = [0, 0, 0, 0];

/// An axis-aligned rectangle in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

/// A `width × height` RGBA8 image, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    width: u32,
    height: u32,
    pixels: Vec<Rgba8>,
}

impl Raster {
    /// Creates a transparent raster. Dimensions that overflow `usize` yield
    /// an empty buffer.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let len = usize::try_from(u64::from(width).saturating_mul(u64::from(height))).unwrap_or(0);
        Self { width, height, pixels: vec![TRANSPARENT; len] }
    }

    /// Creates a raster filled with one colour.
    #[must_use]
    pub fn filled(width: u32, height: u32, color: Rgba8) -> Self {
        let mut r = Self::new(width, height);
        r.pixels.fill(color);
        r
    }

    /// Width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The pixels, row-major.
    #[must_use]
    pub fn pixels(&self) -> &[Rgba8] {
        &self.pixels
    }

    /// The pixels flattened to `R G B A` bytes, as image encoders want them.
    #[must_use]
    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        self.pixels.iter().flatten().copied().collect()
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        usize::try_from(
            u64::from(y).saturating_mul(u64::from(self.width)).saturating_add(u64::from(x)),
        )
        .ok()
    }

    /// Pixel at `(x, y)`, or `None` out of range.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Option<Rgba8> {
        self.pixels.get(self.index(x, y)?).copied()
    }

    /// Sets one pixel; out-of-range writes are ignored.
    pub fn put(&mut self, x: u32, y: u32, color: Rgba8) {
        if let Some(i) = self.index(x, y)
            && let Some(px) = self.pixels.get_mut(i)
        {
            *px = color;
        }
    }

    /// Fills a rectangle, clipped to the raster.
    pub fn fill_rect(&mut self, rect: Rect, color: Rgba8) {
        for yy in rect.y..rect.y.saturating_add(rect.h).min(self.height) {
            for xx in rect.x..rect.x.saturating_add(rect.w).min(self.width) {
                self.put(xx, yy, color);
            }
        }
    }

    /// Returns a copy scaled up by an integer factor (nearest neighbour).
    #[must_use]
    pub fn scaled(&self, factor: u32) -> Self {
        let factor = factor.max(1);
        let mut out =
            Self::new(self.width.saturating_mul(factor), self.height.saturating_mul(factor));
        for y in 0..out.height {
            for x in 0..out.width {
                let (sx, sy) =
                    (x.checked_div(factor).unwrap_or(0), y.checked_div(factor).unwrap_or(0));
                if let Some(c) = self.get(sx, sy) {
                    out.put(x, y, c);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::Raster;

    #[test]
    fn put_get_and_bounds() {
        let mut r = Raster::new(4, 3);
        r.put(3, 2, [1, 2, 3, 4]);
        r.put(4, 0, [9, 9, 9, 9]);
        assert_eq!(r.get(3, 2), Some([1, 2, 3, 4]));
        assert_eq!(r.get(4, 0), None);
        assert_eq!(r.pixels().len(), 12);
        assert_eq!(r.to_rgba_bytes().len(), 48);
    }

    #[test]
    fn scaling() {
        let mut r = Raster::new(2, 1);
        r.put(1, 0, [255, 0, 0, 255]);
        let s = r.scaled(3);
        assert_eq!((s.width(), s.height()), (6, 3));
        assert_eq!(s.get(2, 2), Some([0, 0, 0, 0]));
        assert_eq!(s.get(3, 2), Some([255, 0, 0, 255]));
    }
}
