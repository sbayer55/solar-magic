//! Level numbers.

use core::fmt;

/// A level number, `0x000` to `0x1FF`.
///
/// SMW has 512 level slots. Levels `0x000-0x024` and `0x101-0x13B` are the
/// "main" slots entered from the overworld; `0x025-0x100` and `0x13C-0x1FF`
/// are sublevels reached through exits.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "u16", into = "u16")]
pub struct LevelId(u16);

impl LevelId {
    /// Number of level slots.
    pub const COUNT: usize = 0x200;

    /// The highest valid level number.
    pub const MAX: Self = Self(0x1FF);

    /// Creates a level id, returning `None` if `n >= 0x200`.
    #[must_use]
    pub const fn new(n: u16) -> Option<Self> {
        if n < 0x200 { Some(Self(n)) } else { None }
    }

    /// The raw level number.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// The level number as a table index.
    #[must_use]
    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    /// Iterates over every level id in order.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..0x200u16).map(Self)
    }

    /// Whether this slot is entered from the overworld (`0x000-0x024` or
    /// `0x101-0x13B`) rather than only through exits.
    #[must_use]
    pub const fn is_overworld(self) -> bool {
        matches!(self.0, 0x000..=0x024 | 0x101..=0x13B)
    }

    /// The translevel of slot `0x101`: the overworld subtracts `0x24` from
    /// translevels `>= 0x25` and sets the high byte of the level number
    /// (`CODE_05D8A2`), so slot `0x101` is translevel `0x25`.
    const SECOND_BLOCK_TRANSLEVEL: u16 = 0x25;

    /// The overworld's number for this slot (`$13BF`): the slot number
    /// below `0x25`, `slot - 0xDC` for `0x101-0x13B`, `None` for sublevels.
    #[must_use]
    pub fn translevel(self) -> Option<u8> {
        if !self.is_overworld() {
            return None;
        }
        let n = if self.0 >= 0x100 {
            self.0.checked_sub(0x101)?.checked_add(Self::SECOND_BLOCK_TRANSLEVEL)?
        } else {
            self.0
        };
        u8::try_from(n).ok()
    }

    /// The slot a translevel enters, the inverse of [`LevelId::translevel`].
    #[must_use]
    pub fn from_translevel(translevel: u8) -> Option<Self> {
        let n = u16::from(translevel);
        if n < Self::SECOND_BLOCK_TRANSLEVEL {
            Some(Self(n))
        } else {
            Self::new(n.checked_sub(Self::SECOND_BLOCK_TRANSLEVEL)?.checked_add(0x101)?)
                .filter(|id| id.is_overworld())
        }
    }
}

impl fmt::Debug for LevelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LevelId({:03X})", self.0)
    }
}

impl fmt::Display for LevelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:03X}", self.0)
    }
}

impl From<LevelId> for u16 {
    fn from(id: LevelId) -> Self {
        id.0
    }
}

impl TryFrom<u16> for LevelId {
    type Error = u16;

    fn try_from(n: u16) -> Result<Self, Self::Error> {
        Self::new(n).ok_or(n)
    }
}

#[cfg(test)]
mod tests {
    use super::LevelId;

    #[test]
    fn bounds() {
        assert_eq!(LevelId::new(0).map(LevelId::get), Some(0));
        assert_eq!(LevelId::new(0x1FF), Some(LevelId::MAX));
        assert_eq!(LevelId::new(0x200), None);
        assert_eq!(LevelId::all().count(), LevelId::COUNT);
        assert_eq!(LevelId::new(0x105).expect("valid").to_string(), "105");
    }

    #[test]
    fn overworld_slots() {
        let overworld = LevelId::all().filter(|id| id.is_overworld()).count();
        assert_eq!(overworld, 0x25 + 0x3B);
        assert!(LevelId::new(0x024).unwrap().is_overworld());
        assert!(!LevelId::new(0x025).unwrap().is_overworld());
        assert!(!LevelId::new(0x100).unwrap().is_overworld());
        assert!(LevelId::new(0x101).unwrap().is_overworld());
        assert!(LevelId::new(0x13B).unwrap().is_overworld());
        assert!(!LevelId::new(0x13C).unwrap().is_overworld());
    }

    #[test]
    fn translevels() {
        let id = |n| LevelId::new(n).unwrap();
        assert_eq!(id(0x000).translevel(), Some(0x00));
        assert_eq!(id(0x024).translevel(), Some(0x24));
        assert_eq!(id(0x025).translevel(), None);
        assert_eq!(id(0x100).translevel(), None);
        assert_eq!(id(0x101).translevel(), Some(0x25));
        assert_eq!(id(0x105).translevel(), Some(0x29));
        assert_eq!(id(0x13B).translevel(), Some(0x5F));
        assert_eq!(id(0x13C).translevel(), None);
        for slot in LevelId::all().filter(|slot| slot.is_overworld()) {
            let t = slot.translevel().unwrap();
            assert_eq!(LevelId::from_translevel(t), Some(slot));
        }
        assert_eq!(LevelId::from_translevel(0x60), None);
    }
}
