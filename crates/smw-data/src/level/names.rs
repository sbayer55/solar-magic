//! The names the overworld shows for its levels, read from the ROM's
//! `LevelNames` tables. See `docs/formats/level-names.md`.
//!
//! A name is up to three words: the first table is indexed by byte 1 of the
//! level's entry (bits 0-6), the second by the high nibble of byte 0 and the
//! third by its low nibble. Each word is a run of overworld font tiles that
//! ends with the tile whose bit 7 is set. The border has room for
//! [`LevelNameTables::max_tiles`] tiles; the game drops the rest, which is
//! how `YELLOW SWITCH PALACE` loses the stray `3` its entry also selects.

use snes_rom::Rom;

use crate::{LevelId, LevelNameTables, RomLayout};

/// Longest string the decoder follows before giving up on a missing
/// terminator (the ROM's longest word is 18 tiles).
const MAX_WORD_TILES: usize = 64;

/// What one font tile prints as. Tiles `0x00-0x19` are `A-Z`; the rest of
/// the table is the overworld font as GFX2A lays it out (verified against the
/// sheet on 2026-10-15): punctuation at `0x1A-0x1F`, the squeezed ` ILLUSI`
/// (6 tiles, the gap drawn in the first) and `YELLOW` (5 tiles) at
/// `0x32-0x3C`, lowercase at
/// `0x40-0x59`, `#` and `'` at `0x5A`/`0x5D`, digits at `0x64-0x6B`.
const SPECIAL_TILES: &[(u8, &str)] = &[
    (0x1A, "!"),
    (0x1B, "."),
    (0x1C, "-"),
    (0x1D, ","),
    (0x1E, "?"),
    (0x1F, " "),
    (0x32, " I"),
    (0x33, "L"),
    (0x34, "L"),
    (0x35, "U"),
    (0x36, "S"),
    (0x37, "I"),
    (0x38, "Y"),
    (0x39, "E"),
    (0x3A, "L"),
    (0x3B, "L"),
    (0x3C, "OW"),
    (0x5A, "#"),
    (0x5B, "("),
    (0x5C, ")"),
    (0x5D, "'"),
    (0x64, "1"),
    (0x65, "2"),
    (0x66, "3"),
    (0x67, "4"),
    (0x68, "5"),
    (0x69, "6"),
    (0x6A, "7"),
    (0x6B, "0"),
];

/// Printed for a tile the font table does not cover.
const UNKNOWN_TILE: &str = "_";

/// The text of one overworld font tile (bit 7, the terminator flag, ignored).
fn tile_text(tile: u8) -> String {
    let tile = tile & 0x7F;
    if let Some(letter) = (tile < 26).then(|| char::from(b'A'.saturating_add(tile))) {
        return letter.to_string();
    }
    if let Some(letter) = (0x40..0x5A)
        .contains(&tile)
        .then(|| char::from(b'a'.saturating_add(tile.saturating_sub(0x40))))
    {
        return letter.to_string();
    }
    SPECIAL_TILES
        .iter()
        .find(|(t, _)| *t == tile)
        .map_or_else(|| UNKNOWN_TILE.to_owned(), |(_, text)| (*text).to_owned())
}

/// Decodes a run of name tiles (each already stripped of its terminator
/// flag, or not: the flag is ignored) into text.
#[must_use]
pub fn tiles_to_text(tiles: &[u8]) -> String {
    tiles.iter().map(|&t| tile_text(t)).collect()
}

/// Every overworld level's name, as the ROM draws it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OverworldNames {
    /// Indexed by translevel.
    names: Vec<Option<String>>,
}

impl OverworldNames {
    /// Reads the names from `rom`. When the drawing routine is not where the
    /// layout expects it (a ROM with a replaced level-name system), no name
    /// is trusted and every lookup is `None`.
    #[must_use]
    pub fn read(rom: &Rom, layout: &RomLayout) -> Self {
        let tables = layout.level_names;
        let routine_matches = rom
            .read(tables.routine, tables.routine_fingerprint.len())
            .is_ok_and(|bytes| bytes == tables.routine_fingerprint);
        if !routine_matches {
            return Self::default();
        }
        let names = (0..tables.count)
            .map(|translevel| {
                u8::try_from(translevel).ok().and_then(|t| decode_name(rom, tables, t))
            })
            .collect();
        Self { names }
    }

    /// The name of an overworld slot; `None` for sublevels, for slots past
    /// the table and for names the ROM does not encode.
    #[must_use]
    pub fn get(&self, id: LevelId) -> Option<&str> {
        let translevel = usize::from(id.translevel()?);
        self.names.get(translevel)?.as_deref()
    }

    /// Whether any name was read.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.iter().all(Option::is_none)
    }
}

/// The three words of one `LevelNames` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NameEntry {
    first: usize,
    second: usize,
    third: usize,
}

impl NameEntry {
    /// `byte0`: high nibble second word, low nibble third word; `byte1`
    /// bits 0-6: first word (bit 7 is ignored, as the game masks it).
    fn from_bytes(byte0: u8, byte1: u8) -> Self {
        Self {
            first: usize::from(byte1 & 0x7F),
            second: usize::from(byte0 >> 4),
            third: usize::from(byte0 & 0x0F),
        }
    }
}

/// Reads entry `index` of a table of 16-bit offsets that ends at `end`.
fn word_offset(
    rom: &Rom,
    table: snes_rom::SnesAddr,
    end: snes_rom::SnesAddr,
    index: usize,
) -> Option<usize> {
    let entries = usize::try_from(end.get().saturating_sub(table.get())).ok()? / 2;
    if index >= entries {
        return None;
    }
    let entry = table.checked_add(u32::try_from(index.checked_mul(2)?).ok()?)?;
    rom.read_u16_le(entry).ok().map(usize::from)
}

/// The tiles of the string at `offset` into `strings`, terminator included.
fn word_tiles(rom: &Rom, tables: LevelNameTables, offset: usize) -> Option<Vec<u8>> {
    let start = tables.strings.checked_add(u32::try_from(offset).ok()?)?;
    let bytes = rom.read(start, MAX_WORD_TILES).or_else(|_| rom.read(start, 1)).ok()?;
    let end = bytes.iter().position(|&b| b & 0x80 != 0)?;
    bytes.get(..=end).map(<[u8]>::to_vec)
}

/// The name of one translevel, as the game would draw it: the three words,
/// blank ones skipped, clipped to the border's width, trailing spaces
/// trimmed. `None` when an index points outside its table or a word has no
/// terminator in range.
fn decode_name(rom: &Rom, tables: LevelNameTables, translevel: u8) -> Option<String> {
    let entry_addr = tables.names.checked_add(u32::from(translevel).checked_mul(2)?)?;
    let raw = rom.read(entry_addr, 2).ok()?;
    let entry = NameEntry::from_bytes(*raw.first()?, *raw.get(1)?);
    let words = [
        word_offset(rom, tables.first_words, tables.second_words, entry.first)?,
        word_offset(rom, tables.second_words, tables.third_words, entry.second)?,
        word_offset(rom, tables.third_words, tables.tables_end, entry.third)?,
    ];
    let mut tiles: Vec<u8> = Vec::new();
    for offset in words {
        let word = word_tiles(rom, tables, offset)?;
        // A word that is just the blank tile is skipped (the routine does
        // the same for the first two; for the third it draws one space,
        // which the trim below removes anyway).
        if word.len() == 1 && word.first().copied().unwrap_or(0) & 0x7F == 0x1F {
            continue;
        }
        tiles.extend(word);
    }
    tiles.truncate(tables.max_tiles);
    let text = tiles_to_text(&tiles);
    let text = text.trim_end();
    (!text.is_empty()).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{OverworldNames, tiles_to_text};
    use crate::{LevelId, RomLayout};

    #[test]
    fn tile_text() {
        assert_eq!(tiles_to_text(&[0x18, 0x0E, 0x12, 0x07, 0x08, 0x5D, 0x12, 0x9F]), "YOSHI'S ");
        assert_eq!(tiles_to_text(&[0x5A, 0x64, 0x1F]), "#1 ");
        assert_eq!(tiles_to_text(&[0x38, 0x39, 0x3A, 0x3B, 0x3C]), "YELLOW");
        assert_eq!(tiles_to_text(&[0x32, 0x33, 0x34, 0x35, 0x36, 0x37]), " ILLUSI");
        assert_eq!(tiles_to_text(&[0x40, 0x59, 0x7E]), "az_");
    }

    #[test]
    fn vanilla_names() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).unwrap();
        let layout = RomLayout::detect(&rom).unwrap();
        let names = OverworldNames::read(&rom, &layout);
        let name = |n| names.get(LevelId::new(n).unwrap());
        assert_eq!(name(0x105), Some("YOSHI'S ISLAND 1"));
        assert_eq!(name(0x104), Some("YOSHI'S HOUSE"));
        assert_eq!(name(0x101), Some("#1 IGGY'S CASTLE"));
        assert_eq!(name(0x003), Some("TOP SECRET AREA"));
        assert_eq!(name(0x013), Some("DONUT SECRET HOUSE"));
        // Squeezed tiles, and the border's 19-tile limit clipping the entry.
        assert_eq!(name(0x014), Some("YELLOW SWITCH PALACE"));
        assert_eq!(name(0x11E), Some("FOREST OF ILLUSION 1"));
        // Special world names come from the third table alone.
        assert_eq!(name(LevelId::from_translevel(0x49).unwrap().get()), Some("FUNKY"));
        // Sublevels have no translevel; the last three translevels have no
        // entry (the table stops at 0x5C, free space follows).
        assert_eq!(name(0x025), None);
        assert_eq!(name(0x13B), None);
        // Entries `00 00` (translevels 0x00 and 0x36, slot 112) are blank.
        assert_eq!(name(0x000), None);
        assert_eq!(name(0x112), None);
        let decoded = LevelId::all().filter(|&id| names.get(id).is_some()).count();
        assert_eq!(decoded, 0x5D - 2, "every other translevel in the table decodes");
    }
}
