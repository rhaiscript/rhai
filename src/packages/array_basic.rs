#![cfg(not(feature = "no_index"))]

use crate::engine::OP_EQUALS;
use crate::eval::{calc_index, calc_offset_len};
use crate::plugin::*;
use crate::{
    def_package, Array, Dynamic, ExclusiveRange, FnPtr, InclusiveRange, NativeCallContext,
    Position, RhaiResultOf, StaticVec, ERR, INT,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{any::TypeId, cmp::Ordering, mem};

def_package! {
    /// Package of basic array utilities.
    pub BasicArrayPackage(lib) {
        lib.standard = true;

        combine_with_exported_module!(lib, "array", array_functions);

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
            Dynamic::UNIT
        } else {
            array[index].clone()
        }
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

        if index < array.len() {
            array[index] = value;
        }
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
    /// x.push(y);
    ///
    /// print(x);       // prints "[1, 2, 3, true, 'x']"
    /// ```
    pub fn append(array: &mut Array, new_array: Array) {
        if !new_array.is_empty() {
            if array.is_empty() {
                *array = new_array;
            } else {
                array.extend(new_array);
            }
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
            array1
        } else if array1.is_empty() {
            array2
        } else {
            let mut array = array1;
            array.extend(array2);
            array
        }
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
        if len <= 0 || (len as usize) <= array.len() {
            return Ok(());
        }

        let _ctx = ctx;
        let len = len as usize;

        // Check if array will be over max size limit
        #[cfg(not(feature = "unchecked"))]
        {
            if _ctx.engine().max_array_size() > 0 && len > _ctx.engine().max_array_size() {
                return Err(
                    ERR::ErrorDataTooLarge("Size of array".to_string(), Position::NONE).into(),
                );
            }

            let check_sizes = match item.0 {
                crate::types::dynamic::Union::Array(..) | crate::types::dynamic::Union::Str(..) => {
                    true
                }
                #[cfg(not(feature = "no_object"))]
                crate::types::dynamic::Union::Map(..) => true,
                _ => false,
            };

            if check_sizes {
                let mut arr_len = array.len();
                let mut arr = Dynamic::from_array(mem::take(array));

                let (mut a1, mut m1, mut s1) = crate::Engine::calc_data_sizes(&arr, true);
                let (a2, m2, s2) = crate::Engine::calc_data_sizes(&item, true);

                {
                    let mut guard = arr.write_lock::<Array>().unwrap();

                    while arr_len < len {
                        a1 += a2;
                        m1 += m2;
                        s1 += s2;

                        _ctx.engine()
                            .raise_err_if_over_data_size_limit((a1, m1, s1), Position::NONE)?;

                        guard.push(item.clone());
                        arr_len += 1;
                    }
                }

                *array = arr.into_array().unwrap();
            } else {
                array.resize(len, item);
            }
        }

        #[cfg(feature = "unchecked")]
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
            Dynamic::UNIT
        } else {
            array.pop().unwrap_or(Dynamic::UNIT)
        }
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
            Dynamic::UNIT
        } else {
            array.remove(0)
        }
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
        let index = match calc_index(array.len(), index, true, || Err(())) {
            Ok(n) => n,
            Err(_) => return Dynamic::UNIT,
        };

        array.remove(index)
    }
    /// Clear the array.
    pub fn clear(array: &mut Array) {
        if !array.is_empty() {
            array.clear();
        }
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
        if !array.is_empty() {
            if len > 0 {
                array.truncate(len as usize);
            } else {
                array.clear();
            }
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
        if !array.is_empty() {
            if len <= 0 {
                array.clear();
            } else if (len as usize) < array.len() {
                array.drain(0..array.len() - len as usize);
            }
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
        if !array.is_empty() {
            array.reverse();
        }
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
            Array::new()
        } else {
            array[start..start + len].to_vec()
        }
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

        if start == 0 {
            if len >= array.len() {
                mem::take(array)
            } else {
                let mut result = Array::new();
                result.extend(array.drain(array.len() - len..));
                result
            }
        } else if start >= array.len() {
            Array::new()
        } else {
            let mut result = Array::new();
            result.extend(array.drain(start as usize..));
            result
        }
    }
    /// Iterate through all the elements in the array, applying a `mapper` function to each element
    /// in turn, and return the results as a new array.
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
    #[rhai_fn(return_raw)]
    pub fn map(ctx: NativeCallContext, array: Array, mapper: FnPtr) -> RhaiResultOf<Array> {
        if array.is_empty() {
            return Ok(array);
        }

        let mut ar = Array::with_capacity(array.len());

        for (i, item) in array.into_iter().enumerate() {
            ar.push(
                mapper
                    .call_raw(&ctx, None, [item.clone()])
                    .or_else(|err| match *err {
                        ERR::ErrorFunctionNotFound(fn_sig, ..)
                            if fn_sig.starts_with(mapper.fn_name()) =>
                        {
                            mapper.call_raw(&ctx, None, [item, (i as INT).into()])
                        }
                        _ => Err(err),
                    })
                    .map_err(|err| {
                        Box::new(ERR::ErrorInFunctionCall(
                            "map".to_string(),
                            ctx.source().unwrap_or("").to_string(),
                            err,
                            Position::NONE,
                        ))
                    })?,
            );
        }

        Ok(ar)
    }
    /// Iterate through all the elements in the array, applying a function named by `mapper` to each
    /// element in turn, and return the results as a new array.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `mapper` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn square(x) { x * x }
    ///
    /// fn multiply(x, i) { x * i }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.map("square");
    ///
    /// print(y);       // prints "[1, 4, 9, 16, 25]"
    ///
    /// let y = x.map("multiply");
    ///
    /// print(y);       // prints "[0, 2, 6, 12, 20]"
    /// ```
    #[rhai_fn(name = "map", return_raw)]
    pub fn map_by_fn_name(
        ctx: NativeCallContext,
        array: Array,
        mapper: &str,
    ) -> RhaiResultOf<Array> {
        map(ctx, array, FnPtr::new(mapper)?)
    }

    /// Iterate through all the elements in the array, applying a `filter` function to each element
    /// in turn, and return a copy of all elements (in order) that return `true` as a new array.
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
    #[rhai_fn(return_raw)]
    pub fn filter(ctx: NativeCallContext, array: Array, filter: FnPtr) -> RhaiResultOf<Array> {
        if array.is_empty() {
            return Ok(array);
        }

        let mut ar = Array::new();

        for (i, item) in array.into_iter().enumerate() {
            if filter
                .call_raw(&ctx, None, [item.clone()])
                .or_else(|err| match *err {
                    ERR::ErrorFunctionNotFound(fn_sig, ..)
                        if fn_sig.starts_with(filter.fn_name()) =>
                    {
                        filter.call_raw(&ctx, None, [item.clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(ERR::ErrorInFunctionCall(
                        "filter".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?
                .as_bool()
                .unwrap_or(false)
            {
                ar.push(item);
            }
        }

        Ok(ar)
    }
    /// Iterate through all the elements in the array, applying a function named by `filter` to each
    /// element in turn, and return a copy of all elements (in order) that return `true` as a new array.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn screen(x, i) { x * i >= 10 }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.filter("is_odd");
    ///
    /// print(y);       // prints "[1, 3, 5]"
    ///
    /// let y = x.filter("screen");
    ///
    /// print(y);       // prints "[12, 20]"
    /// ```
    #[rhai_fn(name = "filter", return_raw)]
    pub fn filter_by_fn_name(
        ctx: NativeCallContext,
        array: Array,
        filter_func: &str,
    ) -> RhaiResultOf<Array> {
        filter(ctx, array, FnPtr::new(filter_func)?)
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

        for item in array.iter_mut() {
            if ctx
                .call_fn_raw(OP_EQUALS, true, false, &mut [item, &mut value.clone()])
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
            Ok(-1)
        } else {
            index_of_starting_from(ctx, array, value, 0)
        }
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
                .call_fn_raw(OP_EQUALS, true, false, &mut [item, &mut value.clone()])
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
            Ok(-1)
        } else {
            index_of_filter_starting_from(ctx, array, filter, 0)
        }
    }
    /// Iterate through all the elements in the array, applying a function named by `filter` to each
    /// element in turn, and return the index of the first element that returns `true`.
    /// If no element returns `true`, `-1` is returned.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn is_special(x) { x > 3 }
    ///
    /// fn is_dumb(x) { x > 8 }
    ///
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.index_of("is_special"));    // prints 3
    ///
    /// print(x.index_of("is_dumb"));       // prints -1
    /// ```
    #[rhai_fn(name = "index_of", return_raw, pure)]
    pub fn index_of_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: &str,
    ) -> RhaiResultOf<INT> {
        index_of_filter(ctx, array, FnPtr::new(filter)?)
    }
    /// Iterate through all the elements in the array, starting from a particular `start` position,
    /// applying a `filter` function to each element in turn, and return the index of the first
    /// element that returns `true`. If no element returns `true`, `-1` is returned.
    ///
    /// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `start` < -length of array, position counts from the beginning of the array.
    /// * If `start` ≥ length of array, `-1` is returned.
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

        for (i, item) in array.iter().enumerate().skip(start) {
            if filter
                .call_raw(&ctx, None, [item.clone()])
                .or_else(|err| match *err {
                    ERR::ErrorFunctionNotFound(fn_sig, ..)
                        if fn_sig.starts_with(filter.fn_name()) =>
                    {
                        filter.call_raw(&ctx, None, [item.clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(ERR::ErrorInFunctionCall(
                        "index_of".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?
                .as_bool()
                .unwrap_or(false)
            {
                return Ok(i as INT);
            }
        }

        Ok(-1 as INT)
    }
    /// Iterate through all the elements in the array, starting from a particular `start` position,
    /// applying a function named by `filter` to each element in turn, and return the index of the
    /// first element that returns `true`. If no element returns `true`, `-1` is returned.
    ///
    /// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `start` < -length of array, position counts from the beginning of the array.
    /// * If `start` ≥ length of array, `-1` is returned.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn plural(x) { x > 1 }
    ///
    /// fn singular(x) { x < 2 }
    ///
    /// fn screen(x, i) { x * i > 20 }
    ///
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.index_of("plural", 3));     // prints 5: 2 > 1
    ///
    /// print(x.index_of("singular", 9));   // prints -1: nothing < 2 past index 9
    ///
    /// print(x.index_of("plural", 15));    // prints -1: nothing found past end of array
    ///
    /// print(x.index_of("plural", -5));    // prints 9: -5 = start from index 8
    ///
    /// print(x.index_of("plural", -99));   // prints 1: -99 = start from beginning
    ///
    /// print(x.index_of("screen", 8));     // prints 10: 3 * 10 > 20
    /// ```
    #[rhai_fn(name = "index_of", return_raw, pure)]
    pub fn index_of_by_fn_name_starting_from(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: &str,
        start: INT,
    ) -> RhaiResultOf<INT> {
        index_of_filter_starting_from(ctx, array, FnPtr::new(filter)?, start)
    }
    /// Return `true` if any element in the array that returns `true` when applied the `filter` function.
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

        for (i, item) in array.iter().enumerate() {
            if filter
                .call_raw(&ctx, None, [item.clone()])
                .or_else(|err| match *err {
                    ERR::ErrorFunctionNotFound(fn_sig, ..)
                        if fn_sig.starts_with(filter.fn_name()) =>
                    {
                        filter.call_raw(&ctx, None, [item.clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(ERR::ErrorInFunctionCall(
                        "some".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?
                .as_bool()
                .unwrap_or(false)
            {
                return Ok(true);
            }
        }

        Ok(false)
    }
    /// Return `true` if any element in the array that returns `true` when applied a function named
    /// by `filter`.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn large(x) { x > 3 }
    ///
    /// fn huge(x) { x > 10 }
    ///
    /// fn screen(x, i) { i > x }
    ///
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.some("large"));     // prints true
    ///
    /// print(x.some("huge"));      // prints false
    ///
    /// print(x.some("screen"));    // prints true
    /// ```
    #[rhai_fn(name = "some", return_raw, pure)]
    pub fn some_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: &str,
    ) -> RhaiResultOf<bool> {
        some(ctx, array, FnPtr::new(filter)?)
    }
    /// Return `true` if all elements in the array return `true` when applied the `filter` function.
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

        for (i, item) in array.iter().enumerate() {
            if !filter
                .call_raw(&ctx, None, [item.clone()])
                .or_else(|err| match *err {
                    ERR::ErrorFunctionNotFound(fn_sig, ..)
                        if fn_sig.starts_with(filter.fn_name()) =>
                    {
                        filter.call_raw(&ctx, None, [item.clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(ERR::ErrorInFunctionCall(
                        "all".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?
                .as_bool()
                .unwrap_or(false)
            {
                return Ok(false);
            }
        }

        Ok(true)
    }
    /// Return `true` if all elements in the array return `true` when applied a function named by `filter`.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
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
    #[rhai_fn(name = "all", return_raw, pure)]
    pub fn all_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: &str,
    ) -> RhaiResultOf<bool> {
        all(ctx, array, FnPtr::new(filter)?)
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
    #[rhai_fn(return_raw)]
    pub fn dedup(ctx: NativeCallContext, array: &mut Array) -> RhaiResultOf<()> {
        let comparer = FnPtr::new_unchecked(OP_EQUALS, StaticVec::new_const());
        dedup_by_comparer(ctx, array, comparer)
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
    #[rhai_fn(name = "dedup", return_raw)]
    pub fn dedup_by_comparer(
        ctx: NativeCallContext,
        array: &mut Array,
        comparer: FnPtr,
    ) -> RhaiResultOf<()> {
        if array.is_empty() {
            return Ok(());
        }

        array.dedup_by(|x, y| {
            comparer
                .call_raw(&ctx, None, [y.clone(), x.clone()])
                .unwrap_or(Dynamic::FALSE)
                .as_bool()
                .unwrap_or(false)
        });

        Ok(())
    }
    /// Remove duplicated _consecutive_ elements from the array that return `true` when applied a
    /// function named by `comparer`.
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
    /// fn declining(a, b) { a >= b }
    ///
    /// let x = [1, 2, 2, 2, 3, 1, 2, 3, 4, 3, 3, 2, 1];
    ///
    /// x.dedup("declining");
    ///
    /// print(x);       // prints "[1, 2, 3, 4]"
    /// ```
    #[rhai_fn(name = "dedup", return_raw)]
    pub fn dedup_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        comparer: &str,
    ) -> RhaiResultOf<()> {
        dedup_by_comparer(ctx, array, FnPtr::new(comparer)?)
    }
    /// Reduce an array by iterating through all elements while applying the `reducer` function.
    ///
    /// # Function Parameters
    ///
    /// * `result`: accumulated result, initially `()`
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
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
    /// Reduce an array by iterating through all elements while applying a function named by `reducer`.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `reducer` must exist taking these parameters:
    ///
    /// * `result`: accumulated result, initially `()`
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn process(r, x) {
    ///     x + (r ?? 0)
    /// }
    /// fn process_extra(r, x, i) {
    ///     x + i + (r ?? 0)
    /// }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.reduce("process");
    ///
    /// print(y);       // prints 15
    ///
    /// let y = x.reduce("process_extra");
    ///
    /// print(y);       // prints 25
    /// ```
    #[rhai_fn(name = "reduce", return_raw, pure)]
    pub fn reduce_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: &str,
    ) -> RhaiResult {
        reduce(ctx, array, FnPtr::new(reducer)?)
    }
    /// Reduce an array by iterating through all elements while applying the `reducer` function.
    ///
    /// # Function Parameters
    ///
    /// * `result`: accumulated result, starting with the value of `initial`
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
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

        let mut result = initial;

        for (i, item) in array.iter().enumerate() {
            let item = item.clone();

            result = reducer
                .call_raw(&ctx, None, [result.clone(), item.clone()])
                .or_else(|err| match *err {
                    ERR::ErrorFunctionNotFound(fn_sig, ..)
                        if fn_sig.starts_with(reducer.fn_name()) =>
                    {
                        reducer.call_raw(&ctx, None, [result, item, (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(ERR::ErrorInFunctionCall(
                        "reduce".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?;
        }

        Ok(result)
    }
    /// Reduce an array by iterating through all elements while applying a function named by `reducer`.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `reducer` must exist taking these parameters:
    ///
    /// * `result`: accumulated result, starting with the value of `initial`
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn process(r, x) { x + r }
    ///
    /// fn process_extra(r, x, i) { x + i + r }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.reduce("process", 5);
    ///
    /// print(y);       // prints 20
    ///
    /// let y = x.reduce("process_extra", 5);
    ///
    /// print(y);       // prints 30
    /// ```
    #[rhai_fn(name = "reduce", return_raw, pure)]
    pub fn reduce_by_fn_name_with_initial(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: &str,
        initial: Dynamic,
    ) -> RhaiResult {
        reduce_with_initial(ctx, array, FnPtr::new(reducer)?, initial)
    }
    /// Reduce an array by iterating through all elements, in _reverse_ order,
    /// while applying the `reducer` function.
    ///
    /// # Function Parameters
    ///
    /// * `result`: accumulated result, initially `()`
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
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
    /// while applying a function named by `reducer`.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `reducer` must exist taking these parameters:
    ///
    /// * `result`: accumulated result, initially `()`
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn process(r, x) {
    ///     x + (r ?? 0)
    /// }
    /// fn process_extra(r, x, i) {
    ///     x + i + (r ?? 0)
    /// }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.reduce_rev("process");
    ///
    /// print(y);       // prints 15
    ///
    /// let y = x.reduce_rev("process_extra");
    ///
    /// print(y);       // prints 25
    /// ```
    #[rhai_fn(name = "reduce_rev", return_raw, pure)]
    pub fn reduce_rev_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: &str,
    ) -> RhaiResult {
        reduce_rev(ctx, array, FnPtr::new(reducer)?)
    }
    /// Reduce an array by iterating through all elements, in _reverse_ order,
    /// while applying the `reducer` function.
    ///
    /// # Function Parameters
    ///
    /// * `result`: accumulated result, starting with the value of `initial`
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
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

        let mut result = initial;
        let len = array.len();

        for (i, item) in array.iter().rev().enumerate() {
            let item = item.clone();

            result = reducer
                .call_raw(&ctx, None, [result.clone(), item.clone()])
                .or_else(|err| match *err {
                    ERR::ErrorFunctionNotFound(fn_sig, ..)
                        if fn_sig.starts_with(reducer.fn_name()) =>
                    {
                        reducer.call_raw(&ctx, None, [result, item, ((len - 1 - i) as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(ERR::ErrorInFunctionCall(
                        "reduce_rev".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?;
        }

        Ok(result)
    }
    /// Reduce an array by iterating through all elements, in _reverse_ order,
    /// while applying a function named by `reducer`.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `reducer` must exist taking these parameters:
    ///
    /// * `result`: accumulated result, starting with the value of `initial`
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn process(r, x) { x + r }
    ///
    /// fn process_extra(r, x, i) { x + i + r }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.reduce_rev("process", 5);
    ///
    /// print(y);       // prints 20
    ///
    /// let y = x.reduce_rev("process_extra", 5);
    ///
    /// print(y);       // prints 30
    /// ```
    #[rhai_fn(name = "reduce_rev", return_raw, pure)]
    pub fn reduce_rev_by_fn_name_with_initial(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: &str,
        initial: Dynamic,
    ) -> RhaiResult {
        reduce_rev_with_initial(ctx, array, FnPtr::new(reducer)?, initial)
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
    /// * Any integer > 0 if `element1 > element2`
    /// * Zero if `element1 == element2`
    /// * Any integer < 0 if `element1 < element2`
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
    #[rhai_fn(return_raw)]
    pub fn sort(ctx: NativeCallContext, array: &mut Array, comparer: FnPtr) -> RhaiResultOf<()> {
        if array.len() <= 1 {
            return Ok(());
        }

        array.sort_by(|x, y| {
            comparer
                .call_raw(&ctx, None, [x.clone(), y.clone()])
                .ok()
                .and_then(|v| v.as_int().ok())
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

        Ok(())
    }
    /// Sort the array based on applying a function named by `comparer`.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `comparer` must exist taking these parameters:
    ///
    /// * `element1`: copy of the current array element to compare
    /// * `element2`: copy of the next array element to compare
    ///
    /// ## Return Value
    ///
    /// * Any integer > 0 if `element1 > element2`
    /// * Zero if `element1 == element2`
    /// * Any integer < 0 if `element1 < element2`
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn reverse(a, b) {
    ///     if a > b {
    ///         -1
    ///     } else if a < b {
    ///         1
    ///     } else {
    ///         0
    ///     }
    /// }
    /// let x = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];
    ///
    /// x.sort("reverse");
    ///
    /// print(x);       // prints "[10, 9, 8, 7, 6, 5, 4, 3, 2, 1]"
    /// ```
    #[rhai_fn(name = "sort", return_raw)]
    pub fn sort_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        comparer: &str,
    ) -> RhaiResultOf<()> {
        sort(ctx, array, FnPtr::new(comparer)?)
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
                let a = a.as_int().expect("`INT`");
                let b = b.as_int().expect("`INT`");
                a.cmp(&b)
            });
            return Ok(());
        }
        if type_id == TypeId::of::<char>() {
            array.sort_by(|a, b| {
                let a = a.as_char().expect("char");
                let b = b.as_char().expect("char");
                a.cmp(&b)
            });
            return Ok(());
        }
        #[cfg(not(feature = "no_float"))]
        if type_id == TypeId::of::<crate::FLOAT>() {
            array.sort_by(|a, b| {
                let a = a.as_float().expect("`FLOAT`");
                let b = b.as_float().expect("`FLOAT`");
                a.partial_cmp(&b).unwrap_or(Ordering::Equal)
            });
            return Ok(());
        }
        if type_id == TypeId::of::<ImmutableString>() {
            array.sort_by(|a, b| {
                let a = a.read_lock::<ImmutableString>().expect("`ImmutableString`");
                let b = b.read_lock::<ImmutableString>().expect("`ImmutableString`");
                a.as_str().cmp(b.as_str())
            });
            return Ok(());
        }
        #[cfg(feature = "decimal")]
        if type_id == TypeId::of::<rust_decimal::Decimal>() {
            array.sort_by(|a, b| {
                let a = a.as_decimal().expect("`Decimal`");
                let b = b.as_decimal().expect("`Decimal`");
                a.cmp(&b)
            });
            return Ok(());
        }
        if type_id == TypeId::of::<bool>() {
            array.sort_by(|a, b| {
                let a = a.as_bool().expect("`bool`");
                let b = b.as_bool().expect("`bool`");
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
            if filter
                .call_raw(&ctx, None, [array[x].clone()])
                .or_else(|err| match *err {
                    ERR::ErrorFunctionNotFound(fn_sig, ..)
                        if fn_sig.starts_with(filter.fn_name()) =>
                    {
                        filter.call_raw(&ctx, None, [array[x].clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(ERR::ErrorInFunctionCall(
                        "drain".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?
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
    /// Remove all elements in the array that returns `true` when applied a function named by `filter`
    /// and return them as a new array.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn small(x) { x < 3 }
    ///
    /// fn screen(x, i) { x + i > 5 }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.drain("small");
    ///
    /// print(x);       // prints "[3, 4, 5]"
    ///
    /// print(y);       // prints "[1, 2]"
    ///
    /// let z = x.drain("screen");
    ///
    /// print(x);       // prints "[3, 4]"
    ///
    /// print(z);       // prints "[5]"
    /// ```
    #[rhai_fn(name = "drain", return_raw)]
    pub fn drain_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: &str,
    ) -> RhaiResultOf<Array> {
        drain(ctx, array, FnPtr::new(filter)?)
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
            Array::new()
        } else {
            array.drain(start..start + len).collect()
        }
    }
    /// Remove all elements in the array that do not return `true` when applied the `filter`
    /// function and return them as a new array.
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
            if filter
                .call_raw(&ctx, None, [array[x].clone()])
                .or_else(|err| match *err {
                    ERR::ErrorFunctionNotFound(fn_sig, ..)
                        if fn_sig.starts_with(filter.fn_name()) =>
                    {
                        filter.call_raw(&ctx, None, [array[x].clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(ERR::ErrorInFunctionCall(
                        "retain".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?
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
    /// Remove all elements in the array that do not return `true` when applied a function named by
    /// `filter` and return them as a new array.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn large(x) { x >= 3 }
    ///
    /// fn screen(x, i) { x + i <= 5 }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.retain("large");
    ///
    /// print(x);       // prints "[3, 4, 5]"
    ///
    /// print(y);       // prints "[1, 2]"
    ///
    /// let z = x.retain("screen");
    ///
    /// print(x);       // prints "[3, 4]"
    ///
    /// print(z);       // prints "[5]"
    /// ```
    #[rhai_fn(name = "retain", return_raw)]
    pub fn retain_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: &str,
    ) -> RhaiResultOf<Array> {
        retain(ctx, array, FnPtr::new(filter)?)
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
            Array::new()
        } else {
            let mut drained: Array = array.drain(..start).collect();
            drained.extend(array.drain(len..));

            drained
        }
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
                .call_fn_raw(OP_EQUALS, true, false, &mut [a1, a2])
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
