//! In-memory patch model.

use core::fmt;

use crate::{EOF_OFFSET, Error, MAX_OFFSET, MAX_RECORD_LEN, RAW_HEADER_LEN, RLE_RECORD_LEN};

/// What a record writes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    variant_size_differences,
    reason = "RLE records are deliberately tiny; raw records own their bytes"
)]
pub enum Payload {
    /// Literal bytes (1 to 65535 of them).
    Raw(Vec<u8>),
    /// `len` copies of `value`. A length of 0 is tolerated on read and
    /// writes nothing.
    Rle {
        /// Run length.
        len: u16,
        /// Byte to repeat.
        value: u8,
    },
}

/// One write: `payload` goes to file offset `offset`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// 24-bit file offset of the first byte written.
    pub offset: u32,
    /// The bytes.
    pub payload: Payload,
}

impl Record {
    /// Number of bytes this record writes to the target.
    pub fn len(&self) -> usize {
        match &self.payload {
            Payload::Raw(data) => data.len(),
            Payload::Rle { len, .. } => usize::from(*len),
        }
    }

    /// Whether the record writes nothing (only a zero-length RLE run can).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether the payload is run-length encoded.
    pub const fn is_rle(&self) -> bool {
        matches!(self.payload, Payload::Rle { .. })
    }

    /// One past the last target offset written (saturating).
    pub fn end(&self) -> usize {
        usize::try_from(self.offset).map_or(usize::MAX, |start| start.saturating_add(self.len()))
    }

    /// Number of bytes this record occupies in the patch file.
    pub const fn encoded_len(&self) -> usize {
        match &self.payload {
            Payload::Raw(data) => RAW_HEADER_LEN.saturating_add(data.len()),
            Payload::Rle { .. } => RLE_RECORD_LEN,
        }
    }

    /// Checks that the record can be written to a patch file.
    pub(crate) fn validate(&self, index: usize) -> Result<(), Error> {
        if self.offset > MAX_OFFSET {
            let offset = usize::try_from(self.offset).unwrap_or(usize::MAX);
            return Err(Error::OffsetTooLarge { index, offset });
        }
        if self.offset == EOF_OFFSET {
            return Err(Error::EofOffsetCollision { index });
        }
        if let Payload::Raw(data) = &self.payload
            && (data.is_empty() || data.len() > MAX_RECORD_LEN)
        {
            return Err(Error::BadRecordLen { index, len: data.len() });
        }
        Ok(())
    }
}

impl fmt::Display for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#08X}  len {:#06X}  ", self.offset, self.len())?;
        match &self.payload {
            Payload::Raw(_) => f.write_str("raw"),
            Payload::Rle { value, .. } => write!(f, "rle {value:#04X}"),
        }
    }
}

/// A parsed or constructed patch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Patch {
    /// Records, applied in order.
    pub records: Vec<Record>,
    /// Lunar IPS truncation extension: the output is cut to this length
    /// after all records are applied (if it is longer).
    pub truncate: Option<u32>,
}

impl Patch {
    /// Parses a patch file.
    ///
    /// # Errors
    ///
    /// [`Error::BadMagic`], [`Error::UnexpectedEnd`] or [`Error::TrailingData`].
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        crate::parse::parse(bytes)
    }

    /// Serializes the patch. `parse` of the result gives back an equal patch.
    ///
    /// # Errors
    ///
    /// See [`Patch::validate`].
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        crate::write::to_bytes(self)
    }

    /// Checks that every record and the truncation length can be encoded.
    ///
    /// # Errors
    ///
    /// [`Error::OffsetTooLarge`], [`Error::EofOffsetCollision`],
    /// [`Error::BadRecordLen`] or [`Error::TruncateTooLarge`].
    pub fn validate(&self) -> Result<(), Error> {
        for (index, record) in self.records.iter().enumerate() {
            record.validate(index)?;
        }
        if let Some(len) = self.truncate
            && len > MAX_OFFSET
        {
            return Err(Error::TruncateTooLarge {
                len: usize::try_from(len).unwrap_or(usize::MAX),
            });
        }
        Ok(())
    }

    /// Counts and sizes, for logs and status lines.
    pub fn summary(&self) -> Summary {
        let mut summary = Summary { truncate: self.truncate, ..Summary::default() };
        for record in &self.records {
            summary.records = summary.records.saturating_add(1);
            if record.is_rle() {
                summary.rle_records = summary.rle_records.saturating_add(1);
            } else {
                summary.raw_records = summary.raw_records.saturating_add(1);
            }
            summary.bytes_written = summary.bytes_written.saturating_add(record.len());
            summary.min_output_len = summary.min_output_len.max(record.end());
        }
        summary
    }

    /// Encoded size of the patch file.
    pub fn encoded_len(&self) -> usize {
        let records: usize = self.records.iter().map(Record::encoded_len).sum();
        let tail = if self.truncate.is_some() { 3 } else { 0 };
        records
            .saturating_add(crate::MAGIC.len())
            .saturating_add(crate::EOF_MARKER.len())
            .saturating_add(tail)
    }
}

/// Counts and sizes of a patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    /// Total records.
    pub records: usize,
    /// Records with literal bytes.
    pub raw_records: usize,
    /// Run-length encoded records.
    pub rle_records: usize,
    /// Sum of the bytes all records write.
    pub bytes_written: usize,
    /// The output is at least this long after applying (0 with no records).
    pub min_output_len: usize,
    /// Truncation length, if the patch has one.
    pub truncate: Option<u32>,
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "records: {} ({} raw, {} rle)",
            self.records, self.raw_records, self.rle_records
        )?;
        writeln!(f, "bytes written: {:#X}", self.bytes_written)?;
        writeln!(f, "minimum output size: {:#X}", self.min_output_len)?;
        match self.truncate {
            Some(len) => write!(f, "truncate to: {len:#X}"),
            None => f.write_str("truncate to: none"),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::shadow_unrelated,
    clippy::similar_names,
    clippy::large_stack_arrays,
    reason = "test fixtures"
)]
mod tests {
    use super::{Patch, Payload, Record};
    use crate::{EOF_OFFSET, Error};

    fn raw(offset: u32, data: &[u8]) -> Record {
        Record { offset, payload: Payload::Raw(data.to_vec()) }
    }

    #[test]
    fn lengths_and_ends() {
        let r = raw(0x10, &[1, 2, 3]);
        assert_eq!(r.len(), 3);
        assert_eq!(r.end(), 0x13);
        assert_eq!(r.encoded_len(), 8);
        assert!(!r.is_rle());
        let rle = Record { offset: 0x20, payload: Payload::Rle { len: 100, value: 0xFF } };
        assert_eq!(rle.len(), 100);
        assert_eq!(rle.end(), 0x84);
        assert_eq!(rle.encoded_len(), 8);
        assert!(rle.is_rle());
    }

    #[test]
    fn display() {
        assert_eq!(raw(0x12345, &[0; 16]).to_string(), "0x012345  len 0x0010  raw");
        let rle = Record { offset: 0x20, payload: Payload::Rle { len: 7, value: 0xAB } };
        assert_eq!(rle.to_string(), "0x000020  len 0x0007  rle 0xAB");
    }

    #[test]
    fn validation() {
        assert_eq!(
            raw(0x100_0000, &[1]).validate(3),
            Err(Error::OffsetTooLarge { index: 3, offset: 0x100_0000 })
        );
        assert_eq!(raw(EOF_OFFSET, &[1]).validate(0), Err(Error::EofOffsetCollision { index: 0 }));
        assert_eq!(raw(0, &[]).validate(1), Err(Error::BadRecordLen { index: 1, len: 0 }));
        assert_eq!(
            raw(0, &[0; 0x1_0000]).validate(2),
            Err(Error::BadRecordLen { index: 2, len: 0x1_0000 })
        );
        assert_eq!(raw(0, &[0; 0xFFFF]).validate(0), Ok(()));
        let rle = Record { offset: 0, payload: Payload::Rle { len: 0, value: 0 } };
        assert_eq!(rle.validate(0), Ok(()));
        let patch = Patch { records: vec![], truncate: Some(0x100_0000) };
        assert_eq!(patch.validate(), Err(Error::TruncateTooLarge { len: 0x100_0000 }));
    }

    #[test]
    fn summary() {
        let patch = Patch {
            records: vec![
                raw(0x10, &[1, 2]),
                Record { offset: 0x100, payload: Payload::Rle { len: 0x20, value: 0 } },
            ],
            truncate: Some(0x200),
        };
        let s = patch.summary();
        assert_eq!((s.records, s.raw_records, s.rle_records), (2, 1, 1));
        assert_eq!(s.bytes_written, 0x22);
        assert_eq!(s.min_output_len, 0x120);
        assert_eq!(s.truncate, Some(0x200));
        assert_eq!(patch.encoded_len(), 5 + 7 + 8 + 3 + 3);
        assert_eq!(Patch::default().summary(), super::Summary::default());
        assert!(s.to_string().starts_with("records: 2 (1 raw, 1 rle)\n"));
    }
}
