//! Layer 2: either an object stream or a run-length background tilemap.
//! See `docs/formats/layer2.md`.

use super::ObjectStream;

/// Decoded background tilemap size in bytes (8-bit tilemaps: two screens
/// of tile low bytes).
pub const BG_TILEMAP_LEN: usize = 0x400;

/// Decoded size of a Lunar Magic 16-bit tilemap: 0x400 low bytes followed
/// by 0x400 high bytes.
pub const BG_TILEMAP_LEN_16BIT: usize = 0x800;

/// Rows per screen of an 8-bit background tilemap.
pub const BG_ROWS: u8 = 27;
/// Rows per screen of a 16-bit background tilemap.
pub const BG_ROWS_16BIT: u8 = 32;

/// Backgrounds stored at or above this bank-`$0C` offset use BG tiles 0x100+.
pub(crate) const SECOND_PAGE_OFFSET: u16 = 0xE8FE;

/// Value the game pre-fills the tilemap with.
const BG_FILL: u8 = 0x25;

/// A decoded 8-bit background tilemap: two screens of Map16 tile low bytes.
pub type BgTilemap = Box<[u8; BG_TILEMAP_LEN]>;

/// A decoded background: the tilemap bytes and the RLE stream they came from
/// (including its `FF FF` terminator).
pub type DecodedBackground = (Vec<u8>, Vec<u8>);

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
        /// Which 16-page background Map16 set the tiles index (Lunar Magic;
        /// 0 for vanilla).
        page_set: u8,
        /// 27 for vanilla and Lunar Magic 8-bit tilemaps, 32 for 16-bit ones.
        rows_per_screen: u8,
        /// Decoded tile numbers for two screens, page folded in (0x400 entries).
        tiles: Vec<u16>,
        /// The RLE stream exactly as stored, including the `FF FF` end.
        compressed: Vec<u8>,
    },
}

impl Layer2 {
    /// Builds a background from an 8-bit tilemap whose tiles all sit on `page`.
    #[must_use]
    pub fn background_8bit(page: u8, tilemap: &[u8], compressed: Vec<u8>) -> Self {
        let tiles = tilemap.iter().map(|&low| u16::from(low) | (u16::from(page) << 8)).collect();
        Self::Background { page_set: 0, rows_per_screen: BG_ROWS, tiles, compressed }
    }

    /// Builds a background from a 16-bit tilemap: `decoded` holds 0x400 low
    /// bytes then 0x400 high bytes.
    #[must_use]
    pub fn background_16bit(page_set: u8, decoded: &[u8], compressed: Vec<u8>) -> Self {
        let (low, high) = decoded.split_at(decoded.len().min(BG_TILEMAP_LEN));
        let tiles = low
            .iter()
            .zip(high.iter().chain(core::iter::repeat(&0)))
            .map(|(&lo, &hi)| u16::from_le_bytes([lo, hi]))
            .collect();
        Self::Background { page_set, rows_per_screen: BG_ROWS_16BIT, tiles, compressed }
    }

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

    /// Background tile number at `index` in the decoded tilemap, page folded in.
    #[must_use]
    pub fn background_tile(&self, index: usize) -> Option<u16> {
        match self {
            Self::Background { tiles, .. } => tiles.get(index).copied(),
            Self::Objects { .. } => None,
        }
    }
}

/// Decodes a background RLE stream into `max_len` bytes.
///
/// Returns the decoded bytes (pre-filled with the game's fill value) and the
/// stream's bytes including the terminator, or `None` if the stream is
/// unterminated or would overflow `max_len`.
#[must_use]
pub fn decode_bg_rle_capped(data: &[u8], max_len: usize) -> Option<DecodedBackground> {
    let mut out = vec![BG_FILL; max_len];
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

/// Decodes a vanilla (0x400-byte) background RLE stream. See
/// [`decode_bg_rle_capped`].
#[must_use]
pub fn decode_bg_rle(data: &[u8]) -> Option<(BgTilemap, Vec<u8>)> {
    let (decoded, compressed) = decode_bg_rle_capped(data, BG_TILEMAP_LEN)?;
    let tilemap: BgTilemap = decoded.into_boxed_slice().try_into().ok()?;
    Some((tilemap, compressed))
}

#[cfg(test)]
mod tests {
    use super::{BG_FILL, BG_TILEMAP_LEN_16BIT, Layer2, decode_bg_rle, decode_bg_rle_capped};

    #[test]
    fn rle_literal_and_repeat_runs() {
        let data = [0x02, 0x10, 0x11, 0x12, 0x83, 0x77, 0xFF, 0xFF, 0xAA];
        let (map, compressed) = decode_bg_rle(&data).expect("terminated");
        assert_eq!(&map[..7], &[0x10, 0x11, 0x12, 0x77, 0x77, 0x77, 0x77]);
        assert_eq!(map[7], BG_FILL);
        assert_eq!(compressed, data[..8].to_vec());
        let layer = Layer2::background_8bit(1, &map[..], compressed);
        assert_eq!(layer.background_tile(0), Some(0x110));
        assert_eq!(layer.background_tile(0x400), None);
        assert_eq!(layer.to_bytes(), data[..8].to_vec());
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
        // A 16-bit tilemap needs the larger cap.
        let mut wide: Vec<u8> = (0..16).flat_map(|i| [0xFF, i]).collect();
        wide.extend([0xFF, 0xFF]);
        assert!(decode_bg_rle(&wide).is_none());
        let (decoded, _) = decode_bg_rle_capped(&wide, BG_TILEMAP_LEN_16BIT).expect("fits");
        let layer = Layer2::background_16bit(2, &decoded, wide.clone());
        assert_eq!(layer.background_tile(0), Some(0x0800), "low 0, high byte 8");
        assert_eq!(layer.background_tile(0x3FF), Some(0x0F07));
        assert!(matches!(layer, Layer2::Background { page_set: 2, rows_per_screen: 32, .. }));
    }
}
