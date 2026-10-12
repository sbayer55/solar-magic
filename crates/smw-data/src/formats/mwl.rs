//! The `.mwl` level file container. `docs/formats/mwl.md`
//!
//! A 64-byte header, a pointer table of eight `{offset, length}` entries and
//! eight sections stored back to back. This module keeps every section as
//! raw bytes and round-trips Lunar Magic's own exports byte for byte; the
//! sibling `mwl_level` module gives the sections meaning.

use core::fmt;

/// The first two bytes of every `.mwl`.
pub const MAGIC: [u8; 2] = *b"LM";
/// Header length; the pointer table follows it.
pub const HEADER_LEN: usize = 0x40;
/// Length of the comment field inside the header.
pub const COMMENT_LEN: usize = 48;
/// Number of sections.
pub const SECTION_COUNT: usize = 8;
/// Bytes per pointer-table entry.
const ENTRY_LEN: usize = 8;
/// Pointer table length.
const TABLE_LEN: usize = SECTION_COUNT * ENTRY_LEN;
/// Lunar Magic version written by [`Mwl::new`] (3.71, the format this
/// crate follows).
pub const FORMAT_VERSION: u16 = 0x0371;

/// Which section an index refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Level number, secondary header, Lunar Magic's per-level bytes.
    LevelInfo = 0,
    /// Primary header plus the layer 1 object stream.
    Layer1 = 1,
    /// Layer 2 objects, or the background tilemap as 16-bit tiles.
    Layer2 = 2,
    /// Sprite header plus the sprite list.
    Sprites = 3,
    /// 256 colours plus the back area colour.
    Palette = 4,
    /// Secondary exits leading into the level.
    SecondaryExits = 5,
    /// Per-level `ExAnimation` data.
    ExAnimation = 6,
    /// The 16-word GFX bypass row.
    GfxBypass = 7,
}

impl Section {
    /// Every section in file order.
    pub const ALL: [Self; SECTION_COUNT] = [
        Self::LevelInfo,
        Self::Layer1,
        Self::Layer2,
        Self::Sprites,
        Self::Palette,
        Self::SecondaryExits,
        Self::ExAnimation,
        Self::GfxBypass,
    ];

    const fn index(self) -> usize {
        match self {
            Self::LevelInfo => 0,
            Self::Layer1 => 1,
            Self::Layer2 => 2,
            Self::Sprites => 3,
            Self::Palette => 4,
            Self::SecondaryExits => 5,
            Self::ExAnimation => 6,
            Self::GfxBypass => 7,
        }
    }
}

/// A parsed `.mwl` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mwl {
    /// Lunar Magic version, little-endian BCD-ish (`0x0371` = 3.71).
    pub version: u16,
    /// Header flags (bit 0 = exported from Super Mario Advance 2).
    pub flags: u32,
    /// Free-text comment.
    pub comment: [u8; COMMENT_LEN],
    /// Raw bytes of each section, in file order.
    pub sections: [Vec<u8>; SECTION_COUNT],
}

/// Why a file is not a usable `.mwl`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MwlError {
    /// The file does not start with `LM`.
    #[error("not a .mwl file (bad magic)")]
    BadMagic,
    /// The file is shorter than its header or tables claim.
    #[error("truncated .mwl: {what}")]
    Truncated {
        /// Which structure ran past the end.
        what: &'static str,
    },
    /// The pointer table is not the eight-entry one this crate understands.
    #[error("unsupported pointer table: offset {offset:#X}, length {len:#X}")]
    BadTable {
        /// Table offset.
        offset: u32,
        /// Table length.
        len: u32,
    },
    /// A section's bytes do not have the expected shape.
    #[error("{section:?}: {reason}")]
    BadSection {
        /// Which section.
        section: Section,
        /// What is wrong.
        reason: &'static str,
    },
    /// The level needs a Lunar Magic ROM feature the target lacks.
    #[error("importing needs a Lunar Magic ROM: {0}")]
    NeedsLunarMagic(&'static str),
}

impl Mwl {
    /// An empty file with the format version and `comment` (truncated or
    /// space-padded to 48 bytes).
    #[must_use]
    pub fn new(comment: &[u8]) -> Self {
        let mut padded = [b' '; COMMENT_LEN];
        for (slot, byte) in padded.iter_mut().zip(comment) {
            *slot = *byte;
        }
        Self { version: FORMAT_VERSION, flags: 0, comment: padded, sections: Default::default() }
    }

    /// Parses a file image.
    ///
    /// # Errors
    ///
    /// See [`MwlError`].
    pub fn parse(bytes: &[u8]) -> Result<Self, MwlError> {
        let header = bytes.get(..HEADER_LEN).ok_or(MwlError::Truncated { what: "header" })?;
        if header.get(..2) != Some(&MAGIC[..]) {
            return Err(MwlError::BadMagic);
        }
        let version = u16::from_le_bytes([byte(header, 2), byte(header, 3)]);
        let table_offset = u32_at(header, 4);
        let table_len = u32_at(header, 8);
        let flags = u32_at(header, 12);
        let mut comment = [0u8; COMMENT_LEN];
        comment.copy_from_slice(header.get(0x10..HEADER_LEN).unwrap_or(&[0; COMMENT_LEN]));
        if usize::try_from(table_len).ok() != Some(TABLE_LEN) {
            return Err(MwlError::BadTable { offset: table_offset, len: table_len });
        }
        let table_start = usize::try_from(table_offset)
            .map_err(|_| MwlError::BadTable { offset: table_offset, len: table_len })?;
        let table = bytes
            .get(table_start..table_start.saturating_add(TABLE_LEN))
            .ok_or(MwlError::Truncated { what: "pointer table" })?;
        let mut sections: [Vec<u8>; SECTION_COUNT] = Default::default();
        for (i, slot) in sections.iter_mut().enumerate() {
            let entry = i.saturating_mul(ENTRY_LEN);
            let offset = usize::try_from(u32_at(table, entry)).unwrap_or(usize::MAX);
            let len = usize::try_from(u32_at(table, entry.saturating_add(4))).unwrap_or(usize::MAX);
            let end = offset.checked_add(len).ok_or(MwlError::Truncated { what: "section" })?;
            *slot = bytes.get(offset..end).ok_or(MwlError::Truncated { what: "section" })?.to_vec();
        }
        Ok(Self { version, flags, comment, sections })
    }

    /// Serializes with the sections laid out back to back after the pointer
    /// table, as Lunar Magic writes them.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + TABLE_LEN);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&u32::try_from(HEADER_LEN).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(&u32::try_from(TABLE_LEN).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&self.comment);
        let mut offset = HEADER_LEN.saturating_add(TABLE_LEN);
        for section in &self.sections {
            out.extend_from_slice(&u32::try_from(offset).unwrap_or(0).to_le_bytes());
            out.extend_from_slice(&u32::try_from(section.len()).unwrap_or(0).to_le_bytes());
            offset = offset.saturating_add(section.len());
        }
        for section in &self.sections {
            out.extend_from_slice(section);
        }
        out
    }

    /// The raw bytes of `section`.
    #[must_use]
    pub fn section(&self, section: Section) -> &[u8] {
        self.sections.get(section.index()).map_or(&[], Vec::as_slice)
    }

    /// Replaces the bytes of `section`.
    pub fn set_section(&mut self, section: Section, bytes: Vec<u8>) {
        if let Some(slot) = self.sections.get_mut(section.index()) {
            *slot = bytes;
        }
    }

    /// The comment as text (trailing spaces trimmed).
    #[must_use]
    pub fn comment_text(&self) -> String {
        String::from_utf8_lossy(&self.comment).trim_end().to_owned()
    }
}

impl fmt::Display for Mwl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "mwl version {:X}.{:02X}, flags {:#X}, comment {:?}",
            self.version >> 8,
            self.version & 0xFF,
            self.flags,
            self.comment_text()
        )?;
        for (section, bytes) in Section::ALL.iter().zip(&self.sections) {
            writeln!(f, "  {section:?}: {} bytes", bytes.len())?;
        }
        Ok(())
    }
}

fn byte(bytes: &[u8], i: usize) -> u8 {
    bytes.get(i).copied().unwrap_or(0)
}

fn u32_at(bytes: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([
        byte(bytes, i),
        byte(bytes, i.saturating_add(1)),
        byte(bytes, i.saturating_add(2)),
        byte(bytes, i.saturating_add(3)),
    ])
}

#[cfg(test)]
mod tests {
    use super::{Mwl, MwlError, Section};

    #[test]
    fn round_trips_a_synthetic_file() {
        let mut mwl = Mwl::new(b"Solar Magic test");
        mwl.set_section(Section::Layer1, vec![1, 2, 3]);
        mwl.set_section(Section::GfxBypass, vec![0x7F, 0]);
        let bytes = mwl.to_bytes();
        assert_eq!(&bytes[..2], b"LM");
        let back = Mwl::parse(&bytes).expect("parses");
        assert_eq!(back, mwl);
        assert_eq!(back.section(Section::Layer1), &[1, 2, 3]);
        assert!(back.section(Section::Palette).is_empty());
        assert_eq!(back.comment_text(), "Solar Magic test");
        assert_eq!(back.to_bytes(), bytes);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(Mwl::parse(b"XY").unwrap_err(), MwlError::Truncated { what: "header" });
        let mut bad = Mwl::new(b"x").to_bytes();
        bad[0] = b'X';
        assert_eq!(Mwl::parse(&bad).unwrap_err(), MwlError::BadMagic);
        let short = Mwl::new(b"x").to_bytes();
        assert!(matches!(Mwl::parse(&short[..0x60]).unwrap_err(), MwlError::Truncated { .. }));
    }

    #[test]
    fn lm_export_round_trips_byte_for_byte() {
        let Some(path) = std::env::var_os("LM_MWL") else {
            eprintln!("LM_MWL not set; skipping");
            return;
        };
        let bytes = std::fs::read(path).expect("reads");
        let mwl = Mwl::parse(&bytes).expect("parses");
        assert_eq!(mwl.version, 0x0371);
        assert_eq!(mwl.to_bytes(), bytes);
        assert_eq!(mwl.section(Section::GfxBypass).len(), 0x20);
    }
}
