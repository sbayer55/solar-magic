//! Secondary exits: the four `$05:F800` tables. See
//! `docs/formats/level-header.md` ("Secondary exits").

use core::fmt;

use snes_rom::{Rom, RomError, SnesAddr};

use super::fields::{BitField, FieldError, secondary_exit};
use crate::RomLayout;

/// A secondary exit number, `0x000` to `0x1FF`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExitId(u16);

impl ExitId {
    /// Number of secondary exit slots.
    pub const COUNT: usize = 0x200;

    /// Creates an exit id, returning `None` if `n >= 0x200`.
    #[must_use]
    pub const fn new(n: u16) -> Option<Self> {
        if n < 0x200 { Some(Self(n)) } else { None }
    }

    /// The raw exit number.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// The exit number as a table index.
    #[must_use]
    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    /// Iterates over every exit id in order.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..0x200u16).map(Self)
    }
}

impl fmt::Debug for ExitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ExitId({:03X})", self.0)
    }
}

impl fmt::Display for ExitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:03X}", self.0)
    }
}

/// One secondary exit: its byte from each of the four tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SecondaryExit {
    /// Bytes from `$05:F800`, `$05:FA00`, `$05:FC00`, `$05:FE00`.
    pub raw: [u8; 4],
}

/// Bit of the fourth table byte Lunar Magic uses for the destination's bit 8.
const LM_DESTINATION_HIGH: u8 = 0x08;

impl SecondaryExit {
    /// The 9-bit destination level. Vanilla takes bit 8 from the current
    /// submap at run time (treated as 0 here); Lunar Magic stores it in bit
    /// 3 of the `$05:FE00` byte (verified on a 3.71 ROM, `docs/formats/mwl.md`).
    #[must_use]
    pub fn destination(self, layout: &RomLayout) -> u16 {
        let high = layout.lm.is_some() && self.raw[3] & LM_DESTINATION_HIGH != 0;
        u16::from(self.raw[0]) | (u16::from(high) << 8)
    }

    /// Reads exit `id` from the tables.
    ///
    /// # Errors
    ///
    /// ROM read failures.
    pub fn load(rom: &Rom, layout: &RomLayout, id: ExitId) -> Result<Self, RomError> {
        let mut raw = [0u8; 4];
        for (slot, table) in raw.iter_mut().zip(layout.secondary_exits.iter()) {
            *slot = rom.read_u8(entry(*table, id)?)?;
        }
        Ok(Self { raw })
    }

    /// Reads every secondary exit.
    ///
    /// # Errors
    ///
    /// ROM read failures.
    pub fn load_all(rom: &Rom, layout: &RomLayout) -> Result<Vec<Self>, RomError> {
        ExitId::all().map(|id| Self::load(rom, layout, id)).collect()
    }

    /// Writes this exit's bytes into the tables as exit `id`.
    ///
    /// # Errors
    ///
    /// ROM write failures.
    pub fn write(self, rom: &mut Rom, layout: &RomLayout, id: ExitId) -> Result<(), RomError> {
        for (byte, table) in self.raw.iter().zip(layout.secondary_exits.iter()) {
            rom.write(entry(*table, id)?, &[*byte])?;
        }
        Ok(())
    }

    /// Reads a field.
    #[must_use]
    pub fn get(self, field: BitField) -> u8 {
        field.get(&self.raw)
    }

    /// Writes a field.
    ///
    /// # Errors
    ///
    /// [`FieldError::OutOfRange`] if the value does not fit.
    pub fn set(&mut self, field: BitField, value: u8) -> Result<(), FieldError> {
        field.set(&mut self.raw, value)
    }

    /// Destination level low byte (the high bit is implied by the submap in
    /// vanilla; Lunar Magic stores it separately).
    #[must_use]
    pub fn destination_low(self) -> u8 {
        self.get(secondary_exit::DESTINATION_LOW)
    }

    /// Entrance screen, 0–31.
    #[must_use]
    pub fn entrance_screen(self) -> u8 {
        self.get(secondary_exit::ENTRANCE_SCREEN)
    }

    /// Entrance X position index, 0–7.
    #[must_use]
    pub fn entrance_x_index(self) -> u8 {
        self.get(secondary_exit::ENTRANCE_X)
    }

    /// Entrance Y position index, 0–15.
    #[must_use]
    pub fn entrance_y_index(self) -> u8 {
        self.get(secondary_exit::ENTRANCE_Y)
    }

    /// Entrance type, 0–7.
    #[must_use]
    pub fn entrance_type(self) -> u8 {
        self.get(secondary_exit::ENTRANCE_TYPE)
    }

    /// Whether every byte is zero (an unused slot).
    #[must_use]
    pub const fn is_blank(self) -> bool {
        self.raw[0] == 0 && self.raw[1] == 0 && self.raw[2] == 0 && self.raw[3] == 0
    }
}

fn entry(table: SnesAddr, id: ExitId) -> Result<SnesAddr, RomError> {
    table.checked_add(u32::from(id.get())).ok_or(RomError::Unmapped(table))
}

#[cfg(test)]
mod tests {
    use super::{ExitId, SecondaryExit, secondary_exit};

    #[test]
    fn exit_fields() {
        let e = SecondaryExit { raw: [0x9A, 0b1001_0101, 0b1011_0011, 0x03] };
        assert_eq!(e.destination_low(), 0x9A);
        assert_eq!(e.entrance_y_index(), 5);
        assert_eq!(e.get(secondary_exit::LAYER1_INITIAL_Y), 1);
        assert_eq!(e.get(secondary_exit::LAYER2_INITIAL_Y), 2);
        assert_eq!(e.entrance_screen(), 0x13);
        assert_eq!(e.entrance_x_index(), 5);
        assert_eq!(e.entrance_type(), 3);
        assert!(!e.is_blank());
        assert!(SecondaryExit::default().is_blank());
    }

    #[test]
    fn set_rewrites_only_its_bits() {
        let mut e = SecondaryExit { raw: [0, 0xFF, 0, 0] };
        e.set(secondary_exit::ENTRANCE_Y, 3).expect("fits");
        assert_eq!(e.raw[1], 0xF3);
        assert!(e.set(secondary_exit::ENTRANCE_TYPE, 8).is_err());
        assert_eq!(ExitId::new(0x1FF).map(ExitId::get), Some(0x1FF));
        assert!(ExitId::new(0x200).is_none());
        assert_eq!(ExitId::all().count(), ExitId::COUNT);
    }

    /// Every vanilla exit round-trips through load and write bytes.
    #[test]
    fn real_rom_secondary_exits() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = crate::RomLayout::detect(&rom).expect("vanilla");
        let exits = SecondaryExit::load_all(&rom, &layout).expect("reads");
        assert_eq!(exits.len(), ExitId::COUNT);
        let used = exits.iter().filter(|e| !e.is_blank()).count();
        assert!(used > 30, "expected dozens of used secondary exits, got {used}");
        // Writing every exit back into a copy changes nothing.
        let mut copy = rom.clone();
        for (id, exit) in ExitId::all().zip(exits.iter()) {
            exit.write(&mut copy, &layout, id).expect("writes");
        }
        assert_eq!(copy.bytes(), rom.bytes());
    }
}
