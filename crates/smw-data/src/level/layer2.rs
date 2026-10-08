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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
        #[serde(with = "tilemap_serde")]
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

    /// Replaces one tilemap byte and re-encodes the stream. Returns `None`
    /// (unchanged) if this is not a background or `index` is out of range.
    pub fn set_background_tile(&mut self, index: usize, low: u8) -> Option<()> {
        let Self::Background { tilemap, compressed, .. } = self else {
            return None;
        };
        *tilemap.get_mut(index)? = low;
        *compressed = encode_bg_rle(tilemap);
        Some(())
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

/// Longest run one control byte can describe.
const MAX_RUN: usize = 128;

/// Encodes a tilemap as a background RLE stream ending in `FF FF`.
///
/// Trailing fill bytes (`0x25`) are omitted because the game pre-fills the
/// tilemap with them. Runs of three or more equal bytes become repeat
/// commands, everything else literal runs; a repeat of 128 × `0xFF` would
/// collide with the terminator and is capped at 127.
#[must_use]
pub fn encode_bg_rle(tilemap: &[u8; BG_TILEMAP_LEN]) -> Vec<u8> {
    let end = tilemap.iter().rposition(|&b| b != BG_FILL).map_or(0, |i| i.saturating_add(1));
    let data = tilemap.get(..end).unwrap_or(&[]);
    let mut out = Vec::with_capacity(end.saturating_add(2));
    let mut literal: Vec<u8> = Vec::new();
    let mut pos = 0usize;
    while let Some(&value) = data.get(pos) {
        let max_run = if value == 0xFF { MAX_RUN.saturating_sub(1) } else { MAX_RUN };
        let run = data
            .get(pos..)
            .map_or(0, |rest| rest.iter().take(max_run).take_while(|&&b| b == value).count());
        if run >= 3 {
            flush_literal(&mut out, &mut literal);
            out.push(0x80 | u8::try_from(run.saturating_sub(1)).unwrap_or(0x7F));
            out.push(value);
            pos = pos.saturating_add(run);
        } else {
            literal.push(value);
            if literal.len() == MAX_RUN {
                flush_literal(&mut out, &mut literal);
            }
            pos = pos.saturating_add(1);
        }
    }
    flush_literal(&mut out, &mut literal);
    out.extend([0xFF, 0xFF]);
    out
}

/// Emits a pending literal run (1 to 128 bytes).
fn flush_literal(out: &mut Vec<u8>, literal: &mut Vec<u8>) {
    if literal.is_empty() {
        return;
    }
    out.push(u8::try_from(literal.len().saturating_sub(1)).unwrap_or(0x7F));
    out.append(literal);
}

/// Serializes the boxed tilemap as a plain byte sequence.
mod tilemap_serde {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{BG_TILEMAP_LEN, BgTilemap};

    pub(super) fn serialize<S: Serializer>(map: &BgTilemap, s: S) -> Result<S::Ok, S::Error> {
        map.as_slice().serialize(s)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<BgTilemap, D::Error> {
        let bytes = Vec::<u8>::deserialize(d)?;
        let len = bytes.len();
        let array: [u8; BG_TILEMAP_LEN] = bytes
            .try_into()
            .map_err(|_| D::Error::invalid_length(len, &"exactly 1024 tilemap bytes"))?;
        Ok(Box::new(array))
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
    fn encoder_round_trips_and_handles_edge_cases() {
        let mut map = [BG_FILL; super::BG_TILEMAP_LEN];
        map[..7].copy_from_slice(&[0x10, 0x11, 0x12, 0x77, 0x77, 0x77, 0x77]);
        let encoded = super::encode_bg_rle(&map);
        assert_eq!(encoded, vec![0x02, 0x10, 0x11, 0x12, 0x83, 0x77, 0xFF, 0xFF]);
        let (decoded, _) = decode_bg_rle(&encoded).expect("valid");
        assert_eq!(*decoded, map);

        // 130 × 0xFF must not produce a (FF, FF) control pair.
        let mut ffs = [BG_FILL; super::BG_TILEMAP_LEN];
        ffs[..130].fill(0xFF);
        let ff_stream = super::encode_bg_rle(&ffs);
        assert_eq!(ff_stream, vec![0xFE, 0xFF, 0x82, 0xFF, 0xFF, 0xFF]);
        assert_eq!(*decode_bg_rle(&ff_stream).expect("valid").0, ffs);

        // Long literal runs split at 128; an all-fill map is just the terminator.
        let mut lit = [BG_FILL; super::BG_TILEMAP_LEN];
        for (i, b) in lit.iter_mut().take(200).enumerate() {
            *b = u8::try_from(i & 0xFF).unwrap_or(0);
        }
        let lit_stream = super::encode_bg_rle(&lit);
        assert_eq!(*decode_bg_rle(&lit_stream).expect("valid").0, lit);
        assert_eq!(super::encode_bg_rle(&[BG_FILL; super::BG_TILEMAP_LEN]), vec![0xFF, 0xFF]);

        let mut layer = Layer2::Background { page: 0, tilemap: Box::new(map), compressed: vec![] };
        assert_eq!(layer.set_background_tile(3, 0x20), Some(()));
        assert_eq!(layer.background_tile(3), Some(0x20));
        assert_eq!(layer.to_bytes(), vec![0x03, 0x10, 0x11, 0x12, 0x20, 0x82, 0x77, 0xFF, 0xFF]);
        assert_eq!(layer.set_background_tile(0x400, 0), None);
    }

    /// Every vanilla background re-encodes no larger than it is stored.
    #[test]
    fn real_rom_backgrounds_reencode() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = crate::RomLayout::detect(&rom).expect("vanilla");
        let mut checked = 0;
        for id in crate::LevelId::all() {
            let level = crate::level::Level::load(&rom, &layout, id).expect("loads");
            let Layer2::Background { tilemap, compressed, .. } = &level.layer2 else {
                continue;
            };
            let encoded = super::encode_bg_rle(tilemap);
            let (decoded, _) = decode_bg_rle(&encoded).expect("valid");
            assert_eq!(decoded, *tilemap, "level {id}");
            assert!(
                encoded.len() <= compressed.len(),
                "level {id}: {} > {}",
                encoded.len(),
                compressed.len()
            );
            checked += 1;
        }
        assert!(checked > 50);
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
