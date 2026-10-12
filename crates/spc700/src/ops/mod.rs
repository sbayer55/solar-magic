//! Instruction implementations, grouped by family.

pub(crate) mod alu;
pub(crate) mod bits;
pub(crate) mod branch;
pub(crate) mod misc;
pub(crate) mod mov;
pub(crate) mod word;

use crate::addressing::Mode;

/// A register operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reg {
    /// Accumulator.
    A,
    /// X index.
    X,
    /// Y index.
    Y,
    /// Stack pointer.
    Sp,
    /// Status word.
    Psw,
}

/// Arithmetic/logic operations on bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Alu {
    Or,
    And,
    Eor,
    Cmp,
    Adc,
    Sbc,
}

/// Read-modify-write operations on bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rmw {
    Asl,
    Rol,
    Lsr,
    Ror,
    Inc,
    Dec,
}

/// Branch conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cond {
    Pl,
    Mi,
    Vc,
    Vs,
    Cc,
    Cs,
    Ne,
    Eq,
    Always,
}

/// The parameter each table entry hands its handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum P {
    /// Nothing.
    None,
    /// A bit number 0-7 (SET1/CLR1/BBS/BBC) or a TCALL number 0-15.
    Num(u8),
    /// ALU with a memory operand against A.
    Alu(Alu, Mode),
    /// Read-modify-write on memory.
    Rmw(Rmw, Mode),
    /// Read-modify-write on a register.
    RmwReg(Rmw, Reg),
    /// Load a register from memory.
    Load(Reg, Mode),
    /// Store a register to memory.
    Store(Reg, Mode),
    /// A register (immediate loads, pushes, pops, compares).
    Reg(Reg),
    /// Register to register (`MOV dst, src`).
    Reg2(Reg, Reg),
    /// Branch condition.
    Cond(Cond),
    /// ALU with two memory operands or memory and immediate.
    AluMem(Alu),
}
