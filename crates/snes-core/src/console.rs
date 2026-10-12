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
    /// A breakpoint that was just reported; the next run steps over it.
    resume_pc: Option<Addr>,
}

impl Console {
    /// Powers on with `rom` inserted and runs the reset sequence.
    #[must_use]
    pub fn new(rom: &Rom, region: Region) -> Self {
        Self::with_cartridge(Cartridge::new(rom), region)
    }

    /// Powers on with an explicit cartridge (test ROMs without a full header).
    #[must_use]
    pub fn with_cartridge(cart: Cartridge, region: Region) -> Self {
        let mut console = Self {
            cpu: Cpu::new(),
            bus: SystemBus::new(cart, region),
            breakpoints: Vec::new(),
            resume_pc: None,
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
        self.resume_pc = None;
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

    /// The picture as rendered so far this frame (complete after vblank).
    #[must_use]
    pub const fn frame(&self) -> &snes_ppu::Frame {
        self.bus.ppu.frame()
    }

    /// Moves the sound output produced so far (interleaved stereo, 32 kHz)
    /// into `out`.
    pub fn take_audio(&mut self, out: &mut Vec<i16>) {
        self.bus.apu.take_audio(out);
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

    /// Breakpoints ignore bank bit 7, so code running from the FastROM
    /// mirror (`$80+`) still stops at an address given for `$00+`.
    const fn mirror_free(pc24: Addr) -> Addr {
        pc24 & 0x7F_FFFF
    }

    /// Stops a run before the instruction at `pc24` (or its mirror) executes.
    pub fn add_breakpoint(&mut self, pc24: Addr) {
        let pc = Self::mirror_free(pc24);
        if !self.breakpoints.contains(&pc) {
            self.breakpoints.push(pc);
        }
    }

    /// Removes a breakpoint.
    pub fn remove_breakpoint(&mut self, pc24: Addr) {
        let pc = Self::mirror_free(pc24);
        self.breakpoints.retain(|&b| b != pc);
    }

    /// Reads a byte with no side effects (WRAM, cartridge; registers read as 0).
    #[must_use]
    pub fn peek(&self, addr: Addr) -> u8 {
        let bank = crate::bits::bank(addr);
        let offset = addr & 0xFFFF;
        match bank {
            0x7E | 0x7F => self.bus.wram.read(addr & 0x1_FFFF),
            0x00..=0x3F | 0x80..=0xBF if offset < 0x2000 => self.bus.wram.read(offset),
            0x00..=0x3F | 0x80..=0xBF if offset < 0x8000 => self.peek_copro(addr).unwrap_or(0),
            _ => self.peek_cart(addr),
        }
    }

    /// A coprocessor's memories (the SA-1's I-RAM and BW-RAM).
    fn peek_copro(&self, addr: Addr) -> Option<u8> {
        self.bus.cart.copro.as_ref().and_then(|c| c.peek(addr))
    }

    fn peek_cart(&self, addr: Addr) -> u8 {
        use snes_rom::Mapper;
        if let Some(v) = self.peek_copro(addr) {
            return v;
        }
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

    /// The cartridge's battery-backed memory (`.srm` contents).
    #[must_use]
    pub fn battery(&self) -> &[u8] {
        self.bus.cart.battery()
    }

    /// Loads a `.srm` into the battery-backed memory.
    pub fn load_battery(&mut self, data: &[u8]) {
        self.bus.cart.load_battery(data);
    }

    /// Whether the battery-backed memory changed since the last
    /// `clear_battery_dirty`.
    #[must_use]
    pub fn battery_dirty(&self) -> bool {
        self.bus.cart.battery_dirty()
    }

    /// Marks the battery-backed memory as saved.
    pub fn clear_battery_dirty(&mut self) {
        self.bus.cart.clear_battery_dirty();
    }

    /// The breakpoints (mirror-free 24-bit addresses).
    #[must_use]
    pub fn breakpoints(&self) -> &[Addr] {
        &self.breakpoints
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
    /// idle cycles while the CPU waits for an interrupt or has executed
    /// `STP`; the rest of the machine keeps running either way).
    ///
    /// # Errors
    ///
    /// A fatal `BRK` or an exhausted budget.
    pub fn step_instruction(&mut self) -> Result<Exec, CpuError> {
        if self.cpu.stopped {
            self.bus.idle();
            return Ok(Exec::Waiting);
        }
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
        match self.cpu.step(&mut self.bus) {
            Err(CpuError::Stopped(w65c816::StopReason::Stp)) => Ok(Exec::Waiting),
            other => other,
        }
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
            let pc = self.cpu.pc24();
            let resuming = self.resume_pc.take() == Some(pc);
            if !resuming
                && !self.breakpoints.is_empty()
                && self.breakpoints.contains(&Self::mirror_free(pc))
            {
                self.resume_pc = Some(pc);
                return self.report(Some(Hit::Breakpoint(pc)));
            }
            if let Err(e) = self.step_instruction() {
                return self.report(Some(Hit::Stopped(e)));
            }
        }
        self.report(None)
    }
}
