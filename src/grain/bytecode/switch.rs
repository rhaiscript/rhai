use crate::func::{calc_switch_value_hash, StraightHashMap};
use crate::grain::Program;
use crate::{eval::RangeCase, Dynamic, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// A `switch`, as one dispatch table.
///
/// Matching a case is *hash* equality, not `==`. That distinction is Rhai's
/// and it is visible: `switch 1 { 1.0 => .. }` does not match, because an
/// integer and a float hash differently, while `1 == 1.0` is true.
#[derive(Debug, Clone)]
pub struct Switch {
    /// One entry per distinct case value.
    /// The tuple is the head of that value's chain of guarded arms, and the
    /// value itself.
    pub cases: Option<StraightHashMap<(u32, u32)>>,
    /// Checked only when no case matched in this table.
    ///
    /// Disjoint and in ascending order, which Rhai's are not: the compiler
    /// splits overlapping arms apart so that the first entry containing a
    /// value is the only one that can match it. See `compile::cases`.
    pub ranges: Vec<SwitchRange>,
    /// Where to go when nothing matched. Always present: an absent `_` arm
    /// compiles to a jump past the statement.
    pub default: u32,
}

impl Switch {
    /// Dump the disassembly of the operation.
    pub fn disassemble(&self, program: &Program) -> String {
        format!(
            "{{ {} }}",
            self.cases
                .iter()
                .flat_map(|cases| cases.iter())
                .map(|(_, (target, value))| format!(
                    "case {} => {target}",
                    program.constant(*value).unwrap()
                ))
                .chain(self.ranges.iter().map(|r| r.disassemble()))
                .chain(std::iter::once(format!("default => {}", self.default)))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
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
    /// Dump the disassembly of the switch range.
    pub fn disassemble(&self) -> String {
        if self.inclusive {
            format!("range {}..={} => {}", self.from, self.to, self.target)
        } else {
            format!("range {}..{} => {}", self.from, self.to, self.target)
        }
    }
}

impl SwitchRange {
    /// Whether a subject falls in this range.    ///
    /// Delegates to Rhai's own `RangeCase` rather than comparing integers,
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
    /// The order is Rhai's table order (`eval/stmt.rs:517-564`): reject
    /// non-hashable subjects, then try hashed cases, then ranges, then default.
    #[must_use]
    pub fn dispatch(&self, subject: &Dynamic) -> u32 {
        // Hashing an non-hashable value panics, so this is a guard and not an
        // optimization.
        if !subject.is_hashable() {
            return self.default;
        }

        let hash = calc_switch_value_hash(subject);
        if let Some(cases) = &self.cases {
            if let Some(&(target, _)) = cases.get(&hash) {
                return target;
            }
        }

        // Disjoint, so the first containing entry is the only one.
        if let Some(range) = self.ranges.iter().find(|r| r.contains(subject)) {
            return range.target;
        }

        self.default
    }
}

/// The hash Rhai's `switch` would key `value` under.
#[cfg(test)]
fn case_hash(value: &Dynamic) -> Option<u64> {
    value.is_hashable().then(|| calc_switch_value_hash(value))
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
            cases: Some(
                cases
                    .iter()
                    .filter_map(|(value, target)| Some((case_hash(value)?, (*target, 42))))
                    .collect(),
            ),
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

    /// The distinction that makes this hashing rather than `==`: Rhai does not
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
    /// in them — which is why the check delegates to Rhai's own `RangeCase`
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
    fn a_non_hashable_subject_falls_through_rather_than_panicking() {
        let one = int(1);
        let table = table(&[(&one, 10)], Vec::new(), 99);
        let non_hashable = Dynamic::from(Opaque);
        assert!(
            !non_hashable.is_hashable(),
            "this test needs an non-hashable value",
        );
        assert_eq!(table.dispatch(&non_hashable), 99);
    }

    #[test]
    fn a_non_hashable_case_has_no_hash_to_key_on() {
        assert_eq!(case_hash(&Dynamic::from(Opaque)), None);
        assert!(case_hash(&int(1)).is_some());
    }
}
