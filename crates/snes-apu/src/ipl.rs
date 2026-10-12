//! The 64-byte boot ROM, modelled at the level the S-CPU can observe.
//!
//! Nintendo's ROM is copyrighted and is not shipped; by default its
//! behaviour is reproduced by a state machine that follows the disassembly
//! in fullsnes ("Boot ROM Disassembly", read 2026-10-11) phase by phase,
//! with the cycle costs of the real instructions. A dumped ROM can be
//! supplied instead with [`Ipl::Rom`], in which case the core executes it.

use spc700::Spc700;

use crate::smp::Smp;

/// How `$FFC0-$FFFF` is provided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ipl {
    /// Port-level model of the boot ROM (default).
    Hle,
    /// A user-supplied 64-byte dump, executed by the core.
    Rom(Vec<u8>),
}

/// Where the modelled boot ROM is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IplPhase {
    /// `$FFC0`: zero-fill RAM, announce `$AA/$BB`.
    Entry,
    /// `$FFCF`: waiting for the S-CPU's `$CC`.
    WaitKick,
    /// `$FFEF`: read the address and command, acknowledge.
    Main,
    /// `$FFD6`: waiting for the S-CPU's index 0.
    WaitZero,
    /// `$FFDA`: receiving bytes; `y` is the next expected index.
    Transfer {
        /// Next index.
        y: u8,
    },
}

/// What a read of the boot ROM region returns under the model.
pub(crate) const fn hle_rom_byte(_addr: u16) -> u8 {
    0xFF
}

/// Cycle costs of the ROM's loops (sum of its instructions' cycles).
const ZERO_FILL_ITERATION: u64 = 10;
const WAIT_KICK_ITERATION: u64 = 9;
const MAIN_CYCLES: u64 = 29;
const WAIT_ZERO_ITERATION: u64 = 7;
const TRANSFER_POLL: u64 = 11;
const TRANSFER_BYTE: u64 = 27;
const JUMP_CYCLES: u64 = 6;

/// Aligns the model's phase to where the core jumped in.
const fn phase_for(pc: u16) -> IplPhase {
    match pc {
        0xFFC0..=0xFFCE => IplPhase::Entry,
        0xFFCF..=0xFFD5 => IplPhase::WaitKick,
        0xFFD6..=0xFFD9 => IplPhase::WaitZero,
        0xFFDA..=0xFFEE => IplPhase::Transfer { y: 0 },
        _ => IplPhase::Main,
    }
}

/// Runs one bounded piece of the boot ROM.
pub(crate) fn step(cpu: &mut Spc700, smp: &mut Smp) {
    if cpu.pc != 0xFFFF {
        smp.ipl_phase = phase_for(cpu.pc);
        // Park the core on the ROM's last byte while the model runs.
        cpu.pc = 0xFFFF;
    }
    match smp.ipl_phase {
        IplPhase::Entry => entry(cpu, smp),
        IplPhase::WaitKick => {
            smp.idle_cycles(WAIT_KICK_ITERATION);
            if smp.inbox[0] == 0xCC {
                smp.ipl_phase = IplPhase::Main;
            }
        }
        IplPhase::Main => main(cpu, smp),
        IplPhase::WaitZero => {
            smp.idle_cycles(WAIT_ZERO_ITERATION);
            if smp.inbox[0] == 0 {
                smp.ipl_phase = IplPhase::Transfer { y: 0 };
            }
        }
        IplPhase::Transfer { y } => transfer(smp, y),
    }
}

fn entry(cpu: &mut Spc700, smp: &mut Smp) {
    for addr in 0x0001..=0x00EFu16 {
        smp.set_ram_byte(addr, 0);
    }
    smp.idle_cycles(ZERO_FILL_ITERATION.wrapping_mul(0xEF));
    cpu.sp = 0xEF;
    cpu.a = 0;
    cpu.x = 0;
    smp.outbox[0] = 0xAA;
    smp.outbox[1] = 0xBB;
    smp.idle_cycles(10);
    smp.ipl_phase = IplPhase::WaitKick;
}

/// `main`: copy the address from `$F6/$F7` to `$0000`, acknowledge the kick
/// byte, then transfer (command ≠ 0) or jump (command = 0).
fn main(cpu: &mut Spc700, smp: &mut Smp) {
    smp.idle_cycles(MAIN_CYCLES);
    smp.set_ram_byte(0x0000, smp.inbox[2]);
    smp.set_ram_byte(0x0001, smp.inbox[3]);
    let kick = smp.inbox[0];
    let command = smp.inbox[1];
    smp.outbox[0] = kick;
    cpu.a = kick;
    cpu.y = command;
    cpu.x = command;
    if command != 0 {
        smp.ipl_phase = IplPhase::WaitZero;
    } else {
        smp.idle_cycles(JUMP_CYCLES);
        let lo = smp.ram_byte(0x0000);
        let hi = smp.ram_byte(0x0001);
        cpu.pc = u16::from_le_bytes([lo, hi]);
        cpu.a = 0;
        cpu.x = 0;
        cpu.y = 0;
        cpu.sp = 0xEF;
        cpu.psw = spc700::Psw::from_byte(0x02);
        smp.ipl_phase = IplPhase::Entry;
    }
}

/// `@@transfer_lop`: when `$F4` equals the expected index, store the byte
/// from `$F5` and acknowledge; when it has moved past it, return to `main`.
fn transfer(smp: &mut Smp, y: u8) {
    let index = smp.inbox[0];
    if index == y {
        smp.idle_cycles(TRANSFER_BYTE);
        let data = smp.inbox[1];
        smp.outbox[0] = y;
        let lo = smp.ram_byte(0x0000);
        let hi = smp.ram_byte(0x0001);
        let base = u16::from_le_bytes([lo, hi]);
        smp.set_ram_byte(base.wrapping_add(u16::from(y)), data);
        let next = y.wrapping_add(1);
        if next == 0 {
            // `inc [01]`: the page advances with the index wrap.
            smp.set_ram_byte(0x0001, hi.wrapping_add(1));
        }
        smp.ipl_phase = IplPhase::Transfer { y: next };
    } else if y.wrapping_sub(index) & 0x80 != 0 {
        // `cmp y,[F4]` went negative: the S-CPU signalled the end of the block.
        smp.idle_cycles(TRANSFER_POLL);
        smp.ipl_phase = IplPhase::Main;
    } else {
        smp.idle_cycles(TRANSFER_POLL);
    }
}
