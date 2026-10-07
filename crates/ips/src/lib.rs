//! IPS patch files.
//!
//! IPS is the classic ROM-patch format: a `PATCH` magic, a list of records
//! that each write some bytes at a 24-bit file offset, and an `EOF` marker.
//! Lunar IPS adds an optional 3-byte truncation length after `EOF`. This
//! crate parses and serializes patches byte for byte, applies them, and
//! creates them from an original/modified pair with the same RLE and
//! truncation support as Lunar IPS. See `docs/formats/ips.md`.
//!
//! The crate knows nothing about SNES ROMs or copier headers; it works on
//! plain byte slices.

mod apply;
mod create;
mod diff;
mod error;
mod parse;
mod patch;
mod write;

pub use apply::{Applied, ApplyReport, LogEntry, apply};
pub use create::create;
pub use error::Error;
pub use patch::{Patch, Payload, Record, Summary};

/// Every patch starts with these bytes.
pub const MAGIC: &[u8; 5] = b"PATCH";

/// The record list ends with these bytes.
pub const EOF_MARKER: &[u8; 3] = b"EOF";

/// `0x454F46`: a record starting at this offset is indistinguishable from the
/// [`EOF_MARKER`], so encoders must never start one there.
pub const EOF_OFFSET: u32 = 0x45_4F46;

/// Largest offset a record can carry (24 bits).
pub const MAX_OFFSET: u32 = 0xFF_FFFF;

/// Files are limited to 16 MiB because offsets are 24-bit (Lunar IPS's limit).
pub const MAX_FILE_LEN: usize = 0x100_0000;

/// Longest data run a single record can carry (16-bit length).
pub const MAX_RECORD_LEN: usize = 0xFFFF;

/// Encoded size of a raw record's header: offset (3) + size (2).
pub const RAW_HEADER_LEN: usize = 5;

/// Encoded size of an RLE record: offset (3) + zero size (2) + run (2) + value (1).
pub const RLE_RECORD_LEN: usize = 8;

#[cfg(test)]
pub(crate) mod test_util;
