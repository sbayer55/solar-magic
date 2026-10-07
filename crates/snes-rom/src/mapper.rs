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
}

#[cfg(test)]
mod tests {
    use super::{LoRom, Mapper};
    use crate::SnesAddr;

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
