//! Layer 1 / layer 2 object streams. See `docs/formats/level-objects.md`.

use super::lm_objects;

/// Byte that ends an object stream.
const TERMINATOR: u8 = 0xFF;

/// Longest object any supported format produces (Lunar Magic's direct Map16
/// object with every extension byte).
pub const MAX_OBJECT_LEN: usize = 8;

/// How objects are encoded: vanilla, or with Lunar Magic's additions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ObjectFormat {
    /// Vanilla: 3 bytes, or 4 for a screen exit.
    #[default]
    Vanilla,
    /// Vanilla plus Lunar Magic's direct Map16 objects (0x22, 0x23, 0x27,
    /// 0x29) and 5-byte screen exit (extended object 2).
    LunarMagic,
}

/// One object exactly as stored plus the screen it resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RawObject {
    /// The object's bytes; only the first `len` are meaningful.
    pub bytes: [u8; MAX_OBJECT_LEN],
    /// 3 to 8; see [`ObjectFormat`].
    pub len: u8,
    /// Screen the object is placed on, after applying new-screen flags and
    /// screen jumps that precede it.
    pub screen: u8,
    /// Byte offset of this object from the start of the stream.
    pub offset: usize,
    /// The format it was parsed with, which decides how [`RawObject::kind`]
    /// reads it.
    pub format: ObjectFormat,
}

/// What a [`RawObject`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    /// A standard object (number 0x01–0x3F) with a size byte.
    Standard {
        /// Object number.
        number: u8,
        /// Size byte: usually `hhhhwwww`, object-specific.
        size: u8,
    },
    /// An extended object (number 0x00) selected by its third byte.
    Extended {
        /// Extended object number (0x02–0xFF; 0 and 1 are the variants below).
        number: u8,
    },
    /// Extended object 0: a screen exit.
    ScreenExit {
        /// Screen the exit applies to.
        screen: u8,
        /// Destination level, 0x000–0x1FF.
        destination: u16,
        /// Whether `destination` is a secondary exit number instead.
        secondary: bool,
    },
    /// Extended object 1: continue placing objects on `screen`.
    ScreenJump {
        /// New current screen.
        screen: u8,
    },
    /// Lunar Magic extended object 2: a screen exit with a 16-bit
    /// destination word (5 bytes).
    ScreenExitExt {
        /// Screen the exit applies to.
        screen: u8,
        /// Destination level or secondary exit, 0x000–0x1FF.
        destination: u16,
        /// Whether `destination` is a secondary exit number instead.
        secondary: bool,
        /// The whole flags byte (bits 0–1 are the two fields above).
        flags: u8,
    },
    /// Lunar Magic objects 0x22 / 0x23: a rectangle of one Map16 tile from
    /// page 0 / page 1 (4 bytes).
    DirectMap16 {
        /// Map16 tile number, 0x000–0x1FF.
        tile: u16,
        /// Size byte `hhhhwwww` (counts minus one).
        size: u8,
    },
    /// Lunar Magic objects 0x27 / 0x29: a rectangle of one Map16 tile from
    /// any page (5 to 8 bytes).
    DirectMap16Ext {
        /// Map16 tile number, 0x0000–0x7FFF (0x4000 and up for object 0x29).
        tile: u16,
        /// Size byte (`hhhhwwww`, bit 7 = a conditional byte follows).
        size: u8,
        /// Extension bytes 5–7 as stored (pattern nibbles, height, condition).
        ext: [u8; 3],
        /// How many of `ext` are present.
        ext_len: u8,
    },
}

impl RawObject {
    /// Whether the new-screen flag is set (bit 7 of byte 0).
    #[must_use]
    pub const fn new_screen(&self) -> bool {
        self.bytes[0] & 0x80 != 0
    }

    /// Object number 0x00–0x3F (0 = extended).
    #[must_use]
    pub const fn number(&self) -> u8 {
        number_of(self.bytes[0], self.bytes[1])
    }

    /// Y position within the screen, 0–31 (byte 0 bits 0–4).
    #[must_use]
    pub const fn y(&self) -> u8 {
        self.bytes[0] & 0x1F
    }

    /// X position within the screen, 0–15 (byte 1 bits 0–3).
    #[must_use]
    pub const fn x(&self) -> u8 {
        self.bytes[1] & 0x0F
    }

    /// Third byte: size for standard objects, number for extended ones.
    #[must_use]
    pub const fn third(&self) -> u8 {
        self.bytes[2]
    }

    /// Classifies the object.
    #[must_use]
    pub fn kind(&self) -> ObjectKind {
        if self.format == ObjectFormat::LunarMagic
            && let Some(kind) = lm_objects::kind(self)
        {
            return kind;
        }
        let number = self.number();
        if number != 0 {
            return ObjectKind::Standard { number, size: self.third() };
        }
        match self.third() {
            0 => ObjectKind::ScreenExit {
                screen: self.y(),
                destination: (u16::from(self.bytes[1] & 0x01) << 8) | u16::from(self.bytes[3]),
                secondary: self.bytes[1] & 0x02 != 0,
            },
            1 => ObjectKind::ScreenJump { screen: self.y() },
            ext => ObjectKind::Extended { number: ext },
        }
    }

    /// The stored bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..usize::from(self.len)).unwrap_or(&[])
    }
}

/// Object number from the first two bytes: bits 6–5 of byte 0 are the high
/// bits, the high nibble of byte 1 the low bits.
pub(crate) const fn number_of(b0: u8, b1: u8) -> u8 {
    ((b0 & 0x60) >> 1) | (b1 >> 4)
}

/// Encoded length of the object starting with `b0 b1 b2`, where `b3` is the
/// byte after them if any. `None` means the format needs `b3` and it is missing.
fn object_len(format: ObjectFormat, b0: u8, b1: u8, b2: u8, b3: Option<u8>) -> Option<usize> {
    let number = number_of(b0, b1);
    match format {
        ObjectFormat::Vanilla => Some(if number == 0 && b2 == 0 { 4 } else { 3 }),
        ObjectFormat::LunarMagic => lm_objects::object_len(number, b2, b3),
    }
}

/// A parsed object stream (without the level header).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObjectStream {
    /// Objects in file order.
    pub objects: Vec<RawObject>,
}

impl ObjectStream {
    /// Parses objects up to and including the terminator. Returns `None` if
    /// the data ends before a terminator.
    #[must_use]
    pub fn parse(data: &[u8], format: ObjectFormat) -> Option<Self> {
        let mut objects = Vec::new();
        let mut pos = 0usize;
        let mut screen = 0u8;
        loop {
            let b0 = *data.get(pos)?;
            if b0 == TERMINATOR {
                return Some(Self { objects });
            }
            let b1 = *data.get(pos.checked_add(1)?)?;
            let b2 = *data.get(pos.checked_add(2)?)?;
            let b3 = data.get(pos.checked_add(3)?).copied();
            let len = object_len(format, b0, b1, b2, b3)?;
            let stored = data.get(pos..pos.checked_add(len)?)?;
            let mut bytes = [0u8; MAX_OBJECT_LEN];
            bytes.get_mut(..len)?.copy_from_slice(stored);
            let mut object =
                RawObject { bytes, len: u8::try_from(len).ok()?, screen: 0, offset: pos, format };
            if object.new_screen() {
                screen = screen.wrapping_add(1);
            }
            object.screen = screen;
            if let ObjectKind::ScreenJump { screen: target } = object.kind() {
                screen = target;
            }
            pos = pos.checked_add(len)?;
            objects.push(object);
        }
    }

    /// Serializes the objects followed by the terminator.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out: Vec<u8> =
            self.objects.iter().flat_map(|o| o.as_bytes().iter().copied()).collect();
        out.push(TERMINATOR);
        out
    }

    /// Total encoded length including the terminator.
    #[must_use]
    pub fn encoded_len(&self) -> usize {
        self.objects.iter().map(|o| usize::from(o.len)).sum::<usize>().saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_OBJECT_LEN, ObjectFormat, ObjectKind, ObjectStream, RawObject};

    fn raw(bytes: &[u8]) -> RawObject {
        let mut b = [0u8; MAX_OBJECT_LEN];
        b[..bytes.len()].copy_from_slice(bytes);
        RawObject {
            bytes: b,
            len: u8::try_from(bytes.len()).expect("short"),
            screen: 0,
            offset: 0,
            format: ObjectFormat::Vanilla,
        }
    }

    #[test]
    fn number_bits_come_from_both_bytes() {
        // bits 6-5 of byte0 = 0b11, high nibble of byte1 = 0xA => number 0b11_1010 = 0x3A
        let o = raw(&[0x60 | 0x15, 0xA7, 0x42]);
        assert_eq!(o.number(), 0x3A);
        assert_eq!(o.y(), 0x15);
        assert_eq!(o.x(), 7);
        assert_eq!(o.kind(), ObjectKind::Standard { number: 0x3A, size: 0x42 });
        assert!(!o.new_screen());
    }

    const MIXED: [u8; 17] = [
        0x05,
        0x13,
        0x11, // standard object 1 at (3,5), size 0x11, screen 0
        0x80 | 0x02,
        0x04,
        0x30, // new screen (1): extended object 0x30 at (4,2)
        0x07,
        0x03,
        0x00,
        0x05, // screen exit on screen 7 -> level 0x105 (bit0 of byte1 = 1), secondary
        0x0A,
        0x00,
        0x01, // screen jump to screen 0x0A
        0x80 | 0x01,
        0x20,
        0x00, // new screen -> screen 0x0B, standard object 2 at (0,1)
        0xFF,
    ];

    #[test]
    fn parses_mixed_stream_and_tracks_screens() {
        let data = MIXED;
        let stream = ObjectStream::parse(&data, ObjectFormat::Vanilla).expect("terminated");
        assert_eq!(stream.objects.len(), 5);
        assert_eq!(stream.objects[0].screen, 0);
        assert_eq!(stream.objects[1].screen, 1);
        assert_eq!(stream.objects[1].kind(), ObjectKind::Extended { number: 0x30 });
        assert_eq!(
            stream.objects[2].kind(),
            ObjectKind::ScreenExit { screen: 7, destination: 0x105, secondary: true }
        );
        assert_eq!(stream.objects[2].len, 4);
        assert_eq!(stream.objects[3].kind(), ObjectKind::ScreenJump { screen: 0x0A });
        assert_eq!(stream.objects[4].screen, 0x0B);
        assert_eq!(stream.objects[4].offset, 13);
        assert_eq!(stream.to_bytes(), data.to_vec());
        assert_eq!(stream.encoded_len(), data.len());
    }

    #[test]
    fn vanilla_stream_parses_identically_under_both_formats() {
        let vanilla = ObjectStream::parse(&MIXED, ObjectFormat::Vanilla).expect("terminated");
        let lm = ObjectStream::parse(&MIXED, ObjectFormat::LunarMagic).expect("terminated");
        assert_eq!(vanilla.objects.len(), lm.objects.len());
        for (a, b) in vanilla.objects.iter().zip(&lm.objects) {
            assert_eq!(a.as_bytes(), b.as_bytes());
            assert_eq!(a.kind(), b.kind());
            assert_eq!(a.screen, b.screen);
        }
    }

    #[test]
    fn unterminated_is_none() {
        let f = ObjectFormat::Vanilla;
        assert!(ObjectStream::parse(&[0x05, 0x13, 0x11], f).is_none());
        assert!(ObjectStream::parse(&[0x07, 0x03, 0x00], f).is_none(), "exit needs its 4th byte");
        assert!(ObjectStream::parse(&[], f).is_none());
        assert_eq!(ObjectStream::parse(&[0xFF], f).expect("empty").objects.len(), 0);
    }
}
