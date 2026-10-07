//! Decoder.

use crate::Error;

/// Default output limit: the largest buffer SMW ever decompresses into is
/// well under this, and the repeat command can only address 64 KiB anyway.
pub const MAX_OUTPUT: usize = 0x1_0000;

/// A header byte of this value ends the stream.
const TERMINATOR: u8 = 0xFF;

/// Result of a successful decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decompressed {
    /// The decoded bytes.
    pub data: Vec<u8>,
    /// Number of input bytes consumed, including the terminator.
    pub consumed: usize,
}

/// Decodes a stream, allowing up to [`MAX_OUTPUT`] bytes of output.
///
/// # Errors
///
/// See [`Error`].
pub fn decompress(input: &[u8]) -> Result<Decompressed, Error> {
    decompress_with_limit(input, MAX_OUTPUT)
}

/// Decodes a stream, failing if the output would exceed `limit` bytes.
///
/// # Errors
///
/// See [`Error`].
pub fn decompress_with_limit(input: &[u8], limit: usize) -> Result<Decompressed, Error> {
    let mut reader = Reader { input, pos: 0 };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let header_offset = reader.pos;
        let header = reader.byte()?;
        if header == TERMINATOR {
            return Ok(Decompressed { data: out, consumed: reader.pos });
        }
        let (command, len) = decode_header(header, &mut reader)?;
        if out.len().saturating_add(len) > limit {
            return Err(Error::OutputTooLarge { limit });
        }
        match command {
            0 => out.extend_from_slice(reader.bytes(len)?),
            1 => {
                let b = reader.byte()?;
                out.extend(core::iter::repeat_n(b, len));
            }
            2 => {
                let pair = [reader.byte()?, reader.byte()?];
                out.extend(pair.iter().copied().cycle().take(len));
            }
            3 => {
                let start = reader.byte()?;
                out.extend(
                    (0..len).map(|k| start.wrapping_add(u8::try_from(k & 0xFF).unwrap_or(0))),
                );
            }
            4 => {
                let source = usize::from(u16::from_be_bytes([reader.byte()?, reader.byte()?]));
                repeat(&mut out, source, len).ok_or(Error::RepeatOutOfRange {
                    offset: header_offset,
                    from_offset: source,
                    produced: out.len(),
                })?;
            }
            other => return Err(Error::InvalidCommand { offset: header_offset, command: other }),
        }
    }
}

/// Splits a header into `(command, length)`, reading the second byte of a
/// long-length header when needed. The returned length is already `+1`.
fn decode_header(header: u8, reader: &mut Reader<'_>) -> Result<(u8, usize), Error> {
    let command = header >> 5;
    if command == 7 {
        let second = reader.byte()?;
        let real_command = (header >> 2) & 0x07;
        let len = (usize::from(header & 0x03) << 8) | usize::from(second);
        Ok((real_command, len.saturating_add(1)))
    } else {
        Ok((command, usize::from(header & 0x1F).saturating_add(1)))
    }
}

/// Appends `len` bytes copied from `out[source..]`, byte by byte so that
/// overlapping ranges behave like the original routine. Returns `None` if
/// `source` is past the end of the output produced so far.
fn repeat(out: &mut Vec<u8>, source: usize, len: usize) -> Option<()> {
    if source >= out.len() {
        return None;
    }
    out.reserve(len);
    for k in 0..len {
        let b = *out.get(source.checked_add(k)?)?;
        out.push(b);
    }
    Some(())
}

/// Cursor over the input with end-of-stream errors.
struct Reader<'a> {
    input: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, Error> {
        let b = *self.input.get(self.pos).ok_or(Error::UnexpectedEnd { offset: self.pos })?;
        self.pos = self.pos.saturating_add(1);
        Ok(b)
    }

    fn bytes(&mut self, len: usize) -> Result<&[u8], Error> {
        let end = self.pos.checked_add(len).ok_or(Error::UnexpectedEnd { offset: self.pos })?;
        let slice = self
            .input
            .get(self.pos..end)
            .ok_or(Error::UnexpectedEnd { offset: self.input.len() })?;
        self.pos = end;
        Ok(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::{Decompressed, decompress, decompress_with_limit};
    use crate::Error;

    fn ok(input: &[u8]) -> Vec<u8> {
        decompress(input).expect("valid stream").data
    }

    #[test]
    fn empty_stream() {
        assert_eq!(decompress(&[0xFF]), Ok(Decompressed { data: vec![], consumed: 1 }));
    }

    #[test]
    fn direct_copy() {
        // 000_00010 => copy 3 bytes
        assert_eq!(ok(&[0x02, 0xAA, 0xBB, 0xCC, 0xFF]), vec![0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn byte_fill() {
        // 001_00011 => 4 copies of 0x5A
        assert_eq!(ok(&[0x23, 0x5A, 0xFF]), vec![0x5A; 4]);
    }

    #[test]
    fn word_fill_odd_length() {
        // 010_00100 => 5 bytes alternating 0x12 0x34
        assert_eq!(ok(&[0x44, 0x12, 0x34, 0xFF]), vec![0x12, 0x34, 0x12, 0x34, 0x12]);
    }

    #[test]
    fn increasing_fill_wraps() {
        // 011_00011 => 4 bytes starting at 0xFE: FE FF 00 01
        assert_eq!(ok(&[0x63, 0xFE, 0xFF]), vec![0xFE, 0xFF, 0x00, 0x01]);
    }

    #[test]
    fn repeat_non_overlapping_and_overlapping() {
        // copy "AB", then repeat 2 bytes from offset 0, then repeat 4 bytes from offset 2
        // (which overlaps the write position: produces AB AB).
        let input = [0x01, b'A', b'B', 0x81, 0x00, 0x00, 0x83, 0x00, 0x02, 0xFF];
        assert_eq!(ok(&input), b"ABABABAB".to_vec());
    }

    #[test]
    fn long_length_header() {
        // 111_001_01 00000011 => byte fill, length 0x103 + 1 = 260
        let out = ok(&[0xE5, 0x03, 0x77, 0xFF]);
        assert_eq!(out.len(), 260);
        assert!(out.iter().all(|&b| b == 0x77));
    }

    #[test]
    fn consumed_counts_terminator() {
        let d = decompress(&[0x20, 0x01, 0xFF, 0xDE, 0xAD]).expect("valid");
        assert_eq!(d.consumed, 3);
        assert_eq!(d.data, vec![0x01]);
    }

    #[test]
    fn errors() {
        assert_eq!(decompress(&[]), Err(Error::UnexpectedEnd { offset: 0 }));
        assert_eq!(decompress(&[0x02, 0xAA]), Err(Error::UnexpectedEnd { offset: 2 }));
        assert_eq!(decompress(&[0xA0, 0x00]), Err(Error::InvalidCommand { offset: 0, command: 5 }));
        assert_eq!(decompress(&[0xC0, 0x00]), Err(Error::InvalidCommand { offset: 0, command: 6 }));
        assert_eq!(
            decompress(&[0x80, 0x00, 0x05, 0xFF]),
            Err(Error::RepeatOutOfRange { offset: 0, from_offset: 5, produced: 0 })
        );
        assert_eq!(
            decompress_with_limit(&[0x23, 0x5A, 0xFF], 3),
            Err(Error::OutputTooLarge { limit: 3 })
        );
    }
}
