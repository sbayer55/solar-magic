//! Screen bookkeeping for object streams: new-screen flags, screen jumps,
//! and the position encoding that depends on the level orientation.
//! See `docs/formats/level-objects.md`.

use super::objects::{MAX_OBJECT_LEN, ObjectFormat, ObjectKind, ObjectStream, RawObject};

/// Tiles per screen along the scrolling axis.
pub const SCREEN_TILES: u16 = 16;

/// An absolute tile position within a level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TilePos {
    /// Column in 16 px tiles.
    pub x: u16,
    /// Row in 16 px tiles.
    pub y: u16,
}

impl TilePos {
    /// Creates a position.
    #[must_use]
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

/// Splits an absolute position into screen number, X within the screen and
/// Y within the screen for the given orientation. Returns `None` when a
/// field does not fit its bits.
#[must_use]
pub fn split_position(pos: TilePos, vertical: bool) -> Option<(u8, u8, u8)> {
    if vertical {
        let screen = u8::try_from(pos.y / SCREEN_TILES).ok().filter(|&s| s < 32)?;
        let x = u8::try_from(pos.x).ok().filter(|&x| x < 32)?;
        let y = u8::try_from(pos.y % SCREEN_TILES).ok()?;
        Some((screen, x, y))
    } else {
        let screen = u8::try_from(pos.x / SCREEN_TILES).ok().filter(|&s| s < 32)?;
        let x = u8::try_from(pos.x % SCREEN_TILES).ok()?;
        let y = u8::try_from(pos.y).ok().filter(|&y| y < 32)?;
        Some((screen, x, y))
    }
}

/// Joins screen number and in-screen coordinates back into a position.
#[must_use]
pub fn join_position(screen: u8, x: u8, y: u8, vertical: bool) -> TilePos {
    let screen = u16::from(screen).saturating_mul(SCREEN_TILES);
    if vertical {
        TilePos { x: u16::from(x), y: screen.saturating_add(u16::from(y)) }
    } else {
        TilePos { x: screen.saturating_add(u16::from(x)), y: u16::from(y) }
    }
}

impl RawObject {
    /// Whether the object has a position: everything except screen exits and
    /// screen jumps (the loader skips its vertical nibble swap for those).
    #[must_use]
    pub fn is_positioned(&self) -> bool {
        !matches!(
            self.kind(),
            ObjectKind::ScreenExit { .. }
                | ObjectKind::ScreenJump { .. }
                | ObjectKind::ScreenExitExt { .. }
        )
    }

    /// An object built from its leading bytes in `format`.
    fn from_bytes(leading: &[u8], format: ObjectFormat) -> Self {
        let mut bytes = [0u8; MAX_OBJECT_LEN];
        let len = leading.len().min(MAX_OBJECT_LEN);
        if let (Some(dst), Some(src)) = (bytes.get_mut(..len), leading.get(..len)) {
            dst.copy_from_slice(src);
        }
        Self { bytes, len: u8::try_from(len).unwrap_or(3), screen: 0, offset: 0, format }
    }

    /// Whether this is a screen jump (extended object 1).
    #[must_use]
    pub const fn is_jump(&self) -> bool {
        self.number() == 0 && self.third() == 1
    }

    /// A standard or extended 3-byte object with the given number, third
    /// byte and position. Returns `None` if the position does not fit.
    #[must_use]
    pub fn at(
        number: u8,
        third: u8,
        pos: TilePos,
        vertical: bool,
        format: ObjectFormat,
    ) -> Option<Self> {
        let mut object =
            Self::from_bytes(&[(number & 0x30) << 1, (number & 0x0F) << 4, third], format);
        object.place(pos, vertical)?;
        Some(object)
    }

    /// A (vanilla, 4-byte) screen exit on `screen` to `destination` (or
    /// secondary exit `destination` when `secondary`).
    #[must_use]
    pub fn screen_exit(
        screen: u8,
        destination: u16,
        secondary: bool,
        format: ObjectFormat,
    ) -> Self {
        let [low, high] = destination.to_le_bytes();
        let flags = (high & 0x01) | if secondary { 0x02 } else { 0 };
        Self::from_bytes(&[screen & 0x1F, flags, 0x00, low], format)
    }

    /// A screen jump to `screen`.
    #[must_use]
    pub fn screen_jump(screen: u8, format: ObjectFormat) -> Self {
        Self::from_bytes(&[screen & 0x1F, 0x00, 0x01], format)
    }

    /// Sets the new-screen flag.
    pub const fn set_new_screen(&mut self, flag: bool) {
        self.bytes[0] = (self.bytes[0] & 0x7F) | if flag { 0x80 } else { 0 };
    }

    /// The object's absolute position, or `None` for exits and jumps.
    #[must_use]
    pub fn position(&self, vertical: bool) -> Option<TilePos> {
        if !self.is_positioned() {
            return None;
        }
        let swap = vertical;
        let (x, y) = if swap {
            (self.bytes[0] & 0x1F, self.bytes[1] & 0x0F)
        } else {
            (self.bytes[1] & 0x0F, self.bytes[0] & 0x1F)
        };
        Some(join_position(self.screen, x, y, vertical))
    }

    /// Moves the object to `pos`, updating its screen and in-screen bits.
    /// Returns `None` (leaving the object unchanged) if `pos` does not fit or
    /// the object has no position.
    pub fn place(&mut self, pos: TilePos, vertical: bool) -> Option<()> {
        if !self.is_positioned() {
            return None;
        }
        let (screen, x, y) = split_position(pos, vertical)?;
        let (b0_low, b1_low) = if vertical { (x, y) } else { (y, x) };
        self.bytes[0] = (self.bytes[0] & 0xE0) | (b0_low & 0x1F);
        self.bytes[1] = (self.bytes[1] & 0xF0) | (b1_low & 0x0F);
        self.screen = screen;
        Some(())
    }
}

impl ObjectStream {
    /// Rewrites new-screen flags and screen jumps so that every object lands
    /// on the screen recorded in its `screen` field, then recomputes byte
    /// offsets. Existing jumps are discarded and re-created only where a
    /// flag cannot express the change.
    pub fn renormalize(&mut self) {
        let format = self.objects.first().map_or(ObjectFormat::Vanilla, |o| o.format);
        let mut out = Vec::with_capacity(self.objects.len());
        let mut current = 0u8;
        for mut object in self.objects.drain(..).filter(|o| !o.is_jump()) {
            let target = object.screen & 0x1F;
            if target == current.wrapping_add(1) {
                object.set_new_screen(true);
            } else {
                if target != current {
                    let mut jump = RawObject::screen_jump(target, format);
                    jump.screen = current;
                    out.push(jump);
                }
                object.set_new_screen(false);
            }
            current = target;
            out.push(object);
        }
        self.objects = out;
        self.recompute_offsets();
    }

    /// Ordinal of the object at stream `index` among the non-jump objects.
    #[must_use]
    pub fn rank_of(&self, index: usize) -> Option<usize> {
        let object = self.objects.get(index)?;
        if object.is_jump() {
            return None;
        }
        Some(self.objects.iter().take(index).filter(|o| !o.is_jump()).count())
    }

    /// Stream index of the `rank`-th object that is not a screen jump.
    #[must_use]
    pub fn index_of_rank(&self, rank: usize) -> Option<usize> {
        self.objects.iter().enumerate().filter(|(_, o)| !o.is_jump()).nth(rank).map(|(i, _)| i)
    }

    /// Recomputes every object's byte offset from the stream order.
    pub fn recompute_offsets(&mut self) {
        let mut offset = 0usize;
        for object in &mut self.objects {
            object.offset = offset;
            offset = offset.saturating_add(usize::from(object.len));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ObjectStream, RawObject, TilePos, join_position, split_position};
    use crate::level::{ObjectFormat, ObjectKind};

    #[test]
    fn positions_split_and_join() {
        assert_eq!(split_position(TilePos::new(0x25, 0x1A), false), Some((2, 5, 0x1A)));
        assert_eq!(split_position(TilePos::new(0x1F, 0x3B), true), Some((3, 0x1F, 0x0B)));
        assert_eq!(split_position(TilePos::new(0x200, 0), false), None);
        assert_eq!(split_position(TilePos::new(0, 32), false), None);
        assert_eq!(split_position(TilePos::new(32, 0), true), None);
        assert_eq!(join_position(2, 5, 0x1A, false), TilePos::new(0x25, 0x1A));
        assert_eq!(join_position(3, 0x1F, 0x0B, true), TilePos::new(0x1F, 0x3B));
    }

    #[test]
    fn object_round_trips_position_in_both_orientations() {
        let pos = TilePos::new(0x25, 0x1A);
        let o = RawObject::at(0x3A, 0x22, pos, false, ObjectFormat::Vanilla).expect("fits");
        assert_eq!(o.kind(), ObjectKind::Standard { number: 0x3A, size: 0x22 });
        assert_eq!(o.position(false), Some(pos));
        assert_eq!(o.screen, 2);
        assert_eq!(&o.bytes[..3], &[0x60 | 0x1A, 0xA5, 0x22]);

        let vpos = TilePos::new(0x1F, 0x3B);
        let v = RawObject::at(0x01, 0x11, vpos, true, ObjectFormat::Vanilla).expect("fits");
        assert_eq!(v.position(true), Some(vpos));
        assert_eq!(v.bytes[0] & 0x1F, 0x1F, "vertical: byte 0 holds x");
        assert_eq!(v.bytes[1] & 0x0F, 0x0B, "vertical: byte 1 holds y");
        assert_eq!(v.screen, 3);

        let exit = RawObject::screen_exit(7, 0x105, true, ObjectFormat::Vanilla);
        assert_eq!(
            exit.kind(),
            ObjectKind::ScreenExit { screen: 7, destination: 0x105, secondary: true }
        );
        assert_eq!(exit.position(false), None);
        assert_eq!(
            RawObject::screen_jump(9, ObjectFormat::Vanilla).kind(),
            ObjectKind::ScreenJump { screen: 9 }
        );
        assert!(
            RawObject::at(0x01, 0x11, TilePos::new(0x200, 0), false, ObjectFormat::Vanilla)
                .is_none()
        );
    }

    #[test]
    fn renormalize_rewrites_flags_and_jumps() {
        let mut stream = ObjectStream::default();
        let mut push = |screen: u8| {
            let mut o = RawObject::at(0x01, 0x11, TilePos::new(3, 4), false, ObjectFormat::Vanilla)
                .expect("fits");
            o.screen = screen;
            stream.objects.push(o);
        };
        push(0);
        push(1);
        push(1);
        push(5);
        push(3);
        stream.renormalize();
        let kinds: Vec<(bool, Option<u8>)> = stream
            .objects
            .iter()
            .map(|o| {
                let jump = match o.kind() {
                    ObjectKind::ScreenJump { screen } => Some(screen),
                    ObjectKind::Standard { .. }
                    | ObjectKind::Extended { .. }
                    | ObjectKind::ScreenExit { .. }
                    | ObjectKind::ScreenExitExt { .. }
                    | ObjectKind::DirectMap16 { .. }
                    | ObjectKind::DirectMap16Ext { .. } => None,
                };
                (o.new_screen(), jump)
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                (false, None),
                (true, None),
                (false, None),
                (false, Some(5)),
                (false, None),
                (false, Some(3)),
                (false, None),
            ]
        );
        // Re-parsing the bytes yields the same screens.
        let parsed =
            ObjectStream::parse(&stream.to_bytes(), ObjectFormat::Vanilla).expect("terminated");
        let screens: Vec<u8> = parsed
            .objects
            .iter()
            .filter(|o| !matches!(o.kind(), ObjectKind::ScreenJump { .. }))
            .map(|o| o.screen)
            .collect();
        assert_eq!(screens, vec![0, 1, 1, 5, 3]);
        assert_eq!(parsed, stream, "offsets and screens were recomputed");
    }
}
