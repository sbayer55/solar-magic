//! The SNES audio processing unit as seen from the S-CPU: an SPC700 core,
//! 64 KiB of RAM, the `$F0-$FF` registers (timers, DSP port, CPU ports), the
//! boot-ROM handshake, and (later) the S-DSP. `docs/emulation/apu.md`.
//!
//! The APU runs on its own 24.576 MHz crystal. [`Apu::run_to`] converts the
//! S-CPU's master-clock position into SPC cycles exactly
//! (`master × 1 024 000 / 21 477 272`), so the two clocks never drift.

mod dsp;
mod ipl;
mod smp;
mod timer;

use spc700::Spc700;

pub use dsp::DspRegs;
pub use ipl::{Ipl, IplPhase};
pub use smp::Smp;

/// Low byte of a word.
#[allow(clippy::as_conversions, reason = "masked truncation")]
pub(crate) const fn lo(v: u16) -> u8 {
    (v & 0xFF) as u8
}

/// SPC cycles per master cycle, as a fraction: `SPC_NUM / SPC_DEN`.
const SPC_NUM: u64 = 128_000;
/// Denominator of the clock ratio (21 477 272 / 8).
const SPC_DEN: u64 = 2_684_659;

/// The audio unit.
#[derive(Debug, Clone)]
pub struct Apu {
    /// The SPC700 core.
    pub cpu: Spc700,
    /// Memory, registers, timers, ports.
    pub smp: Smp,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new(Ipl::Hle)
    }
}

impl Apu {
    /// Power-on with the given boot ROM strategy.
    #[must_use]
    pub fn new(ipl: Ipl) -> Self {
        let mut apu = Self { cpu: Spc700::new(), smp: Smp::new(ipl) };
        apu.reset();
        apu
    }

    /// Reset: RAM keeps its contents, registers and the core restart.
    pub fn reset(&mut self) {
        self.smp.reset();
        self.cpu = Spc700::new();
        self.cpu.reset(&mut self.smp);
    }

    /// SPC cycles that should have elapsed at `master_cycles`.
    #[must_use]
    pub const fn target_cycles(master_cycles: u64) -> u64 {
        master_cycles.wrapping_mul(SPC_NUM) / SPC_DEN
    }

    /// Runs the APU until it has caught up with the S-CPU's master clock.
    pub fn run_to(&mut self, master_cycles: u64) {
        let target = Self::target_cycles(master_cycles);
        while self.smp.cycles < target {
            if self.smp.in_boot_rom(self.cpu.pc) {
                ipl::step(&mut self.cpu, &mut self.smp);
            } else if self.cpu.stopped {
                self.smp.idle_cycles(target.saturating_sub(self.smp.cycles));
            } else {
                self.cpu.step(&mut self.smp);
            }
        }
    }

    /// S-CPU write to `$2140 + port`.
    pub fn cpu_write_port(&mut self, port: u8, v: u8) {
        self.smp.cpu_write_port(port, v);
    }

    /// S-CPU read of `$2140 + port`.
    #[must_use]
    pub fn cpu_read_port(&self, port: u8) -> u8 {
        self.smp.cpu_read_port(port)
    }

    /// SPC cycles executed so far.
    #[must_use]
    pub const fn cycles(&self) -> u64 {
        self.smp.cycles
    }
}
