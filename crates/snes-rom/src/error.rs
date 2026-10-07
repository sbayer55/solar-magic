//! Error type for ROM loading and access.

use std::io;
use std::path::PathBuf;

use crate::SnesAddr;

/// Errors produced while loading or reading a ROM image.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RomError {
    /// The file could not be read.
    #[error("failed to read ROM file {path}: {source}")]
    Io {
        /// Path that failed to load.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// The image is smaller than the minimum size that can hold a header.
    #[error("ROM is too small ({len} bytes); need at least 32 KiB")]
    TooSmall {
        /// Size of the image after stripping any copier header.
        len: usize,
    },

    /// Neither header location held a plausible SNES internal header.
    #[error("no valid SNES internal header found at $7FC0 (LoROM) or $FFC0 (HiROM)")]
    NoHeader,

    /// The header describes a mapping this crate does not implement yet.
    #[error("unsupported ROM mapping: {0}")]
    UnsupportedMapping(&'static str),

    /// The address does not correspond to any ROM byte under the active mapper.
    #[error("address {0} does not map to ROM")]
    Unmapped(SnesAddr),

    /// A read ran past the end of the image.
    #[error("read of {len} bytes at {addr} exceeds ROM size {rom_len}")]
    OutOfRange {
        /// Start of the attempted read.
        addr: SnesAddr,
        /// Length of the attempted read.
        len: usize,
        /// Size of the image.
        rom_len: usize,
    },
}
