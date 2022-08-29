use crate::eval::calc_index;
use crate::plugin::*;
use crate::{def_package, ExclusiveRange, InclusiveRange, RhaiResultOf, INT, INT_BITS};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::type_name,
    cmp::Ordering,
    fmt::Debug,
    iter::{ExactSizeIterator, FusedIterator},
    ops::{Range, RangeInclusive},
};

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

#[cfg(feature = "decimal")]
use rust_decimal::Decimal;

#[cfg(not(feature = "unchecked"))]
#[inline(always)]
fn std_add<T>(x: T, y: T) -> Option<T>
where
    T: Debug + Copy + PartialOrd + num_traits::CheckedAdd<Output = T>,
{
    x.checked_add(&y)
}
#[inline(always)]
#[allow(dead_code)]
fn regular_add<T>(x: T, y: T) -> Option<T>
where
    T: Debug + Copy + PartialOrd + std::ops::Add<Output = T>,
{
    Some(x + y)
}

// Range iterator with step
#[derive(Clone, Hash, Eq, PartialEq)]
pub struct StepRange<T: Debug + Copy + PartialOrd> {
    pub from: T,
    pub to: T,
    pub step: T,
    pub add: fn(T, T) -> Option<T>,
    pub dir: i8,
}

impl<T: Debug + Copy + PartialOrd> Debug for StepRange<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(&format!("StepRange<{}>", type_name::<T>()))
            .field(&self.from)
            .field(&self.to)
            .field(&self.step)
            .finish()
    }
}

impl<T: Debug + Copy + PartialOrd> StepRange<T> {
    pub fn new(from: T, to: T, step: T, add: fn(T, T) -> Option<T>) -> RhaiResultOf<Self> {
        let mut dir = 0;

        if let Some(n) = add(from, step) {
            #[cfg(not(feature = "unchecked"))]
            if n == from {
                return Err(crate::ERR::ErrorInFunctionCall(
                    "range".to_string(),
                    String::new(),
                    crate::ERR::ErrorArithmetic(
                        "step value cannot be zero".to_string(),
                        Position::NONE,
                    )
                    .into(),
                    Position::NONE,
                )
                .into());
            }

            match from.partial_cmp(&to).unwrap_or(Ordering::Equal) {
                Ordering::Less if n > from => dir = 1,
                Ordering::Greater if n < from => dir = -1,
                _ => (),
            }
        }

        Ok(Self {
            from,
            to,
            step,
            add,
            dir,
        })
    }
}

impl<T: Debug + Copy + PartialOrd> Iterator for StepRange<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.dir == 0 {
            return None;
        }

        let v = self.from;

        self.from = (self.add)(self.from, self.step)?;

        match self.dir.cmp(&0) {
            Ordering::Greater if self.from >= self.to => self.dir = 0,
            Ordering::Less if self.from <= self.to => self.dir = 0,
            Ordering::Equal => unreachable!("`dir` != 0"),
            _ => (),
        }

        Some(v)
    }
}

impl<T: Debug + Copy + PartialOrd> FusedIterator for StepRange<T> {}

// Bit-field iterator with step
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct BitRange(INT, INT, usize);

impl BitRange {
    pub fn new(value: INT, from: INT, len: INT) -> RhaiResultOf<Self> {
        let from = calc_index(INT_BITS, from, true, || {
            crate::ERR::ErrorBitFieldBounds(INT_BITS, from, Position::NONE).into()
        })?;

        let len = if len < 0 {
            0
        } else if from + (len as usize) > INT_BITS {
            INT_BITS - from
        } else {
            len as usize
        };

        Ok(Self(value, 1 << from, len))
    }
}

impl Iterator for BitRange {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.2 == 0 {
            None
        } else {
            let r = (self.0 & self.1) != 0;
            self.1 <<= 1;
            self.2 -= 1;
            Some(r)
        }
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.2, Some(self.2))
    }
}

impl FusedIterator for BitRange {}

impl ExactSizeIterator for BitRange {
    #[inline(always)]
    fn len(&self) -> usize {
        self.2
    }
}

// String iterator over characters
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CharsStream(Vec<char>, usize);

impl CharsStream {
    pub fn new(string: &str, from: INT, len: INT) -> Self {
        if len <= 0 {
            return Self(Vec::new(), 0);
        }
        if from >= 0 {
            return Self(
                string
                    .chars()
                    .skip(from as usize)
                    .take(len as usize)
                    .collect(),
                0,
            );
        }

        let abs_from = from.unsigned_abs() as usize;
        let num_chars = string.chars().count();
        let offset = if num_chars < abs_from {
            0
        } else {
            num_chars - abs_from
        };
        Self(string.chars().skip(offset).take(len as usize).collect(), 0)
    }
}

impl Iterator for CharsStream {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if self.1 >= self.0.len() {
            None
        } else {
            let ch = self.0[self.1];
            self.1 += 1;
            Some(ch)
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.0.len() - self.1;
        (remaining, Some(remaining))
    }
}

impl FusedIterator for CharsStream {}

impl ExactSizeIterator for CharsStream {
    #[inline]
    fn len(&self) -> usize {
        self.0.len() - self.1
    }
}

macro_rules! reg_range {
    ($lib:ident | $x:expr => $( $y:ty ),*) => {
        $(
            $lib.set_iterator::<Range<$y>>();
            let _hash = $lib.set_native_fn($x, |from: $y, to: $y| Ok(from..to));

            #[cfg(feature = "metadata")]
            $lib.update_fn_metadata_with_comments(_hash, [
                    concat!("from: ", stringify!($y)),
                    concat!("to: ", stringify!($y)),
                    concat!("Iterator<", stringify!($y), ">"),
            ], [
                "/// Return an iterator over the exclusive range of `from..to`.",
                "/// The value `to` is never included.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                "/// // prints all values from 8 to 17",
                "/// for n in range(8, 18) {",
                "///     print(n);",
                "/// }",
                "/// ```"
            ]);

            $lib.set_iterator::<RangeInclusive<$y>>();
        )*
    };
    ($lib:ident | step $x:expr => $( $y:ty ),*) => {
        #[cfg(not(feature = "unchecked"))]
        reg_range!($lib | step(std_add) $x => $( $y ),*);
        #[cfg(feature = "unchecked")]
        reg_range!($lib | step(regular_add) $x => $( $y ),*);
    };
    ($lib:ident | step ( $add:ident ) $x:expr => $( $y:ty ),*) => {
        $(
            $lib.set_iterator::<StepRange<$y>>();
            let _hash = $lib.set_native_fn($x, |from: $y, to: $y, step: $y| StepRange::new(from, to, step, $add));

            #[cfg(feature = "metadata")]
            $lib.update_fn_metadata_with_comments(_hash, [
                    concat!("from: ", stringify!($y)),
                    concat!("to: ", stringify!($y)),
                    concat!("step: ", stringify!($y)),
                    concat!("Iterator<", stringify!($y), ">")
            ], [
                "/// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.",
                "/// The value `to` is never included.",
                "///",
                "/// If `from` > `to` and `step` < 0, iteration goes backwards.",
                "///",
                "/// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                "/// // prints all values from 8 to 17 in steps of 3",
                "/// for n in range(8, 18, 3) {",
                "///     print(n);",
                "/// }",
                "///",
                "/// // prints all values down from 18 to 9 in steps of -3",
                "/// for n in range(18, 8, -3) {",
                "///     print(n);",
                "/// }",
                "/// ```"
            ]);

            let _hash = $lib.set_native_fn($x, |range: std::ops::Range<$y>, step: $y| StepRange::new(range.start, range.end, step, $add));

            #[cfg(feature = "metadata")]
            $lib.update_fn_metadata_with_comments(_hash, [
                    concat!("range: Range<", stringify!($y), ">"),
                    concat!("step: ", stringify!($y)),
                    concat!("Iterator<", stringify!($y), ">")
            ], [
                "/// Return an iterator over an exclusive range, each iteration increasing by `step`.",
                "///",
                "/// If `range` is reversed and `step` < 0, iteration goes backwards.",
                "///",
                "/// Otherwise, if `range` is empty, an empty iterator is returned.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                "/// // prints all values from 8 to 17 in steps of 3",
                "/// for n in range(8..18, 3) {",
                "///     print(n);",
                "/// }",
                "///",
                "/// // prints all values down from 18 to 9 in steps of -3",
                "/// for n in range(18..8, -3) {",
                "///     print(n);",
                "/// }",
                "/// ```"
            ]);
        )*
    };
}

def_package! {
    /// Package of basic range iterators
    pub BasicIteratorPackage(lib) {
        lib.standard = true;

        reg_range!(lib | "range" => INT);

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            reg_range!(lib | "range" => i8, u8, i16, u16, i32, u32, i64, u64);

            #[cfg(not(target_family = "wasm"))]
            reg_range!(lib | "range" => i128, u128);
        }

        reg_range!(lib | step "range" => INT);

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            reg_range!(lib | step "range" => i8, u8, i16, u16, i32, u32, i64, u64);

            #[cfg(not(target_family = "wasm"))]
            reg_range!(lib | step "range" => i128, u128);
        }

        #[cfg(not(feature = "no_float"))]
        reg_range!(lib | step(regular_add) "range" => FLOAT);

        #[cfg(feature = "decimal")]
        reg_range!(lib | step "range" => Decimal);

        // Register string iterator
        lib.set_iterator::<CharsStream>();

        #[cfg(feature = "metadata")]
        let (range_type, range_inclusive_type) = (
            format!("range: Range<{}>", type_name::<INT>()),
            format!("range: RangeInclusive<{}>", type_name::<INT>()),
        );

        let _hash = lib.set_native_fn("chars", |string, range: ExclusiveRange| {
            let from = INT::max(range.start, 0);
            let to = INT::max(range.end, from);
            Ok(CharsStream::new(string, from, to - from))
        });
        #[cfg(feature = "metadata")]
        lib.update_fn_metadata_with_comments(
            _hash,
            ["string: &str", &range_type, "Iterator<char>"],
            [
                "/// Return an iterator over an exclusive range of characters in the string.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                r#"/// for ch in "hello, world!".chars(2..5) {"#,
                "///     print(ch);",
                "/// }",
                "/// ```"
        ]
        );

        let _hash = lib.set_native_fn("chars", |string, range: InclusiveRange| {
            let from = INT::max(*range.start(), 0);
            let to = INT::max(*range.end(), from - 1);
            Ok(CharsStream::new(string, from, to-from + 1))
        });
        #[cfg(feature = "metadata")]
        lib.update_fn_metadata_with_comments(
            _hash,
            ["string: &str", &range_inclusive_type, "Iterator<char>"],
            [
                "/// Return an iterator over an inclusive range of characters in the string.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                r#"/// for ch in "hello, world!".chars(2..=6) {"#,
                "///     print(ch);",
                "/// }",
                "/// ```"
            ]
        );

        let _hash = lib.set_native_fn("chars", |string, from, len| Ok(CharsStream::new(string, from, len)));
        #[cfg(feature = "metadata")]
        lib.update_fn_metadata_with_comments(
            _hash,
            ["string: &str", "start: INT", "len: INT", "Iterator<char>"],
            [
                "/// Return an iterator over a portion of characters in the string.",
                "///",
                "/// * If `start` < 0, position counts from the end of the string (`-1` is the last character).",
                "/// * If `start` < -length of string, position counts from the beginning of the string.",
                "/// * If `start` ≥ length of string, an empty iterator is returned.",
                "/// * If `len` ≤ 0, an empty iterator is returned.",
                "/// * If `start` position + `len` ≥ length of string, all characters of the string after the `start` position are iterated.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                r#"/// for ch in "hello, world!".chars(2, 4) {"#,
                "///     print(ch);",
                "/// }",
                "/// ```"
            ]
        );

        let _hash = lib.set_native_fn("chars", |string, from| Ok(CharsStream::new(string, from, INT::MAX)));
        #[cfg(feature = "metadata")]
        lib.update_fn_metadata_with_comments(
            _hash,
            ["string: &str", "from: INT", "Iterator<char>"],
            [
                "/// Return an iterator over the characters in the string starting from the `start` position.",
                "///",
                "/// * If `start` < 0, position counts from the end of the string (`-1` is the last character).",
                "/// * If `start` < -length of string, position counts from the beginning of the string.",
                "/// * If `start` ≥ length of string, an empty iterator is returned.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                r#"/// for ch in "hello, world!".chars(2) {"#,
                "///     print(ch);",
                "/// }",
                "/// ```"
            ]
        );

        let _hash = lib.set_native_fn("chars", |string| Ok(CharsStream::new(string, 0, INT::MAX)));
        #[cfg(feature = "metadata")]
        lib.update_fn_metadata_with_comments(
            _hash,
            ["string: &str", "Iterator<char>"],
            [
                "/// Return an iterator over the characters in the string.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                r#"/// for ch in "hello, world!".chars() {"#,
                "///     print(ch);",
                "/// }",
                "/// ```"
            ]
        );

        #[cfg(not(feature = "no_object"))]
        {
            let _hash = lib.set_getter_fn("chars", |string: &mut ImmutableString| Ok(CharsStream::new(string, 0, INT::MAX)));
            #[cfg(feature = "metadata")]
            lib.update_fn_metadata_with_comments(
                _hash,
                ["string: &mut ImmutableString", "Iterator<char>"],
                [
                    "/// Return an iterator over all the characters in the string.",
                    "///",
                    "/// # Example",
                    "///",
                    "/// ```rhai",
                    r#"/// for ch in "hello, world!".chars {"#,
                    "///     print(ch);",
                    "/// }",
                    "/// ```"
                    ]
            );
        }

        // Register bit-field iterator
        lib.set_iterator::<BitRange>();

        let _hash = lib.set_native_fn("bits", |value, range: ExclusiveRange| {
            let from = INT::max(range.start, 0);
            let to = INT::max(range.end, from);
            BitRange::new(value, from, to - from)
        });
        #[cfg(feature = "metadata")]
        lib.update_fn_metadata_with_comments(
            _hash,
            ["value: INT", &range_type, "Iterator<bool>"],
            [
                "/// Return an iterator over an exclusive range of bits in the number.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                "/// let x = 123456;",
                "///",
                "/// for bit in x.bits(10..24) {",
                "///     print(bit);",
                "/// }",
                "/// ```"
            ]
        );

        let _hash = lib.set_native_fn("bits", |value, range: InclusiveRange| {
            let from = INT::max(*range.start(), 0);
            let to = INT::max(*range.end(), from - 1);
            BitRange::new(value, from, to - from + 1)
        });
        #[cfg(feature = "metadata")]
        lib.update_fn_metadata_with_comments(
            _hash,
            ["value: INT", &range_inclusive_type, "Iterator<bool>"],
            [
                "/// Return an iterator over an inclusive range of bits in the number.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                "/// let x = 123456;",
                "///",
                "/// for bit in x.bits(10..=23) {",
                "///     print(bit);",
                "/// }",
                "/// ```"
            ]
        );

        let _hash = lib.set_native_fn("bits", BitRange::new);
        #[cfg(feature = "metadata")]
        lib.update_fn_metadata_with_comments(
            _hash,
            ["value: INT", "from: INT", "len: INT", "Iterator<bool>"],
            [
                "/// Return an iterator over a portion of bits in the number.",
                "///",
                "/// * If `start` < 0, position counts from the MSB (Most Significant Bit)>.",
                "/// * If `len` ≤ 0, an empty iterator is returned.",
                "/// * If `start` position + `len` ≥ length of string, all bits of the number after the `start` position are iterated.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                "/// let x = 123456;",
                "///",
                "/// for bit in x.bits(10, 8) {",
                "///     print(bit);",
                "/// }",
                "/// ```"
            ]
        );

        let _hash = lib.set_native_fn("bits", |value, from| BitRange::new(value, from, INT::MAX));
        #[cfg(feature = "metadata")]
        lib.update_fn_metadata_with_comments(
            _hash,
            ["value: INT", "from: INT", "Iterator<bool>"],
            [
                "/// Return an iterator over the bits in the number starting from the specified `start` position.",
                "///",
                "/// If `start` < 0, position counts from the MSB (Most Significant Bit)>.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                "/// let x = 123456;",
                "///",
                "/// for bit in x.bits(10) {",
                "///     print(bit);",
                "/// }",
                "/// ```"
            ]
        );

        let _hash = lib.set_native_fn("bits", |value| BitRange::new(value, 0, INT::MAX) );
        #[cfg(feature = "metadata")]
        lib.update_fn_metadata_with_comments(
            _hash,
            ["value: INT", "Iterator<bool>"],
            [
                "/// Return an iterator over all the bits in the number.",
                "///",
                "/// # Example",
                "///",
                "/// ```rhai",
                "/// let x = 123456;",
                "///",
                "/// for bit in x.bits() {",
                "///     print(bit);",
                "/// }",
                "/// ```"
            ]
        );

        #[cfg(not(feature = "no_object"))]
        {
            let _hash = lib.set_getter_fn("bits", |value: &mut INT| BitRange::new(*value, 0, INT::MAX) );
            #[cfg(feature = "metadata")]
            lib.update_fn_metadata_with_comments(
                _hash,
                ["value: &mut INT", "Iterator<bool>"],
                [
                    "/// Return an iterator over all the bits in the number.",
                    "///",
                    "/// # Example",
                    "///",
                    "/// ```rhai",
                    "/// let x = 123456;",
                    "///",
                    "/// for bit in x.bits {",
                    "///     print(bit);",
                    "/// }",
                    "/// ```"
                    ]
            );
        }

        combine_with_exported_module!(lib, "range", range_functions);
    }
}

#[export_module]
mod range_functions {
    /// Return the start of the exclusive range.
    #[rhai_fn(get = "start", name = "start", pure)]
    pub fn start(range: &mut ExclusiveRange) -> INT {
        range.start
    }
    /// Return the end of the exclusive range.
    #[rhai_fn(get = "end", name = "end", pure)]
    pub fn end(range: &mut ExclusiveRange) -> INT {
        range.end
    }
    /// Return `true` if the range is inclusive.
    #[rhai_fn(get = "is_inclusive", name = "is_inclusive", pure)]
    pub fn is_inclusive(range: &mut ExclusiveRange) -> bool {
        let _ = range;
        false
    }
    /// Return `true` if the range is exclusive.
    #[rhai_fn(get = "is_exclusive", name = "is_exclusive", pure)]
    pub fn is_exclusive(range: &mut ExclusiveRange) -> bool {
        let _ = range;
        true
    }
    /// Return true if the range contains no items.
    #[rhai_fn(get = "is_empty", name = "is_empty", pure)]
    pub fn is_empty_exclusive(range: &mut ExclusiveRange) -> bool {
        range.is_empty()
    }
    /// Return the start of the inclusive range.
    #[rhai_fn(get = "start", name = "start", pure)]
    pub fn start_inclusive(range: &mut InclusiveRange) -> INT {
        *range.start()
    }
    /// Return the end of the inclusive range.
    #[rhai_fn(get = "end", name = "end", pure)]
    pub fn end_inclusive(range: &mut InclusiveRange) -> INT {
        *range.end()
    }
    /// Return `true` if the range is inclusive.
    #[rhai_fn(get = "is_inclusive", name = "is_inclusive", pure)]
    pub fn is_inclusive_inclusive(range: &mut InclusiveRange) -> bool {
        let _ = range;
        true
    }
    /// Return `true` if the range is exclusive.
    #[rhai_fn(get = "is_exclusive", name = "is_exclusive", pure)]
    pub fn is_exclusive_inclusive(range: &mut InclusiveRange) -> bool {
        let _ = range;
        false
    }
    /// Return true if the range contains no items.
    #[rhai_fn(get = "is_empty", name = "is_empty", pure)]
    pub fn is_empty_inclusive(range: &mut InclusiveRange) -> bool {
        range.is_empty()
    }
}
