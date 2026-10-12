//! 256 BGR555 palette entries behind `$2121`/`$2122`/`$213B`.

use crate::bits::{hi, lo, w};

/// Palette memory and its byte-pair access port.
#[derive(Clone)]
pub struct Cgram {
    colors: Box<[u16; 256]>,
    /// 9-bit byte address.
    address: u16,
    latch: u8,
}

impl core::fmt::Debug for Cgram {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Cgram").field("address", &self.address).finish_non_exhaustive()
    }
}

impl Default for Cgram {
    fn default() -> Self {
        Self::new()
    }
}

impl Cgram {
    /// Black palette.
    #[must_use]
    pub fn new() -> Self {
        Self { colors: Box::new([0; 256]), address: 0, latch: 0 }
    }

    /// All 256 colors.
    #[must_use]
    pub fn colors(&self) -> &[u16; 256] {
        &self.colors
    }

    /// One color (15 bits).
    #[must_use]
    pub fn color(&self, index: u8) -> u16 {
        self.colors.get(usize::from(index)).copied().unwrap_or(0)
    }

    /// `$2121 CGADD`: selects a color and restarts the byte pair.
    pub const fn set_address(&mut self, v: u8) {
        self.address = w(v) << 1;
    }

    /// `$2122 CGDATA`: low byte is latched, the high byte (7 bits) commits.
    pub fn write(&mut self, v: u8) {
        if self.address & 1 == 0 {
            self.latch = v;
        } else if let Some(slot) = self.colors.get_mut(usize::from(self.address >> 1)) {
            *slot = (w(v & 0x7F) << 8) | w(self.latch);
        }
        self.address = self.address.wrapping_add(1) & 0x1FF;
    }

    /// `$213B CGDATAREAD`: low byte, then high byte with bit 7 from open bus.
    pub fn read(&mut self, mdr: u8) -> u8 {
        let color = self.color(lo(self.address >> 1));
        let v = if self.address & 1 == 0 { lo(color) } else { (mdr & 0x80) | hi(color) };
        self.address = self.address.wrapping_add(1) & 0x1FF;
        v
    }
}

#[cfg(test)]
mod tests {
    use super::Cgram;

    #[test]
    fn write_pairs_and_read_back() {
        let mut c = Cgram::new();
        c.set_address(3);
        c.write(0x34);
        c.write(0xFF); // bit 7 is dropped
        assert_eq!(c.color(3), 0x7F34);
        c.set_address(3);
        assert_eq!(c.read(0), 0x34);
        assert_eq!(c.read(0x80), 0xFF);
    }
}
