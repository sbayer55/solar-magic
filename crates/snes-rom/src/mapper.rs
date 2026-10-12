//! SNES address-space to file-offset mapping.
//!
//! See `docs/formats/rom-mapping.md`.

use core::fmt;

use crate::SnesAddr;

/// Translates between SNES bus addresses and byte offsets ("PC addresses") in the ROM image.
pub trait Mapper: fmt::Debug + Send + Sync {
    /// Short human-readable name, e.g. `"LoROM"`.
    fn name(&self) -> &'static str;

    /// Maps a bus address to a file offset, or `None` if the address is not ROM
    /// (hardware registers, WRAM, SRAM, open bus).
    fn to_pc(&self, addr: SnesAddr) -> Option<usize>;

    /// Maps a file offset back to the canonical bus address that reads it, or
    /// `None` if the offset is beyond what the mapping can address.
    fn to_snes(&self, pc: usize) -> Option<SnesAddr>;

    /// Largest image size, in bytes, that this mapping can address.
    fn max_size(&self) -> usize;
}

/// The closed set of mappers a [`crate::Rom`] can use.
///
/// An enum rather than a trait object so that `Rom` is `Clone`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MapperKind {
    /// Standard LoROM.
    LoRom(LoRom),
    /// Lunar Magic's 8 MiB ExLoROM (map mode `$32`).
    ExLoRom(ExLoRom),
    /// SA-1 cartridge at its power-on Super MMC setting (map mode `$23`).
    Sa1(Sa1Rom),
}

impl Mapper for MapperKind {
    fn name(&self) -> &'static str {
        match self {
            Self::LoRom(m) => m.name(),
            Self::ExLoRom(m) => m.name(),
            Self::Sa1(m) => m.name(),
        }
    }

    fn to_pc(&self, addr: SnesAddr) -> Option<usize> {
        match self {
            Self::LoRom(m) => m.to_pc(addr),
            Self::ExLoRom(m) => m.to_pc(addr),
            Self::Sa1(m) => m.to_pc(addr),
        }
    }

    fn to_snes(&self, pc: usize) -> Option<SnesAddr> {
        match self {
            Self::LoRom(m) => m.to_snes(pc),
            Self::ExLoRom(m) => m.to_snes(pc),
            Self::Sa1(m) => m.to_snes(pc),
        }
    }

    fn max_size(&self) -> usize {
        match self {
            Self::LoRom(m) => m.max_size(),
            Self::ExLoRom(m) => m.max_size(),
            Self::Sa1(m) => m.max_size(),
        }
    }
}

/// Standard LoROM ("mode 20") mapping.
///
/// Each 32 KiB half-bank of ROM appears at `$8000-$FFFF` of consecutive banks,
/// so `PC = ((bank & 0x7F) << 15) | (offset & 0x7FFF)`. Banks `$7E`/`$7F` are
/// WRAM and never ROM; their ROM contents are reached through the mirror banks
/// `$FE`/`$FF`. The lower half (`$0000-$7FFF`) of every bank is treated as
/// non-ROM even though some cartridges mirror ROM there, because no SMW data
/// is addressed that way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoRom;

impl LoRom {
    /// Largest file offset addressable by plain LoROM: 128 half-banks of 32 KiB.
    pub const MAX_SIZE: usize = 0x80 << 15;
}

impl Mapper for LoRom {
    fn name(&self) -> &'static str {
        "LoROM"
    }

    fn to_pc(&self, addr: SnesAddr) -> Option<usize> {
        let bank = addr.bank();
        let offset = addr.offset();
        if matches!(bank, 0x7E | 0x7F) || offset < 0x8000 {
            return None;
        }
        let half_bank = usize::from(bank & 0x7F);
        let within = usize::from(offset & 0x7FFF);
        Some((half_bank << 15) | within)
    }

    fn to_snes(&self, pc: usize) -> Option<SnesAddr> {
        if pc >= Self::MAX_SIZE {
            return None;
        }
        let half_bank = u8::try_from(pc >> 15).ok()?;
        // Half-banks $7E/$7F are only reachable via the upper mirror.
        let bank = if matches!(half_bank, 0x7E | 0x7F) { half_bank | 0x80 } else { half_bank };
        let offset = u16::try_from(pc & 0x7FFF).ok()? | 0x8000;
        Some(SnesAddr::from_parts(bank, offset))
    }

    fn max_size(&self) -> usize {
        Self::MAX_SIZE
    }
}

/// Lunar Magic's ExLoROM ("mode 32") mapping for ROMs above 4 MiB.
///
/// The bank's top bit is inverted relative to plain LoROM: banks `$80-$FF`
/// hold the first 4 MiB exactly as LoROM would, and banks `$00-$7D` hold the
/// second 4 MiB (`PC = 0x40_0000 + ((bank & 0x7F) << 15 | (offset & 0x7FFF))`).
/// Banks `$40-$7D` and `$C0-$FF` also mirror their upper half into
/// `$0000-$7FFF`; the system banks only expose `$8000-$FFFF`. Half-banks
/// `$7E`/`$7F` of the second half are unreachable (WRAM sits there).
///
/// Matches the extended-LoROM map used by snes9x/bsnes; not yet verified
/// against a Lunar Magic-expanded ROM (`docs/formats/rom-mapping.md`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExLoRom;

impl ExLoRom {
    /// Two 4 MiB halves.
    pub const MAX_SIZE: usize = 0x80_0000;
    /// File offset of the second half.
    const UPPER: usize = 0x40_0000;
}

impl Mapper for ExLoRom {
    fn name(&self) -> &'static str {
        "ExLoROM"
    }

    fn to_pc(&self, addr: SnesAddr) -> Option<usize> {
        let bank = addr.bank();
        let offset = addr.offset();
        let system_bank = matches!(bank, 0x00..=0x3F | 0x80..=0xBF);
        if matches!(bank, 0x7E | 0x7F) || (system_bank && offset < 0x8000) {
            return None;
        }
        let half = (usize::from(bank & 0x7F) << 15) | usize::from(offset & 0x7FFF);
        Some(if bank & 0x80 == 0 { half.checked_add(Self::UPPER)? } else { half })
    }

    fn to_snes(&self, pc: usize) -> Option<SnesAddr> {
        if pc >= Self::MAX_SIZE {
            return None;
        }
        let (half_bank, bank_bit) = if pc < Self::UPPER {
            (u8::try_from(pc >> 15).ok()?, 0x80)
        } else {
            (u8::try_from(pc.checked_sub(Self::UPPER)? >> 15).ok()?, 0x00)
        };
        if bank_bit == 0 && matches!(half_bank, 0x7E | 0x7F) {
            return None;
        }
        let offset = u16::try_from(pc & 0x7FFF).ok()? | 0x8000;
        Some(SnesAddr::from_parts(half_bank | bank_bit, offset))
    }

    fn max_size(&self) -> usize {
        Self::MAX_SIZE
    }
}

/// An SA-1 cartridge's ROM as the S-CPU sees it at power-on.
///
/// Before the game reprograms the Super MMC (`$2220-$2223`), banks
/// `$00-$3F:$8000+` are the first 2 MiB in 32 KiB pieces, `$80-$BF:$8000+`
/// the next 2 MiB, and `$C0-$FF` the same 4 MiB as four 1 MiB HiROM-style
/// blocks. BW-RAM (`$40-$4F`, and the `$6000-$7FFF` window) and I-RAM are
/// not ROM.
///
/// Verified against fullsnes "SNES Cart SA-1 Memory Control" (with the LoROM
/// bit clear the first 2 MiB map to `00h-3Fh` and the next 2 MiB to
/// `80h-BFh`), 2026-10-11. The live mapping belongs to the emulator.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sa1Rom;

impl Sa1Rom {
    /// Eight 1 MiB Super MMC blocks.
    pub const MAX_SIZE: usize = 0x80_0000;
    /// Bytes reachable without reprogramming the Super MMC.
    const STATIC_SIZE: usize = 0x40_0000;
}

impl Mapper for Sa1Rom {
    fn name(&self) -> &'static str {
        "SA-1"
    }

    fn to_pc(&self, addr: SnesAddr) -> Option<usize> {
        let bank = addr.bank();
        let offset = addr.offset();
        let lo_half = |b: u8| (usize::from(b) << 15) | usize::from(offset & 0x7FFF);
        match bank {
            0x00..=0x3F if offset >= 0x8000 => Some(lo_half(bank)),
            0x80..=0xBF if offset >= 0x8000 => Some(0x20_0000 | lo_half(bank & 0x3F)),
            0xC0..=0xFF => Some((usize::from(bank & 0x3F) << 16) | usize::from(offset)),
            _ => None,
        }
    }

    fn to_snes(&self, pc: usize) -> Option<SnesAddr> {
        if pc >= Self::STATIC_SIZE {
            return None;
        }
        let half_bank = u8::try_from((pc >> 15) & 0x3F).ok()?;
        let bank = if pc < 0x20_0000 { half_bank } else { half_bank | 0x80 };
        let offset = u16::try_from(pc & 0x7FFF).ok()? | 0x8000;
        Some(SnesAddr::from_parts(bank, offset))
    }

    fn max_size(&self) -> usize {
        Self::MAX_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::{ExLoRom, LoRom, Mapper, Sa1Rom};
    use crate::SnesAddr;

    #[test]
    fn exlorom_to_pc() {
        let m = ExLoRom;
        // Upper bank bit set: plain LoROM over the first 4 MiB.
        assert_eq!(m.to_pc(SnesAddr::lit(0x80_8000)), Some(0x00_0000));
        assert_eq!(m.to_pc(SnesAddr::lit(0x85_E000)), Some(0x02_E000));
        assert_eq!(m.to_pc(SnesAddr::lit(0xFF_FFFF)), Some(0x3F_FFFF));
        assert_eq!(m.to_pc(SnesAddr::lit(0xC0_0000)), Some(0x20_0000));
        // Bit clear: the second 4 MiB.
        assert_eq!(m.to_pc(SnesAddr::lit(0x00_8000)), Some(0x40_0000));
        assert_eq!(m.to_pc(SnesAddr::lit(0x05_E000)), Some(0x42_E000));
        assert_eq!(m.to_pc(SnesAddr::lit(0x40_0000)), Some(0x60_0000));
        assert_eq!(m.to_pc(SnesAddr::lit(0x7D_FFFF)), Some(0x7E_FFFF));
        assert_eq!(m.to_pc(SnesAddr::lit(0x00_7FFF)), None);
        assert_eq!(m.to_pc(SnesAddr::lit(0x7E_8000)), None);
    }

    #[test]
    fn exlorom_to_snes_round_trips() {
        let m = ExLoRom;
        for pc in [0usize, 0x02_E000, 0x3F_FFFF, 0x40_0000, 0x42_E000, 0x7E_FFFF] {
            let snes = m.to_snes(pc).expect("in range");
            assert_eq!(m.to_pc(snes), Some(pc), "pc {pc:#X} -> {snes} -> back");
        }
        assert_eq!(m.to_snes(0x00_0000), Some(SnesAddr::lit(0x80_8000)));
        assert_eq!(m.to_snes(0x40_0000), Some(SnesAddr::lit(0x00_8000)));
        assert_eq!(m.to_snes(0x7F_0000), None);
        assert_eq!(m.to_snes(ExLoRom::MAX_SIZE), None);
    }

    #[test]
    fn sa1_power_on_map() {
        let m = Sa1Rom;
        assert_eq!(m.to_pc(SnesAddr::lit(0x00_8000)), Some(0x00_0000));
        assert_eq!(m.to_pc(SnesAddr::lit(0x05_E000)), Some(0x02_E000));
        assert_eq!(m.to_pc(SnesAddr::lit(0x3F_FFFF)), Some(0x1F_FFFF));
        assert_eq!(m.to_pc(SnesAddr::lit(0x80_8000)), Some(0x20_0000));
        assert_eq!(m.to_pc(SnesAddr::lit(0xBF_FFFF)), Some(0x3F_FFFF));
        assert_eq!(m.to_pc(SnesAddr::lit(0xC0_0000)), Some(0x00_0000));
        assert_eq!(m.to_pc(SnesAddr::lit(0xD0_1234)), Some(0x10_1234));
        assert_eq!(m.to_pc(SnesAddr::lit(0xFF_FFFF)), Some(0x3F_FFFF));
        assert_eq!(m.to_pc(SnesAddr::lit(0x40_0000)), None); // BW-RAM
        assert_eq!(m.to_pc(SnesAddr::lit(0x00_6000)), None); // BW-RAM window
        assert_eq!(m.to_pc(SnesAddr::lit(0x7E_8000)), None);
        for pc in [0usize, 0x02_E000, 0x1F_FFFF, 0x20_0000, 0x3F_FFFF] {
            let snes = m.to_snes(pc).expect("in range");
            assert_eq!(m.to_pc(snes), Some(pc), "pc {pc:#X} -> {snes} -> back");
        }
        assert_eq!(m.to_snes(0x40_0000), None);
    }

    #[test]
    fn lorom_known_addresses() {
        let m = LoRom;
        assert_eq!(m.to_pc(SnesAddr::lit(0x00_8000)), Some(0x0000));
        assert_eq!(m.to_pc(SnesAddr::lit(0x00_FFC0)), Some(0x7FC0));
        assert_eq!(m.to_pc(SnesAddr::lit(0x01_8000)), Some(0x8000));
        assert_eq!(m.to_pc(SnesAddr::lit(0x05_E000)), Some(0x2_E000));
        assert_eq!(m.to_pc(SnesAddr::lit(0x0F_FFFF)), Some(0x7_FFFF));
        // Upper mirror maps to the same bytes.
        assert_eq!(m.to_pc(SnesAddr::lit(0x85_E000)), Some(0x2_E000));
    }

    #[test]
    fn lorom_rejects_non_rom() {
        let m = LoRom;
        assert_eq!(m.to_pc(SnesAddr::lit(0x00_0000)), None);
        assert_eq!(m.to_pc(SnesAddr::lit(0x00_7FFF)), None);
        assert_eq!(m.to_pc(SnesAddr::lit(0x7E_8000)), None);
        assert_eq!(m.to_pc(SnesAddr::lit(0x7F_FFFF)), None);
    }

    #[test]
    fn lorom_to_snes_round_trips() {
        let m = LoRom;
        for pc in [0usize, 0x7FFF, 0x8000, 0x2_E000, 0x7_FFFF, 0x3F_8000] {
            let snes = m.to_snes(pc).expect("in range");
            assert_eq!(m.to_pc(snes), Some(pc), "pc {pc:#X} -> {snes} -> back");
        }
        assert_eq!(m.to_snes(0x3F_0000), Some(SnesAddr::lit(0xFE_8000)));
        assert_eq!(m.to_snes(LoRom::MAX_SIZE), None);
    }
}
