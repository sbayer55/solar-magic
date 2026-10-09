//! Lunar Magic's `"LM" kind revision` markers. `docs/formats/lunar-magic-ext.md`
//!
//! Lunar Magic stamps a 4-byte marker before each 16-byte-aligned routine it
//! installs in the fixed banks and at the end of the RATS blocks holding its
//! own code and tables (level data blocks carry none). Both bytes after the
//! ASCII `LM` are small, so ordinary text never matches: the vanilla ROM has
//! no hits, a Lunar Magic 3.71 save of it has 26.

use snes_rom::rats::{self, RatsTag};
use snes_rom::{Rom, SnesAddr};

/// The two ASCII bytes every marker starts with.
const MAGIC: &[u8; 2] = b"LM";
/// Kind and revision bytes are below this.
const MAX_FIELD: u8 = 0x20;
/// Marker length.
pub const MARKER_LEN: usize = 4;

/// Where a marker sits relative to Lunar Magic's blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(variant_size_differences, reason = "a tag is three words; markers are few")]
pub enum MarkerPlacement {
    /// The last four bytes of a RATS-tagged block.
    BlockEnd(RatsTag),
    /// Immediately before a 16-byte-aligned routine.
    RoutinePrefix,
    /// Neither (padding, or a block whose tag was not found).
    Loose,
}

/// One marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LmMarker {
    /// File offset of the `L`.
    pub pc: usize,
    /// Bus address, when the mapper covers it.
    pub addr: Option<SnesAddr>,
    /// Kind byte (`00` code/data, `01`, `02`, `03`, `10`, `11` seen).
    pub kind: u8,
    /// Revision byte (`01` or `02` seen).
    pub revision: u8,
    /// Placement.
    pub placement: MarkerPlacement,
}

/// Finds every marker in `rom`.
#[must_use]
pub fn scan_markers(rom: &Rom) -> Vec<LmMarker> {
    scan_marker_bytes(rom.bytes())
        .into_iter()
        .map(|m| LmMarker { addr: rom.mapper().to_snes(m.pc), ..m })
        .collect()
}

/// Finds every marker in a raw image (no bus addresses).
#[must_use]
pub fn scan_marker_bytes(bytes: &[u8]) -> Vec<LmMarker> {
    let tags = rats::scan(bytes);
    bytes
        .windows(MARKER_LEN)
        .enumerate()
        .filter_map(|(pc, window)| {
            let [l, m, kind, revision] = *window else {
                return None;
            };
            if [l, m] != *MAGIC || kind >= MAX_FIELD || revision >= MAX_FIELD || revision == 0 {
                return None;
            }
            let end = pc.checked_add(MARKER_LEN)?;
            let placement = match tags.iter().find(|t| t.end_pc() == end) {
                Some(tag) => MarkerPlacement::BlockEnd(*tag),
                None if end % 16 == 0 => MarkerPlacement::RoutinePrefix,
                None => MarkerPlacement::Loose,
            };
            Some(LmMarker { pc, addr: None, kind, revision, placement })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use snes_rom::rats::RatsTag;

    use super::{MarkerPlacement, scan_marker_bytes};

    #[test]
    fn classifies_markers() {
        let mut image = vec![0xFFu8; 0x400];
        // A routine prefix: the marker ends on a 16-byte boundary.
        image[0x1C..0x20].copy_from_slice(b"LM\x01\x01");
        // A block trailer: RATS tag at 0x100 covering 16 bytes ending with a marker.
        let tag = RatsTag::header_bytes(16).unwrap();
        image[0x100..0x108].copy_from_slice(&tag);
        image[0x108..0x118].fill(0x20);
        image[0x114..0x118].copy_from_slice(b"LM\x00\x02");
        // Loose, and two non-matches (ASCII text, zero revision).
        image[0x203..0x207].copy_from_slice(b"LM\x10\x01");
        image[0x300..0x304].copy_from_slice(b"LMAB");
        image[0x310..0x314].copy_from_slice(b"LM\x01\x00");

        let found = scan_marker_bytes(&image);
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(found[0].pc, 0x1C);
        assert_eq!((found[0].kind, found[0].revision), (1, 1));
        assert_eq!(found[0].placement, MarkerPlacement::RoutinePrefix);
        assert_eq!(found[1].pc, 0x114);
        assert!(matches!(found[1].placement, MarkerPlacement::BlockEnd(t) if t.data_len == 16));
        assert_eq!(found[2].pc, 0x203);
        assert_eq!(found[2].placement, MarkerPlacement::Loose);
    }

    #[test]
    fn real_roms() {
        if let Some(path) = std::env::var_os("SMW_ROM") {
            let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
            assert!(super::scan_markers(&rom).is_empty(), "vanilla has no markers");
        } else {
            eprintln!("SMW_ROM not set; skipping vanilla marker test");
        }
        if let Some(path) = std::env::var_os("LM_ROM") {
            let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
            let markers = super::scan_markers(&rom);
            assert!(markers.len() >= 10, "{}", markers.len());
            // The acts-like getter's prefix at $06:F5FC (both 2.5x and 3.71).
            let getter = markers.iter().find(|m| m.pc == 0x03_75FC).expect("marker at $06:F5FC");
            assert_eq!((getter.kind, getter.revision), (0x10, 1));
            assert_eq!(getter.placement, MarkerPlacement::RoutinePrefix);
        } else {
            eprintln!("LM_ROM not set; skipping Lunar Magic marker test");
        }
    }
}
