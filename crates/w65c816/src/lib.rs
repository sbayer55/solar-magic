//! A small 65c816 interpreter.
//!
//! The goal is functional accuracy for ordinary code (all instructions, all
//! addressing modes, native and emulation mode, decimal arithmetic) without
//! cycle counting or hardware. Memory is abstracted behind [`Bus`]; the CPU
//! only ever asks it to read or write a byte at a 24-bit address.
//!
//! Dispatch is a 256-entry table (`OPCODES` in `table.rs`) of small functions,
//! one per instruction family, so no single function is large.
//!
//! Timing is reported, not counted: every bus access is one cycle and every
//! internal cycle is a [`Bus::idle`] call, so a timed bus can be exact while
//! an untimed one pays nothing. Hardware interrupts enter through
//! [`Cpu::nmi`] and [`Cpu::irq`].

mod addressing;
mod bits;
mod bus;
mod cpu;
mod disasm;
mod flags;
mod interrupt;
mod ops;
mod table;

pub use bus::{Bus, FlatBus};
pub use cpu::{Cpu, CpuError, Exec, StopReason};
pub use disasm::{Disassembly, disassemble};
pub use flags::Flags;

/// A 24-bit address on the bus.
pub type Addr = u32;

/// Masks a value to 24 bits.
#[must_use]
pub const fn addr24(v: u32) -> Addr {
    v & 0x00FF_FFFF
}
