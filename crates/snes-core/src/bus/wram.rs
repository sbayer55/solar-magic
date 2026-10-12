//! 128 KiB of work RAM and the `$2180-$2183` access port.

use crate::bits::w;

/// Size of work RAM.
pub const WRAM_LEN: usize = 0x2_0000;

/// Work RAM.
#[derive(Clone)]
pub struct Wram {
    bytes: Box<[u8]>,
    /// `WMADD`, 17 bits used.
    address: u32,
}

impl core::fmt::Debug for Wram {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Wram").field("address", &self.address).finish_non_exhaustive()
    }
}

impl Default for Wram {
    fn default() -> Self {
        Self::new()
    }
}

impl Wram {
    /// Zeroed RAM. (Hardware powers up with garbage; zero keeps runs deterministic.)
    #[must_use]
    pub fn new() -> Self {
        Self { bytes: vec![0; WRAM_LEN].into_boxed_slice(), address: 0 }
    }

    /// Whole memory, `$7E:0000` first.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Reads by 17-bit offset.
    #[must_use]
    pub fn read(&self, offset: u32) -> u8 {
        self.bytes.get(usize::try_from(offset & 0x1_FFFF).unwrap_or(0)).copied().unwrap_or(0)
    }

    /// Writes by 17-bit offset.
    pub fn write(&mut self, offset: u32, v: u8) {
        if let Some(slot) = self.bytes.get_mut(usize::try_from(offset & 0x1_FFFF).unwrap_or(0)) {
            *slot = v;
        }
    }

    /// `$2180 WMDATA` read: the byte at `WMADD`, then increments.
    pub fn port_read(&mut self) -> u8 {
        let v = self.read(self.address);
        self.address = self.address.wrapping_add(1) & 0x1_FFFF;
        v
    }

    /// `$2180 WMDATA` write.
    pub fn port_write(&mut self, v: u8) {
        self.write(self.address, v);
        self.address = self.address.wrapping_add(1) & 0x1_FFFF;
    }

    /// `$2181 WMADDL`.
    pub fn set_address_low(&mut self, v: u8) {
        self.address = (self.address & 0x1_FF00) | u32::from(w(v));
    }

    /// `$2182 WMADDM`.
    pub fn set_address_mid(&mut self, v: u8) {
        self.address = (self.address & 0x1_00FF) | (u32::from(w(v)) << 8);
    }

    /// `$2183 WMADDH` (one bit).
    pub fn set_address_high(&mut self, v: u8) {
        self.address = (self.address & 0x0_FFFF) | (u32::from(v & 1) << 16);
    }
}

#[cfg(test)]
mod tests {
    use super::Wram;

    #[test]
    fn port_autoincrements_through_17_bits() {
        let mut w = Wram::new();
        w.set_address_low(0xFF);
        w.set_address_mid(0xFF);
        w.set_address_high(0x01);
        w.port_write(0xAA);
        w.port_write(0xBB);
        assert_eq!(w.read(0x1_FFFF), 0xAA);
        assert_eq!(w.read(0x0_0000), 0xBB);
        w.set_address_low(0xFF);
        w.set_address_mid(0xFF);
        w.set_address_high(0x01);
        assert_eq!(w.port_read(), 0xAA);
        assert_eq!(w.port_read(), 0xBB);
    }
}
