//! The 5A22's own registers: `$4016/$4017` and `$4200-$421F`.

pub mod alu;
pub mod irq;
pub mod joypad;

pub use alu::Alu;
pub use irq::{Interrupts, IrqMode};
pub use joypad::Joypads;

/// CPU-side register state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuRegs {
    /// Interrupt enables and flags.
    pub irq: Interrupts,
    /// Multiply/divide unit.
    pub alu: Alu,
    /// Controller ports.
    pub joypads: Joypads,
    /// `NMITIMEN` bit 0.
    pub auto_joypad: bool,
    /// `WRIO`.
    pub wrio: u8,
    /// `MEMSEL` bit 0: FastROM access in banks `$80-$FF`.
    pub fast_rom: bool,
    /// 5A22 revision reported in `RDNMI` bits 0-3.
    pub cpu_version: u8,
}

impl Default for CpuRegs {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuRegs {
    /// Power-on values.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            irq: Interrupts::new(),
            alu: Alu::new(),
            joypads: Joypads::new(),
            auto_joypad: false,
            wrio: 0xFF,
            fast_rom: false,
            cpu_version: 2,
        }
    }
}
