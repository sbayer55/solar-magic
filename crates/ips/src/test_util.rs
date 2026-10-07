//! Shared assertions for the crate's tests.

use crate::{EOF_OFFSET, Patch, apply, create};

/// Creates a patch from `source` to `target` and checks every invariant:
/// applying it reproduces `target` exactly, no record starts at the EOF
/// offset, and serializing then parsing is lossless in both directions.
pub(crate) fn roundtrip(source: &[u8], target: &[u8]) -> Patch {
    let patch = create(source, target).expect("create");
    assert!(patch.records.iter().all(|r| r.offset != EOF_OFFSET), "record at EOF offset");
    let applied = apply(source, &patch).expect("apply");
    assert!(applied.data == target, "apply(create(source, target)) != target");
    assert_eq!(applied.report.output_len, target.len());
    let bytes = patch.to_bytes().expect("serialize");
    assert_eq!(bytes.len(), patch.encoded_len());
    let reparsed = Patch::parse(&bytes).expect("reparse");
    assert_eq!(reparsed, patch);
    assert_eq!(reparsed.to_bytes().expect("reserialize"), bytes);
    patch
}

/// `"raw"` / `"rle"` per record, for compact assertions on encoder choices.
pub(crate) fn kinds(patch: &Patch) -> Vec<&'static str> {
    patch.records.iter().map(|r| if r.is_rle() { "rle" } else { "raw" }).collect()
}
