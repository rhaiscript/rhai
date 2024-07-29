#![cfg(not(feature = "no_index"))]

use crate::api::deprecated::deprecated_array_functions;
use crate::engine::OP_EQUALS;
use crate::eval::{calc_index, calc_offset_len};
use crate::plugin::*;
use crate::{
    def_package, Array, Dynamic, ExclusiveRange, FnPtr, InclusiveRange, NativeCallContext,
    Position, RhaiResultOf, ERR, INT, MAX_USIZE_INT,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{any::TypeId, cmp::Ordering, mem};

def_package! {
    /// Package of basic array utilities.
    pub BasicArrayPackage(lib) {
        lib.set_standard_lib(true);

        combine_with_exported_module!(lib, "array", array_functions);
        combine_with_exported_module!(lib, "deprecated_array", deprecated_array_functions);

        // Register array iterator
        lib.set_iterable::<Array>();
    }
}

#[export_module]
pub mod array_functions {
    /// Number of elements in the array.
    #[rhai_fn(name = "len", get = "len", pure)]
    pub fn len(array: &mut Array) -> INT {
        array.len() as INT
    }
    /// Return true if the array is empty.
    #[rhai_fn(name = "is_empty", get = "is_empty", pure)]
    pub fn is_empty(array: &mut Array) -> bool {
        array.len() == 0
    }
    /// Get a copy of the element at the `index` position in the array.
    ///
    /// * If `index` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `index` < -length of array, `()` is returned.
    /// * If `index` ≥ length of array, `()` is returned.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3];
    ///
    /// print(x.get(0));        // prints 1
    ///
    /// print(x.get(-1));       // prints 3
    ///
    /// print(x.get(99));       // prints empty (for '()')
    /// ```
    pub fn get(array: &mut Array, index: INT) -> Dynamic {
        if array.is_empty() {
            return Dynamic::UNIT;
        }

        let (index, ..) = calc_offset_len(array.len(), index, 0);

        if index >= array.len() {
            return Dynamic::UNIT;
        }

        array[index].clone()
    }
    /// Set the element at the `index` position in the array to a new `value`.
    ///
    /// * If `index` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `index` < -length of array, the array is not modified.
    /// * If `index` ≥ length of array, the array is not modified.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3];
    ///
    /// x.set(0, 42);
    ///
    /// print(x);           // prints "[42, 2, 3]"
    ///
    /// x.set(-3, 0);
    ///
    /// print(x);           // prints "[0, 2, 3]"
    ///
    /// x.set(99, 123);
    ///
    /// print(x);           // prints "[0, 2, 3]"
    /// ```
    pub fn set(array: &mut Array, index: INT, value: Dynamic) {
        if array.is_empty() {
            return;
        }

        let (index, ..) = calc_offset_len(array.len(), index, 0);

        if index >= array.len() {
            return;
        }

        array[index] = value;
    }
    /// Add a new element, which is not another array, to the end of the array.
    ///
    /// If `item` is `Array`, then `append` is more specific and will be called instead.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3];
    ///
    /// x.push("hello");
    ///
    /// print(x);       // prints [1, 2, 3, "hello"]
    /// ```
    pub fn push(array: &mut Array, item: Dynamic) {
        array.push(item);
    }
    /// Add all the elements of another array to the end of the array.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3];
    /// let y = [true, 'x'];
    ///
    /// x.append(y);
    ///
    /// print(x);       // prints "[1, 2, 3, true, 'x']"
    /// ```
    pub fn append(array: &mut Array, new_array: Array) {
        if new_array.is_empty() {
            return;
        }

        if array.is_empty() {
            *array = new_array;
        } else {
            array.extend(new_array);
        }
    }
    /// Combine two arrays into a new array and return it.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3];
    /// let y = [true, 'x'];
    ///
    /// print(x + y);   // prints "[1, 2, 3, true, 'x']"
    ///
    /// print(x);       // prints "[1, 2, 3"
    /// ```
    #[rhai_fn(name = "+")]
    pub fn concat(array1: Array, array2: Array) -> Array {
        if array2.is_empty() {
            return array1;
        }

        if array1.is_empty() {
            return array2;
        }

        let mut array = array1;
        array.extend(array2);
        array
    }
    /// Add a new element into the array at a particular `index` position.
    ///
    /// * If `index` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `index` < -length of array, the element is added to the beginning of the array.
    /// * If `index` ≥ length of array, the element is appended to the end of the array.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3];
    ///
    /// x.insert(0, "hello");
    ///
    /// x.insert(2, true);
    ///
    /// x.insert(-2, 42);
    ///
    /// print(x);       // prints ["hello", 1, true, 2, 42, 3]
    /// ```
    pub fn insert(array: &mut Array, index: INT, item: Dynamic) {
        if array.is_empty() {
            array.push(item);
            return;
        }

        let (index, ..) = calc_offset_len(array.len(), index, 0);

        if index >= array.len() {
            array.push(item);
        } else {
            array.insert(index, item);
        }
    }
    /// Pad the array to at least the specified length with copies of a specified element.
    ///
    /// If `len` ≤ length of array, no padding is done.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3];
    ///
    /// x.pad(5, 42);
    ///
    /// print(x);       // prints "[1, 2, 3, 42, 42]"
    ///
    /// x.pad(3, 123);
    ///
    /// print(x);       // prints "[1, 2, 3, 42, 42]"
    /// ```
    #[rhai_fn(return_raw)]
    pub fn pad(
        ctx: NativeCallContext,
        array: &mut Array,
        len: INT,
        item: Dynamic,
    ) -> RhaiResultOf<()> {
        if len <= 0 {
            return Ok(());
        }

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let len = len.min(MAX_USIZE_INT) as usize;

        if len <= array.len() {
            return Ok(());
        }

        let _ctx = ctx;

        // Check if array will be over max size limit
        #[cfg(not(feature = "unchecked"))]
        if _ctx.engine().max_array_size() > 0 {
            let pad = len - array.len();
            let (a, m, s) = crate::eval::calc_array_sizes(array);
            let (ax, mx, sx) = crate::eval::calc_data_sizes(&item, true);

            _ctx.engine()
                .throw_on_size((a + pad + ax * pad, m + mx * pad, s + sx * pad))?;
        }

        array.resize(len, item);

        Ok(())
    }
    /// Remove the last element from the array and return it.
    ///
    /// If the array is empty, `()` is returned.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3];
    ///
    /// print(x.pop());     // prints 3
    ///
    /// print(x);           // prints "[1, 2]"
    /// ```
    pub fn pop(array: &mut Array) -> Dynamic {
        if array.is_empty() {
            return Dynamic::UNIT;
        }

        array.pop().unwrap_or(Dynamic::UNIT)
    }
    /// Remove the first element from the array and return it.
    ///
    /// If the array is empty, `()` is returned.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3];
    ///
    /// print(x.shift());   // prints 1
    ///
    /// print(x);           // prints "[2, 3]"
    /// ```
    pub fn shift(array: &mut Array) -> Dynamic {
        if array.is_empty() {
            return Dynamic::UNIT;
        }

        array.remove(0)
    }
    /// Remove the element at the specified `index` from the array and return it.
    ///
    /// * If `index` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `index` < -length of array, `()` is returned.
    /// * If `index` ≥ length of array, `()` is returned.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3];
    ///
    /// print(x.remove(1));     // prints 2
    ///
    /// print(x);               // prints "[1, 3]"
    ///
    /// print(x.remove(-2));    // prints 1
    ///
    /// print(x);               // prints "[3]"
    /// ```
    pub fn remove(array: &mut Array, index: INT) -> Dynamic {
        let Ok(index) = calc_index(array.len(), index, true, || Err(())) else {
            return Dynamic::UNIT;
        };

        array.remove(index)
    }
    /// Clear the array.
    pub fn clear(array: &mut Array) {
        if array.is_empty() {
            return;
        }

        array.clear();
    }
    /// Cut off the array at the specified length.
    ///
    /// * If `len` ≤ 0, the array is cleared.
    /// * If `len` ≥ length of array, the array is not truncated.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// x.truncate(3);
    ///
    /// print(x);       // prints "[1, 2, 3]"
    ///
    /// x.truncate(10);
    ///
    /// print(x);       // prints "[1, 2, 3]"
    /// ```
    pub fn truncate(array: &mut Array, len: INT) {
        if len <= 0 {
            array.clear();
            return;
        }
        if array.is_empty() {
            return;
        }

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let len = len.min(MAX_USIZE_INT) as usize;

        if len > 0 {
            array.truncate(len);
        } else {
            array.clear();
        }
    }
    /// Cut off the head of the array, leaving a tail of the specified length.
    ///
    /// * If `len` ≤ 0, the array is cleared.
    /// * If `len` ≥ length of array, the array is not modified.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// x.chop(3);
    ///
    /// print(x);       // prints "[3, 4, 5]"
    ///
    /// x.chop(10);
    ///
    /// print(x);       // prints "[3, 4, 5]"
    /// ```
    pub fn chop(array: &mut Array, len: INT) {
        if len <= 0 {
            array.clear();
            return;
        }
        if array.is_empty() {
            return;
        }

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let len = len.min(MAX_USIZE_INT) as usize;

        if len == 0 {
            array.clear();
        } else if len < array.len() {
            array.drain(0..array.len() - len);
        }
    }
    /// Reverse all the elements in the array.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// x.reverse();
    ///
    /// print(x);       // prints "[5, 4, 3, 2, 1]"
    /// ```
    pub fn reverse(array: &mut Array) {
        if array.is_empty() {
            return;
        }

        array.reverse();
    }
    /// Replace an exclusive range of the array with another array.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    /// let y = [7, 8, 9, 10];
    ///
    /// x.splice(1..3, y);
    ///
    /// print(x);       // prints "[1, 7, 8, 9, 10, 4, 5]"
    /// ```
    #[rhai_fn(name = "splice")]
    pub fn splice_range(array: &mut Array, range: ExclusiveRange, replace: Array) {
        let start = INT::max(range.start, 0);
        let end = INT::max(range.end, start);
        splice(array, start, end - start, replace);
    }
    /// Replace an inclusive range of the array with another array.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    /// let y = [7, 8, 9, 10];
    ///
    /// x.splice(1..=3, y);
    ///
    /// print(x);       // prints "[1, 7, 8, 9, 10, 5]"
    /// ```
    #[rhai_fn(name = "splice")]
    pub fn splice_inclusive_range(array: &mut Array, range: InclusiveRange, replace: Array) {
        let start = INT::max(*range.start(), 0);
        let end = INT::max(*range.end(), start);
        splice(array, start, end - start + 1, replace);
    }
    /// Replace a portion of the array with another array.
    ///
    /// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `start` < -length of array, position counts from the beginning of the array.
    /// * If `start` ≥ length of array, the other array is appended to the end of the array.
    /// * If `len` ≤ 0, the other array is inserted into the array at the `start` position without replacing any element.
    /// * If `start` position + `len` ≥ length of array, entire portion of the array after the `start` position is replaced.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    /// let y = [7, 8, 9, 10];
    ///
    /// x.splice(1, 2, y);
    ///
    /// print(x);       // prints "[1, 7, 8, 9, 10, 4, 5]"
    ///
    /// x.splice(-5, 4, y);
    ///
    /// print(x);       // prints "[1, 7, 7, 8, 9, 10, 5]"
    /// ```
    pub fn splice(array: &mut Array, start: INT, len: INT, replace: Array) {
        if array.is_empty() {
            *array = replace;
            return;
        }

        let (start, len) = calc_offset_len(array.len(), start, len);

        if start >= array.len() {
            array.extend(replace);
        } else {
            array.splice(start..start + len, replace);
        }
    }
    /// Copy an exclusive range of the array and return it as a new array.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// print(x.extract(1..3));     // prints "[2, 3]"
    ///
    /// print(x);                   // prints "[1, 2, 3, 4, 5]"
    /// ```
    #[rhai_fn(name = "extract")]
    pub fn extract_range(array: &mut Array, range: ExclusiveRange) -> Array {
        let start = INT::max(range.start, 0);
        let end = INT::max(range.end, start);
        extract(array, start, end - start)
    }
    /// Copy an inclusive range of the array and return it as a new array.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// print(x.extract(1..=3));    // prints "[2, 3, 4]"
    ///
    /// print(x);                   // prints "[1, 2, 3, 4, 5]"
    /// ```
    #[rhai_fn(name = "extract")]
    pub fn extract_inclusive_range(array: &mut Array, range: InclusiveRange) -> Array {
        let start = INT::max(*range.start(), 0);
        let end = INT::max(*range.end(), start);
        extract(array, start, end - start + 1)
    }
    /// Copy a portion of the array and return it as a new array.
    ///
    /// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `start` < -length of array, position counts from the beginning of the array.
    /// * If `start` ≥ length of array, an empty array is returned.
    /// * If `len` ≤ 0, an empty array is returned.
    /// * If `start` position + `len` ≥ length of array, entire portion of the array after the `start` position is copied and returned.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// print(x.extract(1, 3));     // prints "[2, 3, 4]"
    ///
    /// print(x.extract(-3, 2));    // prints "[3, 4]"
    ///
    /// print(x);                   // prints "[1, 2, 3, 4, 5]"
    /// ```
    pub fn extract(array: &mut Array, start: INT, len: INT) -> Array {
        if array.is_empty() || len <= 0 {
            return Array::new();
        }

        let (start, len) = calc_offset_len(array.len(), start, len);

        if len == 0 {
            return Array::new();
        }

        array[start..start + len].to_vec()
    }
    /// Copy a portion of the array beginning at the `start` position till the end and return it as
    /// a new array.
    ///
    /// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `start` < -length of array, the entire array is copied and returned.
    /// * If `start` ≥ length of array, an empty array is returned.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// print(x.extract(2));        // prints "[3, 4, 5]"
    ///
    /// print(x.extract(-3));       // prints "[3, 4, 5]"
    ///
    /// print(x);                   // prints "[1, 2, 3, 4, 5]"
    /// ```
    #[rhai_fn(name = "extract")]
    pub fn extract_tail(array: &mut Array, start: INT) -> Array {
        extract(array, start, INT::MAX)
    }
    /// Cut off the array at `index` and return it as a new array.
    ///
    /// * If `index` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `index` is zero, the entire array is cut and returned.
    /// * If `index` < -length of array, the entire array is cut and returned.
    /// * If `index` ≥ length of array, nothing is cut from the array and an empty array is returned.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.split(2);
    ///
    /// print(y);           // prints "[3, 4, 5]"
    ///
    /// print(x);           // prints "[1, 2]"
    /// ```
    #[rhai_fn(name = "split")]
    pub fn split_at(array: &mut Array, index: INT) -> Array {
        if array.is_empty() {
            return Array::new();
        }

        let (start, len) = calc_offset_len(array.len(), index, INT::MAX);

        if start >= array.len() {
            return Array::new();
        }

        if start == 0 {
            if len >= array.len() {
                return mem::take(array);
            }

            let mut result = Array::new();
            result.extend(array.drain(array.len() - len..));
            return result;
        }

        let mut result = Array::new();
        result.extend(array.drain(start..));
        result
    }

    /// Iterate through all the elements in the array, applying a `process` function to each element in turn.
    /// Each element is bound to `this` before calling the function.
    ///
    /// # Function Parameters
    ///
    /// * `this`: bound to array element (mutable)
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// x.for_each(|| this *= this);
    ///
    /// print(x);       // prints "[1, 4, 9, 16, 25]"
    ///
    /// x.for_each(|i| this *= i);
    ///
    /// print(x);       // prints "[0, 2, 6, 12, 20]"
    /// ```
    #[rhai_fn(return_raw)]
    pub fn for_each(ctx: NativeCallContext, array: &mut Array, map: FnPtr) -> RhaiResultOf<()> {
        if array.is_empty() {
            return Ok(());
        }

        for (i, item) in array.iter_mut().enumerate() {
            let ex = [(i as INT).into()];

            let _ = map.call_raw_with_extra_args("map", &ctx, Some(item), [], ex, None)?;
        }

        Ok(())
    }

    /// Iterate through all the elements in the array, applying a `mapper` function to each element
    /// in turn, and return the results as a new array.
    ///
    /// # No Function Parameter
    ///
    /// Array element (mutable) is bound to `this`.
    ///
    /// This method is marked _pure_; the `mapper` function should not mutate array elements.
    ///
    /// # Function Parameters
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.map(|v| v * v);
    ///
    /// print(y);       // prints "[1, 4, 9, 16, 25]"
    ///
    /// let y = x.map(|v, i| v * i);
    ///
    /// print(y);       // prints "[0, 2, 6, 12, 20]"
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn map(ctx: NativeCallContext, array: &mut Array, map: FnPtr) -> RhaiResultOf<Array> {
        if array.is_empty() {
            return Ok(Array::new());
        }

        let mut ar = Array::with_capacity(array.len());

        for (i, item) in array.iter_mut().enumerate() {
            let ex = [(i as INT).into()];
            ar.push(map.call_raw_with_extra_args("map", &ctx, Some(item), [], ex, Some(0))?);
        }

        Ok(ar)
    }

    /// Iterate through all the elements in the array, applying a `filter` function to each element
    /// in turn, and return a copy of all elements (in order) that return `true` as a new array.
    ///
    /// # No Function Parameter
    ///
    /// Array element (mutable) is bound to `this`.
    ///
    /// This method is marked _pure_; the `filter` function should not mutate array elements.
    ///
    /// # Function Parameters
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.filter(|v| v >= 3);
    ///
    /// print(y);       // prints "[3, 4, 5]"
    ///
    /// let y = x.filter(|v, i| v * i >= 10);
    ///
    /// print(y);       // prints "[12, 20]"
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn filter(ctx: NativeCallContext, array: &mut Array, filter: FnPtr) -> RhaiResultOf<Array> {
        if array.is_empty() {
            return Ok(Array::new());
        }

        let mut ar = Array::new();

        for (i, item) in array.iter_mut().enumerate() {
            let ex = [(i as INT).into()];

            if filter
                .call_raw_with_extra_args("filter", &ctx, Some(item), [], ex, Some(0))?
                .as_bool()
                .unwrap_or(false)
            {
                ar.push(item.clone());
            }
        }

        Ok(ar)
    }
    /// Return `true` if the array contains an element that equals `value`.
    ///
    /// The operator `==` is used to compare elements with `value` and must be defined,
    /// otherwise `false` is assumed.
    ///
    /// This function also drives the `in` operator.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// // The 'in' operator calls 'contains' in the background
    /// if 4 in x {
    ///     print("found!");
    /// }
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn contains(
        ctx: NativeCallContext,
        array: &mut Array,
        value: Dynamic,
    ) -> RhaiResultOf<bool> {
        if array.is_empty() {
            return Ok(false);
        }

        for item in array {
            if ctx
                .call_native_fn_raw(OP_EQUALS, true, &mut [item, &mut value.clone()])
                .or_else(|err| match *err {
                    ERR::ErrorFunctionNotFound(ref fn_sig, ..) if fn_sig.starts_with(OP_EQUALS) => {
                        if item.type_id() == value.type_id() {
                            // No default when comparing same type
                            Err(err)
                        } else {
                            Ok(Dynamic::FALSE)
                        }
                    }
                    _ => Err(err),
                })?
                .as_bool()
                .unwrap_or(false)
            {
                return Ok(true);
            }
        }

        Ok(false)
    }
    /// Find the first element in the array that equals a particular `value` and return its index.
    /// If no element equals `value`, `-1` is returned.
    ///
    /// The operator `==` is used to compare elements with `value` and must be defined,
    /// otherwise `false` is assumed.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.index_of(4));       // prints 3 (first index)
    ///
    /// print(x.index_of(9));       // prints -1
    ///
    /// print(x.index_of("foo"));   // prints -1: strings do not equal numbers
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn index_of(
        ctx: NativeCallContext,
        array: &mut Array,
        value: Dynamic,
    ) -> RhaiResultOf<INT> {
        if array.is_empty() {
            return Ok(-1);
        }

        index_of_starting_from(ctx, array, value, 0)
    }
    /// Find the first element in the array, starting from a particular `start` position, that
    /// equals a particular `value` and return its index. If no element equals `value`, `-1` is returned.
    ///
    /// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `start` < -length of array, position counts from the beginning of the array.
    /// * If `start` ≥ length of array, `-1` is returned.
    ///
    /// The operator `==` is used to compare elements with `value` and must be defined,
    /// otherwise `false` is assumed.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.index_of(4, 2));        // prints 3
    ///
    /// print(x.index_of(4, 5));        // prints 7
    ///
    /// print(x.index_of(4, 15));       // prints -1: nothing found past end of array
    ///
    /// print(x.index_of(4, -5));       // prints 11: -5 = start from index 8
    ///
    /// print(x.index_of(9, 1));        // prints -1: nothing equals 9
    ///
    /// print(x.index_of("foo", 1));    // prints -1: strings do not equal numbers
    /// ```
    #[rhai_fn(name = "index_of", return_raw, pure)]
    pub fn index_of_starting_from(
        ctx: NativeCallContext,
        array: &mut Array,
        value: Dynamic,
        start: INT,
    ) -> RhaiResultOf<INT> {
        if array.is_empty() {
            return Ok(-1);
        }

        let (start, ..) = calc_offset_len(array.len(), start, 0);

        for (i, item) in array.iter_mut().enumerate().skip(start) {
            if ctx
                .call_native_fn_raw(OP_EQUALS, true, &mut [item, &mut value.clone()])
                .or_else(|err| match *err {
                    ERR::ErrorFunctionNotFound(ref fn_sig, ..) if fn_sig.starts_with(OP_EQUALS) => {
                        if item.type_id() == value.type_id() {
                            // No default when comparing same type
                            Err(err)
                        } else {
                            Ok(Dynamic::FALSE)
                        }
                    }
                    _ => Err(err),
                })?
                .as_bool()
                .unwrap_or(false)
            {
                return Ok(i as INT);
            }
        }

        Ok(-1 as INT)
    }
    /// Iterate through all the elements in the array, applying a `filter` function to each element
    /// in turn, and return the index of the first element that returns `true`.
    /// If no element returns `true`, `-1` is returned.
    ///
    /// # No Function Parameter
    ///
    /// Array element (mutable) is bound to `this`.
    ///
    /// This method is marked _pure_; the `filter` function should not mutate array elements.
    ///
    /// # Function Parameters
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.index_of(|v| v > 3));           // prints 3: 4 > 3
    ///
    /// print(x.index_of(|v| v > 8));           // prints -1: nothing is > 8
    ///
    /// print(x.index_of(|v, i| v * i > 20));   // prints 7: 4 * 7 > 20
    /// ```
    #[rhai_fn(name = "index_of", return_raw, pure)]
    pub fn index_of_filter(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: FnPtr,
    ) -> RhaiResultOf<INT> {
        if array.is_empty() {
            return Ok(-1);
        }

        index_of_filter_starting_from(ctx, array, filter, 0)
    }
    /// Iterate through all the elements in the array, starting from a particular `start` position,
    /// applying a `filter` function to each element in turn, and return the index of the first
    /// element that returns `true`. If no element returns `true`, `-1` is returned.
    ///
    /// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `start` < -length of array, position counts from the beginning of the array.
    /// * If `start` ≥ length of array, `-1` is returned.
    ///
    /// # No Function Parameter
    ///
    /// Array element (mutable) is bound to `this`.
    ///
    /// This method is marked _pure_; the `filter` function should not mutate array elements.
    ///
    /// # Function Parameters
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.index_of(|v| v > 1, 3));    // prints 5: 2 > 1
    ///
    /// print(x.index_of(|v| v < 2, 9));    // prints -1: nothing < 2 past index 9
    ///
    /// print(x.index_of(|v| v > 1, 15));   // prints -1: nothing found past end of array
    ///
    /// print(x.index_of(|v| v > 1, -5));   // prints 9: -5 = start from index 8
    ///
    /// print(x.index_of(|v| v > 1, -99));  // prints 1: -99 = start from beginning
    ///
    /// print(x.index_of(|v, i| v * i > 20, 8));    // prints 10: 3 * 10 > 20
    /// ```
    #[rhai_fn(name = "index_of", return_raw, pure)]
    pub fn index_of_filter_starting_from(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: FnPtr,
        start: INT,
    ) -> RhaiResultOf<INT> {
        if array.is_empty() {
            return Ok(-1);
        }

        let (start, ..) = calc_offset_len(array.len(), start, 0);

        for (i, item) in array.iter_mut().enumerate().skip(start) {
            let ex = [(i as INT).into()];

            if filter
                .call_raw_with_extra_args("index_of", &ctx, Some(item), [], ex, Some(0))?
                .as_bool()
                .unwrap_or(false)
            {
                return Ok(i as INT);
            }
        }

        Ok(-1 as INT)
    }
    /// Iterate through all the elements in the array, applying a `filter` function to each element
    /// in turn, and return a copy of the first element that returns `true`. If no element returns
    /// `true`, `()` is returned.
    ///
    /// # No Function Parameter
    ///
    /// Array element (mutable) is bound to `this`.
    ///
    /// # Function Parameters
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 5, 8, 13];
    ///
    /// print(x.find(|v| v > 3));                    // prints 5: 5 > 3
    ///
    /// print(x.find(|v| v > 13) ?? "not found");    // prints "not found": nothing is > 13
    ///
    /// print(x.find(|v, i| v * i > 13));            // prints 5: 3 * 5 > 13
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn find(ctx: NativeCallContext, array: &mut Array, filter: FnPtr) -> RhaiResult {
        find_starting_from(ctx, array, filter, 0)
    }
    /// Iterate through all the elements in the array, starting from a particular `start` position,
    /// applying a `filter` function to each element in turn, and return a copy of the first element
    /// that returns `true`. If no element returns `true`, `()` is returned.
    ///
    /// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `start` < -length of array, position counts from the beginning of the array.
    /// * If `start` ≥ length of array, `-1` is returned.
    ///
    /// # No Function Parameter
    ///
    /// Array element (mutable) is bound to `this`.
    ///
    /// This method is marked _pure_; the `filter` function should not mutate array elements.
    ///
    /// # Function Parameters
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 5, 8, 13];
    ///
    /// print(x.find(|v| v > 1, 2));                     // prints 3: 3 > 1
    ///
    /// print(x.find(|v| v < 2, 3) ?? "not found");      // prints "not found": nothing < 2 past index 3
    ///
    /// print(x.find(|v| v > 1, 8) ?? "not found");      // prints "not found": nothing found past end of array
    ///
    /// print(x.find(|v| v > 1, -3));                    // prints 5: -3 = start from index 4
    ///
    /// print(x.find(|v| v > 0, -99));                   // prints 1: -99 = start from beginning
    ///
    /// print(x.find(|v, i| v * i > 6, 3));              // prints 5: 5 * 4 > 6
    /// ```
    #[rhai_fn(name = "find", return_raw, pure)]
    pub fn find_starting_from(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: FnPtr,
        start: INT,
    ) -> RhaiResult {
        let index = index_of_filter_starting_from(ctx, array, filter, start)?;

        if index < 0 {
            return Ok(Dynamic::UNIT);
        }

        Ok(get(array, index))
    }
    /// Iterate through all the elements in the array, applying a `mapper` function to each element
    /// in turn, and return the first result that is not `()`. Otherwise, `()` is returned.
    ///
    /// # No Function Parameter
    ///
    /// Array element (mutable) is bound to `this`.
    ///
    /// This method is marked _pure_; the `mapper` function should not mutate array elements.
    ///
    /// # Function Parameters
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [#{alice: 1}, #{bob: 2}, #{clara: 3}];
    ///
    /// print(x.find_map(|v| v.alice));                  // prints 1
    ///
    /// print(x.find_map(|v| v.dave) ?? "not found");    // prints "not found"
    ///
    /// print(x.find_map(|| this.dave) ?? "not found");  // prints "not found"
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn find_map(ctx: NativeCallContext, array: &mut Array, filter: FnPtr) -> RhaiResult {
        find_map_starting_from(ctx, array, filter, 0)
    }
    /// Iterate through all the elements in the array, starting from a particular `start` position,
    /// applying a `mapper` function to each element in turn, and return the first result that is not `()`.
    /// Otherwise, `()` is returned.
    ///
    /// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `start` < -length of array, position counts from the beginning of the array.
    /// * If `start` ≥ length of array, `-1` is returned.
    ///
    /// # No Function Parameter
    ///
    /// Array element (mutable) is bound to `this`.
    ///
    /// This method is marked _pure_; the `mapper` function should not mutate array elements.
    ///
    /// # Function Parameters
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [#{alice: 1}, #{bob: 2}, #{bob: 3}, #{clara: 3}, #{alice: 0}, #{clara: 5}];
    ///
    /// print(x.find_map(|v| v.alice, 2));                   // prints 0
    ///
    /// print(x.find_map(|v| v.bob, 4) ?? "not found");      // prints "not found"
    ///
    /// print(x.find_map(|v| v.alice, 8) ?? "not found");    // prints "not found"
    ///
    /// print(x.find_map(|| this.alice, 8) ?? "not found");  // prints "not found"
    ///
    /// print(x.find_map(|v| v.bob, -4));                    // prints 3: -4 = start from index 2
    ///
    /// print(x.find_map(|v| v.alice, -99));                 // prints 1: -99 = start from beginning
    ///
    /// print(x.find_map(|| this.alice, -99));               // prints 1: -99 = start from beginning
    /// ```
    #[rhai_fn(name = "find_map", return_raw, pure)]
    pub fn find_map_starting_from(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: FnPtr,
        start: INT,
    ) -> RhaiResult {
        if array.is_empty() {
            return Ok(Dynamic::UNIT);
        }

        let (start, ..) = calc_offset_len(array.len(), start, 0);

        for (i, item) in array.iter_mut().enumerate().skip(start) {
            let ex = [(i as INT).into()];

            let value =
                filter.call_raw_with_extra_args("find_map", &ctx, Some(item), [], ex, Some(0))?;

            if !value.is_unit() {
                return Ok(value);
            }
        }

        Ok(Dynamic::UNIT)
    }
    /// Return `true` if any element in the array that returns `true` when applied the `filter` function.
    ///
    /// # No Function Parameter
    ///
    /// Array element (mutable) is bound to `this`.
    ///
    /// This method is marked _pure_; the `filter` function should not mutate array elements.
    ///
    /// # Function Parameters
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.some(|v| v > 3));       // prints true
    ///
    /// print(x.some(|v| v > 10));      // prints false
    ///
    /// print(x.some(|v, i| i > v));    // prints true
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn some(ctx: NativeCallContext, array: &mut Array, filter: FnPtr) -> RhaiResultOf<bool> {
        if array.is_empty() {
            return Ok(false);
        }

        for (i, item) in array.iter_mut().enumerate() {
            let ex = [(i as INT).into()];

            if filter
                .call_raw_with_extra_args("some", &ctx, Some(item), [], ex, Some(0))?
                .as_bool()
                .unwrap_or(false)
            {
                return Ok(true);
            }
        }

        Ok(false)
    }
    /// Return `true` if all elements in the array return `true` when applied the `filter` function.
    ///
    /// # No Function Parameter
    ///
    /// Array element (mutable) is bound to `this`.
    ///
    /// This method is marked _pure_; the `filter` function should not mutate array elements.
    ///
    /// # Function Parameters
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.all(|v| v > 3));        // prints false
    ///
    /// print(x.all(|v| v > 1));        // prints true
    ///
    /// print(x.all(|v, i| i > v));     // prints false
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn all(ctx: NativeCallContext, array: &mut Array, filter: FnPtr) -> RhaiResultOf<bool> {
        if array.is_empty() {
            return Ok(true);
        }

        for (i, item) in array.iter_mut().enumerate() {
            let ex = [(i as INT).into()];

            if !filter
                .call_raw_with_extra_args("all", &ctx, Some(item), [], ex, Some(0))?
                .as_bool()
                .unwrap_or(false)
            {
                return Ok(false);
            }
        }

        Ok(true)
    }
    /// Remove duplicated _consecutive_ elements from the array.
    ///
    /// The operator `==` is used to compare elements and must be defined,
    /// otherwise `false` is assumed.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 2, 2, 3, 4, 3, 3, 2, 1];
    ///
    /// x.dedup();
    ///
    /// print(x);       // prints "[1, 2, 3, 4, 3, 2, 1]"
    /// ```
    pub fn dedup(ctx: NativeCallContext, array: &mut Array) {
        let comparer = FnPtr {
            name: ctx.engine().get_interned_string(OP_EQUALS),
            curry: <_>::default(),
            environ: None,
            #[cfg(not(feature = "no_function"))]
            fn_def: None,
        };
        dedup_by_comparer(ctx, array, comparer);
    }
    /// Remove duplicated _consecutive_ elements from the array that return `true` when applied the
    /// `comparer` function.
    ///
    /// No element is removed if the correct `comparer` function does not exist.
    ///
    /// # Function Parameters
    ///
    /// * `element1`: copy of the current array element to compare
    /// * `element2`: copy of the next array element to compare
    ///
    /// ## Return Value
    ///
    /// `true` if `element1 == element2`, otherwise `false`.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 2, 2, 3, 1, 2, 3, 4, 3, 3, 2, 1];
    ///
    /// x.dedup(|a, b| a >= b);
    ///
    /// print(x);       // prints "[1, 2, 3, 4]"
    /// ```
    #[rhai_fn(name = "dedup")]
    pub fn dedup_by_comparer(ctx: NativeCallContext, array: &mut Array, comparer: FnPtr) {
        if array.is_empty() {
            return;
        }

        array.dedup_by(|x, y| {
            comparer
                .call_raw(&ctx, None, [y.clone(), x.clone()])
                .unwrap_or(Dynamic::FALSE)
                .as_bool()
                .unwrap_or(false)
        });
    }
    /// Reduce an array by iterating through all elements while applying the `reducer` function.
    ///
    /// # Function Parameters
    ///
    /// * `result`: accumulated result, initially `()`
    /// * `element`: copy of array element, or bound to `this` if omitted
    /// * `index` _(optional)_: current index in the array
    ///
    /// This method is marked _pure_; the `reducer` function should not mutate array elements.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.reduce(|r, v| v + (r ?? 0));
    ///
    /// print(y);       // prints 15
    ///
    /// let y = x.reduce(|r, v, i| v + i + (r ?? 0));
    ///
    /// print(y);       // prints 25
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn reduce(ctx: NativeCallContext, array: &mut Array, reducer: FnPtr) -> RhaiResult {
        reduce_with_initial(ctx, array, reducer, Dynamic::UNIT)
    }
    /// Reduce an array by iterating through all elements while applying the `reducer` function.
    ///
    /// # Function Parameters
    ///
    /// * `result`: accumulated result, starting with the value of `initial`
    /// * `element`: copy of array element, or bound to `this` if omitted
    /// * `index` _(optional)_: current index in the array
    ///
    /// This method is marked _pure_; the `reducer` function should not mutate array elements.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.reduce(|r, v| v + r, 5);
    ///
    /// print(y);       // prints 20
    ///
    /// let y = x.reduce(|r, v, i| v + i + r, 5);
    ///
    /// print(y);       // prints 30
    /// ```
    #[rhai_fn(name = "reduce", return_raw, pure)]
    pub fn reduce_with_initial(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: FnPtr,
        initial: Dynamic,
    ) -> RhaiResult {
        if array.is_empty() {
            return Ok(initial);
        }

        array
            .iter_mut()
            .enumerate()
            .try_fold(initial, |result, (i, item)| {
                let ex = [(i as INT).into()];
                reducer.call_raw_with_extra_args("reduce", &ctx, Some(item), [result], ex, Some(1))
            })
    }
    /// Reduce an array by iterating through all elements, in _reverse_ order,
    /// while applying the `reducer` function.
    ///
    /// # Function Parameters
    ///
    /// * `result`: accumulated result, initially `()`
    /// * `element`: copy of array element, or bound to `this` if omitted
    /// * `index` _(optional)_: current index in the array
    ///
    /// This method is marked _pure_; the `reducer` function should not mutate array elements.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.reduce_rev(|r, v| v + (r ?? 0));
    ///
    /// print(y);       // prints 15
    ///
    /// let y = x.reduce_rev(|r, v, i| v + i + (r ?? 0));
    ///
    /// print(y);       // prints 25
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn reduce_rev(ctx: NativeCallContext, array: &mut Array, reducer: FnPtr) -> RhaiResult {
        reduce_rev_with_initial(ctx, array, reducer, Dynamic::UNIT)
    }
    /// Reduce an array by iterating through all elements, in _reverse_ order,
    /// while applying the `reducer` function.
    ///
    /// # Function Parameters
    ///
    /// * `result`: accumulated result, starting with the value of `initial`
    /// * `element`: copy of array element, or bound to `this` if omitted
    /// * `index` _(optional)_: current index in the array
    ///
    /// This method is marked _pure_; the `reducer` function should not mutate array elements.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.reduce_rev(|r, v| v + r, 5);
    ///
    /// print(y);       // prints 20
    ///
    /// let y = x.reduce_rev(|r, v, i| v + i + r, 5);
    ///
    /// print(y);       // prints 30
    /// ```
    #[rhai_fn(name = "reduce_rev", return_raw, pure)]
    pub fn reduce_rev_with_initial(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: FnPtr,
        initial: Dynamic,
    ) -> RhaiResult {
        if array.is_empty() {
            return Ok(initial);
        }

        let len = array.len();

        array
            .iter_mut()
            .rev()
            .enumerate()
            .try_fold(initial, |result, (i, item)| {
                let ex = [((len - 1 - i) as INT).into()];

                reducer.call_raw_with_extra_args(
                    "reduce_rev",
                    &ctx,
                    Some(item),
                    [result],
                    ex,
                    Some(1),
                )
            })
    }
    /// Iterate through all elements in two arrays, applying a `mapper` function to them,
    /// and return a new array containing the results.
    ///
    /// # Function Parameters
    ///
    /// * `array1`: First array
    /// * `array2`: Second array
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    /// let y = [9, 8, 7, 6];
    ///
    /// let z = x.zip(y, |a, b| a + b);
    ///
    /// print(z);       // prints [10, 10, 10, 10]
    ///
    /// let z = x.zip(y, |a, b, i| a + b + i);
    ///
    /// print(z);       // prints [10, 11, 12, 13]
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn zip(
        ctx: NativeCallContext,
        array1: &mut Array,
        array2: Array,
        map: FnPtr,
    ) -> RhaiResultOf<Array> {
        if array1.is_empty() && array2.is_empty() {
            return Ok(Array::new());
        }

        array1
            .iter_mut()
            .zip(array2)
            .enumerate()
            .map(|(i, (x, y))| {
                map.call_raw_with_extra_args(
                    "zip",
                    &ctx,
                    None,
                    [x.clone(), y],
                    [(i as INT).into()],
                    None,
                )
            })
            .collect()
    }
    /// Sort the array based on applying the `comparer` function.
    ///
    /// # Function Parameters
    ///
    /// * `element1`: copy of the current array element to compare
    /// * `element2`: copy of the next array element to compare
    ///
    /// ## Return Value
    ///
    /// An integer number:
    ///
    /// * Any positive integer if `element1 > element2`
    /// * 0 if `element1 == element2`
    /// * Any negative integer if `element1 < element2`
    ///
    /// or a boolean value:
    ///
    /// * `true` if `element1 <= element2`
    /// * `false` if `element1 > element2`
    ///
    /// Any other return value type will yield unpredictable order.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];
    ///
    /// // Do comparisons in reverse
    /// x.sort(|a, b| if a > b { -1 } else if a < b { 1 } else { 0 });
    ///
    /// print(x);       // prints "[10, 9, 8, 7, 6, 5, 4, 3, 2, 1]"
    /// ```
    pub fn sort(ctx: NativeCallContext, array: &mut Array, comparer: FnPtr) {
        if array.len() <= 1 {
            return;
        }

        array.sort_by(|x, y| {
            comparer
                .call_raw(&ctx, None, [x.clone(), y.clone()])
                .ok()
                .and_then(|v| {
                    v.as_int()
                        .or_else(|_| v.as_bool().map(|v| if v { -1 } else { 1 }))
                        .ok()
                })
                .map_or_else(
                    || x.type_id().cmp(&y.type_id()),
                    |v| match v {
                        v if v > 0 => Ordering::Greater,
                        v if v < 0 => Ordering::Less,
                        0 => Ordering::Equal,
                        _ => unreachable!("v is {}", v),
                    },
                )
        });
    }
    /// Sort the array.
    ///
    /// All elements in the array must be of the same data type.
    ///
    /// # Supported Data Types
    ///
    /// * integer numbers
    /// * floating-point numbers
    /// * decimal numbers
    /// * characters
    /// * strings
    /// * booleans
    /// * `()`
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];
    ///
    /// x.sort();
    ///
    /// print(x);       // prints "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]"
    /// ```
    #[rhai_fn(name = "sort", return_raw)]
    pub fn sort_with_builtin(array: &mut Array) -> RhaiResultOf<()> {
        if array.len() <= 1 {
            return Ok(());
        }

        let type_id = array[0].type_id();

        if array.iter().any(|a| a.type_id() != type_id) {
            return Err(ERR::ErrorFunctionNotFound(
                "sort() cannot be called with elements of different types".into(),
                Position::NONE,
            )
            .into());
        }

        if type_id == TypeId::of::<INT>() {
            array.sort_by(|a, b| {
                let a = a.as_int().unwrap();
                let b = b.as_int().unwrap();
                a.cmp(&b)
            });
            return Ok(());
        }
        if type_id == TypeId::of::<char>() {
            array.sort_by(|a, b| {
                let a = a.as_char().unwrap();
                let b = b.as_char().unwrap();
                a.cmp(&b)
            });
            return Ok(());
        }
        #[cfg(not(feature = "no_float"))]
        if type_id == TypeId::of::<crate::FLOAT>() {
            array.sort_by(|a, b| {
                let a = a.as_float().unwrap();
                let b = b.as_float().unwrap();
                a.partial_cmp(&b).unwrap_or(Ordering::Equal)
            });
            return Ok(());
        }
        if type_id == TypeId::of::<ImmutableString>() {
            array.sort_by(|a, b| {
                let a = &*a.as_immutable_string_ref().unwrap();
                let b = &*b.as_immutable_string_ref().unwrap();
                a.cmp(b)
            });
            return Ok(());
        }
        #[cfg(feature = "decimal")]
        if type_id == TypeId::of::<rust_decimal::Decimal>() {
            array.sort_by(|a, b| {
                let a = a.as_decimal().unwrap();
                let b = b.as_decimal().unwrap();
                a.cmp(&b)
            });
            return Ok(());
        }
        if type_id == TypeId::of::<bool>() {
            array.sort_by(|a, b| {
                let a = a.as_bool().unwrap();
                let b = b.as_bool().unwrap();
                a.cmp(&b)
            });
            return Ok(());
        }
        if type_id == TypeId::of::<()>() {
            return Ok(());
        }

        Ok(())
    }
    /// Remove all elements in the array that returns `true` when applied the `filter` function and
    /// return them as a new array.
    ///
    /// # No Function Parameter
    ///
    /// Array element (mutable) is bound to `this`.
    ///
    /// # Function Parameters
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.drain(|v| v < 3);
    ///
    /// print(x);       // prints "[3, 4, 5]"
    ///
    /// print(y);       // prints "[1, 2]"
    ///
    /// let z = x.drain(|v, i| v + i > 5);
    ///
    /// print(x);       // prints "[3, 4]"
    ///
    /// print(z);       // prints "[5]"
    /// ```
    #[rhai_fn(return_raw)]
    pub fn drain(ctx: NativeCallContext, array: &mut Array, filter: FnPtr) -> RhaiResultOf<Array> {
        if array.is_empty() {
            return Ok(Array::new());
        }

        let mut drained = Array::with_capacity(array.len());

        let mut i = 0;
        let mut x = 0;

        while x < array.len() {
            let ex = [(i as INT).into()];

            if filter
                .call_raw_with_extra_args("drain", &ctx, Some(&mut array[x]), [], ex, Some(0))?
                .as_bool()
                .unwrap_or(false)
            {
                drained.push(array.remove(x));
            } else {
                x += 1;
            }

            i += 1;
        }

        Ok(drained)
    }
    /// Remove all elements in the array within an exclusive `range` and return them as a new array.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.drain(1..3);
    ///
    /// print(x);       // prints "[1, 4, 5]"
    ///
    /// print(y);       // prints "[2, 3]"
    ///
    /// let z = x.drain(2..3);
    ///
    /// print(x);       // prints "[1, 4]"
    ///
    /// print(z);       // prints "[5]"
    /// ```
    #[rhai_fn(name = "drain")]
    pub fn drain_exclusive_range(array: &mut Array, range: ExclusiveRange) -> Array {
        let start = INT::max(range.start, 0);
        let end = INT::max(range.end, start);
        drain_range(array, start, end - start)
    }
    /// Remove all elements in the array within an inclusive `range` and return them as a new array.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.drain(1..=2);
    ///
    /// print(x);       // prints "[1, 4, 5]"
    ///
    /// print(y);       // prints "[2, 3]"
    ///
    /// let z = x.drain(2..=2);
    ///
    /// print(x);       // prints "[1, 4]"
    ///
    /// print(z);       // prints "[5]"
    /// ```
    #[rhai_fn(name = "drain")]
    pub fn drain_inclusive_range(array: &mut Array, range: InclusiveRange) -> Array {
        let start = INT::max(*range.start(), 0);
        let end = INT::max(*range.end(), start);
        drain_range(array, start, end - start + 1)
    }
    /// Remove all elements within a portion of the array and return them as a new array.
    ///
    /// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `start` < -length of array, position counts from the beginning of the array.
    /// * If `start` ≥ length of array, no element is removed and an empty array is returned.
    /// * If `len` ≤ 0, no element is removed and an empty array is returned.
    /// * If `start` position + `len` ≥ length of array, entire portion of the array after the `start` position is removed and returned.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.drain(1, 2);
    ///
    /// print(x);       // prints "[1, 4, 5]"
    ///
    /// print(y);       // prints "[2, 3]"
    ///
    /// let z = x.drain(-1, 1);
    ///
    /// print(x);       // prints "[1, 4]"
    ///
    /// print(z);       // prints "[5]"
    /// ```
    #[rhai_fn(name = "drain")]
    pub fn drain_range(array: &mut Array, start: INT, len: INT) -> Array {
        if array.is_empty() || len <= 0 {
            return Array::new();
        }

        let (start, len) = calc_offset_len(array.len(), start, len);

        if len == 0 {
            return Array::new();
        }

        array.drain(start..start + len).collect()
    }
    /// Remove all elements in the array that do not return `true` when applied the `filter`
    /// function and return them as a new array.
    ///
    /// # No Function Parameter
    ///
    /// Array element (mutable) is bound to `this`.
    ///
    /// # Function Parameters
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.retain(|v| v >= 3);
    ///
    /// print(x);       // prints "[3, 4, 5]"
    ///
    /// print(y);       // prints "[1, 2]"
    ///
    /// let z = x.retain(|v, i| v + i <= 5);
    ///
    /// print(x);       // prints "[3, 4]"
    ///
    /// print(z);       // prints "[5]"
    /// ```
    #[rhai_fn(return_raw)]
    pub fn retain(ctx: NativeCallContext, array: &mut Array, filter: FnPtr) -> RhaiResultOf<Array> {
        if array.is_empty() {
            return Ok(Array::new());
        }

        let mut drained = Array::new();

        let mut i = 0;
        let mut x = 0;

        while x < array.len() {
            let ex = [(i as INT).into()];

            if filter
                .call_raw_with_extra_args("retain", &ctx, Some(&mut array[x]), [], ex, Some(0))?
                .as_bool()
                .unwrap_or(false)
            {
                x += 1;
            } else {
                drained.push(array.remove(x));
            }

            i += 1;
        }

        Ok(drained)
    }
    /// Remove all elements in the array not within an exclusive `range` and return them as a new array.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.retain(1..4);
    ///
    /// print(x);       // prints "[2, 3, 4]"
    ///
    /// print(y);       // prints "[1, 5]"
    ///
    /// let z = x.retain(1..3);
    ///
    /// print(x);       // prints "[3, 4]"
    ///
    /// print(z);       // prints "[1]"
    /// ```
    #[rhai_fn(name = "retain")]
    pub fn retain_exclusive_range(array: &mut Array, range: ExclusiveRange) -> Array {
        let start = INT::max(range.start, 0);
        let end = INT::max(range.end, start);
        retain_range(array, start, end - start)
    }
    /// Remove all elements in the array not within an inclusive `range` and return them as a new array.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.retain(1..=3);
    ///
    /// print(x);       // prints "[2, 3, 4]"
    ///
    /// print(y);       // prints "[1, 5]"
    ///
    /// let z = x.retain(1..=2);
    ///
    /// print(x);       // prints "[3, 4]"
    ///
    /// print(z);       // prints "[1]"
    /// ```
    #[rhai_fn(name = "retain")]
    pub fn retain_inclusive_range(array: &mut Array, range: InclusiveRange) -> Array {
        let start = INT::max(*range.start(), 0);
        let end = INT::max(*range.end(), start);
        retain_range(array, start, end - start + 1)
    }
    /// Remove all elements not within a portion of the array and return them as a new array.
    ///
    /// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `start` < -length of array, position counts from the beginning of the array.
    /// * If `start` ≥ length of array, all elements are removed returned.
    /// * If `len` ≤ 0, all elements are removed and returned.
    /// * If `start` position + `len` ≥ length of array, entire portion of the array before the `start` position is removed and returned.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.retain(1, 2);
    ///
    /// print(x);       // prints "[2, 3]"
    ///
    /// print(y);       // prints "[1, 4, 5]"
    ///
    /// let z = x.retain(-1, 1);
    ///
    /// print(x);       // prints "[3]"
    ///
    /// print(z);       // prints "[2]"
    /// ```
    #[rhai_fn(name = "retain")]
    pub fn retain_range(array: &mut Array, start: INT, len: INT) -> Array {
        if array.is_empty() || len <= 0 {
            return Array::new();
        }

        let (start, len) = calc_offset_len(array.len(), start, len);

        if len == 0 {
            return Array::new();
        }

        let mut drained: Array = array.drain(..start).collect();
        drained.extend(array.drain(len..));

        drained
    }
    /// Return `true` if two arrays are equal (i.e. all elements are equal and in the same order).
    ///
    /// The operator `==` is used to compare elements and must be defined,
    /// otherwise `false` is assumed.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    /// let y = [1, 2, 3, 4, 5];
    /// let z = [1, 2, 3, 4];
    ///
    /// print(x == y);      // prints true
    ///
    /// print(x == z);      // prints false
    /// ```
    #[rhai_fn(name = "==", return_raw, pure)]
    pub fn equals(ctx: NativeCallContext, array1: &mut Array, array2: Array) -> RhaiResultOf<bool> {
        if array1.len() != array2.len() {
            return Ok(false);
        }
        if array1.is_empty() {
            return Ok(true);
        }

        let mut array2 = array2;

        for (a1, a2) in array1.iter_mut().zip(array2.iter_mut()) {
            if !ctx
                .call_native_fn_raw(OP_EQUALS, true, &mut [a1, a2])
                .or_else(|err| match *err {
                    ERR::ErrorFunctionNotFound(ref fn_sig, ..) if fn_sig.starts_with(OP_EQUALS) => {
                        if a1.type_id() == a2.type_id() {
                            // No default when comparing same type
                            Err(err)
                        } else {
                            Ok(Dynamic::FALSE)
                        }
                    }
                    _ => Err(err),
                })?
                .as_bool()
                .unwrap_or(false)
            {
                return Ok(false);
            }
        }

        Ok(true)
    }
    /// Return `true` if two arrays are not-equal (i.e. any element not equal or not in the same order).
    ///
    /// The operator `==` is used to compare elements and must be defined,
    /// otherwise `false` is assumed.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 5];
    /// let y = [1, 2, 3, 4, 5];
    /// let z = [1, 2, 3, 4];
    ///
    /// print(x != y);      // prints false
    ///
    /// print(x != z);      // prints true
    /// ```
    #[rhai_fn(name = "!=", return_raw, pure)]
    pub fn not_equals(
        ctx: NativeCallContext,
        array1: &mut Array,
        array2: Array,
    ) -> RhaiResultOf<bool> {
        equals(ctx, array1, array2).map(|r| !r)
    }
}
