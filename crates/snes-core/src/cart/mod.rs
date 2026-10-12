//! The cartridge: ROM through the mapper, battery RAM, and an optional
//! coprocessor that may claim addresses before the plain mapping does.

mod sram;

use std::sync::Arc;

use snes_rom::{Mapper, MapperKind, Rom, SnesAddr};

pub use sram::Sram;

/// A chip on the cartridge that sits on the bus (SA-1 later).
pub trait Coprocessor: core::fmt::Debug + Send {
    /// Reads `addr` if the chip decodes it; `now` is the master clock.
    fn read(&mut self, addr: u32, now: u64) -> Option<u8>;
    /// Writes `addr` if the chip decodes it; returns whether it did.
    fn write(&mut self, addr: u32, v: u8, now: u64) -> bool;
    /// Lets the chip catch up to the master clock.
    fn run_to(&mut self, now: u64);
    /// Whether the chip asserts the cartridge IRQ line.
    fn irq_pending(&self) -> bool;
    /// A deep copy, for save states.
    fn boxed_clone(&self) -> Box<dyn Coprocessor>;
}

impl Clone for Box<dyn Coprocessor> {
    fn clone(&self) -> Self {
        self.boxed_clone()
    }
}

/// Everything plugged into the cartridge slot.
#[derive(Debug, Clone)]
pub struct Cartridge {
    rom: Arc<[u8]>,
    /// Mask for power-of-two images; `None` means use modulo.
    rom_mask: Option<usize>,
    mapper: MapperKind,
    /// Battery RAM (may be empty).
    pub sram: Sram,
    /// `FFD5` bit 4: the game expects 3.58 MHz access in banks `$80+`.
    pub fast_rom: bool,
    /// Optional coprocessor.
    pub copro: Option<Box<dyn Coprocessor>>,
}

impl Cartridge {
    /// Wraps a loaded ROM; SRAM is sized from the header (2 KiB for SMW).
    #[must_use]
    pub fn new(rom: &Rom) -> Self {
        let sram_len = usize::try_from(rom.header().sram_size_bytes().unwrap_or(0)).unwrap_or(0);
        Self::from_bytes(rom.bytes(), rom.mapper_kind(), sram_len, rom.header().is_fast_rom())
    }

    /// Wraps raw image bytes with an explicit mapping (for header-less test
    /// ROMs that `Rom::from_bytes` would refuse).
    #[must_use]
    pub fn from_bytes(image: &[u8], mapper: MapperKind, sram_len: usize, fast_rom: bool) -> Self {
        let bytes: Arc<[u8]> = Arc::from(image);
        let len = bytes.len();
        let rom_mask = if len.is_power_of_two() { Some(len.wrapping_sub(1)) } else { None };
        Self { rom: bytes, rom_mask, mapper, sram: Sram::new(sram_len), fast_rom, copro: None }
    }

    /// The ROM image.
    #[must_use]
    pub const fn rom(&self) -> &Arc<[u8]> {
        &self.rom
    }

    /// The address mapping.
    #[must_use]
    pub const fn mapper(&self) -> &MapperKind {
        &self.mapper
    }

    /// A ROM byte by file offset, mirrored over the image size.
    #[must_use]
    pub fn rom_byte(&self, pc: usize) -> u8 {
        let index = match self.rom_mask {
            Some(mask) => pc & mask,
            None if self.rom.is_empty() => 0,
            None => pc.checked_rem(self.rom.len()).unwrap_or(0),
        };
        self.rom.get(index).copied().unwrap_or(0)
    }

    /// Whether `addr` is LoROM-style battery RAM (`$70-$7D`/`$F0-$FF:$0000-$7FFF`).
    const fn is_lorom_sram(addr: u32) -> bool {
        let bank = crate::bits::bank(addr);
        let offset = addr & 0xFFFF;
        offset < 0x8000 && matches!(bank, 0x70..=0x7D | 0xF0..=0xFF)
    }

    /// Reads a cartridge address; `None` means open bus.
    pub fn read(&mut self, addr: u32, now: u64) -> Option<u8> {
        if let Some(copro) = self.copro.as_mut()
            && let Some(v) = copro.read(addr, now)
        {
            return Some(v);
        }
        if matches!(self.mapper, MapperKind::LoRom(_) | MapperKind::ExLoRom(_))
            && Self::is_lorom_sram(addr)
        {
            return self.sram.read(addr);
        }
        let pc = self.mapper.to_pc(SnesAddr::new(addr)?)?;
        Some(self.rom_byte(pc))
    }

    /// Writes a cartridge address (ROM writes are dropped).
    pub fn write(&mut self, addr: u32, v: u8, now: u64) {
        if let Some(copro) = self.copro.as_mut()
            && copro.write(addr, v, now)
        {
            return;
        }
        if matches!(self.mapper, MapperKind::LoRom(_) | MapperKind::ExLoRom(_))
            && Self::is_lorom_sram(addr)
        {
            self.sram.write(addr, v);
        }
    }
}
