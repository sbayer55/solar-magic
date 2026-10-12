//! Memory abstraction.

use crate::{Addr, addr24};

/// Something the CPU can read and write bytes through.
pub trait Bus {
    /// Reads one byte. Unmapped addresses should return a deterministic value
    /// (open bus or zero) rather than fail.
    fn read(&mut self, addr: Addr) -> u8;

    /// Writes one byte. Writes to ROM or unmapped space may be ignored.
    fn write(&mut self, addr: Addr, value: u8);

    /// One internal CPU cycle with no bus activity. Untimed buses ignore it;
    /// a timed bus charges the CPU's internal-cycle cost and lets the rest
    /// of the machine catch up.
    fn idle(&mut self) {}

    /// Reads a little-endian 16-bit word at `addr` and `addr + 1` (no wrap handling).
    fn read_u16(&mut self, addr: Addr) -> u16 {
        let lo = self.read(addr);
        let hi = self.read(addr24(addr.wrapping_add(1)));
        u16::from_le_bytes([lo, hi])
    }

    /// Writes a little-endian 16-bit word.
    fn write_u16(&mut self, addr: Addr, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        self.write(addr, lo);
        self.write(addr24(addr.wrapping_add(1)), hi);
    }

    /// Reads a little-endian 24-bit value.
    fn read_u24(&mut self, addr: Addr) -> u32 {
        let lo = self.read(addr);
        let mid = self.read(addr24(addr.wrapping_add(1)));
        let hi = self.read(addr24(addr.wrapping_add(2)));
        u32::from_le_bytes([lo, mid, hi, 0])
    }
}

/// A flat 16 MiB RAM, handy for tests.
#[derive(Clone)]
pub struct FlatBus {
    mem: Vec<u8>,
}

impl core::fmt::Debug for FlatBus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FlatBus").field("len", &self.mem.len()).finish()
    }
}

impl Default for FlatBus {
    fn default() -> Self {
        Self::new()
    }
}

impl FlatBus {
    /// Allocates a zeroed 16 MiB address space.
    #[must_use]
    pub fn new() -> Self {
        Self { mem: vec![0; 1 << 24] }
    }

    /// Copies `bytes` to `addr`, truncating at the end of the address space.
    pub fn load(&mut self, addr: Addr, bytes: &[u8]) {
        for (i, &b) in bytes.iter().enumerate() {
            let a =
                usize::try_from(addr24(u32::try_from(i).unwrap_or(u32::MAX).wrapping_add(addr)))
                    .unwrap_or(0);
            if let Some(slot) = self.mem.get_mut(a) {
                *slot = b;
            }
        }
    }
}

impl Bus for FlatBus {
    fn read(&mut self, addr: Addr) -> u8 {
        self.mem.get(usize::try_from(addr24(addr)).unwrap_or(0)).copied().unwrap_or(0)
    }

    fn write(&mut self, addr: Addr, value: u8) {
        if let Some(slot) = self.mem.get_mut(usize::try_from(addr24(addr)).unwrap_or(0)) {
            *slot = value;
        }
    }
}
