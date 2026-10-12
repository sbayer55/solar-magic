//! The whole machine behind one handle: what the editor and the CLI drive.

use snes_rom::Rom;
use w65c816::{Addr, Bus, Cpu, CpuError, Exec};

use crate::bus::{Overlay, SystemBus};
use crate::cart::Cartridge;
use crate::clock::Region;
use crate::input::Input;

/// Why a run stopped early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// The program counter reached a breakpoint.
    Breakpoint(Addr),
    /// The CPU stopped (`STP`, or a fatal `BRK`).
    Stopped(CpuError),
}

/// What a run produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunReport {
    /// Frame counter afterwards.
    pub frame: u64,
    /// Master cycles elapsed since power-on.
    pub master_cycles: u64,
    /// Instructions executed since power-on.
    pub instructions: u64,
    /// Why the run ended early, if it did.
    pub hit: Option<Hit>,
}

/// A Super Nintendo.
#[derive(Debug, Clone)]
pub struct Console {
    cpu: Cpu,
    bus: SystemBus,
    breakpoints: Vec<Addr>,
}

impl Console {
    /// Powers on with `rom` inserted and runs the reset sequence.
    #[must_use]
    pub fn new(rom: &Rom, region: Region) -> Self {
        let mut console = Self {
            cpu: Cpu::new(),
            bus: SystemBus::new(Cartridge::new(rom), region),
            breakpoints: Vec::new(),
        };
        console.reset();
        console
    }

    /// Presses reset: registers return to power-on, RAM keeps its contents,
    /// and the CPU starts at the reset vector.
    pub fn reset(&mut self) {
        self.bus.reset();
        self.cpu = Cpu::new();
        self.cpu.reset(&mut self.bus);
    }

    /// The processor.
    #[must_use]
    pub const fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// The rest of the machine.
    #[must_use]
    pub const fn bus(&self) -> &SystemBus {
        &self.bus
    }

    /// The rest of the machine, mutably (debuggers only).
    pub const fn bus_mut(&mut self) -> &mut SystemBus {
        &mut self.bus
    }

    /// Sets both controllers.
    pub const fn set_input(&mut self, input: Input) {
        self.bus.regs.joypads.input = input;
    }

    /// Makes `bytes` shadow the cartridge at `start` for reads.
    pub fn set_overlay(&mut self, start: Addr, bytes: Vec<u8>) {
        self.bus.overlays.push(Overlay { start, bytes });
    }

    /// Removes every overlay.
    pub fn clear_overlays(&mut self) {
        self.bus.overlays.clear();
    }

    /// Stops a run before the instruction at `pc24` executes.
    pub fn add_breakpoint(&mut self, pc24: Addr) {
        if !self.breakpoints.contains(&pc24) {
            self.breakpoints.push(pc24);
        }
    }

    /// Removes a breakpoint.
    pub fn remove_breakpoint(&mut self, pc24: Addr) {
        self.breakpoints.retain(|&b| b != pc24);
    }

    /// Reads a byte with no side effects (WRAM, cartridge; registers read as 0).
    #[must_use]
    pub fn peek(&self, addr: Addr) -> u8 {
        let bank = crate::bits::bank(addr);
        let offset = addr & 0xFFFF;
        match bank {
            0x7E | 0x7F => self.bus.wram.read(addr & 0x1_FFFF),
            0x00..=0x3F | 0x80..=0xBF if offset < 0x2000 => self.bus.wram.read(offset),
            0x00..=0x3F | 0x80..=0xBF if offset < 0x8000 => 0,
            _ => self.peek_cart(addr),
        }
    }

    fn peek_cart(&self, addr: Addr) -> u8 {
        use snes_rom::Mapper;
        if let Some(v) = self.bus.overlays.iter().find_map(|o| {
            let index = usize::try_from(addr.checked_sub(o.start)?).ok()?;
            o.bytes.get(index).copied()
        }) {
            return v;
        }
        snes_rom::SnesAddr::new(addr)
            .and_then(|a| self.bus.cart.mapper().to_pc(a))
            .map_or(0, |pc| self.bus.cart.rom_byte(pc))
    }

    /// Reads a little-endian word with [`Console::peek`].
    #[must_use]
    pub fn peek_u16(&self, addr: Addr) -> u16 {
        u16::from_le_bytes([self.peek(addr), self.peek(addr.wrapping_add(1) & 0xFF_FFFF)])
    }

    /// Writes a WRAM byte (other addresses are ignored).
    pub fn poke(&mut self, addr: Addr, v: u8) {
        let bank = crate::bits::bank(addr);
        let offset = addr & 0xFFFF;
        match bank {
            0x7E | 0x7F => self.bus.wram.write(addr & 0x1_FFFF, v),
            0x00..=0x3F | 0x80..=0xBF if offset < 0x2000 => self.bus.wram.write(offset, v),
            _ => {}
        }
    }

    /// Writes a little-endian word with [`Console::poke`].
    pub fn poke_u16(&mut self, addr: Addr, v: u16) {
        let [lo, hi] = v.to_le_bytes();
        self.poke(addr, lo);
        self.poke(addr.wrapping_add(1) & 0xFF_FFFF, hi);
    }

    /// Delivers pending interrupts, then executes one instruction (or six
    /// idle cycles while the CPU waits for an interrupt).
    ///
    /// # Errors
    ///
    /// The CPU's stop errors.
    pub fn step_instruction(&mut self) -> Result<Exec, CpuError> {
        if self.bus.regs.irq.nmi_pending {
            self.bus.regs.irq.nmi_pending = false;
            self.cpu.nmi(&mut self.bus);
        } else if self.bus.regs.irq.irq_flag
            || self.bus.cart.copro.as_ref().is_some_and(|c| c.irq_pending())
        {
            self.cpu.irq(&mut self.bus);
        }
        if self.cpu.waiting {
            self.bus.idle();
            return Ok(Exec::Waiting);
        }
        self.cpu.step(&mut self.bus)
    }

    const fn report(&self, hit: Option<Hit>) -> RunReport {
        RunReport {
            frame: self.bus.clock.frame,
            master_cycles: self.bus.clock.now,
            instructions: self.cpu.steps,
            hit,
        }
    }

    /// Runs until the frame counter advances, a breakpoint is reached, or
    /// the CPU stops.
    pub fn run_frame(&mut self, input: Input) -> RunReport {
        let target = self.bus.clock.frame.wrapping_add(1);
        self.run_while(input, |c| c.bus.clock.frame < target)
    }

    /// Runs while `keep_going` holds (checked before every instruction),
    /// stopping early on a breakpoint or a CPU stop.
    pub fn run_while(
        &mut self,
        input: Input,
        mut keep_going: impl FnMut(&Self) -> bool,
    ) -> RunReport {
        self.set_input(input);
        while keep_going(self) {
            if !self.breakpoints.is_empty() && self.breakpoints.contains(&self.cpu.pc24()) {
                return self.report(Some(Hit::Breakpoint(self.cpu.pc24())));
            }
            if let Err(e) = self.step_instruction() {
                return self.report(Some(Hit::Stopped(e)));
            }
        }
        self.report(None)
    }
}
