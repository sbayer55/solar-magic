//! Free-space search and RATS allocation.
//!
//! Free space is any run of `0x00` / `0xFF` bytes that no tag protects. An
//! allocation writes an 8-byte tag followed by the data; the whole block must
//! fit inside one `chunk` (32 KiB for LoROM, because the game's pointer
//! arithmetic never crosses a bank). Freeing zero-fills the tag and its data.

use super::{RatsTag, TAG_LEN, scan};
use crate::{Rom, RomError};

/// Where and how much to allocate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocRequest {
    /// Number of data bytes (1 to [`RatsTag::MAX_DATA_LEN`]).
    pub len: usize,
    /// Lowest file offset the tag may start at.
    pub min_pc: usize,
    /// File offset the block must end at or before.
    pub max_pc: usize,
    /// Alignment unit the block may not straddle (`0x8000` for LoROM banks).
    pub chunk: usize,
}

impl AllocRequest {
    /// LoROM bank size.
    pub const LOROM_CHUNK: usize = 0x8000;

    /// A request anywhere in `[min_pc, max_pc)` on LoROM bank boundaries.
    #[must_use]
    pub const fn new(len: usize, min_pc: usize, max_pc: usize) -> Self {
        Self { len, min_pc, max_pc, chunk: Self::LOROM_CHUNK }
    }

    /// A request confined to the LoROM bank that starts at file offset `bank_pc`.
    #[must_use]
    pub const fn in_bank(len: usize, bank_pc: usize) -> Self {
        Self::new(len, bank_pc, bank_pc.saturating_add(Self::LOROM_CHUNK))
    }

    /// Tag plus data length.
    #[must_use]
    pub const fn block_len(self) -> usize {
        self.len.saturating_add(TAG_LEN)
    }
}

/// A run of unprotected `0x00` / `0xFF` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreeRun {
    /// File offset of the first free byte.
    pub pc: usize,
    /// Number of free bytes.
    pub len: usize,
}

impl FreeRun {
    /// File offset one past the last free byte.
    #[must_use]
    pub const fn end_pc(self) -> usize {
        self.pc.saturating_add(self.len)
    }
}

/// Errors from allocating or freeing.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RatsError {
    /// No free run inside the requested range can hold the block.
    #[error("no free space for {len} bytes (+ tag) in {min_pc:#X}..{max_pc:#X}")]
    NoSpace {
        /// Requested data length.
        len: usize,
        /// Lower bound of the search.
        min_pc: usize,
        /// Upper bound of the search.
        max_pc: usize,
    },
    /// The data is empty or longer than one tag can protect.
    #[error("{len} bytes cannot be protected by one RATS tag")]
    BadLength {
        /// Requested data length.
        len: usize,
    },
    /// No valid tag starts at the given offset.
    #[error("no RATS tag at file offset {pc:#X}")]
    BadTag {
        /// Where a tag was expected.
        pc: usize,
    },
    /// The ROM rejected a write.
    #[error(transparent)]
    Rom(#[from] RomError),
}

/// Whether a byte counts as free.
const fn is_free_byte(b: u8) -> bool {
    matches!(b, 0x00 | 0xFF)
}

/// Lists the free runs inside `[min_pc, max_pc)`, skipping tagged blocks.
///
/// `tags` must be sorted by offset, as [`scan`] returns them.
#[must_use]
pub fn free_runs(bytes: &[u8], tags: &[RatsTag], min_pc: usize, max_pc: usize) -> Vec<FreeRun> {
    let max_pc = max_pc.min(bytes.len());
    let mut runs = Vec::new();
    let mut tags = tags.iter().skip_while(|t| t.end_pc() <= min_pc).peekable();
    let mut run_start: Option<usize> = None;
    let mut pc = min_pc;
    while pc < max_pc {
        // Jump over a protected block when we reach its tag.
        if let Some(tag) = tags.peek()
            && tag.tag_pc <= pc
        {
            if let Some(start) = run_start.take() {
                runs.push(FreeRun { pc: start, len: pc.saturating_sub(start) });
            }
            pc = tag.end_pc().max(pc.saturating_add(1));
            tags.next();
            continue;
        }
        let free = bytes.get(pc).copied().is_some_and(is_free_byte);
        match (free, run_start) {
            (true, None) => run_start = Some(pc),
            (false, Some(start)) => {
                runs.push(FreeRun { pc: start, len: pc.saturating_sub(start) });
                run_start = None;
            }
            _ => {}
        }
        pc = pc.saturating_add(1);
    }
    if let Some(start) = run_start
        && max_pc > start
    {
        runs.push(FreeRun { pc: start, len: max_pc.saturating_sub(start) });
    }
    runs
}

/// First-fit search: the file offset where the tag of a block satisfying
/// `req` can start, or `None`.
#[must_use]
pub fn find(bytes: &[u8], tags: &[RatsTag], req: &AllocRequest) -> Option<usize> {
    let block = req.block_len();
    if req.len == 0 || req.len > RatsTag::MAX_DATA_LEN || req.chunk < block {
        return None;
    }
    free_runs(bytes, tags, req.min_pc, req.max_pc).iter().find_map(|run| {
        let mut start = run.pc;
        while start.saturating_add(block) <= run.end_pc() {
            let end = start.saturating_add(block).saturating_sub(1);
            if start.checked_div(req.chunk) == end.checked_div(req.chunk) {
                return Some(start);
            }
            // Straddles a chunk boundary: retry at the next boundary.
            start = end.checked_div(req.chunk)?.saturating_mul(req.chunk);
        }
        None
    })
}

/// Allocates a tagged block for `data` and writes it.
///
/// # Errors
///
/// See [`RatsError`].
pub fn alloc(rom: &mut Rom, req: &AllocRequest, data: &[u8]) -> Result<RatsTag, RatsError> {
    if data.len() != req.len {
        return Err(RatsError::BadLength { len: data.len() });
    }
    let header = RatsTag::header_bytes(req.len).ok_or(RatsError::BadLength { len: req.len })?;
    let tags = scan(rom.bytes());
    let tag_pc = find(rom.bytes(), &tags, req).ok_or(RatsError::NoSpace {
        len: req.len,
        min_pc: req.min_pc,
        max_pc: req.max_pc,
    })?;
    rom.write_pc(tag_pc, &header)?;
    let data_pc = tag_pc.saturating_add(TAG_LEN);
    rom.write_pc(data_pc, data)?;
    Ok(RatsTag { tag_pc, data_pc, data_len: req.len })
}

/// Releases a tagged block by zero-filling the tag and its data.
///
/// # Errors
///
/// [`RatsError::BadTag`] if no valid tag starts at `tag.tag_pc`, or a ROM
/// write error.
pub fn free(rom: &mut Rom, tag: RatsTag) -> Result<(), RatsError> {
    let found = RatsTag::parse_at(rom.bytes(), tag.tag_pc)
        .filter(|t| *t == tag)
        .ok_or(RatsError::BadTag { pc: tag.tag_pc })?;
    let zeros = vec![0u8; found.end_pc().saturating_sub(found.tag_pc)];
    rom.write_pc(found.tag_pc, &zeros)?;
    Ok(())
}

/// The tag whose protected data starts exactly at `data_pc`, if any.
#[must_use]
pub fn tag_before(bytes: &[u8], data_pc: usize) -> Option<RatsTag> {
    let tag_pc = data_pc.checked_sub(TAG_LEN)?;
    RatsTag::parse_at(bytes, tag_pc)
}

#[cfg(test)]
mod tests {
    use super::{AllocRequest, FreeRun, RatsError, alloc, find, free, free_runs, tag_before};
    use crate::Rom;
    use crate::rats::{RatsTag, TAG_LEN, scan};

    /// A 64 KiB LoROM image: used bytes everywhere except where tests clear.
    fn image() -> Vec<u8> {
        let mut data = vec![0x42u8; 0x1_0000];
        data[0x7FC0..0x7FE0].copy_from_slice(&crate::test_fixtures::SMW_HEADER);
        data[0x7FD7] = 0x06;
        data
    }

    #[test]
    fn free_runs_skip_tags_and_used_bytes() {
        let mut data = image();
        data[0x100..0x200].fill(0);
        data[0x180..0x188].copy_from_slice(&RatsTag::header_bytes(0x10).unwrap());
        data[0x300..0x310].fill(0xFF);
        let tags = scan(&data);
        assert_eq!(tags.len(), 1);
        let runs = free_runs(&data, &tags, 0, 0x1000);
        assert_eq!(
            runs,
            vec![
                FreeRun { pc: 0x100, len: 0x80 },
                FreeRun { pc: 0x198, len: 0x68 },
                FreeRun { pc: 0x300, len: 0x10 },
            ]
        );
        // Range bounds clip the runs.
        assert_eq!(free_runs(&data, &tags, 0x1A0, 0x1B0), vec![FreeRun { pc: 0x1A0, len: 0x10 }]);
    }

    #[test]
    fn find_is_first_fit_and_respects_chunks() {
        let mut data = image();
        data[0x7F00..0x8200].fill(0); // straddles the 0x8000 chunk boundary
        let tags = scan(&data);
        // 0x40 bytes + tag fit before the boundary.
        assert_eq!(find(&data, &tags, &AllocRequest::new(0x40, 0, 0x1_0000)), Some(0x7F00));
        // 0x100 bytes + tag do not fit before the boundary, so start at it.
        assert_eq!(find(&data, &tags, &AllocRequest::new(0x100, 0, 0x1_0000)), Some(0x8000));
        // Too big for either side.
        assert_eq!(find(&data, &tags, &AllocRequest::new(0x200, 0, 0x1_0000)), None);
        // Bounds exclude the run.
        assert_eq!(find(&data, &tags, &AllocRequest::new(0x10, 0x9000, 0x1_0000)), None);
        assert_eq!(find(&data, &tags, &AllocRequest::new(0, 0, 0x1_0000)), None);
    }

    #[test]
    fn alloc_then_free_round_trips() {
        let mut data = image();
        data[0x8000..0x9000].fill(0);
        let mut rom = Rom::from_bytes(data).expect("valid");
        let payload: Vec<u8> = (0..0x20u8).collect();
        let tag = alloc(&mut rom, &AllocRequest::in_bank(0x20, 0x8000), &payload).expect("fits");
        assert_eq!(tag, RatsTag { tag_pc: 0x8000, data_pc: 0x8008, data_len: 0x20 });
        assert_eq!(scan(rom.bytes()), vec![tag]);
        assert_eq!(&rom.bytes()[0x8008..0x8028], &payload[..]);
        assert_eq!(tag_before(rom.bytes(), 0x8008), Some(tag));
        assert_eq!(tag_before(rom.bytes(), 0x8009), None);

        // A second block lands right after the first.
        let second = alloc(&mut rom, &AllocRequest::in_bank(4, 0x8000), &[9; 4]).expect("fits");
        assert_eq!(second.tag_pc, 0x8028);

        free(&mut rom, tag).expect("valid tag");
        assert!(rom.bytes()[0x8000..0x8028].iter().all(|&b| b == 0));
        assert_eq!(scan(rom.bytes()), vec![second]);
        assert!(matches!(free(&mut rom, tag), Err(RatsError::BadTag { pc: 0x8000 })));
    }

    #[test]
    fn alloc_errors() {
        let mut rom = Rom::from_bytes(image()).expect("valid");
        assert!(matches!(
            alloc(&mut rom, &AllocRequest::in_bank(4, 0x8000), &[0; 4]),
            Err(RatsError::NoSpace { .. })
        ));
        assert!(matches!(
            alloc(&mut rom, &AllocRequest::in_bank(4, 0x8000), &[0; 5]),
            Err(RatsError::BadLength { len: 5 })
        ));
        assert!(RatsTag::header_bytes(0).is_none());
        assert!(RatsTag::header_bytes(RatsTag::MAX_DATA_LEN + 1).is_none());
        assert_eq!(RatsTag::header_bytes(1).map(|h| h.len()), Some(TAG_LEN));
    }
}
