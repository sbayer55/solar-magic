//! A small 65c816 interpreter.
//!
//! The goal is functional accuracy for ordinary code (all instructions, all
//! addressing modes, native and emulation mode, decimal arithmetic) without
//! cycle counting or hardware. Memory is abstracted behind [`Bus`]; the CPU
//! only ever asks it to read or write a byte at a 24-bit address.
//!
//! Dispatch is a 256-entry table (`OPCODES` in `table.rs`) of small functions,
//! one per instruction family, so no single function is large.

mod addressing;
mod bits;
mod bus;
mod cpu;
mod flags;
mod ops;
mod table;

pub use bus::{Bus, FlatBus};
pub use cpu::{Cpu, CpuError, StopReason};
pub use flags::Flags;

/// A 24-bit address on the bus.
pub type Addr = u32;

/// Masks a value to 24 bits.
#[must_use]
pub const fn addr24(v: u32) -> Addr {
    v & 0x00FF_FFFF
}
