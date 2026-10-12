//! Byte-range differences between two images.

use core::ops::Range;

/// File-offset ranges where `a` and `b` differ.
///
/// Neighbouring ranges closer than `gap` bytes are merged. Beyond the shorter
/// image the missing bytes count as `0x00`, so data added by expansion shows
/// up as changed.
#[must_use]
pub fn changed_ranges(a: &[u8], b: &[u8], gap: usize) -> Vec<Range<usize>> {
    let len = a.len().max(b.len());
    let mut out: Vec<Range<usize>> = Vec::new();
    for pc in 0..len {
        let (x, y) = (a.get(pc).copied().unwrap_or(0), b.get(pc).copied().unwrap_or(0));
        if x == y {
            continue;
        }
        match out.last_mut() {
            Some(last) if pc.saturating_sub(last.end) < gap.max(1) => {
                last.end = pc.saturating_add(1);
            }
            _ => out.push(pc..pc.saturating_add(1)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::changed_ranges;

    #[test]
    fn coalesces_and_pads() {
        let a = [0u8, 1, 2, 3, 4, 5, 6, 7];
        let b = [0u8, 9, 2, 3, 9, 5, 6, 7, 0, 0, 1];
        assert_eq!(changed_ranges(&a, &b, 1), vec![1..2, 4..5, 10..11]);
        assert_eq!(changed_ranges(&a, &b, 4), vec![1..5, 10..11]);
        assert!(changed_ranges(&a, &a, 1).is_empty());
    }
}
