//! Patch file writer.

use crate::{EOF_MARKER, Error, MAGIC, Patch, Payload};

/// Serializes a patch. See [`Patch::to_bytes`].
pub(crate) fn to_bytes(patch: &Patch) -> Result<Vec<u8>, Error> {
    patch.validate()?;
    let mut out = Vec::with_capacity(patch.encoded_len());
    out.extend_from_slice(MAGIC);
    for (index, record) in patch.records.iter().enumerate() {
        push_u24(&mut out, record.offset);
        match &record.payload {
            Payload::Raw(data) => {
                let len = u16::try_from(data.len())
                    .map_err(|_| Error::BadRecordLen { index, len: data.len() })?;
                push_u16(&mut out, len);
                out.extend_from_slice(data);
            }
            Payload::Rle { len, value } => {
                push_u16(&mut out, 0);
                push_u16(&mut out, *len);
                out.push(*value);
            }
        }
    }
    out.extend_from_slice(EOF_MARKER);
    if let Some(len) = patch.truncate {
        push_u24(&mut out, len);
    }
    Ok(out)
}

fn push_u24(out: &mut Vec<u8>, value: u32) {
    let [_, b1, b2, b3] = value.to_be_bytes();
    out.extend_from_slice(&[b1, b2, b3]);
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
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
    use crate::{EOF_OFFSET, Error, Patch, Payload, Record};

    fn raw(offset: u32, data: &[u8]) -> Record {
        Record { offset, payload: Payload::Raw(data.to_vec()) }
    }

    #[test]
    fn writes_every_field() {
        let patch = Patch {
            records: vec![
                raw(0x12345, &[0xAA, 0xBB]),
                Record { offset: 0x10, payload: Payload::Rle { len: 0x1234, value: 0xFF } },
            ],
            truncate: Some(0xAB_CDEF),
        };
        let bytes = patch.to_bytes().unwrap();
        assert_eq!(
            bytes,
            [
                b"PATCH".as_slice(),
                &[0x01, 0x23, 0x45, 0x00, 0x02, 0xAA, 0xBB],
                &[0x00, 0x00, 0x10, 0x00, 0x00, 0x12, 0x34, 0xFF],
                b"EOF",
                &[0xAB, 0xCD, 0xEF],
            ]
            .concat()
        );
        assert_eq!(bytes.len(), patch.encoded_len());
        assert_eq!(Patch::parse(&bytes).unwrap(), patch);
    }

    #[test]
    fn empty_patch() {
        assert_eq!(Patch::default().to_bytes().unwrap(), b"PATCHEOF");
    }

    #[test]
    fn rejects_unencodable_records() {
        let cases = [
            (raw(0x100_0000, &[1]), Error::OffsetTooLarge { index: 0, offset: 0x100_0000 }),
            (raw(EOF_OFFSET, &[1]), Error::EofOffsetCollision { index: 0 }),
            (raw(0, &[]), Error::BadRecordLen { index: 0, len: 0 }),
            (raw(0, &[0; 0x1_0000]), Error::BadRecordLen { index: 0, len: 0x1_0000 }),
        ];
        for (record, expected) in cases {
            let patch = Patch { records: vec![record], truncate: None };
            assert_eq!(patch.to_bytes(), Err(expected));
        }
        let patch = Patch { records: vec![], truncate: Some(0x100_0000) };
        assert_eq!(patch.to_bytes(), Err(Error::TruncateTooLarge { len: 0x100_0000 }));
    }
}
