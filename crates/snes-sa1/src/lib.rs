//! The SA-1: a 10.74 MHz 65c816 on the cartridge with 2 KiB of I-RAM, up
//! to 256 KiB of BW-RAM, the Super MMC bank registers, DMA and character
//! conversion, a multiplier/divider and a variable-length bit reader.
//! fullsnes "SNES Cart SA-1" (read 2026-10-12) and ares `sfc/coprocessor/sa1`
//! for the behaviours fullsnes leaves open (interrupt latches, write
//! protection, conversion layouts).
//!
//! The SA-1 keeps its own clock in master cycles (one SA-1 cycle = 2) and
//! catches up to the S-CPU on demand: before the S-CPU touches shared
//! memory or a register, and at every line start. It may overshoot the
//! target by one instruction. Bus-arbitration stalls between the two CPUs
//! are not modelled.

mod bits;
mod dma;
mod io_cpu;
mod io_sa1;
mod math;
mod mem;
mod regs;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use w65c816::{Bus, Cpu, Exec};

use regs::Io;

/// I-RAM size.
pub const IRAM_LEN: usize = 0x800;
/// Largest BW-RAM.
pub const BWRAM_MAX: usize = 0x4_0000;

/// Master cycles per SA-1 cycle.
const CYCLE: u64 = 2;

/// Which processor is accessing the chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    /// The S-CPU (SNES side).
    Cpu,
    /// The SA-1 itself.
    Sa1,
}

/// Everything but the SA-1's CPU core: what its bus sees.
#[derive(Debug, Clone)]
pub struct Chip {
    /// Registers.
    pub(crate) io: Io,
    rom: Arc<[u8]>,
    rom_mask: Option<usize>,
    iram: Vec<u8>,
    bwram: Vec<u8>,
    /// SA-1 time in master cycles.
    master: u64,
    /// H/V timer counters (master cycles within the line, lines).
    hcounter: u16,
    vcounter: u16,
    scanlines: u16,
    /// Character conversion 2 line counter.
    cc2_line: u8,
    /// Vector supplied for the current interrupt entry (ares `r.vector`).
    vector: Option<u16>,
    /// `CCNT` bit 5 went low: reset the SA-1 CPU before its next step.
    pending_reset: bool,
}

/// The SA-1 coprocessor.
#[derive(Debug, Clone)]
pub struct Sa1 {
    cpu: Cpu,
    /// Memories and registers.
    pub chip: Chip,
}

impl Sa1 {
    /// A powered-on SA-1 over `rom` with `bwram_len` bytes of BW-RAM.
    #[must_use]
    pub fn new(rom: Arc<[u8]>, bwram_len: usize, pal: bool) -> Self {
        let len = rom.len();
        let rom_mask = if len.is_power_of_two() { Some(len.wrapping_sub(1)) } else { None };
        let chip = Chip {
            io: Io::new(),
            rom,
            rom_mask,
            iram: vec![0; IRAM_LEN],
            bwram: vec![0; bwram_len.min(BWRAM_MAX)],
            master: 0,
            hcounter: 0,
            vcounter: 0,
            scanlines: if pal { 312 } else { 262 },
            cc2_line: 0,
            vector: None,
            pending_reset: false,
        };
        Self { cpu: Cpu::new(), chip }
    }

    /// Reset: registers return to power-on values (the SA-1 CPU is held in
    /// reset until the S-CPU releases `CCNT` bit 5); memories are kept.
    pub fn reset(&mut self, now: u64) {
        self.chip.io = Io::new();
        self.chip.master = now;
        self.chip.hcounter = 0;
        self.chip.vcounter = 0;
        self.chip.cc2_line = 0;
        self.chip.vector = None;
        self.chip.pending_reset = false;
        self.cpu = Cpu::new();
    }

    /// The SA-1's CPU.
    #[must_use]
    pub const fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// Whether the chip asserts the S-CPU's IRQ line.
    #[must_use]
    pub const fn irq_pending(&self) -> bool {
        let io = &self.chip.io;
        (io.cpu_irqfl && io.cpu_irqen) || (io.chdma_irqfl && io.chdma_irqen)
    }

    /// A side-effect-free read of `addr` as the S-CPU would see it
    /// (memories only).
    #[must_use]
    pub fn peek(&self, addr: u32) -> Option<u8> {
        self.chip.peek_cpu(addr)
    }

    /// An S-CPU read of `addr`, if the chip decodes it.
    pub fn cpu_read(&mut self, addr: u32, now: u64) -> Option<u8> {
        if mem::is_shared(addr) {
            self.run_to(now);
        }
        self.chip.read_cpu(addr)
    }

    /// An S-CPU write of `addr`; returns whether the chip decoded it.
    pub fn cpu_write(&mut self, addr: u32, v: u8, now: u64) -> bool {
        if mem::is_shared(addr) {
            self.run_to(now);
        }
        let claimed = self.chip.write_cpu(addr, v);
        if self.chip.pending_reset {
            self.chip.pending_reset = false;
            self.cpu = Cpu::new();
            self.cpu.reset(&mut self.chip);
        }
        claimed
    }

    /// Runs the SA-1 until its clock reaches `now` (master cycles).
    pub fn run_to(&mut self, now: u64) {
        while self.chip.master < now {
            if self.chip.io.sa1_rdyb || self.chip.io.sa1_resb || self.cpu.stopped {
                self.chip.step();
                continue;
            }
            self.service_interrupts();
            if self.cpu.waiting {
                self.chip.step();
                continue;
            }
            match self.cpu.step(&mut self.chip) {
                Ok(Exec::Ran | Exec::Waiting) => {}
                Err(_) => self.chip.idle(),
            }
        }
    }

    /// Takes a pending NMI or IRQ (ares `SA1::lastCycle`): the `*cl`
    /// latches are clear while an interrupt is pending.
    fn service_interrupts(&mut self) {
        let io = &mut self.chip.io;
        if io.sa1_nmi && !io.sa1_nmicl {
            io.sa1_nmifl = true;
            io.sa1_nmicl = true;
            let vector = io.cnv;
            self.chip.vector = Some(vector);
            self.cpu.nmi(&mut self.chip);
            self.chip.vector = None;
            return;
        }
        if self.cpu.p.i {
            return;
        }
        let pending = if io.timer_irqen && !io.timer_irqcl {
            io.timer_irqfl = true;
            true
        } else if io.dma_irqen && !io.dma_irqcl {
            io.dma_irqfl = true;
            true
        } else if io.sa1_irq && !io.sa1_irqcl {
            io.sa1_irqfl = true;
            true
        } else {
            false
        };
        if pending {
            let vector = io.civ;
            self.chip.vector = Some(vector);
            self.cpu.irq(&mut self.chip);
            self.chip.vector = None;
        }
    }
}

impl Chip {
    /// I-RAM contents.
    #[must_use]
    pub fn iram(&self) -> &[u8] {
        &self.iram
    }

    /// BW-RAM contents (battery backed on most boards).
    #[must_use]
    pub fn bwram(&self) -> &[u8] {
        &self.bwram
    }

    /// BW-RAM contents, writable (loading a save file).
    pub fn bwram_mut(&mut self) -> &mut [u8] {
        &mut self.bwram
    }

    /// The SA-1's clock in master cycles.
    #[must_use]
    pub const fn master(&self) -> u64 {
        self.master
    }

    /// One SA-1 cycle: advances the clock and the H/V timer.
    pub(crate) const fn step(&mut self) {
        self.master = self.master.wrapping_add(CYCLE);
        self.tick_timer();
    }

    /// Timer counters in master cycles (4 per dot) and lines, HV or linear
    /// (ares `SA1::step`).
    const fn tick_timer(&mut self) {
        if self.io.hvselb {
            self.hcounter = self.hcounter.wrapping_add(2);
            self.vcounter = self.vcounter.wrapping_add(self.hcounter >> 11) & 0x1FF;
            self.hcounter &= 0x7FF;
        } else {
            self.hcounter = self.hcounter.wrapping_add(2);
            if self.hcounter >= 1364 {
                self.hcounter = 0;
                self.vcounter = self.vcounter.wrapping_add(1);
                if self.vcounter >= self.scanlines {
                    self.vcounter = 0;
                }
            }
        }
        let h_match = self.hcounter == self.io.hcnt.wrapping_shl(2);
        let v_match = self.vcounter == self.io.vcnt;
        let fire = match (self.io.hen, self.io.ven) {
            (false, false) => false,
            (true, false) => h_match,
            (false, true) => v_match && self.hcounter == 0,
            (true, true) => v_match && h_match,
        };
        if fire {
            self.io.timer_irqfl = true;
            if self.io.timer_irqen {
                self.io.timer_irqcl = false;
            }
        }
    }

    /// Latched counters for `HCR`/`VCR`: dots and lines.
    pub(crate) const fn latch_counters(&mut self) {
        self.io.hcr = self.hcounter >> 2;
        self.io.vcr = self.vcounter;
    }

    /// Restarts the timer (`CTR`).
    pub(crate) const fn restart_timer(&mut self) {
        self.hcounter = 0;
        self.vcounter = 0;
    }
}

impl Bus for Chip {
    fn read(&mut self, addr: u32) -> u8 {
        if let Some(vector) = self.vector
            && addr & 0xFF_FFE0 == 0x00_FFE0
        {
            self.step();
            let [lo, hi] = vector.to_le_bytes();
            return if addr & 1 == 0 { lo } else { hi };
        }
        self.read_sa1(addr)
    }

    fn write(&mut self, addr: u32, value: u8) {
        self.write_sa1(addr, value);
    }

    fn idle(&mut self) {
        self.step();
    }
}
