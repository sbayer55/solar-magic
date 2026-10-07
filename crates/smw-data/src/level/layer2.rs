//! Layer 2: either an object stream or a run-length background tilemap.
//! See `docs/formats/layer2.md`.

use super::ObjectStream;

/// Decoded background tilemap size in bytes.
pub const BG_TILEMAP_LEN: usize = 0x400;

/// Backgrounds stored at or above this bank-`$0C` offset use BG tiles 0x100+.
pub(crate) const SECOND_PAGE_OFFSET: u16 = 0xE8FE;

/// Value the game pre-fills the tilemap with.
const BG_FILL: u8 = 0x25;

/// A decoded background tilemap: two screens of Map16 tile low bytes.
pub type BgTilemap = Box<[u8; BG_TILEMAP_LEN]>;

/// A level's layer 2 data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layer2 {
    /// An object stream, preceded by an unused copy of a 5-byte header.
    Objects {
        /// The 5 header bytes the game skips over.
        header_copy: [u8; 5],
        /// The objects.
        stream: ObjectStream,
    },
    /// A background tilemap.
    Background {
        /// 0 or 1: which 256-tile half of the background Map16 set is used.
        page: u8,
        /// Decoded tile numbers (low bytes), two screens.
        tilemap: BgTilemap,
        /// The RLE stream exactly as stored, including the `FF FF` end.
        compressed: Vec<u8>,
    },
}

impl Layer2 {
    /// Serializes to the bytes as stored in ROM.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::Objects { header_copy, stream } => {
                let mut out = header_copy.to_vec();
                out.extend(stream.to_bytes());
                out
            }
            Self::Background { compressed, .. } => compressed.clone(),
        }
    }

    /// Background tile number at `(index)` in the decoded tilemap, with the
    /// page folded in.
    #[must_use]
    pub fn background_tile(&self, index: usize) -> Option<u16> {
        match self {
            Self::Background { page, tilemap, .. } => {
                tilemap.get(index).map(|&low| u16::from(low) | (u16::from(*page) << 8))
            }
            Self::Objects { .. } => None,
        }
    }
}

/// Decodes a background RLE stream. Returns the 0x400-byte tilemap and the
/// stream's bytes (including the terminator), or `None` if the stream is
/// unterminated or would overflow the tilemap.
#[must_use]
pub fn decode_bg_rle(data: &[u8]) -> Option<(BgTilemap, Vec<u8>)> {
    let mut out: BgTilemap = Box::new([BG_FILL; BG_TILEMAP_LEN]);
    let mut pos = 0usize;
    let mut written = 0usize;
    loop {
        let control = *data.get(pos)?;
        if control == 0xFF && *data.get(pos.checked_add(1)?)? == 0xFF {
            let consumed = pos.checked_add(2)?;
            return Some((out, data.get(..consumed)?.to_vec()));
        }
        pos = pos.checked_add(1)?;
        let count = usize::from(control & 0x7F).checked_add(1)?;
        if control & 0x80 != 0 {
            let value = *data.get(pos)?;
            pos = pos.checked_add(1)?;
            for _ in 0..count {
                *out.get_mut(written)? = value;
                written = written.checked_add(1)?;
            }
        } else {
            for _ in 0..count {
                *out.get_mut(written)? = *data.get(pos)?;
                pos = pos.checked_add(1)?;
                written = written.checked_add(1)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BG_FILL, Layer2, decode_bg_rle};

    #[test]
    fn rle_literal_and_repeat_runs() {
        let data = [0x02, 0x10, 0x11, 0x12, 0x83, 0x77, 0xFF, 0xFF, 0xAA];
        let (map, compressed) = decode_bg_rle(&data).expect("terminated");
        assert_eq!(&map[..7], &[0x10, 0x11, 0x12, 0x77, 0x77, 0x77, 0x77]);
        assert_eq!(map[7], BG_FILL);
        assert_eq!(compressed, data[..8].to_vec());
        let layer = Layer2::Background { page: 1, tilemap: map, compressed };
        assert_eq!(layer.background_tile(0), Some(0x110));
        assert_eq!(layer.background_tile(0x400), None);
    }

    #[test]
    fn rle_rejects_bad_streams() {
        assert!(decode_bg_rle(&[0x02, 0x10]).is_none(), "unterminated");
        assert!(decode_bg_rle(&[0xFF]).is_none(), "half terminator");
        // Eight 128-byte repeat runs fill the tilemap exactly; one more byte overflows.
        let mut exact: Vec<u8> = (0..8).flat_map(|_| [0xFF, 0x00]).collect();
        let mut overflow = exact.clone();
        exact.extend([0xFF, 0xFF]);
        assert!(decode_bg_rle(&exact).is_some());
        overflow.extend([0x80, 0x00, 0xFF, 0xFF]);
        assert!(decode_bg_rle(&overflow).is_none());
    }
}
