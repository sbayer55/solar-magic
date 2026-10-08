//! LC_LZ2 (de)compression.
//!
//! LC_LZ2 is Lunar Compress's name for the byte-oriented scheme Super Mario
//! World uses for its GFX files. A stream is a sequence of chunks, each
//! introduced by a header byte `CCCLLLLL` (command, length - 1), terminated by
//! a header byte of `0xFF`. See `docs/formats/lc-lz2.md` for the table of
//! commands.

mod compress;
mod decompress;

pub use compress::compress;
pub use decompress::{Decompressed, MAX_OUTPUT, decompress, decompress_with_limit};

/// Errors from decoding an LC_LZ2 stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The input ended before the terminator (or mid-chunk).
    #[error("LC_LZ2 stream ended unexpectedly at input offset {offset:#X}")]
    UnexpectedEnd {
        /// Input offset where more bytes were needed.
        offset: usize,
    },

    /// A header used one of the two unassigned command codes.
    #[error("invalid LC_LZ2 command {command} at input offset {offset:#X}")]
    InvalidCommand {
        /// Input offset of the header byte.
        offset: usize,
        /// The 3-bit command code.
        command: u8,
    },

    /// A repeat command referenced output that has not been produced yet.
    #[error(
        "LC_LZ2 repeat at input offset {offset:#X} reads output offset {from_offset:#X} but only \
         {produced:#X} bytes exist"
    )]
    RepeatOutOfRange {
        /// Input offset of the header byte.
        offset: usize,
        /// Output offset the command tried to copy from.
        from_offset: usize,
        /// Output bytes produced so far.
        produced: usize,
    },

    /// Decompressed output exceeded the caller's limit.
    #[error("LC_LZ2 output exceeded the limit of {limit} bytes")]
    OutputTooLarge {
        /// The limit that was exceeded.
        limit: usize,
    },
}
