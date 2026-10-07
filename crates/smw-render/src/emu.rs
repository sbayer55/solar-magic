//! A SNES-shaped bus for running the game's own routines: ROM through the
//! cartridge mapper, 128 KiB of WRAM with its low mirror, and hardware
//! registers that read as zero and swallow writes.

use snes_rom::{Rom, SnesAddr};
use w65c816::{Addr, Bus};

/// Size of work RAM.
pub const WRAM_LEN: usize = 0x2_0000;

/// ROM plus WRAM, no hardware.
pub struct SnesBus<'a> {
    rom: &'a Rom,
    wram: Vec<u8>,
    hardware_writes: u64,
}

impl core::fmt::Debug for SnesBus<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SnesBus")
            .field("hardware_writes", &self.hardware_writes)
            .finish_non_exhaustive()
    }
}

impl<'a> SnesBus<'a> {
    /// Creates a bus over `rom` with zeroed WRAM.
    #[must_use]
    pub fn new(rom: &'a Rom) -> Self {
        Self { rom, wram: vec![0; WRAM_LEN], hardware_writes: 0 }
    }

    /// WRAM as a slice (`$7E:0000` is index 0).
    #[must_use]
    pub fn wram(&self) -> &[u8] {
        &self.wram
    }

    /// Writes to hardware registers seen so far (useful to confirm a routine
    /// stayed in pure RAM).
    #[must_use]
    pub const fn hardware_writes(&self) -> u64 {
        self.hardware_writes
    }

    /// Reads a WRAM byte by 24-bit address, if it is WRAM.
    #[must_use]
    pub fn wram_at(&self, addr: Addr) -> Option<u8> {
        wram_index(addr).and_then(|i| self.wram.get(i).copied())
    }

    /// Reads a 24-bit little-endian value from WRAM.
    #[must_use]
    pub fn wram_u24(&self, addr: Addr) -> Option<u32> {
        let lo = self.wram_at(addr)?;
        let mid = self.wram_at(addr.wrapping_add(1))?;
        let hi = self.wram_at(addr.wrapping_add(2))?;
        Some(u32::from_le_bytes([lo, mid, hi, 0]))
    }

    /// Stores a byte in WRAM by 24-bit address (ignored if not WRAM).
    pub fn set_wram(&mut self, addr: Addr, value: u8) {
        if let Some(i) = wram_index(addr)
            && let Some(slot) = self.wram.get_mut(i)
        {
            *slot = value;
        }
    }

    /// Stores a 24-bit little-endian value in WRAM.
    pub fn set_wram_u24(&mut self, addr: Addr, value: u32) {
        let [lo, mid, hi, _] = value.to_le_bytes();
        self.set_wram(addr, lo);
        self.set_wram(addr.wrapping_add(1), mid);
        self.set_wram(addr.wrapping_add(2), hi);
    }
}

/// Maps a bus address to a WRAM index: banks `$7E`/`$7F` fully, plus the
/// `$0000-$1FFF` mirror in banks `$00-$3F` and `$80-$BF`.
fn wram_index(addr: Addr) -> Option<usize> {
    let bank = (addr >> 16) & 0xFF;
    let offset = addr & 0xFFFF;
    match bank {
        0x7E => usize::try_from(offset).ok(),
        0x7F => usize::try_from(offset | 0x1_0000).ok(),
        0x00..=0x3F | 0x80..=0xBF if offset < 0x2000 => usize::try_from(offset).ok(),
        _ => None,
    }
}

/// Whether an address is a hardware register (B bus, CPU, DMA).
const fn is_hardware(addr: Addr) -> bool {
    let bank = (addr >> 16) & 0xFF;
    let offset = addr & 0xFFFF;
    matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && offset >= 0x2000 && offset < 0x6000
}

impl Bus for SnesBus<'_> {
    fn read(&mut self, addr: Addr) -> u8 {
        if let Some(v) = self.wram_at(addr) {
            return v;
        }
        if is_hardware(addr) {
            return 0;
        }
        SnesAddr::new(addr & 0xFF_FFFF)
            .and_then(|a| self.rom.pc(a).ok())
            .and_then(|pc| self.rom.bytes().get(pc).copied())
            .unwrap_or(0)
    }

    fn write(&mut self, addr: Addr, value: u8) {
        if wram_index(addr).is_some() {
            self.set_wram(addr, value);
        } else if is_hardware(addr) {
            self.hardware_writes = self.hardware_writes.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::wram_index;

    #[test]
    fn wram_mapping() {
        assert_eq!(wram_index(0x7E_0000), Some(0));
        assert_eq!(wram_index(0x7F_C800), Some(0x1_C800));
        assert_eq!(wram_index(0x00_0065), Some(0x65));
        assert_eq!(wram_index(0x80_1FFF), Some(0x1FFF));
        assert_eq!(wram_index(0x00_2000), None);
        assert_eq!(wram_index(0x05_E000), None);
    }
}
