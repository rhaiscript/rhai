//! A stable sort that cannot panic, whatever the comparison function returns.
//!
//! [`slice::sort_by`] may panic when the comparison function does not implement a total order,
//! and under `panic = "abort"` (common for WebAssembly) that cannot be caught. The sorts here
//! cannot panic. When the comparison function is a total order, they give exactly the same result
//! as [`slice::sort_by`], equal elements included, because a stable sort has only one possible
//! result then.
//!
//! The algorithm is a powersort (the merge policy of CPython's `list.sort`): natural runs, short
//! ones extended by binary insertion, merged in an order chosen from the runs' positions,
//! galloping through long stretches that come from one run. Input that is already in order, or in
//! strictly descending order, takes one comparison less than there are elements.
#![cfg(not(feature = "no_index"))]

#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{cmp::Ordering, convert::Infallible};

/// Sort a slice with a comparison function, like [`slice::sort_by`], but without ever panicking.
///
/// The sort is stable. When `compare` implements a total order, the result is the same as that
/// of [`slice::sort_by`]. Otherwise the order is unspecified, but every element is kept.
///
/// `compare` is only ever asked whether a later element is less than an earlier one, so one that
/// also returns [`Less`][Ordering::Less] for equal elements puts them in reverse order.
//
// Arrays use `try_sort_by`. This is the same sort as a drop-in for `slice::sort_by`, and the
// tests check it against `slice::sort_by` itself.
#[allow(dead_code)]
#[inline]
pub fn sort_by<T, F>(slice: &mut [T], mut compare: F)
where
    F: FnMut(&T, &T) -> Ordering,
{
    match try_sort_by(slice, |a, b| Ok::<_, Infallible>(compare(a, b))) {
        Ok(()) => (),
        Err(never) => match never {},
    }
}

/// Sort a slice with a comparison function that may fail, as [`sort_by`] does.
///
/// The first error from `compare` stops the sort and is returned, with the slice left unchanged.
pub fn try_sort_by<T, E, F>(slice: &mut [T], mut compare: F) -> Result<(), E>
where
    F: FnMut(&T, &T) -> Result<Ordering, E>,
{
    let n = slice.len();
    if n < 2 {
        return Ok(());
    }

    // Sort positions rather than elements, so that the slice is untouched on error.
    let mut perm: Vec<usize> = (0..n).collect();
    let mut tmp = Vec::new();
    let mut less = |a: usize, b: usize| Ok(compare(&slice[a], &slice[b])? == Ordering::Less);

    // Pending runs as (start, length, power), each power greater than the one below it.
    let mut stack: Vec<(usize, usize, u32)> = Vec::new();
    let mut start = 0;
    let mut len = next_run(&mut perm, 0, &mut less)?;

    while start + len < n {
        let next_start = start + len;
        let next_len = next_run(&mut perm, next_start, &mut less)?;
        let power = node_power(n, start, len, next_len);

        while let Some(&(s, l, p)) = stack.last() {
            if p <= power {
                break;
            }
            merge(&mut perm, &mut tmp, s, s + l, start + len, &mut less)?;
            stack.pop();
            len += start - s;
            start = s;
        }
        stack.push((start, len, power));
        start = next_start;
        len = next_len;
    }
    while let Some((s, l, ..)) = stack.pop() {
        merge(&mut perm, &mut tmp, s, s + l, start + len, &mut less)?;
        len += start - s;
        start = s;
    }

    // Put the elements in their new places, one cycle of the permutation at a time:
    // position `k` takes the element at `perm[k]`.
    for first in 0..n {
        let mut k = first;
        while perm[k] != first {
            let next = perm[k];
            slice.swap(k, next);
            perm[k] = k;
            k = next;
        }
        perm[k] = k;
    }

    Ok(())
}

/// Shortest run worth merging; a shorter natural run is extended by binary insertion.
const MIN_RUN: usize = 16;

/// Elements a merge takes from one run in a row before it starts galloping.
const MIN_GALLOP: usize = 7;

/// Find the run starting at `start`, put it in ascending order in `perm` and return its length.
fn next_run<E>(
    perm: &mut [usize],
    start: usize,
    less: &mut impl FnMut(usize, usize) -> Result<bool, E>,
) -> Result<usize, E> {
    let n = perm.len();
    let mut end = start + 1;

    if end < n {
        if less(perm[end], perm[start])? {
            // Strictly descending: no two elements are equal, so reversing keeps the sort stable.
            end += 1;
            while end < n && less(perm[end], perm[end - 1])? {
                end += 1;
            }
            perm[start..end].reverse();
        } else {
            end += 1;
            while end < n && !less(perm[end], perm[end - 1])? {
                end += 1;
            }
        }
    }

    // Extend a short run by binary insertion, placing each element after any equal to it.
    let target = (start + MIN_RUN).min(n);
    while end < target {
        let x = perm[end];
        let (mut lo, mut hi) = (start, end);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if less(x, perm[mid])? {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        perm[lo..=end].rotate_right(1);
        end += 1;
    }

    Ok(end - start)
}

/// Merge the sorted neighbours `perm[lo..mid]` and `perm[mid..hi]`.
///
/// Once one run has given `MIN_GALLOP` elements in a row, the merge gallops: it finds how many
/// more come from that run by an exponential search, until the stretches found get short again.
fn merge<E>(
    perm: &mut [usize],
    tmp: &mut Vec<usize>,
    lo: usize,
    mid: usize,
    hi: usize,
    less: &mut impl FnMut(usize, usize) -> Result<bool, E>,
) -> Result<(), E> {
    // Nothing to do if the two are already in order.
    if !less(perm[mid], perm[mid - 1])? {
        return Ok(());
    }

    tmp.clear();
    tmp.extend_from_slice(&perm[lo..mid]);

    // `tmp[i..]` and `perm[j..hi]` are still to be merged into `perm[k..hi]`, with `k < j`.
    // Take from the right only when strictly less, so that equal elements keep their order.
    let (mut i, mut j, mut k) = (0, mid, lo);

    'merge: loop {
        // One element at a time.
        let (mut from_left, mut from_right) = (0, 0);
        while from_left < MIN_GALLOP && from_right < MIN_GALLOP {
            if less(perm[j], tmp[i])? {
                perm[k] = perm[j];
                j += 1;
                from_right += 1;
                from_left = 0;
            } else {
                perm[k] = tmp[i];
                i += 1;
                from_left += 1;
                from_right = 0;
            }
            k += 1;
            if i == tmp.len() || j == hi {
                break 'merge;
            }
        }

        // Galloping, while the stretches found are long.
        loop {
            // The left elements not greater than the next right one, then that one.
            let from_left = gallop(tmp.len() - i, |x| Ok(!less(perm[j], tmp[i + x])?))?;
            perm[k..k + from_left].copy_from_slice(&tmp[i..i + from_left]);
            i += from_left;
            k += from_left;
            if i == tmp.len() {
                break 'merge;
            }
            perm[k] = perm[j];
            j += 1;
            k += 1;
            if j == hi {
                break 'merge;
            }

            // The right elements less than the next left one, then that one.
            let from_right = gallop(hi - j, |x| less(perm[j + x], tmp[i]))?;
            perm.copy_within(j..j + from_right, k);
            j += from_right;
            k += from_right;
            if j == hi {
                break 'merge;
            }
            perm[k] = tmp[i];
            i += 1;
            k += 1;
            if i == tmp.len() {
                break 'merge;
            }

            if from_left < MIN_GALLOP && from_right < MIN_GALLOP {
                break;
            }
        }
    }

    // Whatever is left of the right run is already in place.
    let rest = &tmp[i..];
    perm[k..k + rest.len()].copy_from_slice(rest);

    Ok(())
}

/// Count the positions at the start of `0..n` where `pred` holds, given that it holds up to some
/// position and not after: probe 0, 1, 3, 7, ... then search between the last two probes.
/// If `pred` does not behave like that, the count is still somewhere in `0..=n`.
fn gallop<E>(n: usize, mut pred: impl FnMut(usize) -> Result<bool, E>) -> Result<usize, E> {
    let (mut lo, mut hi) = (0, n);
    let (mut probe, mut step) = (0, 1);

    while probe < n {
        if !pred(probe)? {
            hi = probe;
            break;
        }
        lo = probe + 1;
        probe += step;
        step *= 2;
    }
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if pred(mid)? {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }

    Ok(lo)
}

/// Powersort's priority for the boundary between a run `[s1, s1 + n1)` and the run of length
/// `n2` after it, in a slice of `n` elements: the first bit at which the runs' midpoints, as
/// fractions of `n`, differ. Merges happen at higher powers first.
fn node_power(n: usize, s1: usize, n1: usize, n2: usize) -> u32 {
    // Twice each midpoint, compared against `n` bit by bit (all values stay below `2 * n`).
    let n = n as u128;
    let mut a = 2 * s1 as u128 + n1 as u128;
    let mut b = a + n1 as u128 + n2 as u128;
    let mut power = 0;

    loop {
        power += 1;
        if a >= n {
            a -= n;
            b -= n;
        } else if b >= n {
            return power;
        }
        a <<= 1;
        b <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_by, try_sort_by};
    use std::cmp::Ordering::{Equal, Greater, Less};

    /// A small deterministic random number generator (xorshift64*).
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) % n
        }
    }

    /// Sort pairs of (key, original position) by key with both sorts: where equal keys end up
    /// shows that the order of equal elements is the same too.
    fn check_same_as_slice_sort_by(keys: &[u64]) {
        let mut expected: Vec<(u64, usize)> = keys.iter().copied().zip(0..).collect();
        let mut actual = expected.clone();

        expected.sort_by(|a, b| a.0.cmp(&b.0));
        sort_by(&mut actual, |a, b| a.0.cmp(&b.0));

        assert_eq!(actual, expected, "keys {keys:?}");
    }

    #[test]
    fn same_as_slice_sort_by_on_every_short_list() {
        // Every list of `n` keys taken from `n` values, for `n` up to 6.
        for n in 0..=6 {
            let mut keys = vec![0; n as usize];
            loop {
                check_same_as_slice_sort_by(&keys);

                // The next list, counting in base `n`.
                let Some(i) = keys.iter().position(|&k| k + 1 < n) else {
                    break;
                };
                keys[i] += 1;
                keys[..i].fill(0);
            }
        }
    }

    #[test]
    fn same_as_slice_sort_by_on_random_lists() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);

        for round in 0..600 {
            let n = rng.below(2000);
            let range = [2, 5, 100, 1 << 40][round % 4];
            let keys: Vec<u64> = (0..n)
                .map(|i| match round / 4 % 6 {
                    0 => rng.below(range),                       // random
                    1 => i / 10 + rng.below(range.min(3)),       // nearly ascending
                    2 => (n - i) / 10 + rng.below(range.min(3)), // nearly descending
                    3 => i % 37 + rng.below(range) * 37,         // ascending runs
                    4 => (n - i) % 29 + rng.below(range) * 29,   // descending runs
                    _ if i < n / 2 => i,                         // sorted, then random
                    _ => rng.below(range.min(n)),
                })
                .collect();

            check_same_as_slice_sort_by(&keys);
        }
    }

    #[test]
    fn keeps_every_element_whatever_compare_returns() {
        let mut rng = Rng(7);

        for n in [2, 3, 15, 16, 17, 64, 65, 300, 2000] {
            let items: Vec<u64> = (0..n).map(|_| rng.below(10)).collect();
            let mut sorted_items = items.clone();
            sorted_items.sort_unstable();

            for kind in 0..5 {
                let mut actual = items.clone();
                sort_by(&mut actual, |a, b| match kind {
                    0 => Less,
                    1 => Greater,
                    2 => [Less, Equal, Greater][rng.below(3) as usize],
                    // "Less" for equal elements too, as `|a, b| a <= b` does in a script.
                    3 if a % 3 <= b % 3 => Less,
                    3 => Greater,
                    // Contradicts itself.
                    _ => (a ^ b).cmp(&5),
                });
                actual.sort_unstable();
                assert_eq!(actual, sorted_items, "kind {kind}, {n} elements");
            }
        }
    }

    #[test]
    fn stops_at_the_first_error_with_the_slice_unchanged() {
        let mut rng = Rng(42);
        let original: Vec<u64> = (0..100).map(|_| rng.below(1000)).collect();

        for fail_at in [1, 2, 50, 400] {
            let mut actual = original.clone();
            let mut calls = 0;

            let result = try_sort_by(&mut actual, |a, b| {
                calls += 1;
                if calls == fail_at {
                    Err(calls)
                } else {
                    Ok(a.cmp(b))
                }
            });

            assert_eq!(result, Err(fail_at));
            assert_eq!(calls, fail_at);
            assert_eq!(actual, original);
        }
    }
}
