//! Error type.

use crate::EOF_OFFSET;

/// Errors from parsing, serializing, applying or creating a patch.
///
/// I/O errors are left to the caller; every variant here is about bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The file does not start with `PATCH`.
    #[error("not an IPS patch: file does not start with \"PATCH\"")]
    BadMagic,

    /// The patch ended in the middle of a record (or before `EOF`).
    #[error("IPS patch ended unexpectedly at offset {offset:#X} while reading {expected}")]
    UnexpectedEnd {
        /// Patch offset where more bytes were needed.
        offset: usize,
        /// What was being read.
        expected: &'static str,
    },

    /// Bytes after `EOF` that are not a 3-byte truncation length.
    #[error(
        "IPS patch has {len} unexpected bytes after EOF at offset {offset:#X} (a truncation \
         extension is exactly 3)"
    )]
    TrailingData {
        /// Patch offset of the first unexpected byte.
        offset: usize,
        /// How many unexpected bytes follow.
        len: usize,
    },

    /// A record offset does not fit in 24 bits.
    #[error("record {index} offset {offset:#X} does not fit in 24 bits")]
    OffsetTooLarge {
        /// Index of the record in the patch.
        index: usize,
        /// The offending offset.
        offset: usize,
    },

    /// A record starts at [`EOF_OFFSET`], which readers mistake for `EOF`.
    #[error("record {index} starts at {EOF_OFFSET:#X}, which readers mistake for the EOF marker")]
    EofOffsetCollision {
        /// Index of the record in the patch.
        index: usize,
    },

    /// A raw record is empty or longer than 65535 bytes.
    #[error("record {index} is {len} bytes long; IPS records hold 1 to 65535")]
    BadRecordLen {
        /// Index of the record in the patch.
        index: usize,
        /// The offending length.
        len: usize,
    },

    /// Applying a record would write past the 16 MiB limit.
    #[error("record {index} would write up to {end:#X}, past the 16 MiB IPS limit")]
    OutputTooLarge {
        /// Index of the record in the patch.
        index: usize,
        /// End offset the record would have written to.
        end: usize,
    },

    /// An input file is larger than IPS can address.
    #[error("{what} is {len} bytes; IPS files are limited to 16 MiB")]
    FileTooLarge {
        /// Which input: `"source"`, `"original"` or `"modified"`.
        what: &'static str,
        /// Its length.
        len: usize,
    },

    /// The truncation length does not fit in 24 bits.
    #[error("truncation length {len:#X} does not fit in 24 bits")]
    TruncateTooLarge {
        /// The offending length.
        len: usize,
    },
}
