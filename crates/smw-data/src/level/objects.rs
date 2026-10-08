//! Layer 1 / layer 2 object streams. See `docs/formats/level-objects.md`.

/// Byte that ends an object stream.
const TERMINATOR: u8 = 0xFF;

/// One object exactly as stored (3 or 4 bytes) plus the screen it resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RawObject {
    /// The object's bytes; only the first `len` are meaningful.
    pub bytes: [u8; 4],
    /// 3, or 4 for screen exits.
    pub len: u8,
    /// Screen the object is placed on, after applying new-screen flags and
    /// screen jumps that precede it.
    pub screen: u8,
    /// Byte offset of this object from the start of the stream.
    pub offset: usize,
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
        ((self.bytes[0] & 0x60) >> 1) | (self.bytes[1] >> 4)
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

/// A parsed object stream (without the level header).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ObjectStream {
    /// Objects in file order.
    pub objects: Vec<RawObject>,
}

impl ObjectStream {
    /// Parses objects up to and including the terminator. Returns `None` if
    /// the data ends before a terminator.
    #[must_use]
    pub fn parse(data: &[u8]) -> Option<Self> {
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
            let mut object = RawObject { bytes: [b0, b1, b2, 0], len: 3, screen: 0, offset: pos };
            if object.number() == 0 && b2 == 0 {
                object.bytes[3] = *data.get(pos.checked_add(3)?)?;
                object.len = 4;
            }
            if object.new_screen() {
                screen = screen.wrapping_add(1);
            }
            object.screen = screen;
            if let ObjectKind::ScreenJump { screen: target } = object.kind() {
                screen = target;
            }
            pos = pos.checked_add(usize::from(object.len))?;
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
    use super::{ObjectKind, ObjectStream, RawObject};

    #[test]
    fn number_bits_come_from_both_bytes() {
        // bits 6-5 of byte0 = 0b11, high nibble of byte1 = 0xA => number 0b11_1010 = 0x3A
        let o = RawObject { bytes: [0x60 | 0x15, 0xA7, 0x42, 0], len: 3, screen: 0, offset: 0 };
        assert_eq!(o.number(), 0x3A);
        assert_eq!(o.y(), 0x15);
        assert_eq!(o.x(), 7);
        assert_eq!(o.kind(), ObjectKind::Standard { number: 0x3A, size: 0x42 });
        assert!(!o.new_screen());
    }

    #[test]
    fn parses_mixed_stream_and_tracks_screens() {
        let data = [
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
        let stream = ObjectStream::parse(&data).expect("terminated");
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
    fn unterminated_is_none() {
        assert!(ObjectStream::parse(&[0x05, 0x13, 0x11]).is_none());
        assert!(ObjectStream::parse(&[0x07, 0x03, 0x00]).is_none(), "exit needs its 4th byte");
        assert!(ObjectStream::parse(&[]).is_none());
        assert_eq!(ObjectStream::parse(&[0xFF]).expect("empty").objects.len(), 0);
    }
}
