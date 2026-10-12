//! Address decoding for both sides of the chip: ROM through the Super MMC
//! (with the vector overrides), BW-RAM linear and bitmap views with write
//! protection, I-RAM with its 256-byte protection bits.
//! fullsnes "SA-1 Memory Map"/"Memory Control"; ares `sa1/{rom,bwram,iram,memory}.cpp`.

use crate::bits::lo;
use crate::{Chip, Side};

/// Whether an S-CPU access to `addr` touches state the SA-1 also uses
/// (registers, I-RAM, BW-RAM), so the SA-1 must catch up first.
#[must_use]
pub(crate) const fn is_shared(addr: u32) -> bool {
    let bank = (addr >> 16) & 0xFF;
    let offset = addr & 0xFFFF;
    match bank {
        0x00..=0x3F | 0x80..=0xBF => {
            matches!(offset, 0x2200..=0x23FF | 0x3000..=0x37FF | 0x6000..=0x7FFF)
        }
        0x40..=0x4F => true,
        _ => false,
    }
}

/// `$00-$3F/$80-$BF:$8000-$FFFF` or `$C0-$FF`.
const fn is_rom(addr: u32) -> bool {
    (addr & 0x40_8000) == 0x00_8000 || (addr & 0xC0_0000) == 0xC0_0000
}

/// Low-half register window `$2200-$23FF` in the system banks.
const fn is_io(addr: u32) -> bool {
    (addr & 0x40_FE00) == 0x00_2200
}

/// `$3000-$37FF` in the system banks (I-RAM as the S-CPU sees it).
const fn is_iram_window(addr: u32) -> bool {
    (addr & 0x40_F800) == 0x00_3000
}

/// `$6000-$7FFF` in the system banks (the mappable BW-RAM block).
const fn is_bwram_window(addr: u32) -> bool {
    (addr & 0x40_E000) == 0x00_6000
}

/// Register number `$2200-$23FF` of an I/O address.
const fn reg_of(addr: u32) -> u16 {
    crate::bits::lo16(0x2200 | (addr & 0x1FF))
}

impl Chip {
    /// A ROM byte by image offset, mirrored over the image size.
    pub(crate) fn rom_byte(&self, pc: usize) -> u8 {
        let index = match self.rom_mask {
            Some(mask) => pc & mask,
            None if self.rom.is_empty() => 0,
            None => pc.checked_rem(self.rom.len()).unwrap_or(0),
        };
        self.rom.get(index).copied().unwrap_or(0)
    }

    /// Reads ROM at a CPU address through the Super MMC. LoROM banks
    /// without the bank's "LoROM" bit map the first 4 MiB linearly;
    /// otherwise, and for the HiROM banks, the block's bank register picks
    /// the 1 MiB chunk. `$00:FFEA/FFEE` are replaced by the programmable
    /// vectors (always for the SA-1; per `SCNT` for the S-CPU, plus the
    /// reset vector for the SA-1 only).
    pub(crate) fn rom_read(&self, addr: u32, side: Side) -> u8 {
        if let Some(v) = self.vector_override(addr, side) {
            return v;
        }
        let (translated, lo_half) = if (addr & 0x40_8000) == 0x00_8000 {
            (((addr & 0x80_0000) >> 2) | ((addr & 0x3F_0000) >> 1) | (addr & 0x7FFF), true)
        } else {
            (addr & 0x3F_FFFF, false)
        };
        let block = usize::try_from(translated >> 20).unwrap_or(0);
        let (bank, lorom_mode) = self.io.mmc.get(block).copied().unwrap_or((0, false));
        let pc = if lo_half && !lorom_mode {
            translated
        } else {
            (u32::from(bank) << 20) | (translated & 0x0F_FFFF)
        };
        self.rom_byte(usize::try_from(pc).unwrap_or(0))
    }

    const fn vector_override(&self, addr: u32, side: Side) -> Option<u8> {
        if addr & 0xFF_FFE0 != 0x00_FFE0 {
            return None;
        }
        let io = &self.io;
        let word = match (side, addr & 0x1F) {
            (Side::Cpu, 0x0A | 0x0B) if io.cpu_nvsw => io.snv,
            (Side::Cpu, 0x0E | 0x0F) if io.cpu_ivsw => io.siv,
            (Side::Sa1, 0x0A | 0x0B) => io.cnv,
            (Side::Sa1, 0x0E | 0x0F) => io.civ,
            (Side::Sa1, 0x1C | 0x1D) => io.crv,
            _ => return None,
        };
        let [l, h] = word.to_le_bytes();
        Some(if addr & 1 == 0 { l } else { h })
    }

    // ---- BW-RAM ----

    fn bwram_index(&self, offset: u32) -> Option<usize> {
        let len = self.bwram.len();
        if len == 0 {
            return None;
        }
        usize::try_from(offset).ok()?.checked_rem(len)
    }

    /// Raw BW-RAM read, mirrored over its size.
    pub(crate) fn bwram_read(&self, offset: u32) -> u8 {
        self.bwram_index(offset).and_then(|i| self.bwram.get(i).copied()).unwrap_or(0)
    }

    /// Raw BW-RAM write, mirrored over its size.
    pub(crate) fn bwram_write(&mut self, offset: u32, v: u8) {
        if let Some(i) = self.bwram_index(offset)
            && let Some(slot) = self.bwram.get_mut(i)
        {
            *slot = v;
        }
    }

    /// A write honouring `BWPA`: the protected area applies only while
    /// both write-enable bits are clear (ares, for Kirby's Dream Land 3).
    fn bwram_write_protected(&mut self, offset: u32, v: u8) {
        let protected = 0x100u32.wrapping_shl(u32::from(self.io.bwp));
        if !self.io.swen && !self.io.cwen && (offset & 0x3_FFFF) < protected {
            return;
        }
        self.bwram_write(offset, v);
    }

    /// The S-CPU's BW-RAM offset for `addr` (block window or `$40-$4F`).
    fn bwram_offset_cpu(&self, addr: u32) -> u32 {
        if is_bwram_window(addr) {
            (u32::from(self.io.sbm) << 13) | (addr & 0x1FFF)
        } else {
            addr & 0x0F_FFFF
        }
    }

    fn bwram_read_cpu(&mut self, addr: u32) -> u8 {
        let offset = self.bwram_offset_cpu(addr);
        if self.io.bwram_dma {
            return self.dma_cc1_read(offset);
        }
        self.bwram_read(offset)
    }

    fn bwram_write_cpu(&mut self, addr: u32, v: u8) {
        let offset = self.bwram_offset_cpu(addr);
        self.bwram_write_protected(offset, v);
    }

    /// Bitmap view (`$60-$6F`): each byte is one 2- or 4-bit pixel.
    fn bitmap_read(&self, addr: u32) -> u8 {
        if self.io.bbf {
            let shift = (addr & 3).wrapping_mul(2);
            (self.bwram_read(addr >> 2) >> shift) & 3
        } else {
            let shift = (addr & 1).wrapping_mul(4);
            (self.bwram_read(addr >> 1) >> shift) & 0x0F
        }
    }

    fn bitmap_write(&mut self, addr: u32, v: u8) {
        let (offset, shift, mask) = if self.io.bbf {
            (addr >> 2, (addr & 3).wrapping_mul(2), 3u8)
        } else {
            (addr >> 1, (addr & 1).wrapping_mul(4), 0x0Fu8)
        };
        let shift = lo(shift);
        let old = self.bwram_read(offset);
        let new = (old & !(mask << shift)) | ((v & mask) << shift);
        self.bwram_write(offset, new);
    }

    /// The SA-1's `$6000-$7FFF` window: `BMAP` picks a linear or bitmap block.
    fn bwram_read_sa1_window(&self, addr: u32) -> u8 {
        if self.io.sw46 {
            self.bitmap_read((u32::from(self.io.cbm) << 13) | (addr & 0x1FFF))
        } else {
            self.bwram_read((u32::from(self.io.cbm & 0x1F) << 13) | (addr & 0x1FFF))
        }
    }

    fn bwram_write_sa1_window(&mut self, addr: u32, v: u8) {
        if self.io.sw46 {
            self.bitmap_write((u32::from(self.io.cbm) << 13) | (addr & 0x1FFF), v);
        } else {
            let offset = (u32::from(self.io.cbm & 0x1F) << 13) | (addr & 0x1FFF);
            self.bwram_write_protected(offset, v);
        }
    }

    // ---- I-RAM ----

    /// Raw I-RAM read.
    pub(crate) fn iram_read(&self, offset: u32) -> u8 {
        self.iram.get(usize::try_from(offset & 0x7FF).unwrap_or(0)).copied().unwrap_or(0)
    }

    /// Raw I-RAM write.
    pub(crate) fn iram_write(&mut self, offset: u32, v: u8) {
        if let Some(slot) = self.iram.get_mut(usize::try_from(offset & 0x7FF).unwrap_or(0)) {
            *slot = v;
        }
    }

    /// A write gated by one of the 256-byte protection masks.
    fn iram_write_gated(&mut self, offset: u32, v: u8, mask: u8) {
        let chunk = lo((offset >> 8) & 7);
        if mask & (1 << chunk) != 0 {
            self.iram_write(offset, v);
        }
    }

    // ---- S-CPU side ----

    /// A side-effect-free S-CPU read for debuggers (no catch-up, no
    /// conversion); registers are not readable this way.
    pub(crate) fn peek_cpu(&self, addr: u32) -> Option<u8> {
        if is_io(addr) {
            return None;
        }
        if is_rom(addr) {
            return Some(self.rom_read(addr, Side::Cpu));
        }
        if is_bwram_window(addr) || (addr & 0xF0_0000) == 0x40_0000 {
            return Some(self.bwram_read(self.bwram_offset_cpu(addr)));
        }
        if is_iram_window(addr) {
            return Some(self.iram_read(addr));
        }
        None
    }

    /// An S-CPU read, if the chip decodes `addr`.
    pub(crate) fn read_cpu(&mut self, addr: u32) -> Option<u8> {
        if is_io(addr) {
            return self.read_io_cpu(reg_of(addr));
        }
        if is_rom(addr) {
            return Some(self.rom_read(addr, Side::Cpu));
        }
        if is_bwram_window(addr) || (addr & 0xF0_0000) == 0x40_0000 {
            return Some(self.bwram_read_cpu(addr));
        }
        if is_iram_window(addr) {
            return Some(self.iram_read(addr));
        }
        None
    }

    /// An S-CPU write; returns whether the chip decoded `addr`.
    pub(crate) fn write_cpu(&mut self, addr: u32, v: u8) -> bool {
        if is_io(addr) {
            self.write_io_cpu(reg_of(addr), v);
        } else if is_rom(addr) {
            // ROM: dropped.
        } else if is_bwram_window(addr) || (addr & 0xF0_0000) == 0x40_0000 {
            self.bwram_write_cpu(addr, v);
        } else if is_iram_window(addr) {
            let mask = self.io.siwp;
            self.iram_write_gated(addr, v, mask);
        } else {
            return false;
        }
        true
    }

    // ---- SA-1 side (every access costs its cycles) ----

    /// An SA-1 read (ares `SA1::read`: one cycle, two for BW-RAM).
    pub(crate) fn read_sa1(&mut self, addr: u32) -> u8 {
        self.step();
        if is_io(addr) {
            return self.read_io_sa1(reg_of(addr));
        }
        if is_rom(addr) {
            return self.rom_read(addr, Side::Sa1);
        }
        if is_bwram_window(addr)
            || (addr & 0xE0_0000) == 0x40_0000
            || (addr & 0xF0_0000) == 0x60_0000
        {
            self.step();
            return match (addr >> 20) & 0xF {
                0x6 => self.bitmap_read(addr & 0x0F_FFFF),
                0x4 | 0x5 => self.bwram_read(addr & 0x0F_FFFF),
                _ => self.bwram_read_sa1_window(addr),
            };
        }
        if (addr & 0x40_F800) == 0 || is_iram_window(addr) {
            return self.iram_read(addr);
        }
        0
    }

    /// An SA-1 write.
    pub(crate) fn write_sa1(&mut self, addr: u32, v: u8) {
        self.step();
        if is_io(addr) {
            self.write_io_sa1(reg_of(addr), v);
        } else if is_rom(addr) {
            // ROM: dropped.
        } else if is_bwram_window(addr)
            || (addr & 0xE0_0000) == 0x40_0000
            || (addr & 0xF0_0000) == 0x60_0000
        {
            self.step();
            match (addr >> 20) & 0xF {
                0x6 => self.bitmap_write(addr & 0x0F_FFFF, v),
                0x4 | 0x5 => self.bwram_write_protected(addr & 0x0F_FFFF, v),
                _ => self.bwram_write_sa1_window(addr, v),
            }
        } else if (addr & 0x40_F800) == 0 || is_iram_window(addr) {
            let mask = self.io.ciwp;
            self.iram_write_gated(addr, v, mask);
        }
    }

    /// The bit reader's view of memory: ROM, BW-RAM and I-RAM without
    /// registers or timing (ares `SA1::readVBR`).
    pub(crate) fn vbr_read(&self, addr: u32) -> u8 {
        if is_rom(addr) {
            self.rom_read(addr, Side::Sa1)
        } else if is_bwram_window(addr) || (addr & 0xF0_0000) == 0x40_0000 {
            self.bwram_read(addr & 0x0F_FFFF)
        } else if (addr & 0x40_F800) == 0 || is_iram_window(addr) {
            self.iram_read(addr)
        } else {
            0xFF
        }
    }
}
