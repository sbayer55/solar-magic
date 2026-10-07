//! Applying a patch to a byte buffer.

use crate::{Error, MAX_FILE_LEN, Patch, Payload, Record};

/// One line of the apply log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogEntry {
    /// Index of the record in the patch.
    pub index: usize,
    /// Target offset written.
    pub offset: u32,
    /// Bytes written.
    pub len: usize,
    /// Whether the record was run-length encoded.
    pub rle: bool,
    /// Whether the record grew the output (the gap was zero-filled).
    pub extended: bool,
}

/// What happened while applying.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyReport {
    /// One entry per record, in order.
    pub entries: Vec<LogEntry>,
    /// Length of the input.
    pub source_len: usize,
    /// Length of the output.
    pub output_len: usize,
    /// Sum of bytes written by all records.
    pub bytes_written: usize,
    /// Set when the truncation extension actually shortened the output.
    pub truncated_to: Option<usize>,
}

impl core::fmt::Display for ApplyReport {
    /// `"524288 -> 1048576 bytes (grew by 524288 bytes)"`.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} -> {} bytes (", self.source_len, self.output_len)?;
        match self.truncated_to {
            Some(len) => write!(f, "truncated to {len:#X}")?,
            None if self.output_len > self.source_len => {
                write!(f, "grew by {} bytes", self.output_len.saturating_sub(self.source_len))?;
            }
            None => f.write_str("same size")?,
        }
        f.write_str(")")
    }
}

/// A patched buffer and its report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The patched bytes.
    pub data: Vec<u8>,
    /// The log.
    pub report: ApplyReport,
}

/// Applies `patch` to a copy of `source`.
///
/// Records are applied in order. A record that reaches past the end of the
/// output grows it, zero-filling any gap. The truncation extension then cuts
/// the output if it is longer than the stated length (never extends it).
///
/// # Errors
///
/// [`Error::FileTooLarge`] if `source` exceeds 16 MiB, or
/// [`Error::OutputTooLarge`] if a record would write past 16 MiB.
pub fn apply(source: &[u8], patch: &Patch) -> Result<Applied, Error> {
    if source.len() > MAX_FILE_LEN {
        return Err(Error::FileTooLarge { what: "source", len: source.len() });
    }
    let mut out = source.to_vec();
    let mut entries = Vec::with_capacity(patch.records.len());
    let mut bytes_written = 0usize;
    for (index, record) in patch.records.iter().enumerate() {
        let entry = apply_record(&mut out, index, record)?;
        bytes_written = bytes_written.saturating_add(entry.len);
        entries.push(entry);
    }
    let truncated_to = patch.truncate.and_then(|len| {
        let len = usize::try_from(len).ok()?;
        (len < out.len()).then_some(len)
    });
    if let Some(len) = truncated_to {
        out.truncate(len);
    }
    let report = ApplyReport {
        entries,
        source_len: source.len(),
        output_len: out.len(),
        bytes_written,
        truncated_to,
    };
    Ok(Applied { data: out, report })
}

fn apply_record(out: &mut Vec<u8>, index: usize, record: &Record) -> Result<LogEntry, Error> {
    let len = record.len();
    let mut entry =
        LogEntry { index, offset: record.offset, len, rle: record.is_rle(), extended: false };
    if len == 0 {
        return Ok(entry);
    }
    let start = usize::try_from(record.offset).unwrap_or(usize::MAX);
    let end = start
        .checked_add(len)
        .filter(|&end| end <= MAX_FILE_LEN)
        .ok_or_else(|| Error::OutputTooLarge { index, end: start.saturating_add(len) })?;
    if end > out.len() {
        out.resize(end, 0);
        entry.extended = true;
    }
    let dest = out.get_mut(start..end).ok_or(Error::OutputTooLarge { index, end })?;
    match &record.payload {
        Payload::Raw(data) => dest.copy_from_slice(data),
        Payload::Rle { value, .. } => dest.fill(*value),
    }
    Ok(entry)
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
    use super::{LogEntry, apply};
    use crate::{Error, MAX_FILE_LEN, Patch, Payload, Record};

    fn raw(offset: u32, data: &[u8]) -> Record {
        Record { offset, payload: Payload::Raw(data.to_vec()) }
    }

    fn rle(offset: u32, len: u16, value: u8) -> Record {
        Record { offset, payload: Payload::Rle { len, value } }
    }

    #[test]
    fn overwrites_in_order() {
        let patch = Patch { records: vec![raw(1, &[9, 9, 9]), rle(2, 1, 5)], truncate: None };
        let applied = apply(&[0, 1, 2, 3, 4], &patch).unwrap();
        assert_eq!(applied.data, [0, 9, 5, 9, 4]);
        assert_eq!(applied.report.bytes_written, 4);
        assert_eq!(applied.report.output_len, 5);
        assert_eq!(
            applied.report.entries[0],
            LogEntry { index: 0, offset: 1, len: 3, rle: false, extended: false }
        );
        assert!(applied.report.entries[1].rle);
    }

    #[test]
    fn extends_with_zero_fill() {
        let patch = Patch { records: vec![raw(4, &[7, 8]), rle(10, 2, 1)], truncate: None };
        let applied = apply(&[1, 2], &patch).unwrap();
        assert_eq!(applied.data, [1, 2, 0, 0, 7, 8, 0, 0, 0, 0, 1, 1]);
        assert!(applied.report.entries.iter().all(|e| e.extended));
    }

    #[test]
    fn report_display() {
        let grown = apply(&[1], &Patch { records: vec![raw(3, &[1])], truncate: None }).unwrap();
        assert_eq!(grown.report.to_string(), "1 -> 4 bytes (grew by 3 bytes)");
        let same = apply(&[1, 2], &Patch::default()).unwrap();
        assert_eq!(same.report.to_string(), "2 -> 2 bytes (same size)");
        let cut = apply(&[1, 2, 3], &Patch { records: vec![], truncate: Some(1) }).unwrap();
        assert_eq!(cut.report.to_string(), "3 -> 1 bytes (truncated to 0x1)");
    }

    #[test]
    fn truncation() {
        let shorter = Patch { records: vec![], truncate: Some(2) };
        let applied = apply(&[1, 2, 3, 4], &shorter).unwrap();
        assert_eq!(applied.data, [1, 2]);
        assert_eq!(applied.report.truncated_to, Some(2));
        let longer = Patch { records: vec![], truncate: Some(10) };
        let applied = apply(&[1, 2, 3, 4], &longer).unwrap();
        assert_eq!(applied.data, [1, 2, 3, 4]);
        assert_eq!(applied.report.truncated_to, None);
    }

    #[test]
    fn zero_length_rle_writes_nothing() {
        let patch = Patch { records: vec![rle(0x100, 0, 0xFF)], truncate: None };
        let applied = apply(&[1], &patch).unwrap();
        assert_eq!(applied.data, [1]);
        assert_eq!(applied.report.entries[0].len, 0);
    }

    #[test]
    fn limits() {
        let patch = Patch { records: vec![raw(0xFF_FFFF, &[1, 2])], truncate: None };
        assert_eq!(apply(&[], &patch), Err(Error::OutputTooLarge { index: 0, end: 0x100_0001 }));
        let patch = Patch { records: vec![raw(0xFF_FFFF, &[1])], truncate: None };
        assert_eq!(apply(&[], &patch).unwrap().data.len(), MAX_FILE_LEN);
        let big = vec![0u8; MAX_FILE_LEN + 1];
        assert_eq!(
            apply(&big, &Patch::default()),
            Err(Error::FileTooLarge { what: "source", len: MAX_FILE_LEN + 1 })
        );
    }
}
