//! Bit-field tables for the packed header bytes.
//!
//! The headers keep their raw bytes so they re-serialize exactly; editors
//! read and write individual fields through these tables instead of a
//! hand-written setter per field. See `docs/formats/level-header.md`.

/// One packed field: `width` bits starting at `shift` in byte `byte`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BitField {
    /// Human-readable name, also the identity used in error messages.
    pub name: &'static str,
    /// Index of the byte the field lives in.
    pub byte: usize,
    /// Bit position of the field's least significant bit.
    pub shift: u8,
    /// Number of bits, 1–8.
    pub width: u8,
}

/// Errors from setting a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FieldError {
    /// The value needs more bits than the field has.
    #[error("{field}: {value} exceeds the maximum of {max}")]
    OutOfRange {
        /// The field's name.
        field: &'static str,
        /// The rejected value.
        value: u8,
        /// Largest value the field holds.
        max: u8,
    },
    /// The field's byte index is outside the record.
    #[error("{field}: byte {byte} is outside the record")]
    BadByte {
        /// The field's name.
        field: &'static str,
        /// The byte index.
        byte: usize,
    },
}

impl BitField {
    /// Defines a field.
    #[must_use]
    pub const fn new(name: &'static str, byte: usize, shift: u8, width: u8) -> Self {
        Self { name, byte, shift, width }
    }

    /// Largest value the field can hold.
    #[must_use]
    pub const fn max(self) -> u8 {
        // Width is at most 8, so the shift cannot overflow a u16.
        #[allow(
            clippy::as_conversions,
            clippy::arithmetic_side_effects,
            reason = "width <= 8: the shift fits a u16 and the result is masked to 8 bits"
        )]
        {
            (((1u16 << self.width) - 1) & 0xFF) as u8
        }
    }

    /// The field's bit mask within its byte.
    #[must_use]
    pub const fn mask(self) -> u8 {
        self.max() << self.shift
    }

    /// Reads the field from `raw`, or 0 if the byte is missing.
    #[must_use]
    pub fn get(self, raw: &[u8]) -> u8 {
        raw.get(self.byte).map_or(0, |b| (b >> self.shift) & self.max())
    }

    /// Writes `value` into the field.
    ///
    /// # Errors
    ///
    /// [`FieldError::OutOfRange`] if `value > max`, [`FieldError::BadByte`] if
    /// `raw` is too short.
    pub fn set(self, raw: &mut [u8], value: u8) -> Result<(), FieldError> {
        if value > self.max() {
            return Err(FieldError::OutOfRange { field: self.name, value, max: self.max() });
        }
        let byte = raw
            .get_mut(self.byte)
            .ok_or(FieldError::BadByte { field: self.name, byte: self.byte })?;
        *byte = (*byte & !self.mask()) | (value << self.shift);
        Ok(())
    }
}

/// Fields of the 5-byte primary header.
pub mod primary {
    use super::BitField;

    /// Number of screens minus one.
    pub const SCREENS_MINUS_ONE: BitField = BitField::new("screens - 1", 0, 0, 5);
    /// Background palette set.
    pub const BACKGROUND_PALETTE: BitField = BitField::new("background palette", 0, 5, 3);
    /// Level mode.
    pub const LEVEL_MODE: BitField = BitField::new("level mode", 1, 0, 5);
    /// Back area colour.
    pub const BACK_AREA_COLOR: BitField = BitField::new("back area color", 1, 5, 3);
    /// Sprite GFX set.
    pub const SPRITE_SET: BitField = BitField::new("sprite set", 2, 0, 4);
    /// Music.
    pub const MUSIC: BitField = BitField::new("music", 2, 4, 3);
    /// Layer 3 priority flag.
    pub const LAYER3_PRIORITY: BitField = BitField::new("layer 3 priority", 2, 7, 1);
    /// Foreground palette set.
    pub const FOREGROUND_PALETTE: BitField = BitField::new("foreground palette", 3, 0, 3);
    /// Sprite palette set.
    pub const SPRITE_PALETTE: BitField = BitField::new("sprite palette", 3, 3, 3);
    /// Time limit code.
    pub const TIME_LIMIT: BitField = BitField::new("time limit", 3, 6, 2);
    /// FG/BG tileset.
    pub const TILESET: BitField = BitField::new("tileset", 4, 0, 4);
    /// Scroll setting.
    pub const SCROLL: BitField = BitField::new("scroll setting", 4, 4, 2);
    /// Item memory setting.
    pub const ITEM_MEMORY: BitField = BitField::new("item memory", 4, 6, 2);

    /// Every primary header field in byte order.
    pub const ALL: [BitField; 13] = [
        SCREENS_MINUS_ONE,
        BACKGROUND_PALETTE,
        LEVEL_MODE,
        BACK_AREA_COLOR,
        SPRITE_SET,
        MUSIC,
        LAYER3_PRIORITY,
        FOREGROUND_PALETTE,
        SPRITE_PALETTE,
        TIME_LIMIT,
        TILESET,
        SCROLL,
        ITEM_MEMORY,
    ];
}

/// Fields of the 4 secondary header bytes.
pub mod secondary {
    use super::BitField;

    /// Entrance Y position index.
    pub const ENTRANCE_Y: BitField = BitField::new("entrance y index", 0, 0, 4);
    /// Layer 2 scroll setting.
    pub const LAYER2_SCROLL: BitField = BitField::new("layer 2 scroll", 0, 4, 4);
    /// Entrance X position index.
    pub const ENTRANCE_X: BitField = BitField::new("entrance x index", 1, 0, 3);
    /// Entrance type.
    pub const ENTRANCE_TYPE: BitField = BitField::new("entrance type", 1, 3, 3);
    /// Layer 3 setting.
    pub const LAYER3: BitField = BitField::new("layer 3 setting", 1, 6, 2);
    /// Initial layer 2 Y code.
    pub const LAYER2_INITIAL_Y: BitField = BitField::new("layer 2 initial y", 2, 0, 2);
    /// Initial layer 1 Y code.
    pub const LAYER1_INITIAL_Y: BitField = BitField::new("layer 1 initial y", 2, 2, 2);
    /// Midway entrance screen.
    pub const MIDWAY_SCREEN: BitField = BitField::new("midway screen", 2, 4, 4);
    /// Main entrance screen.
    pub const ENTRANCE_SCREEN: BitField = BitField::new("entrance screen", 3, 0, 5);
    /// Screen mode bits.
    pub const SCREEN_MODE: BitField = BitField::new("screen mode", 3, 5, 2);
    /// Disable the "No Yoshi" intro.
    pub const DISABLE_NO_YOSHI_INTRO: BitField = BitField::new("disable no-yoshi intro", 3, 7, 1);

    /// Every secondary header field in byte order.
    pub const ALL: [BitField; 11] = [
        ENTRANCE_Y,
        LAYER2_SCROLL,
        ENTRANCE_X,
        ENTRANCE_TYPE,
        LAYER3,
        LAYER2_INITIAL_Y,
        LAYER1_INITIAL_Y,
        MIDWAY_SCREEN,
        ENTRANCE_SCREEN,
        SCREEN_MODE,
        DISABLE_NO_YOSHI_INTRO,
    ];
}

/// Fields of the 4 secondary exit bytes (one from each of the `$05:F800`
/// tables; verified 2026-10-07, see `docs/formats/level-header.md`).
pub mod secondary_exit {
    use super::BitField;

    /// Destination level, low byte.
    pub const DESTINATION_LOW: BitField = BitField::new("destination level low", 0, 0, 8);
    /// Entrance Y position index.
    pub const ENTRANCE_Y: BitField = BitField::new("entrance y index", 1, 0, 4);
    /// Initial layer 1 Y code.
    pub const LAYER1_INITIAL_Y: BitField = BitField::new("layer 1 initial y", 1, 4, 2);
    /// Initial layer 2 Y code.
    pub const LAYER2_INITIAL_Y: BitField = BitField::new("layer 2 initial y", 1, 6, 2);
    /// Entrance screen.
    pub const ENTRANCE_SCREEN: BitField = BitField::new("entrance screen", 2, 0, 5);
    /// Entrance X position index.
    pub const ENTRANCE_X: BitField = BitField::new("entrance x index", 2, 5, 3);
    /// Entrance type.
    pub const ENTRANCE_TYPE: BitField = BitField::new("entrance type", 3, 0, 3);

    /// Every secondary exit field in byte order.
    pub const ALL: [BitField; 7] = [
        DESTINATION_LOW,
        ENTRANCE_Y,
        LAYER1_INITIAL_Y,
        LAYER2_INITIAL_Y,
        ENTRANCE_SCREEN,
        ENTRANCE_X,
        ENTRANCE_TYPE,
    ];
}

/// Fields of the sprite list header byte.
pub mod sprite_header {
    use super::BitField;

    /// Sprite memory setting.
    pub const SPRITE_MEMORY: BitField = BitField::new("sprite memory", 0, 0, 6);
    /// Buoyancy setting.
    pub const BUOYANCY: BitField = BitField::new("buoyancy", 0, 6, 2);

    /// Both fields.
    pub const ALL: [BitField; 2] = [SPRITE_MEMORY, BUOYANCY];
}

#[cfg(test)]
mod tests {
    use super::{BitField, FieldError, primary, secondary, secondary_exit};

    #[test]
    fn get_and_set_isolate_bits() {
        let mut raw = [0xFFu8; 2];
        let f = BitField::new("f", 1, 3, 3);
        assert_eq!(f.max(), 7);
        assert_eq!(f.mask(), 0x38);
        assert_eq!(f.get(&raw), 7);
        f.set(&mut raw, 2).expect("fits");
        assert_eq!(raw, [0xFF, 0xC7 | 0x10]);
        assert_eq!(f.get(&raw), 2);
        assert_eq!(
            f.set(&mut raw, 8),
            Err(FieldError::OutOfRange { field: "f", value: 8, max: 7 })
        );
        assert_eq!(f.set(&mut raw[..1], 1), Err(FieldError::BadByte { field: "f", byte: 1 }));
        assert_eq!(BitField::new("w", 0, 0, 8).max(), 255);
    }

    #[test]
    fn tables_cover_every_bit_exactly_once() {
        for (table, bytes) in
            [(&primary::ALL[..], 5usize), (&secondary::ALL[..], 4), (&secondary_exit::ALL[..], 4)]
        {
            let mut covered = vec![0u8; bytes];
            for f in table {
                assert_eq!(covered[f.byte] & f.mask(), 0, "{} overlaps", f.name);
                covered[f.byte] |= f.mask();
            }
            // The exit's byte 3 has 5 unused bits; everything else is full.
            let full: Vec<u8> = covered.iter().map(|_| 0xFF).collect();
            if table.len() == secondary_exit::ALL.len() {
                assert_eq!(covered, [0xFF, 0xFF, 0xFF, 0x07]);
            } else {
                assert_eq!(covered, full);
            }
        }
    }

    #[test]
    fn primary_fields_match_typed_accessors() {
        let raw = [0x60 | 0x0A, 0xE0 | 0x1F, 0x80 | 0x60 | 0x05, 0xC0 | 0x20 | 0x02, 0x69];
        assert_eq!(primary::SCREENS_MINUS_ONE.get(&raw), 0x0A);
        assert_eq!(primary::BACKGROUND_PALETTE.get(&raw), 3);
        assert_eq!(primary::LEVEL_MODE.get(&raw), 0x1F);
        assert_eq!(primary::MUSIC.get(&raw), 6);
        assert_eq!(primary::LAYER3_PRIORITY.get(&raw), 1);
        assert_eq!(primary::TIME_LIMIT.get(&raw), 3);
        assert_eq!(primary::TILESET.get(&raw), 9);
        assert_eq!(primary::SCROLL.get(&raw), 2);
        assert_eq!(primary::ITEM_MEMORY.get(&raw), 1);
    }
}
