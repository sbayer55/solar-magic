//! Battery-backed RAM, mirrored over its size across the SRAM banks.

/// Save RAM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sram {
    bytes: Vec<u8>,
    /// Set by any write since the last `clear_dirty`.
    dirty: bool,
}

impl Sram {
    /// `len` bytes of `0xFF` (an unformatted battery RAM reads as `FF`).
    #[must_use]
    pub fn new(len: usize) -> Self {
        Self { bytes: vec![0xFF; len], dirty: false }
    }

    /// Replaces the contents (e.g. from a `.srm` file); extra bytes are
    /// dropped, missing bytes stay `FF`.
    pub fn load(&mut self, data: &[u8]) {
        for (slot, &b) in self.bytes.iter_mut().zip(data) {
            *slot = b;
        }
        self.dirty = false;
    }

    /// The contents.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether anything was written since the last `clear_dirty`.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the contents as saved.
    pub const fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Index for a LoROM SRAM address: 32 KiB per bank, mirrored over the size.
    fn index(&self, addr: u32) -> Option<usize> {
        if self.bytes.is_empty() {
            return None;
        }
        let bank = usize::from(crate::bits::bank(addr) & 0x0F);
        let within = usize::try_from(addr & 0x7FFF).ok()?;
        ((bank << 15) | within).checked_rem(self.bytes.len())
    }

    /// Reads; `None` when the cartridge has no SRAM (open bus).
    #[must_use]
    pub fn read(&self, addr: u32) -> Option<u8> {
        self.bytes.get(self.index(addr)?).copied()
    }

    /// Writes (ignored without SRAM).
    pub fn write(&mut self, addr: u32, v: u8) {
        if let Some(i) = self.index(addr)
            && let Some(slot) = self.bytes.get_mut(i)
        {
            *slot = v;
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Sram;

    #[test]
    fn two_kib_mirrors_across_the_bank() {
        let mut s = Sram::new(0x800);
        s.write(0x70_0000, 0x12);
        assert_eq!(s.read(0x70_0800), Some(0x12));
        assert_eq!(s.read(0x71_0000), Some(0x12));
        assert_eq!(s.read(0xF0_0000), Some(0x12));
        assert!(s.is_dirty());
        assert_eq!(Sram::new(0).read(0x70_0000), None);
    }
}
