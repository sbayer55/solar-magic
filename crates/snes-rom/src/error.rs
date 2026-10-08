//! Error type for ROM loading, access and writing.

use std::io;
use std::path::PathBuf;

use crate::SnesAddr;

/// Errors produced while loading, reading or writing a ROM image.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RomError {
    /// The file could not be read or written.
    #[error("failed to access ROM file {path}: {source}")]
    Io {
        /// Path that failed.
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

    /// A write ran past the end of the image.
    #[error("write of {len} bytes at file offset {pc:#X} exceeds ROM size {rom_len}")]
    WriteOutOfRange {
        /// File offset of the attempted write.
        pc: usize,
        /// Length of the attempted write.
        len: usize,
        /// Size of the image.
        rom_len: usize,
    },

    /// An expansion target was not a power of two.
    #[error("ROM size {len} is not a power of two")]
    SizeNotPowerOfTwo {
        /// Requested size.
        len: usize,
    },

    /// An expansion target was smaller than the current image.
    #[error("cannot shrink ROM from {from} to {to} bytes")]
    Shrink {
        /// Current size.
        from: usize,
        /// Requested size.
        to: usize,
    },

    /// An expansion target exceeds what the active mapping can address.
    #[error("{len} bytes exceeds the {mapping} limit of {max} bytes")]
    TooLargeForMapping {
        /// Requested size.
        len: usize,
        /// Largest size the mapping supports.
        max: usize,
        /// Name of the mapping.
        mapping: &'static str,
    },
}
