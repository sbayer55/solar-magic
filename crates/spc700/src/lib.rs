//! The Sony SPC700 core of the SNES sound unit.
//!
//! Every bus access is exactly one SPC cycle and every internal cycle is an
//! [`ApuBus::idle`] call, so instruction timing is emergent and the owner
//! of the bus (the S-SMP) counts cycles. Dispatch is a 256-entry table of
//! small functions parameterised by operand shape (`table.rs`).

mod addressing;
mod bits;
mod ops;
mod psw;
mod table;

pub use psw::Psw;

/// Memory as the core sees it: 64 KiB, one cycle per access.
pub trait ApuBus {
    /// Reads one byte (one cycle).
    fn read(&mut self, addr: u16) -> u8;
    /// Writes one byte (one cycle).
    fn write(&mut self, addr: u16, value: u8);
    /// One internal cycle with no bus activity.
    fn idle(&mut self);
}

/// SPC700 register state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spc700 {
    /// Accumulator.
    pub a: u8,
    /// X index.
    pub x: u8,
    /// Y index.
    pub y: u8,
    /// Stack pointer (page 1).
    pub sp: u8,
    /// Program counter.
    pub pc: u16,
    /// Status flags.
    pub psw: Psw,
    /// `SLEEP`/`STOP` executed; only a reset restarts the core.
    pub stopped: bool,
    /// Instructions executed so far.
    pub steps: u64,
}

impl Default for Spc700 {
    fn default() -> Self {
        Self::new()
    }
}

impl Spc700 {
    /// Power-on state before the reset vector is read.
    #[must_use]
    pub const fn new() -> Self {
        Self { a: 0, x: 0, y: 0, sp: 0xEF, pc: 0xFFC0, psw: Psw::new(), stopped: false, steps: 0 }
    }

    /// Reset: `PC` from `$FFFE`, everything else as power-on.
    pub fn reset(&mut self, bus: &mut dyn ApuBus) {
        *self = Self::new();
        let lo = bus.read(0xFFFE);
        let hi = bus.read(0xFFFF);
        self.pc = u16::from_le_bytes([lo, hi]);
    }

    /// `YA` as a word.
    #[must_use]
    pub const fn ya(&self) -> u16 {
        u16::from_le_bytes([self.a, self.y])
    }

    /// Sets `YA`.
    pub const fn set_ya(&mut self, v: u16) {
        let [a, y] = v.to_le_bytes();
        self.a = a;
        self.y = y;
    }

    /// Executes one instruction. Does nothing once stopped.
    pub fn step(&mut self, bus: &mut dyn ApuBus) {
        if self.stopped {
            return;
        }
        let opcode = self.fetch(bus);
        self.steps = self.steps.saturating_add(1);
        let (f, p) = table::OPCODES.get(usize::from(opcode)).copied().unwrap_or(table::OPCODES[0]);
        f(self, bus, p);
    }
}
