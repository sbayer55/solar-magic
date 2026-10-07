//! Lunar Magic's additions to the object format. See
//! `docs/formats/level-objects.md` ("Lunar Magic objects").
//!
//! Verified by disassembling the object dispatch table at `$0D:A4B2` and the
//! routines it points at in Quickie World 2 (Lunar Magic 2.53) on 2026-10-06.

use super::objects::{ObjectKind, RawObject};

/// Standard object 0x22: direct Map16 tile from page 0 (4 bytes).
pub const DIRECT_MAP16_PAGE0: u8 = 0x22;
/// Standard object 0x23: direct Map16 tile from page 1 (4 bytes).
pub const DIRECT_MAP16_PAGE1: u8 = 0x23;
/// Standard object 0x27: direct Map16 tile with a 16-bit tile number (5–8 bytes).
pub const DIRECT_MAP16: u8 = 0x27;
/// Standard object 0x29: like 0x27 but for tiles 0x4000 and up.
pub const DIRECT_MAP16_HIGH: u8 = 0x29;
/// Extended object 2: screen exit with a 16-bit destination word (5 bytes).
pub const SCREEN_EXIT_EXT: u8 = 0x02;

/// Encoded length of a Lunar Magic-format object, given its number, third
/// byte and (if present) fourth byte. `None` when the fourth byte is needed
/// but missing.
pub(crate) fn object_len(number: u8, b2: u8, b3: Option<u8>) -> Option<usize> {
    Some(match number {
        0 => match b2 {
            0 => 4,
            SCREEN_EXIT_EXT => 5,
            _ => 3,
        },
        DIRECT_MAP16_PAGE0 | DIRECT_MAP16_PAGE1 => 4,
        DIRECT_MAP16 | DIRECT_MAP16_HIGH => direct_map16_len(b2, b3?),
        _ => 3,
    })
}

/// Length of object 0x27/0x29 from its size byte and fourth byte: bit 7 of
/// byte 3 adds a pattern byte, bits 7+6 add a height byte, and then bit 7 of
/// the size byte adds a condition byte (`$0D:F150`, `$0D:F1C0`, `$0D:F290`).
const fn direct_map16_len(b2: u8, b3: u8) -> usize {
    match (b3 & 0xC0, b2 & 0x80) {
        (0x00 | 0x40, _) => 5,
        (0x80, _) => 6,
        (0xC0, 0) => 7,
        _ => 8,
    }
}

/// Classifies a Lunar Magic-only object, or `None` if it is a vanilla kind.
pub(crate) fn kind(raw: &RawObject) -> Option<ObjectKind> {
    let b = &raw.bytes;
    match raw.number() {
        0 if b[2] == SCREEN_EXIT_EXT => Some(ObjectKind::ScreenExitExt {
            screen: raw.y(),
            destination: (u16::from(b[4] & 0x01) << 8) | u16::from(b[3]),
            secondary: b[4] & 0x02 != 0,
            flags: b[4],
        }),
        n @ (DIRECT_MAP16_PAGE0 | DIRECT_MAP16_PAGE1) => Some(ObjectKind::DirectMap16 {
            tile: (u16::from(n & 0x01) << 8) | u16::from(b[3]),
            size: b[2],
        }),
        n @ (DIRECT_MAP16 | DIRECT_MAP16_HIGH) => {
            let high = if n == DIRECT_MAP16_HIGH { 0x4000 } else { 0 };
            Some(ObjectKind::DirectMap16Ext {
                tile: high | (u16::from(b[3] & 0x3F) << 8) | u16::from(b[4]),
                size: b[2],
                ext: [b[5], b[6], b[7]],
                ext_len: raw.len.saturating_sub(5),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{direct_map16_len, object_len};
    use crate::level::{ObjectFormat, ObjectKind, ObjectStream};

    #[test]
    fn lengths() {
        assert_eq!(object_len(0, 0, None), Some(4));
        assert_eq!(object_len(0, 2, None), Some(5));
        assert_eq!(object_len(0, 1, None), Some(3));
        assert_eq!(object_len(0x22, 0x31, None), Some(4));
        assert_eq!(object_len(0x23, 0x31, None), Some(4));
        assert_eq!(object_len(0x24, 0, None), Some(3));
        assert_eq!(object_len(0x27, 0, None), None, "needs byte 3");
        assert_eq!(object_len(0x27, 0x0F, Some(0x07)), Some(5));
        assert_eq!(object_len(0x29, 0x0F, Some(0x40)), Some(5));
        assert_eq!(direct_map16_len(0x00, 0x80), 6);
        assert_eq!(direct_map16_len(0x00, 0xC0), 7);
        assert_eq!(direct_map16_len(0x80, 0xC0), 8);
        assert_eq!(direct_map16_len(0x80, 0x80), 6, "condition byte needs the height byte");
    }

    #[test]
    fn lm_stream_round_trips_and_classifies() {
        // 0x27 simple (tile 0x078E, size 0x0F), 0x22 (tile 0x31), 0x23 (tile 0x1A5),
        // 0x27 with pattern+height+condition, 0x29 simple, ext 2 exit, terminator.
        let data: Vec<u8> = [
            &[0x58, 0x70, 0x0F, 0x07, 0x8E][..],
            &[0x45, 0x23, 0x10, 0x31],
            &[0x45, 0x33, 0x00, 0xA5],
            &[0x45, 0x70, 0x81, 0xC3, 0x21, 0x12, 0x05, 0x33],
            &[0x45, 0x90, 0x00, 0x01, 0x02],
            &[0x07, 0x03, 0x02, 0x05, 0x03],
            &[0xFF],
        ]
        .concat();
        let stream = ObjectStream::parse(&data, ObjectFormat::LunarMagic).expect("terminated");
        assert_eq!(stream.objects.len(), 6);
        assert_eq!(stream.to_bytes(), data);
        assert_eq!(
            stream.objects[0].kind(),
            ObjectKind::DirectMap16Ext { tile: 0x078E, size: 0x0F, ext: [0; 3], ext_len: 0 }
        );
        assert_eq!(stream.objects[1].kind(), ObjectKind::DirectMap16 { tile: 0x031, size: 0x10 });
        assert_eq!(stream.objects[2].kind(), ObjectKind::DirectMap16 { tile: 0x1A5, size: 0x00 });
        assert_eq!(
            stream.objects[3].kind(),
            ObjectKind::DirectMap16Ext {
                tile: 0x0321,
                size: 0x81,
                ext: [0x12, 0x05, 0x33],
                ext_len: 3
            }
        );
        assert_eq!(
            stream.objects[4].kind(),
            ObjectKind::DirectMap16Ext { tile: 0x4102, size: 0x00, ext: [0; 3], ext_len: 0 }
        );
        assert_eq!(
            stream.objects[5].kind(),
            ObjectKind::ScreenExitExt { screen: 7, destination: 0x105, secondary: true, flags: 3 }
        );
        // The same bytes mean something else (and end elsewhere) in vanilla format.
        let vanilla = ObjectStream::parse(&data, ObjectFormat::Vanilla);
        assert!(vanilla.is_none_or(|s| s.encoded_len() != data.len()));
    }
}
