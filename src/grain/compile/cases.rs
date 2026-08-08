//! Turning rhai's `switch` range arms into something that can be looked up.
//!
//! Rhai scans its range arms in order and takes the first one that both
//! contains the subject *and* whose guard passes — so when two arms overlap,
//! which arm runs is not a property of the subject alone. A dispatch table has
//! one answer per subject, so the overlap has to go somewhere, and the only
//! place it can go without keeping the subject alive across the guards is
//! here.
//!
//! [`split`] cuts the arms into disjoint intervals. Each interval carries the
//! arms covering it, in source order, and the compiler emits those as a chain
//! of guards ending at the default — exactly what it already does for the arms
//! sharing a case value.

use crate::{ast::RangeCase, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

use crate::grain::bytecode::SwitchRange;

/// A range arm's interval, as bounds rather than as a `Range`.
fn bounds(range: &RangeCase) -> (INT, INT, bool) {
    match range {
        RangeCase::ExclusiveInt(r, ..) => (r.start, r.end, false),
        RangeCase::InclusiveInt(r, ..) => (*r.start(), *r.end(), true),
    }
}

/// A piece of the number line that every arm either covers whole or not at
/// all.
///
/// Two kinds because the bounds are integers but the values are not: between
/// two adjacent bounds there is nothing to enumerate and yet `5.5` is in
/// there, so the gaps are intervals in their own right rather than something
/// the endpoints cover.
enum Atom {
    /// A single integer.
    Point(INT),
    /// Everything strictly between two adjacent bounds.
    Between(INT, INT),
}

fn covers(range: &RangeCase, atom: &Atom) -> bool {
    match atom {
        Atom::Point(point) => range.contains_int(*point),
        // Every endpoint is a bound, so a range that reaches into a gap
        // between two adjacent bounds spans the whole of it.
        Atom::Between(low, high) => {
            let (start, end, ..) = bounds(range);
            start <= *low && end >= *high
        }
    }
}

/// One interval and the arms that cover it, being built.
struct Run {
    from: INT,
    to: INT,
    inclusive: bool,
    blocks: Vec<usize>,
}

/// Split range arms into disjoint intervals, each carrying the arms that cover
/// it in source order.
///
/// Targets come back as zero: which instruction an interval sends control to
/// is the caller's to fill in, once it has emitted the chain.
///
/// ## Why the lower bounds can overlap after all
///
/// An interval that starts in a gap — everything above `10` but not `10`
/// itself — has no exact form here, and comes back as `10..hi`, which does
/// include `10`. That is sound because entries are scanned in order and `10`
/// is always claimed by an earlier one: any arm reaching into the gap above a
/// bound also covers the bound, so the point is never an interval nobody
/// emitted.
pub(crate) fn split(ranges: &[RangeCase]) -> Vec<(SwitchRange, Vec<usize>)> {
    let mut points: Vec<INT> = Vec::with_capacity(ranges.len() * 2);
    for range in ranges {
        let (start, end, ..) = bounds(range);
        points.push(start);
        points.push(end);
    }
    points.sort_unstable();
    points.dedup();

    let mut atoms = Vec::with_capacity(points.len() * 2);
    for (index, point) in points.iter().enumerate() {
        atoms.push(Atom::Point(*point));
        if let Some(next) = points.get(index + 1) {
            atoms.push(Atom::Between(*point, *next));
        }
    }

    let mut out: Vec<(SwitchRange, Vec<usize>)> = Vec::new();
    let mut run: Option<Run> = None;

    let finish = |run: Run| {
        (
            SwitchRange {
                from: run.from,
                to: run.to,
                inclusive: run.inclusive,
                target: 0,
            },
            run.blocks,
        )
    };

    for atom in &atoms {
        let blocks: Vec<usize> = ranges
            .iter()
            .filter(|range| covers(range, atom))
            .map(RangeCase::index)
            .collect();

        let (from, to, inclusive) = match atom {
            Atom::Point(point) => (*point, *point, true),
            Atom::Between(low, high) => (*low, *high, false),
        };

        // Atoms are contiguous and in order, so one that runs the same arms as
        // the one before it is the same table entry stretched further.
        if run
            .as_ref()
            .map(|open| open.blocks == blocks)
            .unwrap_or(false)
        {
            let open = run.as_mut().expect("just checked");
            open.to = to;
            open.inclusive = inclusive;
            continue;
        }

        if let Some(open) = run.take() {
            out.push(finish(open));
        }
        // An atom no arm covers ends the run and starts nothing: a subject
        // there has no range arm and belongs to the default.
        if !blocks.is_empty() {
            run = Some(Run {
                from,
                to,
                inclusive,
                blocks,
            });
        }
    }

    if let Some(open) = run.take() {
        out.push(finish(open));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `from..to => block`, as rhai's parser would record it.
    fn exclusive(from: INT, to: INT, block: usize) -> RangeCase {
        let mut case: RangeCase = (from..to).into();
        case.set_index(block);
        case
    }

    fn inclusive(from: INT, to: INT, block: usize) -> RangeCase {
        let mut case: RangeCase = (from..=to).into();
        case.set_index(block);
        case
    }

    /// What the table says, as `(from, to, inclusive, blocks)`.
    fn table(ranges: &[RangeCase]) -> Vec<(INT, INT, bool, Vec<usize>)> {
        split(ranges)
            .into_iter()
            .map(|(range, blocks)| (range.from, range.to, range.inclusive, blocks))
            .collect()
    }

    #[test]
    fn nothing_in_gives_nothing_out() {
        assert!(split(&[]).is_empty());
    }

    /// The common shape. Splitting must not turn two arms into six entries,
    /// or every artifact with a `switch` in it pays for the case nobody
    /// wrote.
    #[test]
    fn arms_that_do_not_overlap_come_back_unchanged() {
        assert_eq!(
            table(&[inclusive(0, 9, 0), inclusive(10, 99, 1)]),
            vec![(0, 9, true, vec![0]), (10, 99, true, vec![1])],
        );
    }

    /// Two arms over the same values — the shape that has no single answer at
    /// runtime, and the reason this exists.
    #[test]
    fn identical_arms_become_one_entry_running_both() {
        assert_eq!(
            table(&[inclusive(0, 9, 0), inclusive(0, 9, 1)]),
            vec![(0, 9, true, vec![0, 1])],
        );
    }

    #[test]
    fn a_partial_overlap_splits_into_three() {
        assert_eq!(
            table(&[exclusive(0, 10, 0), exclusive(5, 20, 1)]),
            vec![
                (0, 5, false, vec![0]),
                (5, 10, false, vec![0, 1]),
                (10, 20, false, vec![1]),
            ],
        );
    }

    /// The awkward one: the arms meet at a single integer that only one of
    /// them includes, and the piece above it cannot be named exactly.
    #[test]
    fn an_inclusive_end_meeting_an_exclusive_start_splits_at_the_point() {
        let split = table(&[inclusive(0, 10, 0), exclusive(10, 20, 1)]);

        assert_eq!(
            split,
            vec![
                (0, 10, false, vec![0]),
                (10, 10, true, vec![0, 1]),
                (10, 20, false, vec![1]),
            ],
        );

        // The last entry does contain `10`, which belongs to both arms — the
        // entry before it is what makes that unreachable, so the order is
        // part of the answer and not a presentation detail.
        let (shared, ..) = &split[1];
        assert!(*shared == 10, "the point entry must come first");
    }

    /// A hole between two arms is not an entry: a subject in it has no range
    /// arm at all and must reach the default.
    #[test]
    fn a_gap_between_arms_is_left_out() {
        assert_eq!(
            table(&[inclusive(0, 4, 0), inclusive(10, 14, 1)]),
            vec![(0, 4, true, vec![0]), (10, 14, true, vec![1])],
        );
    }

    /// An arm inside another arm keeps the outer one either side of it.
    #[test]
    fn a_nested_arm_splits_the_one_around_it() {
        assert_eq!(
            table(&[exclusive(0, 100, 0), exclusive(10, 20, 1)]),
            vec![
                (0, 10, false, vec![0]),
                (10, 20, false, vec![0, 1]),
                (20, 100, false, vec![0]),
            ],
        );
    }

    /// Splitting only means anything if what comes out agrees with rhai about
    /// which arms a value belongs to — so check the pieces against the arms
    /// they came from, at every bound and between them.
    #[test]
    fn every_value_reaches_the_arms_rhai_would_have_run() {
        let arms = [
            exclusive(0, 10, 0),
            inclusive(5, 20, 1),
            exclusive(20, 25, 2),
            inclusive(-5, 0, 3),
        ];
        let split = split(&arms);

        let mut probes: Vec<rhai::Dynamic> = Vec::new();
        for value in -8..=28 {
            probes.push(rhai::Dynamic::from(value as INT));
            // Halfway between two integers is where an interval that has no
            // exact form goes wrong, so the integers alone would not find it.
            #[cfg(not(feature = "no_float"))]
            probes.push(rhai::Dynamic::from(value as rhai::FLOAT + 0.5));
        }

        for probe in &probes {
            let expected: Vec<usize> = arms
                .iter()
                .filter(|arm| arm.contains(probe))
                .map(RangeCase::index)
                .collect();

            let found = split
                .iter()
                .find(|(range, ..)| range.contains(probe))
                .map(|(.., blocks)| blocks.clone())
                .unwrap_or_default();

            assert_eq!(found, expected, "for {probe:?}");
        }
    }
}
