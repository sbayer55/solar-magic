//! Encoder.
//!
//! A straightforward greedy encoder: at each position it picks the command
//! that covers the most input (repeat from earlier output, byte fill, word
//! fill, increasing fill), falling back to direct copy. Output is always
//! decodable by [`crate::decompress`] and is typically a little smaller than
//! the game's own files, which were produced by Nintendo's tools.

/// Longest chunk a short header can describe.
const SHORT_MAX: usize = 32;
/// Longest chunk a long header can describe.
const LONG_MAX: usize = 1024;
/// Command codes.
const DIRECT: u8 = 0;
const BYTE_FILL: u8 = 1;
const WORD_FILL: u8 = 2;
const INC_FILL: u8 = 3;
const REPEAT: u8 = 4;

/// A chunk the encoder decided on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chunk {
    ByteFill { len: usize, value: u8 },
    WordFill { len: usize, pair: [u8; 2] },
    IncFill { len: usize, start: u8 },
    Repeat { len: usize, from: u16 },
}

impl Chunk {
    const fn len(self) -> usize {
        match self {
            Self::ByteFill { len, .. }
            | Self::WordFill { len, .. }
            | Self::IncFill { len, .. }
            | Self::Repeat { len, .. } => len,
        }
    }

    /// Bytes this chunk costs in the stream (header plus payload).
    const fn cost(self) -> usize {
        let header: usize = if self.len() > SHORT_MAX { 2 } else { 1 };
        let payload = match self {
            Self::ByteFill { .. } | Self::IncFill { .. } => 1,
            Self::WordFill { .. } | Self::Repeat { .. } => 2,
        };
        header.saturating_add(payload)
    }
}

/// Compresses `data` into an LC_LZ2 stream ending in the `0xFF` terminator.
#[must_use]
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len().saturating_add(8));
    let mut pos = 0usize;
    let mut direct_start: Option<usize> = None;
    while pos < data.len() {
        let best = best_chunk(data, pos);
        // A fill or repeat is worth taking only if it beats copying the
        // same bytes directly as part of a literal run.
        if let Some(chunk) = best.filter(|c| c.cost() < c.len()) {
            flush_direct(&mut out, data, &mut direct_start, pos);
            emit(&mut out, chunk);
            pos = pos.saturating_add(chunk.len());
        } else {
            let start = *direct_start.get_or_insert(pos);
            pos = pos.saturating_add(1);
            if pos.saturating_sub(start) == LONG_MAX {
                flush_direct(&mut out, data, &mut direct_start, pos);
            }
        }
    }
    flush_direct(&mut out, data, &mut direct_start, pos);
    out.push(0xFF);
    out
}

/// The longest fill or repeat chunk that starts at `pos`, if any.
fn best_chunk(data: &[u8], pos: usize) -> Option<Chunk> {
    let rest = data.get(pos..)?;
    let first = *rest.first()?;
    let limit = rest.len().min(LONG_MAX);
    let mut candidates: Vec<Chunk> = Vec::with_capacity(4);

    let byte_len = rest.iter().take(limit).take_while(|&&b| b == first).count();
    if byte_len >= 2 {
        candidates.push(Chunk::ByteFill { len: byte_len, value: first });
    }
    if let Some(&second) = rest.get(1) {
        let pair = [first, second];
        let word_len = rest
            .iter()
            .take(limit)
            .enumerate()
            .take_while(|(i, b)| pair.get(i % 2) == Some(*b))
            .count();
        if word_len >= 3 {
            candidates.push(Chunk::WordFill { len: word_len, pair });
        }
    }
    let inc_len = rest
        .iter()
        .take(limit)
        .enumerate()
        .take_while(|(i, b)| **b == first.wrapping_add(u8::try_from(i % 256).unwrap_or(0)))
        .count();
    if inc_len >= 3 {
        candidates.push(Chunk::IncFill { len: inc_len, start: first });
    }
    if let Some(repeat) = longest_repeat(data, pos, limit) {
        candidates.push(repeat);
    }
    candidates.into_iter().max_by_key(|c| c.len().saturating_sub(c.cost()))
}

/// The longest earlier occurrence of the bytes at `pos` (overlap allowed,
/// as the decoder copies byte by byte).
fn longest_repeat(data: &[u8], pos: usize, limit: usize) -> Option<Chunk> {
    let mut best: Option<(usize, usize)> = None;
    let search_start = pos.saturating_sub(0x1_0000);
    for from in search_start..pos {
        let mut len = 0usize;
        while len < limit
            && let (Some(&a), Some(&b)) =
                (data.get(from.saturating_add(len)), data.get(pos.saturating_add(len)))
            && a == b
        {
            len = len.saturating_add(1);
        }
        if len >= 3 && best.is_none_or(|(_, l)| len > l) {
            best = Some((from, len));
        }
    }
    let (from, len) = best?;
    Some(Chunk::Repeat { len, from: u16::try_from(from).ok()? })
}

/// Writes the pending direct-copy run, if any.
fn flush_direct(out: &mut Vec<u8>, data: &[u8], start: &mut Option<usize>, end: usize) {
    let Some(from) = start.take() else {
        return;
    };
    let mut at = from;
    while at < end {
        let len = end.saturating_sub(at).min(LONG_MAX);
        push_header(out, DIRECT, len);
        if let Some(bytes) = data.get(at..at.saturating_add(len)) {
            out.extend_from_slice(bytes);
        }
        at = at.saturating_add(len);
    }
}

fn emit(out: &mut Vec<u8>, chunk: Chunk) {
    match chunk {
        Chunk::ByteFill { len, value } => {
            push_header(out, BYTE_FILL, len);
            out.push(value);
        }
        Chunk::WordFill { len, pair } => {
            push_header(out, WORD_FILL, len);
            out.extend_from_slice(&pair);
        }
        Chunk::IncFill { len, start } => {
            push_header(out, INC_FILL, len);
            out.push(start);
        }
        Chunk::Repeat { len, from } => {
            push_header(out, REPEAT, len);
            out.extend_from_slice(&from.to_be_bytes());
        }
    }
}

/// Writes a short or long header for `len` (1 to 1024) bytes.
fn push_header(out: &mut Vec<u8>, command: u8, len: usize) {
    let len = len.clamp(1, LONG_MAX);
    let encoded = len.saturating_sub(1);
    if len <= SHORT_MAX {
        out.push((command << 5) | u8::try_from(encoded & 0x1F).unwrap_or(0));
    } else {
        let high = u8::try_from((encoded >> 8) & 0x03).unwrap_or(0);
        out.push(0xE0 | (command << 2) | high);
        out.push(u8::try_from(encoded & 0xFF).unwrap_or(0));
    }
}

#[cfg(test)]
mod tests {
    use super::compress;
    use crate::decompress;

    fn round_trip(data: &[u8]) -> Vec<u8> {
        let packed = compress(data);
        let unpacked = decompress(&packed).expect("decodes");
        assert_eq!(unpacked.data, data, "round trip");
        assert_eq!(unpacked.consumed, packed.len());
        packed
    }

    #[test]
    fn empty_and_small() {
        assert_eq!(compress(&[]), vec![0xFF]);
        assert_eq!(round_trip(&[0x42]), vec![0x00, 0x42, 0xFF]);
        assert_eq!(round_trip(&[1, 2, 3]), vec![0x62, 1, 0xFF], "increasing fill");
        assert_eq!(round_trip(&[1, 5, 3]), vec![0x02, 1, 5, 3, 0xFF]);
    }

    #[test]
    fn fills_and_repeats_are_used() {
        assert_eq!(round_trip(&[7; 40]), vec![0xE4, 0x27, 7, 0xFF]);
        assert_eq!(round_trip(&[1, 2, 1, 2, 1, 2]), vec![0x45, 1, 2, 0xFF]);
        assert_eq!(round_trip(&[10, 11, 12, 13]), vec![0x63, 10, 0xFF]);
        let mut data = vec![9, 8, 7, 6, 5, 4];
        data.extend([9, 8, 7, 6, 5, 4]);
        let packed = round_trip(&data);
        assert_eq!(packed, vec![0x05, 9, 8, 7, 6, 5, 4, 0x85, 0x00, 0x00, 0xFF]);
    }

    #[test]
    fn long_runs_and_random_data() {
        round_trip(&vec![0xAB; 3000]);
        let noisy: Vec<u8> = (0..5000u32)
            .map(|i| u8::try_from(i.wrapping_mul(2_654_435_761) >> 24).unwrap())
            .collect();
        round_trip(&noisy);
        let wrap: Vec<u8> = (250..=255u8).chain(0..5u8).collect();
        round_trip(&wrap);
    }
}
