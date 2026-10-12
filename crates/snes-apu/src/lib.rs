//! The SNES audio processing unit as seen from the S-CPU.
//!
//! Milestone E1 stub: the four communication ports exist and keep what the
//! CPU wrote, but nothing answers. The SPC700 core, timers, IPL handshake and
//! S-DSP arrive in E2/E6 (`docs/emulation/apu.md`).

/// The APU.
#[derive(Debug, Clone, Default)]
pub struct Apu {
    /// Bytes written by the S-CPU to `$2140-$2143` (APU side `$F4-$F7` reads).
    inbox: [u8; 4],
    /// Bytes the APU presents on `$2140-$2143` reads.
    outbox: [u8; 4],
    /// Master-clock position the APU has been run to.
    cycles: u64,
}

impl Apu {
    /// Power-on state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Runs the APU until it has consumed `master_cycles` of SNES master
    /// clock. The stub only records the target.
    pub const fn run_to(&mut self, master_cycles: u64) {
        self.cycles = master_cycles;
    }

    /// S-CPU write to `$2140 + port`.
    pub fn cpu_write_port(&mut self, port: u8, v: u8) {
        if let Some(slot) = self.inbox.get_mut(usize::from(port & 3)) {
            *slot = v;
        }
    }

    /// S-CPU read of `$2140 + port`.
    #[must_use]
    pub fn cpu_read_port(&self, port: u8) -> u8 {
        self.outbox.get(usize::from(port & 3)).copied().unwrap_or(0)
    }

    /// Master-clock position the APU has been run to.
    #[must_use]
    pub const fn cycles(&self) -> u64 {
        self.cycles
    }
}
