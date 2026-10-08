//! The editing API: structural changes to a [`Level`] that keep its
//! serialization consistent. Every method leaves the level in a state that
//! [`Level::layer1_bytes`] and friends can serialize.

use std::collections::BTreeMap;

use super::fields::{BitField, FieldError, primary};
use super::objects::{ObjectKind, ObjectStream, RawObject};
use super::screens::{TilePos, join_position, split_position};
use super::sprites::Sprite;
use super::{Layer2, Level};

/// Which object layer an edit targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// Layer 1.
    One,
    /// Layer 2 (only when it holds objects).
    Two,
}

/// A new standard or extended object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectSpec {
    /// Object number, 0x00 (extended) to 0x3F.
    pub number: u8,
    /// Size byte for standard objects, extended object number otherwise.
    pub third: u8,
    /// Where to place it.
    pub pos: TilePos,
}

/// A new sprite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpriteSpec {
    /// Sprite number.
    pub number: u8,
    /// Extra bits, 0–3.
    pub extra_bits: u8,
    /// Where to place it.
    pub pos: TilePos,
}

/// A screen exit as seen by the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenExitInfo {
    /// Destination level, or secondary exit number when `secondary`.
    pub destination: u16,
    /// Whether `destination` names a secondary exit.
    pub secondary: bool,
}

/// Pixel position of an entrance from its X/Y position indices, relative to
/// the entrance screen (horizontal levels: X within the screen, absolute Y;
/// vertical levels: absolute X, Y within the screen).
///
/// # Errors
///
/// ROM read failures.
pub fn entrance_pixel(
    rom: &snes_rom::Rom,
    layout: &crate::RomLayout,
    x_index: u8,
    y_index: u8,
) -> Result<(u16, u16), snes_rom::RomError> {
    let tables = layout.entrance;
    let read = |table: snes_rom::SnesAddr, index: u8| {
        table
            .checked_add(u32::from(index))
            .ok_or(snes_rom::RomError::Unmapped(table))
            .and_then(|addr| rom.read_u8(addr))
    };
    let x = u16::from_le_bytes([
        read(tables.x_low, x_index & 0x07)?,
        read(tables.x_high, x_index & 0x07)?,
    ]);
    let y = u16::from_le_bytes([
        read(tables.y_low, y_index & 0x0F)?,
        read(tables.y_high, y_index & 0x0F)?,
    ]);
    Ok((x, y))
}

/// Errors from editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EditError {
    /// No object at that index.
    #[error("no object {index} on {layer:?}")]
    NoSuchObject {
        /// The layer.
        layer: Layer,
        /// The index.
        index: usize,
    },
    /// No sprite at that index.
    #[error("no sprite {index}")]
    NoSuchSprite {
        /// The index.
        index: usize,
    },
    /// Layer 2 is a background tilemap, so it has no objects.
    #[error("layer 2 is a background tilemap")]
    Layer2IsBackground,
    /// The position does not fit the level.
    #[error("position ({}, {}) is outside the level", pos.x, pos.y)]
    OutOfBounds {
        /// The rejected position.
        pos: TilePos,
    },
    /// The object has no position (screen exit or jump).
    #[error("object {index} has no position")]
    NotPositioned {
        /// The index.
        index: usize,
    },
    /// More screens than the tile RAM holds in this configuration.
    #[error("{requested} screens exceeds the maximum of {max}")]
    TooManyScreens {
        /// Requested count.
        requested: u8,
        /// Largest allowed count.
        max: u8,
    },
    /// Object numbers are 6 bits.
    #[error("object number {0:#04X} exceeds 0x3F")]
    BadObjectNumber(u8),
    /// A header field rejected the value.
    #[error(transparent)]
    Field(#[from] FieldError),
}

impl Sprite {
    /// A sprite at `pos`, or `None` if the position does not fit.
    #[must_use]
    pub fn at(number: u8, extra_bits: u8, pos: TilePos, vertical: bool) -> Option<Self> {
        let mut sprite = Self { bytes: [(extra_bits & 0x03) << 2, 0, number] };
        sprite.place(pos, vertical)?;
        Some(sprite)
    }

    /// The sprite's absolute tile position.
    #[must_use]
    pub fn position(self, vertical: bool) -> TilePos {
        // `y()` reads the 5-bit field and `x()` the 4-bit one; vertical levels
        // store X in the 5-bit field and Y in the 4-bit one.
        if vertical {
            join_position(self.screen(), self.y(), self.x(), true)
        } else {
            join_position(self.screen(), self.x(), self.y(), false)
        }
    }

    /// Moves the sprite to `pos`. Returns `None` (unchanged) if it does not fit.
    pub fn place(&mut self, pos: TilePos, vertical: bool) -> Option<()> {
        let (screen, x, y) = split_position(pos, vertical)?;
        let (wide, narrow) = if vertical { (x, y) } else { (y, x) };
        self.bytes[0] =
            ((wide & 0x0F) << 4) | (self.bytes[0] & 0x0C) | ((screen & 0x10) >> 3) | (wide >> 4);
        self.bytes[1] = (narrow << 4) | (screen & 0x0F);
        Some(())
    }
}

impl Level {
    /// Largest screen count the tile RAM holds: 32 horizontal or 28 vertical,
    /// halved when layer 2 holds objects (verified against `LoadBlkPtrs`).
    #[must_use]
    pub const fn max_screens(&self) -> u8 {
        let base = if self.vertical { 28 } else { 32 };
        if matches!(self.layer2, Layer2::Objects { .. }) { base / 2 } else { base }
    }

    /// The object stream of `layer`, if it has one.
    #[must_use]
    pub const fn stream(&self, layer: Layer) -> Option<&ObjectStream> {
        match (layer, &self.layer2) {
            (Layer::One, _) => Some(&self.layer1),
            (Layer::Two, Layer2::Objects { stream, .. }) => Some(stream),
            (Layer::Two, Layer2::Background { .. }) => None,
        }
    }

    const fn stream_mut(&mut self, layer: Layer) -> Result<&mut ObjectStream, EditError> {
        match (layer, &mut self.layer2) {
            (Layer::One, _) => Ok(&mut self.layer1),
            (Layer::Two, Layer2::Objects { stream, .. }) => Ok(stream),
            (Layer::Two, Layer2::Background { .. }) => Err(EditError::Layer2IsBackground),
        }
    }

    fn check_bounds(&self, pos: TilePos) -> Result<(), EditError> {
        let screens = u16::from(self.header.screens());
        let (w, h) = if self.vertical {
            (32, screens.saturating_mul(16))
        } else {
            (screens.saturating_mul(16), 32)
        };
        if pos.x < w && pos.y < h { Ok(()) } else { Err(EditError::OutOfBounds { pos }) }
    }

    /// Inserts an object at stream index `at` (clamped to the end) and
    /// returns the index it ends up at once screen jumps are regenerated.
    ///
    /// # Errors
    ///
    /// See [`EditError`].
    pub fn insert_object(
        &mut self,
        layer: Layer,
        at: usize,
        spec: ObjectSpec,
    ) -> Result<usize, EditError> {
        if spec.number > 0x3F {
            return Err(EditError::BadObjectNumber(spec.number));
        }
        self.check_bounds(spec.pos)?;
        let vertical = self.vertical;
        let object = RawObject::at(spec.number, spec.third, spec.pos, vertical, self.format)
            .ok_or(EditError::OutOfBounds { pos: spec.pos })?;
        let stream = self.stream_mut(layer)?;
        let at = at.min(stream.objects.len());
        let rank = stream.objects.iter().take(at).filter(|o| !o.is_jump()).count();
        stream.objects.insert(at, object);
        stream.renormalize();
        Ok(stream.index_of_rank(rank).unwrap_or(at))
    }

    /// Removes and returns the object at `index`.
    ///
    /// # Errors
    ///
    /// See [`EditError`].
    pub fn remove_object(&mut self, layer: Layer, index: usize) -> Result<RawObject, EditError> {
        let stream = self.stream_mut(layer)?;
        if index >= stream.objects.len() {
            return Err(EditError::NoSuchObject { layer, index });
        }
        let removed = stream.objects.remove(index);
        stream.renormalize();
        Ok(removed)
    }

    /// Moves the object at `index` to `pos`.
    ///
    /// # Errors
    ///
    /// See [`EditError`].
    pub fn move_object(
        &mut self,
        layer: Layer,
        index: usize,
        pos: TilePos,
    ) -> Result<(), EditError> {
        self.check_bounds(pos)?;
        let vertical = self.vertical;
        let stream = self.stream_mut(layer)?;
        let object =
            stream.objects.get_mut(index).ok_or(EditError::NoSuchObject { layer, index })?;
        if !object.is_positioned() {
            return Err(EditError::NotPositioned { index });
        }
        object.place(pos, vertical).ok_or(EditError::OutOfBounds { pos })?;
        stream.renormalize();
        Ok(())
    }

    /// Replaces an object's bytes wholesale (number, size, in-screen position)
    /// while keeping its stream screen, then regenerates screen bookkeeping.
    ///
    /// # Errors
    ///
    /// See [`EditError`].
    pub fn replace_object(
        &mut self,
        layer: Layer,
        index: usize,
        object: RawObject,
    ) -> Result<(), EditError> {
        let stream = self.stream_mut(layer)?;
        let slot = stream.objects.get_mut(index).ok_or(EditError::NoSuchObject { layer, index })?;
        slot.bytes = object.bytes;
        slot.len = object.len;
        slot.screen = object.screen;
        stream.renormalize();
        Ok(())
    }

    /// Replaces the third byte (size or extended number) of an object.
    ///
    /// # Errors
    ///
    /// See [`EditError`].
    pub fn set_object_third(
        &mut self,
        layer: Layer,
        index: usize,
        third: u8,
    ) -> Result<(), EditError> {
        let stream = self.stream_mut(layer)?;
        let object =
            stream.objects.get_mut(index).ok_or(EditError::NoSuchObject { layer, index })?;
        object.bytes[2] = third;
        stream.renormalize();
        Ok(())
    }

    /// Moves the object at `from` to index `to` (clamped), keeping its position.
    ///
    /// # Errors
    ///
    /// See [`EditError`].
    pub fn reorder_object(
        &mut self,
        layer: Layer,
        from: usize,
        to: usize,
    ) -> Result<(), EditError> {
        let stream = self.stream_mut(layer)?;
        if from >= stream.objects.len() {
            return Err(EditError::NoSuchObject { layer, index: from });
        }
        let object = stream.objects.remove(from);
        let to = to.min(stream.objects.len());
        stream.objects.insert(to, object);
        stream.renormalize();
        Ok(())
    }

    /// Inserts a sprite at index `at` (clamped) and returns its index.
    ///
    /// # Errors
    ///
    /// See [`EditError`].
    pub fn insert_sprite(&mut self, at: usize, spec: SpriteSpec) -> Result<usize, EditError> {
        self.check_bounds(spec.pos)?;
        let sprite = Sprite::at(spec.number, spec.extra_bits, spec.pos, self.vertical)
            .ok_or(EditError::OutOfBounds { pos: spec.pos })?;
        let at = at.min(self.sprites.sprites.len());
        self.sprites.sprites.insert(at, sprite);
        Ok(at)
    }

    /// Removes and returns the sprite at `index`.
    ///
    /// # Errors
    ///
    /// See [`EditError`].
    pub fn remove_sprite(&mut self, index: usize) -> Result<Sprite, EditError> {
        if index >= self.sprites.sprites.len() {
            return Err(EditError::NoSuchSprite { index });
        }
        Ok(self.sprites.sprites.remove(index))
    }

    /// Moves the sprite at `index` to `pos`.
    ///
    /// # Errors
    ///
    /// See [`EditError`].
    pub fn move_sprite(&mut self, index: usize, pos: TilePos) -> Result<(), EditError> {
        self.check_bounds(pos)?;
        let vertical = self.vertical;
        let sprite =
            self.sprites.sprites.get_mut(index).ok_or(EditError::NoSuchSprite { index })?;
        sprite.place(pos, vertical).ok_or(EditError::OutOfBounds { pos })
    }

    /// Replaces a sprite's bytes wholesale (number, extra bits, position).
    ///
    /// # Errors
    ///
    /// See [`EditError`].
    pub fn set_sprite_bytes(&mut self, index: usize, bytes: [u8; 3]) -> Result<(), EditError> {
        let sprite =
            self.sprites.sprites.get_mut(index).ok_or(EditError::NoSuchSprite { index })?;
        sprite.bytes = bytes;
        Ok(())
    }

    /// Sets the number of screens, 1 to [`Level::max_screens`].
    ///
    /// # Errors
    ///
    /// [`EditError::TooManyScreens`].
    pub fn set_screens(&mut self, screens: u8) -> Result<(), EditError> {
        let max = self.max_screens();
        if screens == 0 || screens > max {
            return Err(EditError::TooManyScreens { requested: screens, max });
        }
        Ok(primary::SCREENS_MINUS_ONE.set(&mut self.header.raw, screens.saturating_sub(1))?)
    }

    /// Sets a primary header field.
    ///
    /// # Errors
    ///
    /// [`EditError::Field`] if the value does not fit.
    pub fn set_header_field(&mut self, field: BitField, value: u8) -> Result<(), EditError> {
        Ok(field.set(&mut self.header.raw, value)?)
    }

    /// Sets a secondary header field.
    ///
    /// # Errors
    ///
    /// [`EditError::Field`] if the value does not fit.
    pub fn set_secondary_field(&mut self, field: BitField, value: u8) -> Result<(), EditError> {
        Ok(field.set(&mut self.secondary.raw, value)?)
    }

    /// The layer 1 screen exits, keyed by screen.
    #[must_use]
    pub fn screen_exits(&self) -> BTreeMap<u8, ScreenExitInfo> {
        self.layer1
            .objects
            .iter()
            .filter_map(|o| match o.kind() {
                ObjectKind::ScreenExit { screen, destination, secondary }
                | ObjectKind::ScreenExitExt { screen, destination, secondary, .. } => {
                    Some((screen, ScreenExitInfo { destination, secondary }))
                }
                ObjectKind::Standard { .. }
                | ObjectKind::Extended { .. }
                | ObjectKind::ScreenJump { .. }
                | ObjectKind::DirectMap16 { .. }
                | ObjectKind::DirectMap16Ext { .. } => None,
            })
            .collect()
    }

    /// Sets (or replaces) the exit of `screen`.
    pub fn set_screen_exit(&mut self, screen: u8, info: ScreenExitInfo) {
        let screen = screen & 0x1F;
        let replacement =
            RawObject::screen_exit(screen, info.destination, info.secondary, self.format);
        let existing = self.layer1.objects.iter().position(|o| is_exit_on(o, screen));
        if let Some(slot) = existing.and_then(|index| self.layer1.objects.get_mut(index)) {
            slot.bytes = replacement.bytes;
            slot.set_new_screen(false);
        } else {
            let mut object = replacement;
            object.screen = self.layer1.objects.last().map_or(0, |o| o.screen);
            self.layer1.objects.push(object);
        }
        self.layer1.renormalize();
    }

    /// Removes the exit of `screen`, if any.
    pub fn remove_screen_exit(&mut self, screen: u8) {
        let screen = screen & 0x1F;
        self.layer1.objects.retain(|o| !is_exit_on(o, screen));
        self.layer1.renormalize();
    }
}

/// Whether `object` is a screen exit (either format) for `screen`.
fn is_exit_on(object: &RawObject, screen: u8) -> bool {
    matches!(
        object.kind(),
        ObjectKind::ScreenExit { screen: s, .. } | ObjectKind::ScreenExitExt { screen: s, .. }
            if s == screen
    )
}

#[cfg(test)]
mod tests {
    use super::{EditError, Layer, ObjectSpec, ScreenExitInfo, SpriteSpec, TilePos};
    use crate::LevelId;
    use crate::level::{
        Layer2, Level, LevelAddresses, ObjectFormat, ObjectStream, PrimaryHeader, SecondaryHeader,
        Sprite, SpriteList,
    };
    use snes_rom::SnesAddr;

    fn level(vertical: bool, screens: u8) -> Level {
        let addr = SnesAddr::lit(0x06_8000);
        Level {
            id: LevelId::new(0x105).expect("valid"),
            addresses: LevelAddresses {
                layer1: addr,
                layer2_raw: addr,
                layer2: addr,
                sprites: addr,
            },
            header: PrimaryHeader { raw: [screens.saturating_sub(1), 0, 0, 0, 0] },
            secondary: SecondaryHeader { raw: [0; 4] },
            vertical,
            format: ObjectFormat::Vanilla,
            layer1: ObjectStream::default(),
            layer2: Layer2::Objects { header_copy: [0; 5], stream: ObjectStream::default() },
            sprites: SpriteList::default(),
            lm: None,
        }
    }

    #[test]
    fn insert_move_remove_objects() {
        let mut lv = level(false, 4);
        let spec = |x, y| ObjectSpec { number: 0x01, third: 0x22, pos: TilePos::new(x, y) };
        assert_eq!(lv.insert_object(Layer::One, 0, spec(0x25, 5)), Ok(1), "after a jump to 2");
        assert_eq!(lv.insert_object(Layer::One, 9, spec(0x02, 6)), Ok(3), "after a jump to 0");
        assert_eq!(lv.layer1.objects.len(), 4, "jumps to screen 2 and back to 0 were inserted");
        assert_eq!(lv.layer1.objects[1].position(false), Some(TilePos::new(0x25, 5)));
        lv.move_object(Layer::One, 1, TilePos::new(0x12, 7)).expect("moves");
        assert_eq!(lv.layer1.objects[0].position(false), Some(TilePos::new(0x12, 7)));
        assert_eq!(lv.layer1.objects.len(), 3, "screen 1 needs a flag, not a jump");
        assert_eq!(
            lv.move_object(Layer::One, 0, TilePos::new(0x40, 0)),
            Err(EditError::OutOfBounds { pos: TilePos::new(0x40, 0) })
        );
        assert_eq!(
            lv.insert_object(
                Layer::One,
                0,
                ObjectSpec { number: 0x40, third: 0, pos: TilePos::new(0, 0) }
            ),
            Err(EditError::BadObjectNumber(0x40))
        );
        lv.remove_object(Layer::One, 0).expect("removes");
        assert_eq!(lv.layer1.objects.len(), 1);
        assert_eq!(
            lv.remove_object(Layer::One, 5),
            Err(EditError::NoSuchObject { layer: Layer::One, index: 5 })
        );
        let bytes = lv.layer1_bytes();
        assert_eq!(
            ObjectStream::parse(&bytes[5..], ObjectFormat::Vanilla).expect("terminated"),
            lv.layer1
        );

        lv.layer2 = Layer2::background_8bit(0, &[0; 0x400], vec![0xFF, 0xFF]);
        assert_eq!(lv.insert_object(Layer::Two, 0, spec(0, 0)), Err(EditError::Layer2IsBackground));
        assert_eq!(lv.max_screens(), 32);
    }

    #[test]
    fn sprites_in_both_orientations() {
        let mut lv = level(true, 3);
        assert_eq!(lv.max_screens(), 14);
        let pos = TilePos::new(0x13, 0x25);
        assert_eq!(lv.insert_sprite(0, SpriteSpec { number: 0x0F, extra_bits: 2, pos }), Ok(0));
        let s = lv.sprites.sprites[0];
        assert_eq!(s.position(true), pos);
        assert_eq!(s.extra_bits(), 2);
        assert_eq!(s.screen(), 2);
        assert_eq!(s.number(), 0x0F);
        lv.move_sprite(0, TilePos::new(1, 0x2F)).expect("moves");
        assert_eq!(lv.sprites.sprites[0].position(true), TilePos::new(1, 0x2F));
        assert!(lv.move_sprite(0, TilePos::new(1, 0x30)).is_err(), "3 screens = 48 rows");

        let h = Sprite::at(0x20, 1, TilePos::new(0x1A7, 0x1F), false).expect("fits");
        assert_eq!(h.position(false), TilePos::new(0x1A7, 0x1F));
        assert_eq!(h.screen(), 0x1A);
        assert_eq!(h.x(), 7);
        assert_eq!(h.y(), 0x1F);
        assert_eq!(h.extra_bits(), 1);
    }

    #[test]
    fn screens_fields_and_exits() {
        let mut lv = level(false, 4);
        assert_eq!(lv.set_screens(16), Ok(()));
        assert_eq!(lv.header.screens(), 16);
        assert_eq!(lv.set_screens(17), Err(EditError::TooManyScreens { requested: 17, max: 16 }));
        assert!(lv.set_header_field(crate::level::fields::primary::MUSIC, 9).is_err());
        lv.set_header_field(crate::level::fields::primary::MUSIC, 5).expect("fits");
        assert_eq!(lv.header.music(), 5);
        lv.set_secondary_field(crate::level::fields::secondary::MIDWAY_SCREEN, 7).expect("fits");
        assert_eq!(lv.secondary.midway_screen(), 7);

        lv.insert_object(
            Layer::One,
            0,
            ObjectSpec { number: 1, third: 0, pos: TilePos::new(0x35, 1) },
        )
        .expect("fits");
        let info = ScreenExitInfo { destination: 0x1C7, secondary: false };
        lv.set_screen_exit(2, info);
        assert_eq!(lv.screen_exits().get(&2), Some(&info));
        let updated = ScreenExitInfo { destination: 0x12, secondary: true };
        lv.set_screen_exit(2, updated);
        assert_eq!(lv.screen_exits().len(), 1);
        assert_eq!(lv.screen_exits().get(&2), Some(&updated));
        let reparsed = ObjectStream::parse(&lv.layer1_bytes()[5..], ObjectFormat::Vanilla)
            .expect("terminated");
        assert_eq!(reparsed, lv.layer1);
        lv.remove_screen_exit(2);
        assert!(lv.screen_exits().is_empty());
    }
}
