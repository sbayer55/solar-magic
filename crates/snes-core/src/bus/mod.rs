//! The system bus.
//!
//! Every CPU access is charged its cycles here, the rest of the machine
//! catches up, and the byte is routed to WRAM, a register block, the APU
//! ports, the cartridge, or returned as open bus.

pub mod wram;

use snes_apu::Apu;
use snes_ppu::Ppu;
use w65c816::{Addr, Bus};

use crate::bits::{bank, lo, lo16};
use crate::cart::Cartridge;
use crate::clock::{Clock, Events};
use crate::cpu_regs::CpuRegs;
use crate::dma::Dma;
pub use wram::{WRAM_LEN, Wram};

/// Bytes that shadow the cartridge for reads of an address range (how
/// unsaved level data reaches the game without touching the ROM).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    /// First bus address covered.
    pub start: Addr,
    /// The bytes.
    pub bytes: Vec<u8>,
}

impl Overlay {
    fn get(&self, addr: Addr) -> Option<u8> {
        let index = usize::try_from(addr.checked_sub(self.start)?).ok()?;
        self.bytes.get(index).copied()
    }
}

/// Everything on the bus.
#[derive(Debug, Clone)]
pub struct SystemBus {
    /// Master clock and raster position.
    pub clock: Clock,
    /// Work RAM.
    pub wram: Wram,
    /// 5A22 registers.
    pub regs: CpuRegs,
    /// Picture processor.
    pub ppu: Ppu,
    /// Audio unit.
    pub apu: Apu,
    /// Cartridge.
    pub cart: Cartridge,
    /// DMA/HDMA channels.
    pub dma: Dma,
    /// Read-only shadows over the cartridge.
    pub overlays: Vec<Overlay>,
    /// Memory data register: the last byte on the bus, returned for open bus.
    pub mdr: u8,
    /// Set while a general-purpose DMA is transferring.
    pub(crate) in_dma: bool,
}

impl SystemBus {
    /// A bus around `cart` at power-on.
    #[must_use]
    pub fn new(cart: Cartridge, region: crate::clock::Region) -> Self {
        Self {
            clock: Clock::new(region),
            wram: Wram::new(),
            regs: CpuRegs::new(),
            ppu: Ppu::new(matches!(region, crate::clock::Region::Pal)),
            apu: Apu::default(),
            cart,
            dma: Dma::new(),
            overlays: Vec::new(),
            mdr: 0,
            in_dma: false,
        }
    }

    /// Reset: registers and clock return to power-on; RAM contents stay.
    pub fn reset(&mut self) {
        self.clock = Clock::new(self.clock.region);
        self.regs = CpuRegs::new();
        self.ppu.reset();
        self.apu.reset();
        self.dma = Dma::new();
        self.mdr = 0;
        self.in_dma = false;
        if let Some(copro) = self.cart.copro.as_mut() {
            copro.reset(self.clock.now);
        }
    }

    /// Master cycles one CPU access to `addr` takes.
    #[must_use]
    pub const fn speed(&self, addr: Addr) -> u8 {
        let bank = bank(addr);
        let offset = addr & 0xFFFF;
        let fast = self.regs.fast_rom && bank >= 0x80;
        if bank & 0x40 == 0 {
            match offset {
                0x0000..=0x1FFF | 0x6000..=0x7FFF => 8,
                0x2000..=0x3FFF | 0x4200..=0x5FFF => 6,
                0x4000..=0x41FF => 12,
                _ => {
                    if fast {
                        6
                    } else {
                        8
                    }
                }
            }
        } else if fast {
            6
        } else {
            8
        }
    }

    /// Advances the machine by `cycles` master cycles and services whatever
    /// timing points were crossed.
    pub fn step(&mut self, cycles: u8) {
        let ev = self.clock.advance(u16::from(cycles));
        if !ev.is_empty() {
            self.service(ev);
        }
    }

    fn service(&mut self, ev: Events) {
        if ev.contains(Events::FRAME_START) {
            self.clock.interlace = self.ppu.interlace();
            self.ppu.frame_start(self.clock.field);
        }
        if ev.contains(Events::LINE_START) {
            self.ppu.run_to(self.clock.position());
            self.apu.run_to(self.clock.now);
            if let Some(copro) = self.cart.copro.as_mut() {
                copro.run_to(self.clock.now);
            }
            self.clock.overscan = self.ppu.overscan();
            if self.clock.v == self.clock.vdisp() {
                self.ppu.vblank_start();
            }
        }
        self.regs.irq.on_events(ev, &self.clock);
        if ev.contains(Events::JOYPAD_START)
            && self.clock.v == self.clock.vdisp()
            && self.regs.auto_joypad
        {
            self.regs.joypads.start_auto_read(self.clock.now);
        }
        if ev.contains(Events::REFRESH) {
            // DRAM refresh: the CPU is held for 40 cycles.
            let nested = self.clock.advance(40);
            if !nested.is_empty() {
                self.service(nested);
            }
        }
        if ev.contains(Events::HDMA_INIT) && self.clock.v == 0 {
            self.hdma_init();
        }
        if ev.contains(Events::HDMA_RUN) && !self.clock.vblank() {
            self.hdma_run();
        }
    }

    /// The PPU-facing raster position.
    #[must_use]
    pub const fn position(&self) -> snes_ppu::Position {
        self.clock.position()
    }

    fn overlay_byte(&self, addr: Addr) -> Option<u8> {
        self.overlays.iter().find_map(|o| o.get(addr))
    }

    /// Reads without charging time (used by DMA and debuggers).
    pub fn read_untimed(&mut self, addr: Addr) -> u8 {
        let bank = bank(addr);
        let offset = lo16(addr);
        match bank {
            0x7E | 0x7F => self.wram.read(addr & 0x1_FFFF),
            0x00..=0x3F | 0x80..=0xBF => match offset {
                0x0000..=0x1FFF => self.wram.read(u32::from(offset)),
                0x2100..=0x213F => {
                    let pos = self.position();
                    self.ppu.run_to(pos);
                    self.ppu.read(lo(offset), self.mdr, pos)
                }
                0x2140..=0x217F => {
                    self.apu.run_to(self.clock.now);
                    self.apu.cpu_read_port(lo(offset))
                }
                0x2180 => self.wram.port_read(),
                0x4016 => (self.mdr & 0xFC) | self.regs.joypads.read_serial(0),
                0x4017 => (self.mdr & 0xE0) | 0x1C | self.regs.joypads.read_serial(1),
                0x4210..=0x421F => self.read_cpu_reg(offset),
                0x4300..=0x437F => self.dma.read_reg(offset).unwrap_or(self.mdr),
                _ => self.read_cart(addr),
            },
            _ => self.read_cart(addr),
        }
    }

    fn read_cart(&mut self, addr: Addr) -> u8 {
        if let Some(v) = self.overlay_byte(addr) {
            return v;
        }
        self.cart.read(addr, self.clock.now).unwrap_or(self.mdr)
    }

    /// Writes without charging time.
    pub fn write_untimed(&mut self, addr: Addr, v: u8) {
        let bank = bank(addr);
        let offset = lo16(addr);
        match bank {
            0x7E | 0x7F => self.wram.write(addr & 0x1_FFFF, v),
            0x00..=0x3F | 0x80..=0xBF => match offset {
                0x0000..=0x1FFF => self.wram.write(u32::from(offset), v),
                0x2100..=0x213F => {
                    let pos = self.position();
                    self.ppu.run_to(pos);
                    self.ppu.write(lo(offset), v, pos);
                }
                0x2140..=0x217F => {
                    self.apu.run_to(self.clock.now);
                    self.apu.cpu_write_port(lo(offset), v);
                }
                0x2180 => self.wram.port_write(v),
                0x2181 => self.wram.set_address_low(v),
                0x2182 => self.wram.set_address_mid(v),
                0x2183 => self.wram.set_address_high(v),
                0x4016 => self.regs.joypads.write_strobe(v),
                0x4200..=0x420D => self.write_cpu_reg(offset, v),
                0x4300..=0x437F => self.dma.write_reg(offset, v),
                _ => self.cart.write(addr, v, self.clock.now),
            },
            _ => self.cart.write(addr, v, self.clock.now),
        }
    }

    fn read_cpu_reg(&mut self, offset: u16) -> u8 {
        let mdr = self.mdr;
        match offset {
            0x4210 => {
                let flag = if self.regs.irq.read_rdnmi() { 0x80 } else { 0 };
                flag | (mdr & 0x70) | self.regs.cpu_version
            }
            0x4211 => {
                let flag = if self.regs.irq.read_timeup() { 0x80 } else { 0 };
                flag | (mdr & 0x7F)
            }
            0x4212 => {
                (if self.clock.vblank() { 0x80 } else { 0 })
                    | (if self.clock.hblank() { 0x40 } else { 0 })
                    | (mdr & 0x3E)
                    | u8::from(self.regs.joypads.busy(self.clock.now))
            }
            0x4213 => self.regs.wrio,
            0x4214 => lo(self.regs.alu.rddiv),
            0x4215 => crate::bits::hi(self.regs.alu.rddiv),
            0x4216 => lo(self.regs.alu.rdmpy),
            0x4217 => crate::bits::hi(self.regs.alu.rdmpy),
            0x4218..=0x421F => self.regs.joypads.read_auto(lo(offset.wrapping_sub(0x4218))),
            _ => mdr,
        }
    }

    fn write_cpu_reg(&mut self, offset: u16, v: u8) {
        match offset {
            0x4200 => {
                self.regs.irq.write_nmitimen(v);
                self.regs.auto_joypad = v & 1 != 0;
            }
            0x4201 => {
                let pos = self.position();
                self.ppu.set_latch_enabled(v & 0x80 != 0, pos);
                self.regs.wrio = v;
            }
            0x4202 => self.regs.alu.mpya = v,
            0x4203 => self.regs.alu.start_multiply(v),
            0x4204 => self.regs.alu.dividend = (self.regs.alu.dividend & 0xFF00) | u16::from(v),
            0x4205 => {
                self.regs.alu.dividend = (u16::from(v) << 8) | (self.regs.alu.dividend & 0xFF);
            }
            0x4206 => self.regs.alu.start_divide(v),
            0x4207 => {
                self.regs.irq.htime = (self.regs.irq.htime & 0x100) | u16::from(v);
                self.clock.set_htime(self.regs.irq.htime);
            }
            0x4208 => {
                self.regs.irq.htime = (u16::from(v & 1) << 8) | (self.regs.irq.htime & 0xFF);
                self.clock.set_htime(self.regs.irq.htime);
            }
            0x4209 => self.regs.irq.vtime = (self.regs.irq.vtime & 0x100) | u16::from(v),
            0x420A => self.regs.irq.vtime = (u16::from(v & 1) << 8) | (self.regs.irq.vtime & 0xFF),
            0x420B => {
                self.dma.gp_enable = v;
                self.dma.gp_pending = v != 0;
            }
            0x420C => self.dma.hdma_enable = v,
            0x420D => self.regs.fast_rom = v & 1 != 0,
            _ => {}
        }
    }

    /// Runs a pending general-purpose DMA before the next CPU cycle.
    fn dma_edge(&mut self) {
        if self.dma.gp_pending && !self.in_dma {
            self.gpdma_run();
        }
    }
}

impl Bus for SystemBus {
    fn read(&mut self, addr: Addr) -> u8 {
        self.dma_edge();
        let speed = self.speed(addr);
        self.step(speed);
        let v = self.read_untimed(addr);
        self.mdr = v;
        self.regs.alu.edge();
        v
    }

    fn write(&mut self, addr: Addr, value: u8) {
        self.dma_edge();
        self.regs.alu.edge();
        let speed = self.speed(addr);
        self.step(speed);
        self.write_untimed(addr, value);
        self.mdr = value;
    }

    fn idle(&mut self) {
        self.dma_edge();
        self.step(6);
        self.regs.alu.edge();
    }
}
