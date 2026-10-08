//! The optional 512-byte copier header some ROM dumps carry.
//!
//! Old copier devices prepended a 512-byte header to their dumps. It is not
//! part of the ROM and [`crate::Rom`] strips it on load, but IPS patches
//! address file bytes, so a patch made from a headered file only lines up
//! with another headered file. These helpers let tools detect, add and strip
//! the header on raw file bytes. See `docs/formats/rom-mapping.md`.

/// Length of the header in bytes.
pub const LEN: usize = 512;

/// Whether a file of `len` bytes carries a copier header.
///
/// ROM images are multiples of 1 KiB, so a remainder of exactly 512 bytes
/// is the header.
#[must_use]
pub const fn detect(len: usize) -> bool {
    len % 1024 == LEN
}

/// What to do with a file's copier header before patching it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeaderMode {
    /// Leave the bytes alone.
    #[default]
    Keep,
    /// Prepend a zeroed header if the file has none.
    Add,
    /// Remove the header if the file has one.
    Strip,
}

impl HeaderMode {
    /// Every mode, for menus.
    pub const ALL: [Self; 3] = [Self::Keep, Self::Add, Self::Strip];

    /// Human-readable name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Keep => "Keep as is",
            Self::Add => "Add 512-byte header",
            Self::Strip => "Strip 512-byte header",
        }
    }
}

/// Removes the header if [`detect`] says there is one.
#[must_use]
pub fn strip(mut bytes: Vec<u8>) -> Vec<u8> {
    if detect(bytes.len()) {
        bytes.drain(..LEN);
    }
    bytes
}

/// Prepends a zeroed header unless [`detect`] says there already is one.
#[must_use]
pub fn add(bytes: Vec<u8>) -> Vec<u8> {
    if detect(bytes.len()) {
        return bytes;
    }
    let mut out = vec![0; LEN];
    out.extend_from_slice(&bytes);
    out
}

/// Applies `mode` to `bytes`.
#[must_use]
pub fn apply_mode(bytes: Vec<u8>, mode: HeaderMode) -> Vec<u8> {
    match mode {
        HeaderMode::Keep => bytes,
        HeaderMode::Add => add(bytes),
        HeaderMode::Strip => strip(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::{HeaderMode, LEN, add, apply_mode, detect, strip};

    #[test]
    fn detects_by_length() {
        assert!(detect(512));
        assert!(detect(0x8_0200));
        assert!(!detect(0x8_0000));
        assert!(!detect(0));
        assert!(!detect(100));
    }

    #[test]
    fn add_and_strip_are_idempotent() {
        let bare = vec![7u8; 2048];
        let headered = add(bare.clone());
        assert_eq!(headered.len(), 2048 + LEN);
        assert!(headered[..LEN].iter().all(|&b| b == 0));
        assert_eq!(&headered[LEN..], &bare[..]);
        assert_eq!(add(headered.clone()), headered);
        assert_eq!(strip(headered), bare);
        assert_eq!(strip(bare.clone()), bare);
    }

    #[test]
    fn modes() {
        let bare = vec![1u8; 1024];
        assert_eq!(apply_mode(bare.clone(), HeaderMode::Keep), bare);
        assert_eq!(apply_mode(bare.clone(), HeaderMode::Add).len(), 1024 + LEN);
        assert_eq!(apply_mode(add(bare.clone()), HeaderMode::Strip), bare);
        assert_eq!(HeaderMode::default(), HeaderMode::Keep);
        assert_eq!(HeaderMode::ALL.len(), 3);
    }
}
