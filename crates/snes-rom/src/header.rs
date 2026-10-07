//! The SNES internal cartridge header.
//!
//! The header sits at `$00:FFC0` (file offset `0x7FC0` in LoROM, `0xFFC0` in
//! HiROM). See `docs/formats/rom-mapping.md` for the byte layout.

/// Which address mapping the header's map-mode byte declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MapKind {
    /// Mode `$20`/`$30`.
    LoRom,
    /// Mode `$21`/`$31`.
    HiRom,
    /// Mode `$23`: SA-1 coprocessor cartridge (used by some Lunar Magic hacks).
    Sa1,
    /// Mode `$25`/`$35`.
    ExHiRom,
    /// Mode `$32`: ExLoROM (used by expanded Lunar Magic hacks above 4 MiB).
    ExLoRom,
    /// Any other value; carries the low nibble of the map-mode byte.
    Unknown(u8),
}

/// Parsed SNES internal header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalHeader {
    /// Game title, trailing spaces trimmed. Non-ASCII bytes are replaced.
    pub title: String,
    /// Raw map-mode byte at `$FFD5`.
    pub map_mode: u8,
    /// Cartridge type byte at `$FFD6` (ROM / RAM / battery / coprocessor flags).
    pub cartridge_type: u8,
    /// ROM size code at `$FFD7`: size in bytes is `1 << (code + 10)`.
    pub rom_size_code: u8,
    /// SRAM size code at `$FFD8`: size in bytes is `1 << (code + 10)`, `0` means none.
    pub sram_size_code: u8,
    /// Destination/region code at `$FFD9`.
    pub region: u8,
    /// Developer ID at `$FFDA`.
    pub developer_id: u8,
    /// Version at `$FFDB`.
    pub version: u8,
    /// Checksum complement at `$FFDC`.
    pub checksum_complement: u16,
    /// Checksum at `$FFDE`.
    pub checksum: u16,
}

impl InternalHeader {
    /// Length of the header region this crate parses (title through checksum).
    pub const LEN: usize = 32;

    /// Parses a header from the 32 bytes starting at `$FFC0`.
    ///
    /// Returns `None` if fewer than 32 bytes are supplied. No plausibility
    /// checks are performed; see [`InternalHeader::plausibility`].
    #[must_use]
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes = bytes.get(..Self::LEN)?;
        let title_bytes = bytes.get(0..21)?;
        let title = title_bytes
            .iter()
            .map(
                |&b| if b.is_ascii() && !b.is_ascii_control() { char::from(b) } else { '\u{FFFD}' },
            )
            .collect::<String>()
            .trim_end()
            .to_owned();
        let word = |i: usize| -> Option<u16> {
            let lo = *bytes.get(i)?;
            let hi = *bytes.get(i.checked_add(1)?)?;
            Some(u16::from_le_bytes([lo, hi]))
        };
        Some(Self {
            title,
            map_mode: *bytes.get(0x15)?,
            cartridge_type: *bytes.get(0x16)?,
            rom_size_code: *bytes.get(0x17)?,
            sram_size_code: *bytes.get(0x18)?,
            region: *bytes.get(0x19)?,
            developer_id: *bytes.get(0x1A)?,
            version: *bytes.get(0x1B)?,
            checksum_complement: word(0x1C)?,
            checksum: word(0x1E)?,
        })
    }

    /// The mapping declared by the low nibble of the map-mode byte.
    #[must_use]
    pub const fn map_kind(&self) -> MapKind {
        match self.map_mode & 0x0F {
            0x0 => MapKind::LoRom,
            0x1 => MapKind::HiRom,
            0x2 => MapKind::ExLoRom,
            0x3 => MapKind::Sa1,
            0x5 => MapKind::ExHiRom,
            other => MapKind::Unknown(other),
        }
    }

    /// Whether the map-mode byte declares FastROM (3.58 MHz access).
    #[must_use]
    pub const fn is_fast_rom(&self) -> bool {
        self.map_mode & 0x10 != 0
    }

    /// Declared ROM size in bytes, or `None` if the code is implausible.
    #[must_use]
    pub fn rom_size_bytes(&self) -> Option<u64> {
        // Codes 5 (32 KiB) through 13 (8 MiB) are the only ones seen in practice.
        (5..=13).contains(&self.rom_size_code).then(|| size_from_code(self.rom_size_code)).flatten()
    }

    /// Declared SRAM size in bytes (`0` for none), or `None` if the code is implausible.
    #[must_use]
    pub fn sram_size_bytes(&self) -> Option<u64> {
        match self.sram_size_code {
            0 => Some(0),
            code if code <= 7 => size_from_code(code),
            _ => None,
        }
    }

    /// Whether checksum and complement are bitwise inverses of each other.
    ///
    /// This is a cheap sanity check used to tell a real header from random
    /// data; it does not verify the checksum against the ROM contents.
    #[must_use]
    pub const fn checksum_consistent(&self) -> bool {
        self.checksum ^ self.checksum_complement == 0xFFFF
    }

    /// Scores how plausible this is as a real header (higher is better).
    ///
    /// Used to choose between the LoROM and HiROM header locations.
    #[must_use]
    pub fn plausibility(&self) -> u8 {
        let mut score = 0u8;
        if self.checksum_consistent() {
            score = score.saturating_add(4);
        }
        if !matches!(self.map_kind(), MapKind::Unknown(_)) {
            score = score.saturating_add(2);
        }
        if self.rom_size_bytes().is_some() {
            score = score.saturating_add(1);
        }
        if self.title.chars().all(|c| c.is_ascii_graphic() || c == ' ') && !self.title.is_empty() {
            score = score.saturating_add(1);
        }
        score
    }
}

/// Size in bytes for a ROM/SRAM size code: `1 << (code + 10)`.
fn size_from_code(code: u8) -> Option<u64> {
    1u64.checked_shl(u32::from(code).checked_add(10)?)
}

#[cfg(test)]
mod tests {
    use super::{InternalHeader, MapKind};
    use crate::test_fixtures::SMW_HEADER;

    #[test]
    fn parses_smw_header() {
        let h = InternalHeader::parse(&SMW_HEADER).expect("32 bytes");
        assert_eq!(h.title, "SUPER MARIOWORLD");
        assert_eq!(h.map_kind(), MapKind::LoRom);
        assert!(!h.is_fast_rom());
        assert_eq!(h.rom_size_bytes(), Some(512 * 1024));
        assert_eq!(h.sram_size_bytes(), Some(2 * 1024));
        assert_eq!(h.checksum, 0xA0DA);
        assert_eq!(h.checksum_complement, 0x5F25);
        assert!(h.checksum_consistent());
        assert_eq!(h.plausibility(), 8);
    }

    #[test]
    fn short_input_is_none() {
        assert!(InternalHeader::parse(&SMW_HEADER[..31]).is_none());
    }

    #[test]
    fn garbage_scores_low() {
        let h = InternalHeader::parse(&[0xFF; 32]).expect("32 bytes");
        assert_eq!(h.map_kind(), MapKind::Unknown(0xF));
        assert!(!h.checksum_consistent());
        assert_eq!(h.plausibility(), 0);
    }
}
