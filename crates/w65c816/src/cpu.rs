//! Register file, fetch/decode loop, and stack helpers.

use crate::bits::{hi, join, lo, lo16, w, w32};
use crate::bus::Bus;
use crate::flags::Flags;
use crate::table::OPCODES;

/// Why execution stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum StopReason {
    /// `STP` executed.
    #[error("STP executed")]
    Stp,
    /// `WAI` executed (no interrupts are modelled, so this is terminal).
    #[error("WAI executed")]
    Wai,
    /// `BRK` executed and the caller asked to treat it as fatal.
    #[error("BRK executed at {pb:02X}:{pc:04X}")]
    Brk {
        /// Program bank of the BRK.
        pb: u8,
        /// Address of the BRK.
        pc: u16,
    },
}

/// Execution errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CpuError {
    /// The CPU has stopped.
    #[error(transparent)]
    Stopped(#[from] StopReason),
    /// The step budget given to [`Cpu::run_until`] ran out.
    #[error("instruction step budget exhausted")]
    Budget,
    /// Internal: an addressing mode with no data operand was resolved as one.
    #[error("internal: mode has no operand")]
    NotAnOperand,
}

/// A 65c816 processor state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpu {
    /// Accumulator (C). In 8-bit mode the high byte is B.
    pub a: u16,
    /// X index.
    pub x: u16,
    /// Y index.
    pub y: u16,
    /// Stack pointer.
    pub s: u16,
    /// Direct page register.
    pub d: u16,
    /// Program bank.
    pub pb: u8,
    /// Data bank.
    pub db: u8,
    /// Program counter within the program bank.
    pub pc: u16,
    /// Status flags.
    pub p: Flags,
    /// Emulation mode.
    pub e: bool,
    /// Instructions executed so far.
    pub steps: u64,
    /// Whether `BRK`/`COP` stop execution instead of taking the vector.
    pub brk_is_fatal: bool,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    /// A CPU in native mode with 8-bit A/X/Y, stack at `$01FF`, flags clear.
    #[must_use]
    pub fn new() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            s: 0x01FF,
            d: 0,
            pb: 0,
            db: 0,
            pc: 0,
            p: Flags { m: true, x: true, i: true, ..Flags::default() },
            e: false,
            steps: 0,
            brk_is_fatal: false,
        }
    }

    /// Whether the accumulator and memory operands are 8 bits wide.
    #[must_use]
    pub const fn m8(&self) -> bool {
        self.e || self.p.m
    }

    /// Whether the index registers are 8 bits wide.
    #[must_use]
    pub const fn x8(&self) -> bool {
        self.e || self.p.x
    }

    /// The 24-bit address of the next instruction byte.
    #[must_use]
    pub const fn pc24(&self) -> u32 {
        join(self.pb, self.pc)
    }

    /// Executes one instruction.
    ///
    /// # Errors
    ///
    /// [`CpuError::Stopped`] on `STP`/`WAI` (or fatal `BRK`).
    pub fn step(&mut self, bus: &mut dyn Bus) -> Result<(), CpuError> {
        let opcode = self.fetch8(bus);
        let entry = OPCODES.get(usize::from(opcode)).copied().unwrap_or(OPCODES[0xEA]);
        self.steps = self.steps.saturating_add(1);
        (entry.0)(self, bus, entry.1)
    }

    /// Runs until the program counter reaches `stop_pc24` (checked before
    /// each instruction) or `max_steps` instructions have executed.
    ///
    /// # Errors
    ///
    /// [`CpuError::Budget`] if the budget runs out; stop errors from [`Cpu::step`].
    pub fn run_until(
        &mut self,
        bus: &mut dyn Bus,
        stop_pc24: u32,
        max_steps: u64,
    ) -> Result<(), CpuError> {
        for _ in 0..max_steps {
            if self.pc24() == stop_pc24 {
                return Ok(());
            }
            self.step(bus)?;
        }
        Err(CpuError::Budget)
    }

    /// Simulates a `JSL` into `target` from a caller at a sentinel address,
    /// then runs until the callee returns there.
    ///
    /// # Errors
    ///
    /// See [`Cpu::run_until`].
    pub fn call_long(
        &mut self,
        bus: &mut dyn Bus,
        target: u32,
        sentinel: u32,
        max_steps: u64,
    ) -> Result<(), CpuError> {
        // RTL pulls PC and PB and adds 1 to PC, so push sentinel - 1.
        let ret = crate::addr24(sentinel.wrapping_sub(1));
        self.push8(bus, crate::bits::bank(ret));
        self.push16(bus, lo16(ret));
        self.pb = crate::bits::bank(target);
        self.pc = lo16(target);
        self.run_until(bus, sentinel, max_steps)
    }

    // ----- fetch -----

    pub(crate) fn fetch8(&mut self, bus: &mut dyn Bus) -> u8 {
        let b = bus.read(self.pc24());
        self.pc = self.pc.wrapping_add(1);
        b
    }

    pub(crate) fn fetch16(&mut self, bus: &mut dyn Bus) -> u16 {
        let lo = self.fetch8(bus);
        let hi = self.fetch8(bus);
        u16::from_le_bytes([lo, hi])
    }

    pub(crate) fn fetch24(&mut self, bus: &mut dyn Bus) -> u32 {
        let lo = self.fetch8(bus);
        let mid = self.fetch8(bus);
        let hi = self.fetch8(bus);
        u32::from_le_bytes([lo, mid, hi, 0])
    }

    // ----- stack -----

    /// Decrements the stack pointer, staying in page 1 in emulation mode.
    const fn dec_s(&mut self) {
        self.s =
            if self.e { 0x0100 | w(lo(self.s).wrapping_sub(1)) } else { self.s.wrapping_sub(1) };
    }

    /// Increments the stack pointer, staying in page 1 in emulation mode.
    const fn inc_s(&mut self) {
        self.s =
            if self.e { 0x0100 | w(lo(self.s).wrapping_add(1)) } else { self.s.wrapping_add(1) };
    }

    pub(crate) fn push8(&mut self, bus: &mut dyn Bus, v: u8) {
        bus.write(w32(self.s), v);
        self.dec_s();
    }

    pub(crate) fn push16(&mut self, bus: &mut dyn Bus, v: u16) {
        self.push8(bus, hi(v));
        self.push8(bus, lo(v));
    }

    pub(crate) fn pull8(&mut self, bus: &mut dyn Bus) -> u8 {
        self.inc_s();
        bus.read(w32(self.s))
    }

    pub(crate) fn pull16(&mut self, bus: &mut dyn Bus) -> u16 {
        let lo = self.pull8(bus);
        let hi = self.pull8(bus);
        u16::from_le_bytes([lo, hi])
    }

    /// Pushes a 16-bit value without the emulation-mode page-1 wrap (used by
    /// the "new" instructions PEA/PEI/PER/PHD, which ignore it).
    pub(crate) fn push16_nowrap(&mut self, bus: &mut dyn Bus, v: u16) {
        bus.write(w32(self.s), hi(v));
        self.s = self.s.wrapping_sub(1);
        bus.write(w32(self.s), lo(v));
        self.s = self.s.wrapping_sub(1);
        self.fix_emulation_stack();
    }

    /// Pulls a 16-bit value without the page-1 wrap (PLD).
    pub(crate) fn pull16_nowrap(&mut self, bus: &mut dyn Bus) -> u16 {
        self.s = self.s.wrapping_add(1);
        let lo = bus.read(w32(self.s));
        self.s = self.s.wrapping_add(1);
        let hi = bus.read(w32(self.s));
        self.fix_emulation_stack();
        u16::from_le_bytes([lo, hi])
    }

    /// In emulation mode the stack pointer's high byte is forced back to 1.
    pub(crate) const fn fix_emulation_stack(&mut self) {
        if self.e {
            self.s = 0x0100 | (self.s & 0xFF);
        }
    }

    // ----- register width maintenance -----

    /// Applies the consequences of a new P value: 8-bit index registers lose
    /// their high bytes.
    pub(crate) const fn apply_flag_widths(&mut self) {
        if self.x8() {
            self.x &= 0xFF;
            self.y &= 0xFF;
        }
    }

    /// Sets the accumulator respecting the current width.
    pub(crate) const fn set_a(&mut self, v: u16) {
        if self.m8() {
            self.a = (self.a & 0xFF00) | (v & 0xFF);
        } else {
            self.a = v;
        }
    }

    /// Sets X respecting the current width.
    pub(crate) const fn set_x(&mut self, v: u16) {
        self.x = if self.x8() { v & 0xFF } else { v };
    }

    /// Sets Y respecting the current width.
    pub(crate) const fn set_y(&mut self, v: u16) {
        self.y = if self.x8() { v & 0xFF } else { v };
    }
}
