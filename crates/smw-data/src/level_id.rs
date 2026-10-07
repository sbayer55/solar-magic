//! Level numbers.

use core::fmt;

/// A level number, `0x000` to `0x1FF`.
///
/// SMW has 512 level slots. Levels `0x000-0x024` and `0x101-0x13B` are the
/// "main" slots entered from the overworld; `0x025-0x100` and `0x13C-0x1FF`
/// are sublevels reached through exits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
}
