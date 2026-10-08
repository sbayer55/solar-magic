//! Finding the byte ranges a patch has to cover.

use core::ops::Range;

/// Gaps of unchanged bytes up to this long are rewritten rather than split:
/// a new record header costs 5 bytes, so rewriting 5 or fewer is never worse.
pub(crate) const MAX_GAP: usize = 5;

/// Maximal runs of indices that a patch must write.
///
/// Inside the source's length a byte is changed when it differs. Past it (the
/// growth region) an applier zero-fills, so only non-zero bytes count, except
/// that the final byte always counts so the output reaches `target.len()`.
pub(crate) fn changed_ranges(source: &[u8], target: &[u8]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut open: Option<usize> = None;
    let last = target.len().checked_sub(1);
    for (i, &t) in target.iter().enumerate() {
        let changed = source.get(i).map_or_else(|| t != 0 || Some(i) == last, |&s| s != t);
        if changed {
            open.get_or_insert(i);
        } else if let Some(start) = open.take() {
            ranges.push(start..i);
        }
    }
    if let Some(start) = open {
        ranges.push(start..target.len());
    }
    ranges
}

/// Joins neighbouring ranges separated by at most `max_gap` bytes.
pub(crate) fn merge_gaps(ranges: Vec<Range<usize>>, max_gap: usize) -> Vec<Range<usize>> {
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = merged.last_mut()
            && range.start.saturating_sub(last.end) <= max_gap
        {
            last.end = range.end;
        } else {
            merged.push(range);
        }
    }
    merged
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
    use super::{MAX_GAP, changed_ranges, merge_gaps};

    #[test]
    fn finds_differences() {
        assert!(changed_ranges(&[1, 2, 3], &[1, 2, 3]).is_empty());
        assert_eq!(changed_ranges(&[1, 2, 3, 4], &[1, 9, 9, 4]), vec![1..3]);
        assert_eq!(changed_ranges(&[1, 2, 3, 4], &[9, 2, 3, 9]), vec![0..1, 3..4]);
    }

    #[test]
    fn growth_region_skips_zeros_but_covers_last_byte() {
        assert_eq!(changed_ranges(&[1], &[1, 0, 0, 0]), vec![3..4]);
        assert_eq!(changed_ranges(&[1], &[1, 0, 7, 0, 0]), vec![2..3, 4..5]);
        assert_eq!(changed_ranges(&[1], &[1, 5, 5]), vec![1..3]);
        assert_eq!(changed_ranges(&[], &[0, 0]), vec![1..2]);
        assert!(changed_ranges(&[], &[]).is_empty());
    }

    #[test]
    fn shrinking_only_reports_changes_inside_target() {
        assert!(changed_ranges(&[1, 2, 3, 4], &[1, 2]).is_empty());
        assert_eq!(changed_ranges(&[1, 2, 3, 4], &[1, 9]), vec![1..2]);
    }

    #[test]
    fn merges_small_gaps() {
        let ranges = vec![0..2, 7..8, 14..15, 15..16];
        assert_eq!(merge_gaps(ranges, MAX_GAP), vec![0..8, 14..16]);
        assert!(merge_gaps(vec![], MAX_GAP).is_empty());
    }
}
