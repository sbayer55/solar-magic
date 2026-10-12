//! The eight DMA/HDMA channels: register file here, the transfer engines in
//! `gpdma.rs` and `hdma.rs`. fullsnes "SNES DMA Transfers"; timings follow
//! bsnes (8 master cycles per byte, 8 per channel, 8 to start).

mod gpdma;
mod hdma;

use crate::bits::{hi, lo};

/// Bytes per transfer unit, by `DMAP` mode.
pub(crate) const UNIT_LEN: [u8; 8] = [1, 2, 2, 4, 4, 4, 2, 4];

/// B-bus address offset for each byte of a unit, by mode.
pub(crate) const B_PATTERN: [[u8; 4]; 8] = [
    [0, 0, 0, 0],
    [0, 1, 0, 1],
    [0, 0, 0, 0],
    [0, 0, 1, 1],
    [0, 1, 2, 3],
    [0, 1, 0, 1],
    [0, 0, 0, 0],
    [0, 0, 1, 1],
];

/// One channel's registers (`$43x0-$43xB`) and HDMA state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channel {
    /// `DMAP`: direction (7), indirect (6), A step (3-4), mode (0-2).
    pub params: u8,
    /// `BBAD`: B-bus address low byte.
    pub b_addr: u8,
    /// `A1T`: A-bus address / HDMA table start.
    pub a_addr: u16,
    /// `A1B`: A-bus bank.
    pub a_bank: u8,
    /// `DAS`: byte count / HDMA indirect address.
    pub count: u16,
    /// `DASB`: HDMA indirect bank.
    pub indirect_bank: u8,
    /// `A2A`: HDMA current table address.
    pub table_addr: u16,
    /// `NTRL`: HDMA line counter (bit 7 = repeat).
    pub line_counter: u8,
    /// `$43xB`/`$43xF`: unused byte.
    pub unused: u8,
    /// This frame's HDMA on this channel has terminated.
    pub hdma_completed: bool,
    /// Transfer a unit on the next HDMA line.
    pub hdma_do_transfer: bool,
}

impl Default for Channel {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel {
    /// Power-on: all `FF`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            params: 0xFF,
            b_addr: 0xFF,
            a_addr: 0xFFFF,
            a_bank: 0xFF,
            count: 0xFFFF,
            indirect_bank: 0xFF,
            table_addr: 0xFFFF,
            line_counter: 0xFF,
            unused: 0xFF,
            hdma_completed: false,
            hdma_do_transfer: false,
        }
    }

    /// `DMAP` bit 7: B-bus to A-bus.
    #[must_use]
    pub const fn b_to_a(&self) -> bool {
        self.params & 0x80 != 0
    }

    /// `DMAP` bit 6: indirect HDMA table.
    #[must_use]
    pub const fn indirect(&self) -> bool {
        self.params & 0x40 != 0
    }

    /// `DMAP` bits 0-2.
    #[must_use]
    pub const fn mode(&self) -> usize {
        (self.params & 7) as usize
    }

    /// Bytes in one unit.
    #[must_use]
    pub const fn unit_len(&self) -> u8 {
        UNIT_LEN[self.mode()]
    }

    /// B-bus register for byte `index` of a unit.
    #[must_use]
    pub const fn b_reg(&self, index: usize) -> u8 {
        self.b_addr.wrapping_add(B_PATTERN[self.mode()][index & 3])
    }

    /// Steps the A-bus address after one GPDMA byte (`DMAP` bits 3-4).
    pub const fn step_a(&mut self) {
        match (self.params >> 3) & 3 {
            0 => self.a_addr = self.a_addr.wrapping_add(1),
            2 => self.a_addr = self.a_addr.wrapping_sub(1),
            _ => {}
        }
    }
}

/// All channels plus the enable registers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dma {
    /// Channels 0-7.
    pub channels: [Channel; 8],
    /// `MDMAEN`: channels still to transfer.
    pub gp_enable: u8,
    /// `HDMAEN`.
    pub hdma_enable: u8,
    /// A `MDMAEN` write that has not started yet.
    pub gp_pending: bool,
}

impl Default for Dma {
    fn default() -> Self {
        Self::new()
    }
}

impl Dma {
    /// Power-on state.
    #[must_use]
    pub const fn new() -> Self {
        Self { channels: [Channel::new(); 8], gp_enable: 0, hdma_enable: 0, gp_pending: false }
    }

    /// Reads `$4300 + offset`; `None` for the open-bus holes.
    #[must_use]
    pub fn read_reg(&self, offset: u16) -> Option<u8> {
        let ch = self.channels.get(usize::from((offset >> 4) & 7))?;
        Some(match offset & 0xF {
            0x0 => ch.params,
            0x1 => ch.b_addr,
            0x2 => lo(ch.a_addr),
            0x3 => hi(ch.a_addr),
            0x4 => ch.a_bank,
            0x5 => lo(ch.count),
            0x6 => hi(ch.count),
            0x7 => ch.indirect_bank,
            0x8 => lo(ch.table_addr),
            0x9 => hi(ch.table_addr),
            0xA => ch.line_counter,
            0xB | 0xF => ch.unused,
            _ => return None,
        })
    }

    /// Writes `$4300 + offset`.
    pub fn write_reg(&mut self, offset: u16, v: u8) {
        let Some(ch) = self.channels.get_mut(usize::from((offset >> 4) & 7)) else {
            return;
        };
        match offset & 0xF {
            0x0 => ch.params = v,
            0x1 => ch.b_addr = v,
            0x2 => ch.a_addr = (ch.a_addr & 0xFF00) | u16::from(v),
            0x3 => ch.a_addr = (u16::from(v) << 8) | (ch.a_addr & 0xFF),
            0x4 => ch.a_bank = v,
            0x5 => ch.count = (ch.count & 0xFF00) | u16::from(v),
            0x6 => ch.count = (u16::from(v) << 8) | (ch.count & 0xFF),
            0x7 => ch.indirect_bank = v,
            0x8 => ch.table_addr = (ch.table_addr & 0xFF00) | u16::from(v),
            0x9 => ch.table_addr = (u16::from(v) << 8) | (ch.table_addr & 0xFF),
            0xA => ch.line_counter = v,
            0xB | 0xF => ch.unused = v,
            _ => {}
        }
    }
}

/// Whether an A-bus address is one the DMA unit refuses to touch (the B bus
/// and its own registers), reading open bus instead.
pub(crate) const fn a_bus_valid(addr: u32) -> bool {
    let bank = crate::bits::bank(addr);
    let offset = addr & 0xFFFF;
    if bank & 0x40 != 0 {
        return true;
    }
    !matches!(offset, 0x2100..=0x21FF | 0x4016..=0x4017 | 0x4200..=0x43FF)
}

/// Whether an A-bus address is work RAM (a WRAM↔`$2180` transfer is void).
pub(crate) const fn a_bus_is_wram(addr: u32) -> bool {
    let bank = crate::bits::bank(addr);
    let offset = addr & 0xFFFF;
    matches!(bank, 0x7E | 0x7F) || (bank & 0x40 == 0 && offset < 0x2000)
}
