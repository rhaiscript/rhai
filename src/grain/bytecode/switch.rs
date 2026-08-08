use crate::{ast::RangeCase, Dynamic, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// A `switch`, as one dispatch table.
///
/// Matching a case is *hash* equality, not `==`. That distinction is rhai's
/// and it is visible: `switch 1 { 1.0 => .. }` does not match, because an
/// integer and a float hash differently, while `1 == 1.0` is true.
///
/// ## Why the hashes travel, and what that costs
///
/// Rhai's parser keeps only the hash of each case — the value itself is not in
/// the AST (`ast/stmt.rs:336`), so there is nothing to re-hash later. The
/// hashes have to be written out as they are.
///
/// And by default they do not survive the trip: `get_hasher` falls back to
/// `ahash::AHasher::default()`, and rhai's default features include
/// `ahash/runtime-rng`, so the seed is drawn per process. Rhai gets away with
/// baking hashes into its AST only because it parses and evaluates in one.
///
/// So an artifact containing a `switch` requires
/// [`rhai::config::hashing::set_hashing_seed`] to have been called with the
/// same seed on both sides. That is not something the format can enforce, but
/// it is something it can *check*: [`probe`] hashes a fixed value, the
/// artifact carries the result, and a loader that computes a different one
/// refuses rather than dispatching every case to the default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Switch {
    /// One entry per distinct case value, in source order. The target is the
    /// head of that value's chain of guarded arms.
    pub cases: Vec<SwitchCase>,
    /// Checked only when no case matched at all, as rhai does — a case that
    /// matched but whose guards all failed goes to the default rather than on
    /// to the ranges (`eval/stmt.rs:544`).
    ///
    /// Disjoint and in ascending order, which rhai's are not: the compiler
    /// splits overlapping arms apart so that the first entry containing a
    /// value is the only one that can match it. See `compile::cases`.
    pub ranges: Vec<SwitchRange>,
    /// Where to go when nothing matched. Always present: an absent `_` arm
    /// compiles to a jump past the statement.
    pub default: u32,
}

/// One `value => ...` arm, keyed by rhai's hash of the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchCase {
    /// Rhai's hash of the case value.
    pub hash: u64,
    /// Where to jump to.
    pub target: u32,
}

/// One `a..b => ...` arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchRange {
    /// The lower bound
    pub from: INT,
    /// The upper bound
    pub to: INT,
    /// Whether the upper bound is included.
    pub inclusive: bool,
    /// Where to jump to.
    pub target: u32,
}

impl SwitchRange {
    /// Whether a subject falls in this range.
    ///
    /// Delegates to rhai's own `RangeCase` rather than comparing integers,
    /// because a range arm matches more than integers: `switch 5.5 { 0..10 =>
    /// .. }` matches, and under the `decimal` feature so does a `Decimal`
    /// (`ast/stmt.rs:254`). Rebuilding the case is two moves and no
    /// allocation, and it means there is one definition of what a range arm
    /// covers.
    #[must_use]
    pub fn contains(&self, value: &Dynamic) -> bool {
        let case: RangeCase = if self.inclusive {
            (self.from..=self.to).into()
        } else {
            (self.from..self.to).into()
        };
        case.contains(value)
    }
}

impl Switch {
    /// Where a subject sends control.
    ///
    /// The order is rhai's (`eval/stmt.rs:517-564`) and each step of it is
    /// load-bearing: an unhashable subject reaches neither the cases nor the
    /// ranges, and a subject whose hash *did* find a case never reaches the
    /// ranges even when that case's guards all decline it — the compiler
    /// points such a chain at the default.
    #[must_use]
    pub fn dispatch(&self, subject: &Dynamic) -> u32 {
        // Hashing an unhashable value panics, so this is a guard and not an
        // optimization.
        if !subject.is_hashable() {
            return self.default;
        }

        let hash = hash_of(subject);
        if let Some(case) = self.cases.iter().find(|case| case.hash == hash) {
            return case.target;
        }

        // Disjoint, so the first containing entry is the only one.
        if let Some(range) = self.ranges.iter().find(|r| r.contains(subject)) {
            return range.target;
        }

        self.default
    }
}

/// A fixed value hashed with the engine's hasher, so two processes can find
/// out whether their case hashes mean the same thing.
///
/// Not a checksum of the seed — the seed is not readable as a number the
/// format could compare. This is the observable consequence of it.
#[must_use]
pub fn probe() -> u64 {
    hash_of(&Dynamic::from("rhaigrain switch probe"))
}

fn hash_of(value: &Dynamic) -> u64 {
    use core::hash::{Hash, Hasher};

    let mut hasher = crate::func::get_hasher();
    value.hash(&mut hasher);
    hasher.finish()
}

/// The hash rhai's `switch` would key `value` under.
///
/// Test-only. The compiler never hashes anything: rhai's parser has already
/// grouped the arms by hash, and the hashes are all it kept.
#[cfg(test)]
fn case_hash(value: &Dynamic) -> Option<u64> {
    value.is_hashable().then(|| hash_of(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A script integer, as a subject or a case.
    ///
    /// Spelled through `INT` rather than as an `i64` literal because `only_i32`
    /// narrows it: a `Dynamic` built from the wider type there is a boxed host
    /// value, which has no hash and so matches nothing.
    fn int(value: INT) -> Dynamic {
        Dynamic::from(value)
    }

    /// A host type, which has no hash.
    #[derive(Debug, Clone)]
    struct Opaque;

    fn table(cases: &[(&Dynamic, u32)], ranges: Vec<SwitchRange>, default: u32) -> Switch {
        Switch {
            cases: cases
                .iter()
                .filter_map(|(value, target)| {
                    Some(SwitchCase {
                        hash: case_hash(value)?,
                        target: *target,
                    })
                })
                .collect(),
            ranges,
            default,
        }
    }

    #[test]
    fn a_matching_case_wins() {
        let (one, two) = (int(1), int(2));
        let table = table(&[(&one, 10), (&two, 20)], Vec::new(), 99);

        assert_eq!(table.dispatch(&one), 10);
        assert_eq!(table.dispatch(&two), 20);
        assert_eq!(table.dispatch(&int(3)), 99);
    }

    /// The distinction that makes this hashing rather than `==`: rhai does not
    /// match an integer against a float case, even though `1 == 1.0`.
    #[cfg(not(feature = "no_float"))]
    #[test]
    fn a_float_does_not_match_an_integer_case() {
        let one = int(1);
        let table = table(&[(&one, 10)], Vec::new(), 99);

        let float = Dynamic::from(1.0 as crate::FLOAT);
        // The subject has to reach the hasher for this to say anything. Built
        // from a literal `f64` it would not under `f32_float`; that is a boxed
        // host value, and it would land on the default for having no hash at
        // all rather than for hashing differently.
        assert!(float.is_hashable(), "this test needs a hashable float");
        assert_eq!(table.dispatch(&float), 99);
    }

    #[test]
    fn strings_and_characters_match_by_value() {
        let (text, ch, flag) = (
            Dynamic::from("hello"),
            Dynamic::from('x'),
            Dynamic::from(true),
        );
        let table = table(&[(&text, 10), (&ch, 20), (&flag, 30)], Vec::new(), 99);

        assert_eq!(table.dispatch(&Dynamic::from("hello")), 10);
        assert_eq!(table.dispatch(&Dynamic::from("other")), 99);
        assert_eq!(table.dispatch(&ch), 20);
        assert_eq!(table.dispatch(&flag), 30);
    }

    /// A range arm covers the reals between its bounds, not just the integers
    /// in them — which is why the check delegates to rhai's own `RangeCase`
    /// rather than comparing integers.
    #[test]
    #[cfg(not(feature = "no_float"))]
    fn a_range_catches_a_float_between_its_bounds() {
        let table = table(
            &[],
            vec![SwitchRange {
                from: 0,
                to: 10,
                inclusive: false,
                target: 20,
            }],
            99,
        );

        // The script float type, not `f64`: under `f32_float` a `Dynamic`
        // holding an `f64` is a foreign type and never matches a range arm.
        assert_eq!(table.dispatch(&Dynamic::from(5.5 as rhai::FLOAT)), 20);
        assert_eq!(
            table.dispatch(&Dynamic::from(10.0 as rhai::FLOAT)),
            99,
            "exclusive end"
        );
        assert_eq!(table.dispatch(&Dynamic::from(-0.5 as rhai::FLOAT)), 99);
    }

    /// Ranges are consulted only after the cases miss.
    #[test]
    fn a_range_catches_what_no_case_did() {
        let one = int(1);
        let table = table(
            &[(&one, 10)],
            vec![
                SwitchRange {
                    from: 5,
                    to: 8,
                    inclusive: false,
                    target: 20,
                },
                SwitchRange {
                    from: 8,
                    to: 10,
                    inclusive: true,
                    target: 30,
                },
            ],
            99,
        );

        assert_eq!(table.dispatch(&one), 10, "a case still wins");
        assert_eq!(table.dispatch(&int(5)), 20);
        assert_eq!(table.dispatch(&int(7)), 20);
        assert_eq!(table.dispatch(&int(8)), 30, "exclusive end");
        assert_eq!(table.dispatch(&int(10)), 30, "inclusive end");
        assert_eq!(table.dispatch(&int(11)), 99);
    }

    /// Hashing one would panic, so it must never reach the hasher — and it
    /// must still be able to reach the default.
    #[test]
    fn an_unhashable_subject_falls_through_rather_than_panicking() {
        let one = int(1);
        let table = table(&[(&one, 10)], Vec::new(), 99);

        // A bare function pointer *is* hashable; only one carrying an
        // environment is not. A host type is the reliable case.
        let unhashable = Dynamic::from(Opaque);
        assert!(
            !unhashable.is_hashable(),
            "this test needs an unhashable value",
        );
        assert_eq!(table.dispatch(&unhashable), 99);
    }

    #[test]
    fn an_unhashable_case_has_no_hash_to_key_on() {
        assert_eq!(case_hash(&Dynamic::from(Opaque)), None);
        assert!(case_hash(&int(1)).is_some());
    }

    /// The probe is only worth carrying if it actually depends on the seed.
    #[test]
    fn the_probe_is_stable_within_a_process() {
        assert_eq!(probe(), probe());
        assert_ne!(probe(), 0, "a probe of zero could not be told from absent");
    }
}
