//! The S-SMP: RAM, the `$F0-$FF` registers, timers, CPU ports, and the
//! boot-ROM window, as the SPC700 core's bus (one cycle per access).
//! fullsnes "SNES APU SPC700 I/O Ports", read 2026-10-11.

use spc700::ApuBus;

use crate::dsp::DspRegs;
use crate::ipl::{Ipl, IplPhase};
use crate::timer::Timer;

/// Memory, registers and ports.
#[derive(Debug, Clone)]
pub struct Smp {
    ram: Box<[u8]>,
    /// `$F0 TEST` (stored, not modelled).
    pub test: u8,
    /// `$F1 CONTROL` bit 7: the boot ROM is mapped at `$FFC0`.
    pub rom_enabled: bool,
    /// `$F2 DSPADDR`.
    pub dsp_addr: u8,
    /// The DSP.
    pub dsp: DspRegs,
    /// `$F4-$F7` as the SPC700 reads them (written by the S-CPU).
    pub inbox: [u8; 4],
    /// `$2140-$2143` as the S-CPU reads them (written by the SPC700).
    pub outbox: [u8; 4],
    /// `$F8`/`$F9`.
    pub aux: [u8; 2],
    /// Timers 0-2.
    pub timers: [Timer; 3],
    /// SPC cycles executed.
    pub cycles: u64,
    /// Boot ROM strategy.
    pub ipl: Ipl,
    /// High-level boot ROM state.
    pub(crate) ipl_phase: IplPhase,
}

impl Smp {
    /// Power-on state with zeroed RAM.
    #[must_use]
    pub fn new(ipl: Ipl) -> Self {
        Self {
            ram: vec![0; 0x1_0000].into_boxed_slice(),
            test: 0x0A,
            rom_enabled: true,
            dsp_addr: 0,
            dsp: DspRegs::new(),
            inbox: [0; 4],
            outbox: [0; 4],
            aux: [0xFF; 2],
            timers: [Timer::default(); 3],
            cycles: 0,
            ipl,
            ipl_phase: IplPhase::Entry,
        }
    }

    /// Registers return to power-on; RAM is kept.
    pub fn reset(&mut self) {
        let ram = core::mem::replace(&mut self.ram, Box::new([]));
        let ipl = self.ipl.clone();
        let cycles = self.cycles;
        *self = Self::new(ipl);
        self.ram = ram;
        self.cycles = cycles;
    }

    /// The 64 KiB of audio RAM.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Reads RAM directly (no cycle).
    #[must_use]
    pub fn ram_byte(&self, addr: u16) -> u8 {
        self.ram.get(usize::from(addr)).copied().unwrap_or(0)
    }

    /// Writes RAM directly (no cycle).
    pub fn set_ram_byte(&mut self, addr: u16, v: u8) {
        if let Some(slot) = self.ram.get_mut(usize::from(addr)) {
            *slot = v;
        }
    }

    /// Whether `pc` would execute boot ROM through the high-level model.
    #[must_use]
    pub const fn in_boot_rom(&self, pc: u16) -> bool {
        self.rom_enabled && pc >= 0xFFC0 && matches!(self.ipl, Ipl::Hle)
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

    /// One SPC cycle: advances the timers' prescalers.
    pub(crate) fn tick(&mut self) {
        self.cycles = self.cycles.wrapping_add(1);
        if self.cycles.trailing_zeros() >= 4 {
            // 64 kHz: every 16 cycles.
            if let Some(t2) = self.timers.get_mut(2) {
                t2.tick();
            }
        }
        if self.cycles.trailing_zeros() >= 7 {
            // 8 kHz: every 128 cycles.
            for timer in self.timers.iter_mut().take(2) {
                timer.tick();
            }
        }
    }

    /// Spends `n` cycles doing nothing (halted core, boot-ROM delays).
    pub(crate) fn idle_cycles(&mut self, n: u64) {
        for _ in 0..n {
            self.tick();
        }
    }

    fn io_read(&mut self, reg: u8) -> u8 {
        match reg {
            0x2 => self.dsp_addr,
            0x3 => self.dsp.read(self.dsp_addr),
            0x4..=0x7 => self.inbox.get(usize::from(reg & 3)).copied().unwrap_or(0),
            0x8 | 0x9 => self.aux.get(usize::from(reg & 1)).copied().unwrap_or(0xFF),
            0xD..=0xF => self
                .timers
                .get_mut(usize::from(reg.wrapping_sub(0xD)))
                .map_or(0, Timer::read_output),
            _ => 0,
        }
    }

    fn io_write(&mut self, reg: u8, v: u8) {
        match reg {
            0x0 => self.test = v,
            0x1 => self.write_control(v),
            0x2 => self.dsp_addr = v,
            0x3 => self.dsp.write(self.dsp_addr, v),
            0x4..=0x7 => {
                if let Some(slot) = self.outbox.get_mut(usize::from(reg & 3)) {
                    *slot = v;
                }
            }
            0x8 | 0x9 => {
                if let Some(slot) = self.aux.get_mut(usize::from(reg & 1)) {
                    *slot = v;
                }
            }
            0xA..=0xC => {
                if let Some(timer) = self.timers.get_mut(usize::from(reg.wrapping_sub(0xA))) {
                    timer.target = v;
                }
            }
            _ => {}
        }
    }

    /// `$F1 CONTROL`.
    fn write_control(&mut self, v: u8) {
        for (n, timer) in self.timers.iter_mut().enumerate() {
            timer.set_enabled(v & (1 << n) != 0);
        }
        if v & 0x10 != 0 {
            self.inbox[0] = 0;
            self.inbox[1] = 0;
        }
        if v & 0x20 != 0 {
            self.inbox[2] = 0;
            self.inbox[3] = 0;
        }
        self.rom_enabled = v & 0x80 != 0;
    }
}

impl ApuBus for Smp {
    fn read(&mut self, addr: u16) -> u8 {
        self.tick();
        match addr {
            0x00F0..=0x00FF => self.io_read(crate::lo(addr) & 0x0F),
            0xFFC0..=0xFFFF if self.rom_enabled => match &self.ipl {
                Ipl::Rom(bytes) => {
                    bytes.get(usize::from(addr.wrapping_sub(0xFFC0))).copied().unwrap_or(0xFF)
                }
                Ipl::Hle => crate::ipl::hle_rom_byte(addr),
            },
            _ => self.ram.get(usize::from(addr)).copied().unwrap_or(0),
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.tick();
        if let 0x00F0..=0x00FF = addr {
            self.io_write(crate::lo(addr) & 0x0F, value);
        }
        // Writes always reach RAM, including under the boot ROM and the ports.
        if let Some(slot) = self.ram.get_mut(usize::from(addr)) {
            *slot = value;
        }
    }

    fn idle(&mut self) {
        self.tick();
    }
}
