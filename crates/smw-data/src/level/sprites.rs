//! Sprite lists. See `docs/formats/sprites.md`.

/// Byte that ends a sprite list.
const TERMINATOR: u8 = 0xFF;

/// One sprite entry as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sprite {
    /// `YYYYEEsy`, `XXXXssss`, sprite number.
    pub bytes: [u8; 3],
}

impl Sprite {
    /// Y position within the screen, 0–31.
    #[must_use]
    pub const fn y(self) -> u8 {
        (self.bytes[0] >> 4) | ((self.bytes[0] & 0x01) << 4)
    }

    /// Extra bits, 0–3.
    #[must_use]
    pub const fn extra_bits(self) -> u8 {
        (self.bytes[0] >> 2) & 0x03
    }

    /// Screen number, 0–31.
    #[must_use]
    pub const fn screen(self) -> u8 {
        (self.bytes[1] & 0x0F) | ((self.bytes[0] & 0x02) << 3)
    }

    /// X position within the screen, 0–15.
    #[must_use]
    pub const fn x(self) -> u8 {
        self.bytes[1] >> 4
    }

    /// Sprite number.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.bytes[2]
    }

    /// Whether this is a scroll command rather than a sprite (number ≥ 0xE7).
    #[must_use]
    pub const fn is_scroll_command(self) -> bool {
        self.bytes[2] >= 0xE7
    }
}

/// A level's sprite data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpriteList {
    /// Header byte: bits 0–5 sprite memory, bits 6–7 buoyancy.
    pub header: u8,
    /// Sprites in file order.
    pub sprites: Vec<Sprite>,
}

impl SpriteList {
    /// Parses a header byte and entries up to the terminator.
    #[must_use]
    pub fn parse(data: &[u8]) -> Option<Self> {
        let header = *data.first()?;
        let mut sprites = Vec::new();
        let mut pos = 1usize;
        loop {
            let b0 = *data.get(pos)?;
            if b0 == TERMINATOR {
                return Some(Self { header, sprites });
            }
            let b1 = *data.get(pos.checked_add(1)?)?;
            let b2 = *data.get(pos.checked_add(2)?)?;
            sprites.push(Sprite { bytes: [b0, b1, b2] });
            pos = pos.checked_add(3)?;
        }
    }

    /// Sprite memory setting, 0–63.
    #[must_use]
    pub const fn sprite_memory(&self) -> u8 {
        self.header & 0x3F
    }

    /// Buoyancy setting, 0–3.
    #[must_use]
    pub const fn buoyancy(&self) -> u8 {
        self.header >> 6
    }

    /// Serializes header, sprites and terminator.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![self.header];
        out.extend(self.sprites.iter().flat_map(|s| s.bytes));
        out.push(TERMINATOR);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{Sprite, SpriteList};

    #[test]
    fn sprite_fields() {
        // Y = 0b1_0110 (0x16): high nibble 0110, low bit 1; extra 0b10; screen bit4 set.
        let s = Sprite { bytes: [0x60 | 0x08 | 0x02 | 0x01, 0x90 | 0x03, 0x1E] };
        assert_eq!(s.y(), 0x16);
        assert_eq!(s.extra_bits(), 2);
        assert_eq!(s.screen(), 0x13);
        assert_eq!(s.x(), 9);
        assert_eq!(s.number(), 0x1E);
        assert!(!s.is_scroll_command());
    }

    #[test]
    fn parse_and_round_trip() {
        let data = [0xC5, 0x10, 0x25, 0x04, 0x00, 0x11, 0xE7, 0xFF, 0x12];
        let list = SpriteList::parse(&data).expect("terminated");
        assert_eq!(list.sprite_memory(), 5);
        assert_eq!(list.buoyancy(), 3);
        assert_eq!(list.sprites.len(), 2);
        assert!(list.sprites[1].is_scroll_command());
        assert_eq!(list.to_bytes(), data[..8].to_vec());
        assert!(SpriteList::parse(&[0x00, 0x10, 0x25]).is_none());
        assert!(SpriteList::parse(&[]).is_none());
    }
}
