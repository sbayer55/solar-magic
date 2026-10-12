//! S-DSP register file. Milestone E2 only stores the 128 registers; voice
//! processing, echo and output arrive with E6.

/// The 128 DSP registers behind `$F2`/`$F3`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DspRegs {
    regs: [u8; 128],
}

impl Default for DspRegs {
    fn default() -> Self {
        Self::new()
    }
}

impl DspRegs {
    /// Power-on: zero, except `FLG` (`$6C`) = `$E0` (reset, mute, echo off).
    #[must_use]
    pub const fn new() -> Self {
        let mut regs = [0u8; 128];
        regs[0x6C] = 0xE0;
        Self { regs }
    }

    /// Reads a register (`$80-$FF` mirror `$00-$7F`).
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        self.regs.get(usize::from(addr & 0x7F)).copied().unwrap_or(0)
    }

    /// Writes a register; the mirror half is read-only.
    pub fn write(&mut self, addr: u8, v: u8) {
        if addr & 0x80 != 0 {
            return;
        }
        if let Some(slot) = self.regs.get_mut(usize::from(addr)) {
            *slot = v;
        }
    }
}
