//! 64 KiB of word-addressed VRAM behind `$2115-$2119`/`$2139-$213A`.

use crate::bits::{hi, lo, w};

/// Address translation selected by `VMAIN` bits 2-3.
const fn remap(mode: u8, addr: u16) -> u16 {
    match mode & 3 {
        0 => addr,
        1 => (addr & 0xFF00) | ((addr & 0x1F) << 3) | ((addr >> 5) & 7),
        2 => (addr & 0xFE00) | ((addr & 0x3F) << 3) | ((addr >> 6) & 7),
        _ => (addr & 0xFC00) | ((addr & 0x7F) << 3) | ((addr >> 7) & 7),
    }
}

/// Address step selected by `VMAIN` bits 0-1.
const STEPS: [u16; 4] = [1, 32, 128, 128];

/// VRAM plus the access port state.
#[derive(Clone)]
pub struct Vram {
    words: Box<[u16]>,
    /// `VMADD`, 15 bits used.
    pub address: u16,
    /// `VMAIN` bit 7: increment after the high byte (`true`) or the low byte.
    pub increment_on_high: bool,
    step: u16,
    remap_mode: u8,
    /// The read prefetch latch.
    latch: u16,
}

impl core::fmt::Debug for Vram {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Vram").field("address", &self.address).finish_non_exhaustive()
    }
}

impl Default for Vram {
    fn default() -> Self {
        Self::new()
    }
}

impl Vram {
    /// Zeroed VRAM, address 0, increment 1 after the high byte.
    #[must_use]
    pub fn new() -> Self {
        Self {
            words: vec![0; 0x8000].into_boxed_slice(),
            address: 0,
            increment_on_high: true,
            step: 1,
            remap_mode: 0,
            latch: 0,
        }
    }

    /// The whole memory (32 K words), word addressed.
    #[must_use]
    pub fn words(&self) -> &[u16] {
        &self.words
    }

    /// Reads a word by (untranslated) word address.
    #[must_use]
    pub fn word(&self, addr: u16) -> u16 {
        self.words.get(usize::from(addr & 0x7FFF)).copied().unwrap_or(0)
    }

    /// `$2115 VMAIN`.
    pub fn set_main(&mut self, v: u8) {
        self.increment_on_high = v & 0x80 != 0;
        self.step = STEPS.get(usize::from(v & 3)).copied().unwrap_or(1);
        self.remap_mode = (v >> 2) & 3;
    }

    /// `$2116 VMADDL`: also refills the prefetch latch.
    pub fn set_address_low(&mut self, v: u8, readable: bool) {
        self.address = (self.address & 0x7F00) | w(v);
        self.prefetch(readable);
    }

    /// `$2117 VMADDH`.
    pub fn set_address_high(&mut self, v: u8, readable: bool) {
        self.address = (w(v & 0x7F) << 8) | (self.address & 0xFF);
        self.prefetch(readable);
    }

    fn translated(&self) -> usize {
        usize::from(remap(self.remap_mode, self.address) & 0x7FFF)
    }

    fn prefetch(&mut self, readable: bool) {
        self.latch =
            if readable { self.words.get(self.translated()).copied().unwrap_or(0) } else { 0 };
    }

    /// `$2118 VMDATAL`. Writes are dropped during active display.
    pub fn write_low(&mut self, v: u8, writable: bool) {
        if writable {
            let i = self.translated();
            if let Some(slot) = self.words.get_mut(i) {
                *slot = (*slot & 0xFF00) | w(v);
            }
        }
        if !self.increment_on_high {
            self.address = self.address.wrapping_add(self.step) & 0x7FFF;
        }
    }

    /// `$2119 VMDATAH`.
    pub fn write_high(&mut self, v: u8, writable: bool) {
        if writable {
            let i = self.translated();
            if let Some(slot) = self.words.get_mut(i) {
                *slot = (w(v) << 8) | (*slot & 0xFF);
            }
        }
        if self.increment_on_high {
            self.address = self.address.wrapping_add(self.step) & 0x7FFF;
        }
    }

    /// `$2139 VMDATALREAD`: returns the latched low byte; in
    /// increment-on-low mode refills the latch from the current address and
    /// then steps it (so the first read after setting the address repeats).
    pub fn read_low(&mut self, readable: bool) -> u8 {
        let v = lo(self.latch);
        if !self.increment_on_high {
            self.prefetch(readable);
            self.address = self.address.wrapping_add(self.step) & 0x7FFF;
        }
        v
    }

    /// `$213A VMDATAHREAD`.
    pub fn read_high(&mut self, readable: bool) -> u8 {
        let v = hi(self.latch);
        if self.increment_on_high {
            self.prefetch(readable);
            self.address = self.address.wrapping_add(self.step) & 0x7FFF;
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remap_modes() {
        assert_eq!(remap(0, 0x1234), 0x1234);
        // Mode 1 rotates the low 8 bits: aaaaaaaa YYYbbbbb -> aaaaaaaa bbbbbYYY.
        assert_eq!(remap(1, 0x0021), 0x0008 | 0x0001);
        assert_eq!(remap(2, 0x0041), 0x0008 | 0x0001);
        assert_eq!(remap(3, 0x0081), 0x0008 | 0x0001);
    }

    #[test]
    fn sequential_word_write_and_dummy_read() {
        let mut v = Vram::new();
        v.set_main(0x80);
        v.set_address_low(0x00, true);
        v.set_address_high(0x10, true);
        for (i, word) in [0x1122u16, 0x3344, 0x5566].into_iter().enumerate() {
            v.write_low(lo(word), true);
            v.write_high(hi(word), true);
            assert_eq!(v.word(0x1000 + u16::try_from(i).unwrap()), word);
        }
        v.set_address_low(0x00, true);
        v.set_address_high(0x10, true);
        // Dummy read returns the word at the address; data then lags by one.
        let read = |m: &mut Vram| u16::from_le_bytes([m.read_low(true), m.read_high(true)]);
        assert_eq!(read(&mut v), 0x1122);
        assert_eq!(read(&mut v), 0x1122);
        assert_eq!(read(&mut v), 0x3344);
        assert_eq!(read(&mut v), 0x5566);
    }

    #[test]
    fn writes_blocked_during_display_still_step() {
        let mut v = Vram::new();
        v.set_main(0x80);
        v.write_low(0xAA, false);
        v.write_high(0xBB, false);
        assert_eq!(v.word(0), 0);
        assert_eq!(v.address, 1);
    }
}
