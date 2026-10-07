//! Patch file reader.

use crate::{EOF_MARKER, Error, MAGIC, Patch, Payload, Record};

/// Parses a patch file. See [`Patch::parse`].
pub(crate) fn parse(bytes: &[u8]) -> Result<Patch, Error> {
    if bytes.get(..MAGIC.len()) != Some(MAGIC.as_slice()) {
        return Err(Error::BadMagic);
    }
    let mut reader = Reader { input: bytes, pos: MAGIC.len() };
    let mut records = Vec::new();
    loop {
        if reader.peek_is(EOF_MARKER) {
            reader.skip(EOF_MARKER.len());
            let truncate = match reader.remaining() {
                0 => None,
                3 => Some(reader.u24_be("truncation length")?),
                len => return Err(Error::TrailingData { offset: reader.pos, len }),
            };
            return Ok(Patch { records, truncate });
        }
        let offset = reader.u24_be("record offset or EOF")?;
        let size = reader.u16_be("record size")?;
        let payload = if size == 0 {
            let len = reader.u16_be("RLE run length")?;
            let value = reader.byte("RLE value")?;
            Payload::Rle { len, value }
        } else {
            Payload::Raw(reader.bytes(usize::from(size), "record data")?.to_vec())
        };
        records.push(Record { offset, payload });
    }
}

/// Cursor over the input with end-of-file errors.
struct Reader<'a> {
    input: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn byte(&mut self, expected: &'static str) -> Result<u8, Error> {
        let b =
            *self.input.get(self.pos).ok_or(Error::UnexpectedEnd { offset: self.pos, expected })?;
        self.pos = self.pos.saturating_add(1);
        Ok(b)
    }

    fn bytes(&mut self, len: usize, expected: &'static str) -> Result<&[u8], Error> {
        let end =
            self.pos.checked_add(len).ok_or(Error::UnexpectedEnd { offset: self.pos, expected })?;
        let slice = self
            .input
            .get(self.pos..end)
            .ok_or(Error::UnexpectedEnd { offset: self.input.len(), expected })?;
        self.pos = end;
        Ok(slice)
    }

    fn u16_be(&mut self, expected: &'static str) -> Result<u16, Error> {
        Ok(u16::from_be_bytes([self.byte(expected)?, self.byte(expected)?]))
    }

    fn u24_be(&mut self, expected: &'static str) -> Result<u32, Error> {
        Ok(u32::from_be_bytes([
            0,
            self.byte(expected)?,
            self.byte(expected)?,
            self.byte(expected)?,
        ]))
    }

    fn peek_is(&self, tag: &[u8]) -> bool {
        self.pos.checked_add(tag.len()).and_then(|end| self.input.get(self.pos..end)) == Some(tag)
    }

    const fn skip(&mut self, len: usize) {
        self.pos = self.pos.saturating_add(len);
    }

    const fn remaining(&self) -> usize {
        self.input.len().saturating_sub(self.pos)
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
    use super::parse;
    use crate::{Error, Patch, Payload, Record};

    #[test]
    fn empty_patch() {
        assert_eq!(parse(b"PATCHEOF"), Ok(Patch::default()));
    }

    #[test]
    fn raw_rle_and_truncation() {
        let bytes = [
            b"PATCH".as_slice(),
            &[0x01, 0x23, 0x45, 0x00, 0x03, 0xAA, 0xBB, 0xCC],
            &[0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x20, 0xFF],
            b"EOF",
            &[0x07, 0xFF, 0xFF],
        ]
        .concat();
        let patch = parse(&bytes).unwrap();
        assert_eq!(
            patch,
            Patch {
                records: vec![
                    Record { offset: 0x12345, payload: Payload::Raw(vec![0xAA, 0xBB, 0xCC]) },
                    Record { offset: 0x10, payload: Payload::Rle { len: 0x20, value: 0xFF } },
                ],
                truncate: Some(0x7_FFFF),
            }
        );
    }

    #[test]
    fn zero_length_rle_is_kept() {
        let bytes = [b"PATCH".as_slice(), &[0, 0, 0x10, 0, 0, 0, 0, 0x55], b"EOF"].concat();
        let patch = parse(&bytes).unwrap();
        assert_eq!(patch.records[0].payload, Payload::Rle { len: 0, value: 0x55 });
        assert_eq!(patch.to_bytes().unwrap(), bytes);
    }

    #[test]
    fn eof_wins_at_record_boundary() {
        // A record at 0x454F46 cannot be expressed: those bytes are EOF.
        let bytes = [b"PATCH".as_slice(), b"EOF", &[0x00, 0x01, 0x42]].concat();
        let patch = parse(&bytes).unwrap();
        assert!(patch.records.is_empty());
        assert_eq!(patch.truncate, Some(0x142));
    }

    #[test]
    fn errors() {
        let cases: &[(&[u8], Error)] = &[
            (b"", Error::BadMagic),
            (b"PATC", Error::BadMagic),
            (b"PATCX", Error::BadMagic),
            (b"PATCH", Error::UnexpectedEnd { offset: 5, expected: "record offset or EOF" }),
            (
                b"PATCH\x00\x00",
                Error::UnexpectedEnd { offset: 7, expected: "record offset or EOF" },
            ),
            (b"PATCH\x00\x00\x10\x00", Error::UnexpectedEnd { offset: 9, expected: "record size" }),
            (
                b"PATCH\x00\x00\x10\x00\x04\xAA",
                Error::UnexpectedEnd { offset: 11, expected: "record data" },
            ),
            (
                b"PATCH\x00\x00\x10\x00\x00\x00",
                Error::UnexpectedEnd { offset: 11, expected: "RLE run length" },
            ),
            (
                b"PATCH\x00\x00\x10\x00\x00\x00\x08",
                Error::UnexpectedEnd { offset: 12, expected: "RLE value" },
            ),
            (
                b"PATCH\x00\x00\x10\x00\x01\xAA",
                Error::UnexpectedEnd { offset: 11, expected: "record offset or EOF" },
            ),
            (b"PATCHEOF\x00", Error::TrailingData { offset: 8, len: 1 }),
            (b"PATCHEOF\x00\x00", Error::TrailingData { offset: 8, len: 2 }),
            (b"PATCHEOF\x00\x00\x00\x00", Error::TrailingData { offset: 8, len: 4 }),
        ];
        for (input, expected) in cases {
            assert_eq!(parse(input), Err(*expected), "input {input:02X?}");
        }
    }
}
