//! Building a patch from an original/modified pair.

use core::ops::Range;

use crate::{EOF_OFFSET, Error, MAX_FILE_LEN, MAX_RECORD_LEN, Patch, Payload, Record, diff};

/// Builds a patch that turns `source` into `target`.
///
/// Changed bytes are grouped into records (gaps of up to 5 unchanged bytes
/// are rewritten rather than split), runs of one byte become RLE records
/// when that is smaller, records are chunked at 65535 bytes, no record starts
/// at [`EOF_OFFSET`], and a truncation length is added when `target` is
/// shorter than `source`. Identical inputs give an empty patch.
///
/// # Errors
///
/// [`Error::FileTooLarge`] if either input exceeds 16 MiB.
pub fn create(source: &[u8], target: &[u8]) -> Result<Patch, Error> {
    if source.len() > MAX_FILE_LEN {
        return Err(Error::FileTooLarge { what: "original", len: source.len() });
    }
    if target.len() > MAX_FILE_LEN {
        return Err(Error::FileTooLarge { what: "modified", len: target.len() });
    }
    let ranges = diff::merge_gaps(diff::changed_ranges(source, target), diff::MAX_GAP);
    let mut emitter = Emitter { target, records: Vec::new() };
    for range in ranges {
        emitter.encode_range(range)?;
    }
    let truncate = if target.len() < source.len() {
        Some(
            u32::try_from(target.len())
                .map_err(|_| Error::TruncateTooLarge { len: target.len() })?,
        )
    } else {
        None
    };
    Ok(Patch { records: emitter.records, truncate })
}

/// Minimum run length for which an RLE record (8 bytes) beats literal bytes.
///
/// With no raw record open and the run ending the range, the alternative is
/// a raw record of its own (5 + r). With exactly one of those true, the run
/// shares a header with neighbouring raw bytes, so only r itself competes.
/// A run inside a raw block also forces a second header (8 + 5).
const fn rle_threshold(no_pending_raw: bool, at_end: bool) -> usize {
    if no_pending_raw && at_end {
        4
    } else if no_pending_raw || at_end {
        9
    } else {
        14
    }
}

/// Length of the run of identical bytes starting at `pos`, capped at `end`.
fn run_len(bytes: &[u8], pos: usize, end: usize) -> usize {
    let Some(window) = bytes.get(pos..end) else { return 0 };
    let Some(&first) = window.first() else { return 0 };
    window.iter().take_while(|&&b| b == first).count()
}

struct Emitter<'a> {
    target: &'a [u8],
    records: Vec<Record>,
}

impl Emitter<'_> {
    /// Encodes `[range.start, range.end)` of the target as raw and RLE records.
    fn encode_range(&mut self, range: Range<usize>) -> Result<(), Error> {
        let end = range.end;
        let mut pos = range.start;
        let mut raw_start: Option<usize> = None;
        while pos < end {
            let run = run_len(self.target, pos, end).max(1);
            let next = pos.saturating_add(run);
            if run >= rle_threshold(raw_start.is_none(), next == end) {
                if let Some(start) = raw_start.take() {
                    self.flush_raw(start, pos)?;
                }
                let value = self.target.get(pos).copied().unwrap_or(0);
                self.emit_rle(pos, run, value)?;
            } else {
                raw_start.get_or_insert(pos);
            }
            pos = next;
        }
        if let Some(start) = raw_start {
            self.flush_raw(start, end)?;
        }
        Ok(())
    }

    /// Emits `[start, end)` as raw records of at most 65535 bytes.
    fn flush_raw(&mut self, mut start: usize, end: usize) -> Result<(), Error> {
        while start < end {
            let stop = start.saturating_add(end.saturating_sub(start).min(MAX_RECORD_LEN));
            let data = self.target.get(start..stop).unwrap_or(&[]);
            if data.is_empty() {
                return Ok(());
            }
            self.push(start, Payload::Raw(data.to_vec()))?;
            start = stop;
        }
        Ok(())
    }

    /// Emits a run as RLE records of at most 65535 bytes each.
    fn emit_rle(&mut self, mut start: usize, mut run: usize, value: u8) -> Result<(), Error> {
        while run > 0 {
            let len = run.min(MAX_RECORD_LEN);
            self.push(start, Payload::Rle { len: u16::try_from(len).unwrap_or(u16::MAX), value })?;
            start = start.saturating_add(len);
            run = run.saturating_sub(len);
        }
        Ok(())
    }

    /// Appends a record, rewriting any that would start at [`EOF_OFFSET`].
    fn push(&mut self, offset: usize, payload: Payload) -> Result<(), Error> {
        if offset == usize::try_from(EOF_OFFSET).unwrap_or(usize::MAX) {
            return self.push_avoiding_eof(offset, payload);
        }
        let index = self.records.len();
        let offset = u32::try_from(offset).map_err(|_| Error::OffsetTooLarge { index, offset })?;
        self.records.push(Record { offset, payload });
        Ok(())
    }

    /// Starts the record one byte earlier, rewriting the (unchanged) byte
    /// before it, so that it no longer reads as the EOF marker.
    fn push_avoiding_eof(&mut self, offset: usize, payload: Payload) -> Result<(), Error> {
        let before = offset.saturating_sub(1);
        let prev = self.target.get(before).copied().unwrap_or(0);
        match payload {
            Payload::Raw(mut data) => {
                data.insert(0, prev);
                if data.len() > MAX_RECORD_LEN {
                    let last = data.pop().unwrap_or(0);
                    let last_offset = before.saturating_add(data.len());
                    self.push(before, Payload::Raw(data))?;
                    self.push(last_offset, Payload::Raw(vec![last]))
                } else {
                    self.push(before, Payload::Raw(data))
                }
            }
            Payload::Rle { len, value } => {
                if prev == value && len < u16::MAX {
                    return self.push(before, Payload::Rle { len: len.saturating_add(1), value });
                }
                self.push(before, Payload::Raw(vec![prev, value]))?;
                let rest = len.saturating_sub(1);
                if rest == 0 {
                    return Ok(());
                }
                self.push(offset.saturating_add(1), Payload::Rle { len: rest, value })
            }
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
    use super::create;
    use crate::test_util::{kinds, roundtrip};
    use crate::{EOF_OFFSET, Error, MAX_FILE_LEN, Payload, Record};

    /// Non-repeating filler so runs only appear where a test puts them.
    fn noise(len: usize) -> Vec<u8> {
        (0..len).map(|i| u8::try_from((i * 7 + 3) & 0xFF).unwrap()).collect()
    }

    fn with_run(block: usize, at: usize, run: usize) -> (Vec<u8>, Vec<u8>) {
        let source = noise(block + 64);
        let mut target = source.clone();
        for b in &mut target[32..32 + block] {
            *b ^= 0xFF;
        }
        for b in &mut target[32 + at..32 + at + run] {
            *b = 0x5A;
        }
        (source, target)
    }

    #[test]
    fn identical_inputs_give_empty_patch() {
        let data = noise(1024);
        let patch = roundtrip(&data, &data);
        assert!(patch.records.is_empty());
        assert_eq!(patch.truncate, None);
        assert_eq!(patch.to_bytes().unwrap(), b"PATCHEOF");
    }

    #[test]
    fn single_byte() {
        let source = noise(100);
        let mut target = source.clone();
        target[50] ^= 1;
        let patch = roundtrip(&source, &target);
        assert_eq!(
            patch.records,
            vec![Record { offset: 50, payload: Payload::Raw(vec![target[50]]) }]
        );
    }

    #[test]
    fn gap_merging() {
        for (gap, expected) in [(5, 1), (6, 2)] {
            let source = noise(100);
            let mut target = source.clone();
            target[10] ^= 1;
            target[10 + 1 + gap] ^= 1;
            let patch = roundtrip(&source, &target);
            assert_eq!(patch.records.len(), expected, "gap {gap}");
        }
    }

    #[test]
    fn whole_range_run_threshold() {
        for (run, expected) in [(3, "raw"), (4, "rle")] {
            let source = noise(64);
            let mut target = source.clone();
            for b in &mut target[16..16 + run] {
                *b = 0x5A;
            }
            let patch = roundtrip(&source, &target);
            assert_eq!(kinds(&patch), vec![expected], "run {run}");
        }
    }

    #[test]
    fn edge_run_threshold() {
        for (run, expected) in [(8, vec!["raw"]), (9, vec!["rle", "raw"])] {
            let (source, target) = with_run(40, 0, run);
            let patch = roundtrip(&source, &target);
            assert_eq!(kinds(&patch), expected, "run {run} at start");
        }
        for (run, expected) in [(8, vec!["raw"]), (9, vec!["raw", "rle"])] {
            let (source, target) = with_run(40, 40 - run, run);
            let patch = roundtrip(&source, &target);
            assert_eq!(kinds(&patch), expected, "run {run} at end");
        }
    }

    #[test]
    fn middle_run_threshold() {
        for (run, expected) in [(13, vec!["raw"]), (14, vec!["raw", "rle", "raw"])] {
            let (source, target) = with_run(60, 20, run);
            let patch = roundtrip(&source, &target);
            assert_eq!(kinds(&patch), expected, "run {run} in the middle");
        }
    }

    #[test]
    fn long_runs_are_chunked() {
        let source = noise(70_100);
        let mut target = source.clone();
        for b in &mut target[50..50 + 70_000] {
            *b = 0;
        }
        let patch = roundtrip(&source, &target);
        assert_eq!(kinds(&patch), vec!["rle", "rle"]);
        assert_eq!(patch.records[0].len(), 0xFFFF);
        assert_eq!(patch.records[1].len(), 70_000 - 0xFFFF);
    }

    #[test]
    fn long_raw_blocks_are_chunked() {
        let source = vec![0u8; 70_100];
        let mut target = source.clone();
        target[50..50 + 70_000].copy_from_slice(&noise(70_000));
        let patch = roundtrip(&source, &target);
        assert_eq!(kinds(&patch), vec!["raw", "raw"]);
        assert_eq!(patch.records[0].len(), 0xFFFF);
    }

    #[test]
    fn growth() {
        let source = noise(1000);
        let mut target = source.clone();
        target.resize(2000, 0);
        let patch = roundtrip(&source, &target);
        assert_eq!(patch.records.len(), 1);
        assert_eq!(patch.records[0].offset, 1999);
        assert_eq!(patch.records[0].len(), 1);

        let mut target = source.clone();
        target.extend_from_slice(&noise(500));
        let patch = roundtrip(&source, &target);
        assert_eq!(patch.records.len(), 1);
        assert_eq!(patch.records[0].offset, 1000);
    }

    #[test]
    fn shrink() {
        let source = noise(1000);
        let target = source[..900].to_vec();
        let patch = roundtrip(&source, &target);
        assert!(patch.records.is_empty());
        assert_eq!(patch.truncate, Some(900));

        let mut target = source[..900].to_vec();
        target[0] ^= 1;
        let patch = roundtrip(&source, &target);
        assert_eq!(patch.records.len(), 1);
        assert_eq!(patch.truncate, Some(900));
    }

    fn eof_offset() -> usize {
        usize::try_from(EOF_OFFSET).unwrap()
    }

    #[test]
    fn single_change_at_eof_offset() {
        let source = noise(eof_offset() + 0x20);
        let mut target = source.clone();
        target[eof_offset()] ^= 1;
        let patch = roundtrip(&source, &target);
        assert_eq!(patch.records.len(), 1);
        assert_eq!(patch.records[0].offset, EOF_OFFSET - 1);
        assert_eq!(patch.records[0].len(), 2);
        let parsed = crate::Patch::parse(&patch.to_bytes().unwrap()).unwrap();
        assert_eq!(parsed.records.len(), 1);
    }

    #[test]
    fn rle_run_at_eof_offset() {
        let source = noise(eof_offset() + 0x80);
        // Previous byte differs from the run value.
        let mut target = source.clone();
        for b in &mut target[eof_offset()..eof_offset() + 50] {
            *b = 0x11;
        }
        target[eof_offset() - 1] = 0x22;
        roundtrip(&source, &target);
        // Previous byte equals the run value.
        let mut target = source.clone();
        for b in &mut target[eof_offset()..eof_offset() + 50] {
            *b = 0x11;
        }
        target[eof_offset() - 1] = 0x11;
        roundtrip(&source, &target);
        // Run of exactly one byte, previous byte unchanged and different.
        let mut target = source.clone();
        target[eof_offset()] = 0x33;
        target[eof_offset() + 1] ^= 0xFF;
        target[eof_offset() + 2] ^= 0xFF;
        roundtrip(&source, &target);
    }

    #[test]
    fn raw_chunk_boundary_at_eof_offset() {
        let start = eof_offset() - 0xFFFF;
        let source = vec![0u8; eof_offset() + 0x2000];
        let mut target = source.clone();
        target[start..start + 70_000].copy_from_slice(&noise(70_000));
        let patch = roundtrip(&source, &target);
        assert!(patch.records.len() >= 2);
    }

    #[test]
    fn rle_chunk_boundary_at_eof_offset() {
        let start = eof_offset() - 0xFFFF;
        let source = noise(eof_offset() + 0x2000);
        let mut target = source.clone();
        for b in &mut target[start..start + 70_000] {
            *b = 0x77;
        }
        roundtrip(&source, &target);
    }

    #[test]
    fn rejects_huge_inputs() {
        let big = vec![0u8; MAX_FILE_LEN + 1];
        assert_eq!(
            create(&big, &[]),
            Err(Error::FileTooLarge { what: "original", len: MAX_FILE_LEN + 1 })
        );
        assert_eq!(
            create(&[], &big),
            Err(Error::FileTooLarge { what: "modified", len: MAX_FILE_LEN + 1 })
        );
    }

    #[test]
    fn real_rom_round_trip() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = std::fs::read(path).unwrap();
        let mut expanded = rom.clone();
        for b in &mut expanded[0x7FC0..0x7FC4] {
            *b ^= 0x20;
        }
        expanded.resize(0x10_0000, 0);
        *expanded.last_mut().unwrap() = 0xFF;
        let patch = roundtrip(&rom, &expanded);
        assert!(patch.encoded_len() < 128, "patch is {} bytes", patch.encoded_len());

        let truncated = rom[..0x4_0000].to_vec();
        let patch = roundtrip(&rom, &truncated);
        assert_eq!(patch.truncate, Some(0x4_0000));
    }
}
