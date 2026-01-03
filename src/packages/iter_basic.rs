use crate::eval::calc_index;
use crate::plugin::*;
use crate::FuncRegistration;
use crate::{def_package, ExclusiveRange, InclusiveRange, RhaiResultOf, ERR, INT, INT_BITS};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::type_name,
    cmp::Ordering,
    convert::TryFrom,
    fmt::Debug,
    iter::{ExactSizeIterator, FusedIterator},
    ops::{Range, RangeInclusive},
    vec::IntoIter,
};

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

#[cfg(feature = "decimal")]
use rust_decimal::Decimal;

#[cfg(not(feature = "unchecked"))]
#[inline(always)]
#[allow(clippy::needless_pass_by_value)]
fn std_add<T>(x: T, y: T) -> Option<T>
where
    T: num_traits::CheckedAdd<Output = T>,
{
    x.checked_add(&y)
}
#[inline(always)]
#[allow(dead_code)]
#[allow(clippy::unnecessary_wraps, clippy::needless_pass_by_value)]
fn regular_add<T>(x: T, y: T) -> Option<T>
where
    T: std::ops::Add<Output = T>,
{
    Some(x + y)
}

// Range iterator with step
#[derive(Clone)]
pub struct StepRange<T> {
    /// Start of the range.
    pub from: T,
    /// End of the range (exclusive).
    pub to: T,
    /// Step value.
    pub step: T,
    /// Increment function.
    pub add: fn(T, T) -> Option<T>,
    /// Direction of iteration.
    /// > 0 = forward, < 0 = backward, 0 = done.
    pub dir: i8,
}

impl<T: Debug> Debug for StepRange<T> {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(&format!("StepRange<{}>", type_name::<T>()))
            .field(&self.from)
            .field(&self.to)
            .field(&self.step)
            .finish()
    }
}

impl<T: Copy + PartialOrd> StepRange<T> {
    /// Create a new [`StepRange`].
    pub fn new(from: T, to: T, step: T, add: fn(T, T) -> Option<T>) -> RhaiResultOf<Self> {
        let mut dir = 0;

        if let Some(n) = add(from, step) {
            #[cfg(not(feature = "unchecked"))]
            if n == from {
                return Err(ERR::ErrorInFunctionCall(
                    "range".to_string(),
                    String::new(),
                    ERR::ErrorArithmetic("step value cannot be zero".to_string(), Position::NONE)
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

impl<T: Copy + PartialOrd> Iterator for StepRange<T> {
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

impl<T: Copy + PartialOrd> FusedIterator for StepRange<T> {}

/// Bit-field iterator with step.
///
/// Values are the base number and the number of bits to iterate.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct BitRange(INT, usize);

impl BitRange {
    /// Create a new [`BitRange`].
    pub fn new(value: INT, from: INT, len: INT) -> RhaiResultOf<Self> {
        let from = calc_index(INT_BITS, from, true, || {
            ERR::ErrorBitFieldBounds(INT_BITS, from, Position::NONE).into()
        })?;

        let len = if len < 0 {
            0
        } else if let Ok(len) = usize::try_from(len) {
            if len.checked_add(from).map(|x| x > INT_BITS).unwrap_or(true) {
                INT_BITS - from
            } else {
                len
            }
        } else {
            INT_BITS - from
        };

        Ok(Self(value >> from, len))
    }
}

impl Iterator for BitRange {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.1 == 0 {
            None
        } else {
            let r = (self.0 & 0x0001) != 0;
            self.0 >>= 1;
            self.1 -= 1;
            Some(r)
        }
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.1, Some(self.1))
    }
}

impl FusedIterator for BitRange {}

impl ExactSizeIterator for BitRange {
    #[inline(always)]
    fn len(&self) -> usize {
        self.1
    }
}

// String iterator over characters.
#[derive(Debug, Clone)]
pub struct CharsStream(IntoIter<char>);

impl CharsStream {
    /// Create a new [`CharsStream`].
    pub fn new(string: &str, from: INT, len: INT) -> Self {
        if len <= 0 {
            return Self(Vec::new().into_iter());
        }
        let len = usize::try_from(len).unwrap_or(usize::MAX);

        if from >= 0 {
            let from = usize::try_from(from).unwrap_or(usize::MAX);

            return Self(
                string
                    .chars()
                    .skip(from)
                    .take(len)
                    .collect::<Vec<_>>()
                    .into_iter(),
            );
        }

        let abs_from = usize::try_from(from.unsigned_abs()).unwrap_or(usize::MAX);
        let num_chars = string.chars().count();
        let offset = num_chars.saturating_sub(abs_from);
        Self(
            string
                .chars()
                .skip(offset)
                .take(len)
                .collect::<Vec<_>>()
                .into_iter(),
        )
    }
}

impl Iterator for CharsStream {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl FusedIterator for CharsStream {}

impl ExactSizeIterator for CharsStream {
    #[inline(always)]
    fn len(&self) -> usize {
        self.0.len()
    }
}

macro_rules! reg_range {
    ($lib:ident => $( $arg_type:ty ),*) => {
        $({
            $lib.set_iterator::<Range<$arg_type>>();

            #[export_module]
            mod range_function {
                /// Return an iterator over the exclusive range of `from..to`.
                /// The value `to` is never included.
                ///
                /// # Example
                ///
                /// ```rhai
                /// // prints all values from 8 to 17
                /// for n in range(8, 18) {
                ///     print(n);
                /// }
                /// ```
                pub const fn range (from: $arg_type, to: $arg_type) -> Range<$arg_type> {
                    from..to
                }
            }

            combine_with_exported_module!($lib, stringify!($arg_type), range_function);

            $lib.set_iterator::<RangeInclusive<$arg_type>>();

        })*
    };
    ($lib:ident |> $( $arg_type:ty ),*) => {
        #[cfg(not(feature = "unchecked"))]
        reg_range!($lib |> std_add => $( $arg_type ),*);
        #[cfg(feature = "unchecked")]
        reg_range!($lib |> regular_add => $( $arg_type ),*);
    };
    ($lib:ident |> $add:ident => $( $arg_type:ty ),*) => {
        $({
            $lib.set_iterator::<StepRange<$arg_type>>();

            #[export_module]
            mod range_functions {
                /// Return an iterator over the exclusive range of `from..to`, each iteration increasing by `step`.
                /// The value `to` is never included.
                ///
                /// If `from` > `to` and `step` < 0, iteration goes backwards.
                ///
                /// If `from` > `to` and `step` > 0 or `from` < `to` and `step` < 0, an empty iterator is returned.
                ///
                /// # Example
                ///
                /// ```rhai
                /// // prints all values from 8 to 17 in steps of 3
                /// for n in range(8, 18, 3) {
                ///     print(n);
                /// }
                ///
                /// // prints all values down from 18 to 9 in steps of -3
                /// for n in range(18, 8, -3) {
                ///     print(n);
                /// }
                /// ```
                #[rhai_fn(name = "range", return_raw)]
                pub fn range_from_to_stepped (from: $arg_type, to: $arg_type, step: $arg_type) -> RhaiResultOf<StepRange<$arg_type>> {
                    StepRange::new(from, to, step, $add)
                }
                /// Return an iterator over an exclusive range, each iteration increasing by `step`.
                ///
                /// If `range` is reversed and `step` < 0, iteration goes backwards.
                ///
                /// Otherwise, if `range` is empty, an empty iterator is returned.
                ///
                /// # Example
                ///
                /// ```rhai
                /// // prints all values from 8 to 17 in steps of 3
                /// for n in range(8..18, 3) {
                ///     print(n);
                /// }
                ///
                /// // prints all values down from 18 to 9 in steps of -3
                /// for n in range(18..8, -3) {
                ///     print(n);
                /// }
                /// ```
                #[rhai_fn(name = "range", return_raw)]
                pub fn range_stepped (range: std::ops::Range<$arg_type>, step: $arg_type) -> RhaiResultOf<StepRange<$arg_type>> {
                    StepRange::new(range.start, range.end, step, $add)
                }
            }

            combine_with_exported_module!($lib, stringify!($arg_type), range_functions);
        })*
    };
}

def_package! {
    /// Package of basic range iterators
    pub BasicIteratorPackage(lib) {
        lib.set_standard_lib(true);

        // Register iterators for standard types.
        #[cfg(not(feature = "no_index"))]
        {
            lib.set_iterable::<crate::Array>();
            lib.set_iterable::<crate::Blob>();
        }
        lib.set_iter(TypeId::of::<ImmutableString>(), |value| Box::new(
            CharsStream::new(value.cast::<ImmutableString>().as_str(), 0, INT::MAX).map(Into::into)
        ));

        // Register iterator types.
        lib.set_iterator::<CharsStream>();
        lib.set_iterator::<BitRange>();

        // Register range functions.
        reg_range!(lib => INT);

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            reg_range!(lib => i8, u8, i16, u16, i32, u32, i64, u64);

            #[cfg(not(target_family = "wasm"))]
            reg_range!(lib => i128, u128);
        }

        reg_range!(lib |> INT);

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            reg_range!(lib |> i8, u8, i16, u16, i32, u32, i64, u64);

            #[cfg(not(target_family = "wasm"))]
            reg_range!(lib |> i128, u128);
        }

        #[cfg(not(feature = "no_float"))]
        reg_range!(lib |> regular_add => FLOAT);

        #[cfg(feature = "decimal")]
        reg_range!(lib |> Decimal);

        // Register iterator functions
        combine_with_exported_module!(lib, "iterator", iterator_functions);
        combine_with_exported_module!(lib, "range", range_functions);
    }
}

#[export_module]
mod iterator_functions {
    /// Return an iterator over an exclusive range of characters in the string.
    ///
    /// # Example
    ///
    /// ```rhai
    /// for ch in "hello, world!".chars(2..5) {
    ///     print(ch);
    /// }
    /// ```
    #[rhai_fn(name = "chars")]
    pub fn chars_from_exclusive_range(string: &str, range: ExclusiveRange) -> CharsStream {
        let from = INT::max(range.start, 0);
        let to = INT::max(range.end, from);
        CharsStream::new(string, from, to - from)
    }
    /// Return an iterator over an inclusive range of characters in the string.
    ///
    /// # Example
    ///
    /// ```rhai
    /// for ch in "hello, world!".chars(2..=6) {
    ///     print(ch);
    /// }
    /// ```
    #[rhai_fn(name = "chars")]
    pub fn chars_from_inclusive_range(string: &str, range: InclusiveRange) -> CharsStream {
        let from = INT::max(*range.start(), 0);
        let to = INT::min(INT::max(*range.end(), from - 1), INT::MAX - 1);
        CharsStream::new(string, from, to - from + 1)
    }
    /// Return an iterator over a portion of characters in the string.
    ///
    /// * If `start` < 0, position counts from the end of the string (`-1` is the last character).
    /// * If `start` < -length of string, position counts from the beginning of the string.
    /// * If `start` ≥ length of string, an empty iterator is returned.
    /// * If `len` ≤ 0, an empty iterator is returned.
    /// * If `start` position + `len` ≥ length of string, all characters of the string after the `start` position are iterated.
    ///
    /// # Example
    ///
    /// ```rhai
    /// for ch in "hello, world!".chars(2, 4) {
    ///     print(ch);
    /// }
    /// ```
    #[rhai_fn(name = "chars")]
    pub fn chars_from_start_len(string: &str, start: INT, len: INT) -> CharsStream {
        CharsStream::new(string, start, len)
    }
    /// Return an iterator over the characters in the string starting from the `start` position.
    ///
    /// * If `start` < 0, position counts from the end of the string (`-1` is the last character).
    /// * If `start` < -length of string, position counts from the beginning of the string.
    /// * If `start` ≥ length of string, an empty iterator is returned.
    ///
    /// # Example
    ///
    /// ```rhai
    /// for ch in "hello, world!".chars(2) {
    ///     print(ch);
    /// }
    /// ```
    #[rhai_fn(name = "chars")]
    pub fn chars_from_start(string: &str, start: INT) -> CharsStream {
        CharsStream::new(string, start, INT::MAX)
    }
    /// Return an iterator over the characters in the string.
    ///
    /// # Example
    ///
    /// ```rhai
    /// for ch in "hello, world!".chars() {
    ///     print(ch);
    /// }
    /// ```
    #[rhai_fn(name = "chars")]
    pub fn chars(string: &str) -> CharsStream {
        CharsStream::new(string, 0, INT::MAX)
    }
    /// Return an iterator over all the characters in the string.
    ///
    /// # Example
    ///
    /// ```rhai
    /// for ch in "hello, world!".chars {"
    ///     print(ch);
    /// }
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[rhai_fn(get = "chars")]
    pub fn get_chars(string: &str) -> CharsStream {
        CharsStream::new(string, 0, INT::MAX)
    }

    /// Return an iterator over an exclusive range of bits in the number.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = 123456;
    ///
    /// for bit in x.bits(10..24) {
    ///     print(bit);
    /// }
    /// ```
    #[rhai_fn(name = "bits", return_raw)]
    pub fn bits_from_exclusive_range(value: INT, range: ExclusiveRange) -> RhaiResultOf<BitRange> {
        let from = INT::max(range.start, 0);
        let to = INT::max(range.end, from);
        BitRange::new(value, from, to - from)
    }
    /// Return an iterator over an inclusive range of bits in the number.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = 123456;
    ///
    /// for bit in x.bits(10..=23) {
    ///     print(bit);
    /// }
    /// ```
    #[rhai_fn(name = "bits", return_raw)]
    pub fn bits_from_inclusive_range(value: INT, range: InclusiveRange) -> RhaiResultOf<BitRange> {
        let from = INT::max(*range.start(), 0);
        let to = INT::min(INT::max(*range.end(), from - 1), INT::MAX - 1);
        BitRange::new(value, from, to - from + 1)
    }
    /// Return an iterator over a portion of bits in the number.
    ///
    /// * If `start` < 0, position counts from the MSB (Most Significant Bit)>.
    /// * If `len` ≤ 0, an empty iterator is returned.
    /// * If `start` position + `len` ≥ length of string, all bits of the number after the `start` position are iterated.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = 123456;
    ///
    /// for bit in x.bits(10, 8) {
    ///     print(bit);
    /// }
    /// ```
    #[rhai_fn(name = "bits", return_raw)]
    pub fn bits_from_start_and_len(value: INT, from: INT, len: INT) -> RhaiResultOf<BitRange> {
        BitRange::new(value, from, len)
    }
    /// Return an iterator over the bits in the number starting from the specified `start` position.
    ///
    /// If `start` < 0, position counts from the MSB (Most Significant Bit)>.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = 123456;
    ///
    /// for bit in x.bits(10) {
    ///     print(bit);
    /// }
    /// ```
    #[rhai_fn(name = "bits", return_raw)]
    pub fn bits_from_start(value: INT, from: INT) -> RhaiResultOf<BitRange> {
        BitRange::new(value, from, INT::MAX)
    }
    /// Return an iterator over all the bits in the number.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = 123456;
    ///
    /// for bit in x.bits() {
    ///     print(bit);
    /// }
    /// ```
    #[rhai_fn(name = "bits", return_raw)]
    pub fn bits(value: INT) -> RhaiResultOf<BitRange> {
        BitRange::new(value, 0, INT::MAX)
    }
    /// Return an iterator over all the bits in the number.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = 123456;
    ///
    /// for bit in x.bits {
    ///     print(bit);
    /// }
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[rhai_fn(get = "bits", return_raw)]
    pub fn get_bits(value: INT) -> RhaiResultOf<BitRange> {
        BitRange::new(value, 0, INT::MAX)
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
    #[allow(unstable_name_collisions)]
    pub fn is_empty_exclusive(range: &mut ExclusiveRange) -> bool {
        range.is_empty()
    }
    /// Return `true` if the range contains a specified value.
    #[rhai_fn(name = "contains")]
    pub fn contains_exclusive(range: &mut ExclusiveRange, value: INT) -> bool {
        range.contains(&value)
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
    /// Return `true` if the range contains a specified value.
    #[rhai_fn(name = "contains")]
    pub fn contains_inclusive(range: &mut InclusiveRange, value: INT) -> bool {
        range.contains(&value)
    }
}
