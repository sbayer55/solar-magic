//! RATS ("Recognized Allocated Tag System") tags.
//!
//! Lunar Magic and other SMW tools mark every block they allocate in expanded
//! ROM space with an 8-byte tag so other tools will not overwrite it:
//!
//! ```text
//! offset  size  meaning
//! 0       4     ASCII "STAR"
//! 4       2     (data length - 1), little-endian
//! 6       2     bitwise NOT of the previous word, little-endian
//! 8       n     the protected data
//! ```
//!
//! See `docs/formats/rats.md`. Allocation lives in [`alloc`].

mod alloc;

pub use alloc::{AllocRequest, FreeRun, RatsError, alloc, find, free, free_runs, tag_before};

/// Length of a RATS tag header in bytes.
pub const TAG_LEN: usize = 8;

/// The four-byte magic that opens a tag.
pub const MAGIC: &[u8; 4] = b"STAR";

/// A located RATS tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RatsTag {
    /// File offset of the `STAR` magic.
    pub tag_pc: usize,
    /// File offset of the first protected data byte (`tag_pc + 8`).
    pub data_pc: usize,
    /// Number of protected data bytes (1 to 65536).
    pub data_len: usize,
}

impl RatsTag {
    /// Largest number of data bytes one tag can protect.
    pub const MAX_DATA_LEN: usize = 0x1_0000;

    /// The 8 header bytes for a tag protecting `data_len` bytes
    /// (1 to [`RatsTag::MAX_DATA_LEN`]).
    #[must_use]
    pub fn header_bytes(data_len: usize) -> Option<[u8; TAG_LEN]> {
        if data_len == 0 || data_len > Self::MAX_DATA_LEN {
            return None;
        }
        let size = u16::try_from(data_len.saturating_sub(1)).ok()?;
        let [s0, s1] = size.to_le_bytes();
        let [i0, i1] = (!size).to_le_bytes();
        Some([MAGIC[0], MAGIC[1], MAGIC[2], MAGIC[3], s0, s1, i0, i1])
    }

    /// File offset one past the last protected byte.
    #[must_use]
    pub const fn end_pc(&self) -> usize {
        self.data_pc.saturating_add(self.data_len)
    }

    /// Parses a tag whose magic starts at `pc`, if the bytes there form a valid tag.
    #[must_use]
    pub fn parse_at(bytes: &[u8], pc: usize) -> Option<Self> {
        let end = pc.checked_add(TAG_LEN)?;
        let tag = bytes.get(pc..end)?;
        if tag.get(..4)? != MAGIC {
            return None;
        }
        let size = u16::from_le_bytes([*tag.get(4)?, *tag.get(5)?]);
        let inverse = u16::from_le_bytes([*tag.get(6)?, *tag.get(7)?]);
        if size ^ inverse != 0xFFFF {
            return None;
        }
        Some(Self { tag_pc: pc, data_pc: end, data_len: usize::from(size).saturating_add(1) })
    }
}

/// Scans the whole image for RATS tags, in file order.
///
/// Tags are not expected to overlap, so scanning resumes after each tag's
/// protected data. A tag whose data runs past the end of the image is still
/// reported.
#[must_use]
pub fn scan(bytes: &[u8]) -> Vec<RatsTag> {
    scan_from(bytes, 0)
}

/// Like [`scan`] but starts at file offset `start`.
#[must_use]
pub fn scan_from(bytes: &[u8], start: usize) -> Vec<RatsTag> {
    let mut tags = Vec::new();
    let mut pc = start;
    while pc.saturating_add(TAG_LEN) <= bytes.len() {
        match RatsTag::parse_at(bytes, pc) {
            Some(tag) => {
                pc = tag.end_pc();
                tags.push(tag);
            }
            None => pc = pc.saturating_add(1),
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::{RatsTag, TAG_LEN, scan};

    fn tag_bytes(data_len: u16) -> [u8; TAG_LEN] {
        let size = data_len.saturating_sub(1);
        let inv = !size;
        let [s0, s1] = size.to_le_bytes();
        let [i0, i1] = inv.to_le_bytes();
        [b'S', b'T', b'A', b'R', s0, s1, i0, i1]
    }

    #[test]
    fn parses_valid_tag() {
        let mut buf = vec![0u8; 16];
        buf[4..12].copy_from_slice(&tag_bytes(3));
        let tag = RatsTag::parse_at(&buf, 4).expect("valid tag");
        assert_eq!(tag, RatsTag { tag_pc: 4, data_pc: 12, data_len: 3 });
        assert_eq!(tag.end_pc(), 15);
    }

    #[test]
    fn rejects_bad_inverse() {
        let mut t = tag_bytes(3);
        t[6] ^= 1;
        assert!(RatsTag::parse_at(&t, 0).is_none());
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut t = tag_bytes(3);
        t[0] = b'X';
        assert!(RatsTag::parse_at(&t, 0).is_none());
    }

    #[test]
    fn scan_finds_tags_and_skips_data() {
        let mut buf = vec![0u8; 64];
        buf[0..8].copy_from_slice(&tag_bytes(8));
        // A fake "STAR" inside the first tag's data must not be reported.
        buf[8..16].copy_from_slice(&tag_bytes(1));
        buf[40..48].copy_from_slice(&tag_bytes(2));
        let tags = scan(&buf);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].tag_pc, 0);
        assert_eq!(tags[0].data_len, 8);
        assert_eq!(tags[1].tag_pc, 40);
        assert_eq!(tags[1].data_len, 2);
    }

    #[test]
    fn scan_empty_and_vanilla_like() {
        assert!(scan(&[]).is_empty());
        assert!(scan(&[0u8; 1024]).is_empty());
    }
}
