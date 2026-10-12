//! 544 bytes of sprite attributes behind `$2102-$2104`/`$2138`.

use crate::bits::{lo, w};

/// Object attribute memory and its access port.
#[derive(Clone)]
pub struct Oam {
    low: Box<[u8; 512]>,
    high: [u8; 32],
    /// 10-bit byte address, auto-incremented by the data port.
    pub address: u16,
    /// `OAMADD` as written: reloaded into `address` at vblank.
    base_address: u16,
    /// `OAMADDH` bit 7: rotate sprite priority around `first_sprite`.
    priority_rotation: bool,
    /// Index of the highest-priority sprite.
    pub first_sprite: u8,
    latch: u8,
}

impl core::fmt::Debug for Oam {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Oam").field("address", &self.address).finish_non_exhaustive()
    }
}

impl Default for Oam {
    fn default() -> Self {
        Self::new()
    }
}

impl Oam {
    /// Zeroed OAM.
    #[must_use]
    pub fn new() -> Self {
        Self {
            low: Box::new([0; 512]),
            high: [0; 32],
            address: 0,
            base_address: 0,
            priority_rotation: false,
            first_sprite: 0,
            latch: 0,
        }
    }

    /// The 512-byte low table (4 bytes per sprite).
    #[must_use]
    pub fn low(&self) -> &[u8; 512] {
        &self.low
    }

    /// The 32-byte high table (2 bits per sprite).
    #[must_use]
    pub const fn high(&self) -> &[u8; 32] {
        &self.high
    }

    /// Reads any of the 544 bytes by 10-bit address (the high table repeats).
    #[must_use]
    pub fn byte(&self, address: u16) -> u8 {
        if address & 0x200 == 0 {
            self.low.get(usize::from(address & 0x1FF)).copied().unwrap_or(0)
        } else {
            self.high.get(usize::from(address & 0x1F)).copied().unwrap_or(0)
        }
    }

    fn store(&mut self, address: u16, v: u8) {
        let slot = if address & 0x200 == 0 {
            self.low.get_mut(usize::from(address & 0x1FF))
        } else {
            self.high.get_mut(usize::from(address & 0x1F))
        };
        if let Some(slot) = slot {
            *slot = v;
        }
    }

    /// `$2102 OAMADDL`.
    pub const fn set_address_low(&mut self, v: u8) {
        self.base_address = (self.base_address & 0x200) | (w(v) << 1);
        self.reload_address();
    }

    /// `$2103 OAMADDH`.
    pub const fn set_address_high(&mut self, v: u8) {
        self.base_address = (w(v & 1) << 9) | (self.base_address & 0x1FF);
        self.priority_rotation = v & 0x80 != 0;
        self.reload_address();
    }

    /// Resets the access address to `OAMADD` (at vblank start, when forced
    /// blank toggles there, and on every `OAMADD` write).
    pub const fn reload_address(&mut self) {
        self.address = self.base_address;
        self.first_sprite = if self.priority_rotation { lo((self.address >> 2) & 0x7F) } else { 0 };
    }

    /// `$2104 OAMDATA`: low-table bytes are written in latched pairs, high
    /// table bytes immediately.
    pub fn write(&mut self, v: u8) {
        let address = self.address;
        self.address = address.wrapping_add(1) & 0x3FF;
        if address & 1 == 0 {
            self.latch = v;
        }
        if address & 0x200 != 0 {
            self.store(address, v);
        } else if address & 1 == 1 {
            self.store(address & !1, self.latch);
            self.store(address, v);
        }
    }

    /// `$2138 OAMDATAREAD`.
    pub fn read(&mut self) -> u8 {
        let v = self.byte(self.address);
        self.address = self.address.wrapping_add(1) & 0x3FF;
        v
    }
}

#[cfg(test)]
mod tests {
    use super::Oam;

    #[test]
    fn low_table_pairs_and_high_table_bytes() {
        let mut o = Oam::new();
        o.set_address_low(0);
        o.set_address_high(0);
        o.write(0x11); // latched only
        assert_eq!(o.byte(0), 0);
        o.write(0x22);
        assert_eq!((o.byte(0), o.byte(1)), (0x11, 0x22));
        o.set_address_low(0);
        o.set_address_high(1);
        o.write(0xAB);
        assert_eq!(o.byte(0x200), 0xAB);
        assert_eq!(o.byte(0x220), 0xAB); // mirror
    }

    #[test]
    fn priority_rotation_sets_first_sprite() {
        let mut o = Oam::new();
        o.set_address_low(0x10); // byte address 0x20 = sprite 8
        o.set_address_high(0x80);
        assert_eq!(o.first_sprite, 8);
        o.set_address_high(0x00);
        assert_eq!(o.first_sprite, 0);
    }
}
