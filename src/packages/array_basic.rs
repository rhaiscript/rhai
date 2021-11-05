#![cfg(not(feature = "no_index"))]
#![allow(non_snake_case)]

use crate::engine::OP_EQUALS;
use crate::plugin::*;
use crate::{def_package, Array, Dynamic, EvalAltResult, FnPtr, NativeCallContext, Position, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{any::TypeId, cmp::Ordering, mem};

def_package!(crate:BasicArrayPackage:"Basic array utilities.", lib, {
    lib.standard = true;

    combine_with_exported_module!(lib, "array", array_functions);

    // Register array iterator
    lib.set_iterable::<Array>();
});

#[export_module]
mod array_functions {
    #[rhai_fn(name = "len", get = "len", pure)]
    pub fn len(array: &mut Array) -> INT {
        array.len() as INT
    }
    #[rhai_fn(name = "push", name = "+=")]
    pub fn push(array: &mut Array, item: Dynamic) {
        array.push(item);
    }
    #[rhai_fn(name = "append", name = "+=")]
    pub fn append(array: &mut Array, y: Array) {
        if !y.is_empty() {
            if array.is_empty() {
                *array = y;
            } else {
                array.extend(y);
            }
        }
    }
    #[rhai_fn(name = "+")]
    pub fn concat(mut array: Array, y: Array) -> Array {
        if !y.is_empty() {
            if array.is_empty() {
                array = y;
            } else {
                array.extend(y);
            }
        }
        array
    }
    pub fn insert(array: &mut Array, position: INT, item: Dynamic) {
        if array.is_empty() {
            array.push(item);
        } else if position < 0 {
            if let Some(n) = position.checked_abs() {
                if n as usize > array.len() {
                    array.insert(0, item);
                } else {
                    array.insert(array.len() - n as usize, item);
                }
            } else {
                array.insert(0, item);
            }
        } else if (position as usize) >= array.len() {
            array.push(item);
        } else {
            array.insert(position as usize, item);
        }
    }
    #[rhai_fn(return_raw)]
    pub fn pad(
        _ctx: NativeCallContext,
        array: &mut Array,
        len: INT,
        item: Dynamic,
    ) -> Result<(), Box<EvalAltResult>> {
        if len <= 0 {
            return Ok(());
        }

        // Check if array will be over max size limit
        #[cfg(not(feature = "unchecked"))]
        if _ctx.engine().max_array_size() > 0 && (len as usize) > _ctx.engine().max_array_size() {
            return EvalAltResult::ErrorDataTooLarge("Size of array".to_string(), Position::NONE)
                .into();
        }

        if len as usize > array.len() {
            array.resize(len as usize, item);
        }

        Ok(())
    }
    pub fn pop(array: &mut Array) -> Dynamic {
        if array.is_empty() {
            Dynamic::UNIT
        } else {
            array.pop().unwrap_or_else(|| Dynamic::UNIT)
        }
    }
    pub fn shift(array: &mut Array) -> Dynamic {
        if array.is_empty() {
            Dynamic::UNIT
        } else {
            array.remove(0)
        }
    }
    pub fn remove(array: &mut Array, len: INT) -> Dynamic {
        if len < 0 || (len as usize) >= array.len() {
            Dynamic::UNIT
        } else {
            array.remove(len as usize)
        }
    }
    pub fn clear(array: &mut Array) {
        if !array.is_empty() {
            array.clear();
        }
    }
    pub fn truncate(array: &mut Array, len: INT) {
        if !array.is_empty() {
            if len >= 0 {
                array.truncate(len as usize);
            } else {
                array.clear();
            }
        }
    }
    pub fn chop(array: &mut Array, len: INT) {
        if !array.is_empty() && len as usize >= array.len() {
            if len >= 0 {
                array.drain(0..array.len() - len as usize);
            } else {
                array.clear();
            }
        }
    }
    pub fn reverse(array: &mut Array) {
        if !array.is_empty() {
            array.reverse();
        }
    }
    pub fn splice(array: &mut Array, start: INT, len: INT, replace: Array) {
        if array.is_empty() {
            *array = replace;
            return;
        }

        let start = if start < 0 {
            let arr_len = array.len();
            start
                .checked_abs()
                .map_or(0, |n| arr_len - (n as usize).min(arr_len))
        } else if start as usize >= array.len() {
            array.extend(replace.into_iter());
            return;
        } else {
            start as usize
        };

        let len = if len < 0 {
            0
        } else if len as usize > array.len() - start {
            array.len() - start
        } else {
            len as usize
        };

        array.splice(start..start + len, replace.into_iter());
    }
    pub fn extract(array: &mut Array, start: INT, len: INT) -> Array {
        if array.is_empty() || len <= 0 {
            return Array::new();
        }

        let start = if start < 0 {
            let arr_len = array.len();
            start
                .checked_abs()
                .map_or(0, |n| arr_len - (n as usize).min(arr_len))
        } else if start as usize >= array.len() {
            return Array::new();
        } else {
            start as usize
        };

        let len = if len <= 0 {
            0
        } else if len as usize > array.len() - start {
            array.len() - start
        } else {
            len as usize
        };

        if len == 0 {
            Array::new()
        } else {
            array[start..start + len].to_vec()
        }
    }
    #[rhai_fn(name = "extract")]
    pub fn extract_tail(array: &mut Array, start: INT) -> Array {
        if array.is_empty() {
            return Array::new();
        }

        let start = if start < 0 {
            let arr_len = array.len();
            start
                .checked_abs()
                .map_or(0, |n| arr_len - (n as usize).min(arr_len))
        } else if start as usize >= array.len() {
            return Array::new();
        } else {
            start as usize
        };

        array[start..].to_vec()
    }
    #[rhai_fn(name = "split")]
    pub fn split_at(array: &mut Array, start: INT) -> Array {
        if array.is_empty() {
            Array::new()
        } else if start < 0 {
            if let Some(n) = start.checked_abs() {
                if n as usize > array.len() {
                    mem::take(array)
                } else {
                    let mut result = Array::new();
                    result.extend(array.drain(array.len() - n as usize..));
                    result
                }
            } else {
                mem::take(array)
            }
        } else if start as usize >= array.len() {
            Array::new()
        } else {
            let mut result = Array::new();
            result.extend(array.drain(start as usize..));
            result
        }
    }
    #[rhai_fn(return_raw, pure)]
    pub fn map(
        ctx: NativeCallContext,
        array: &mut Array,
        mapper: FnPtr,
    ) -> Result<Array, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(array.clone());
        }

        let mut ar = Array::with_capacity(array.len());

        for (i, item) in array.iter().enumerate() {
            ar.push(
                mapper
                    .call_dynamic(&ctx, None, [item.clone()])
                    .or_else(|err| match *err {
                        EvalAltResult::ErrorFunctionNotFound(fn_sig, _)
                            if fn_sig.starts_with(mapper.fn_name()) =>
                        {
                            mapper.call_dynamic(&ctx, None, [item.clone(), (i as INT).into()])
                        }
                        _ => Err(err),
                    })
                    .map_err(|err| {
                        Box::new(EvalAltResult::ErrorInFunctionCall(
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
    #[rhai_fn(return_raw, pure)]
    pub fn filter(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: FnPtr,
    ) -> Result<Array, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(array.clone());
        }

        let mut ar = Array::new();

        for (i, item) in array.iter().enumerate() {
            if filter
                .call_dynamic(&ctx, None, [item.clone()])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(fn_sig, _)
                        if fn_sig.starts_with(filter.fn_name()) =>
                    {
                        filter.call_dynamic(&ctx, None, [item.clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(EvalAltResult::ErrorInFunctionCall(
                        "filter".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?
                .as_bool()
                .unwrap_or(false)
            {
                ar.push(item.clone());
            }
        }

        Ok(ar)
    }
    #[rhai_fn(return_raw, pure)]
    pub fn contains(
        ctx: NativeCallContext,
        array: &mut Array,
        value: Dynamic,
    ) -> Result<bool, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(false);
        }

        for item in array.iter_mut() {
            if ctx
                .call_fn_dynamic_raw(OP_EQUALS, true, &mut [item, &mut value.clone()])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(ref fn_sig, _)
                        if fn_sig.starts_with(OP_EQUALS) =>
                    {
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
    #[rhai_fn(return_raw, pure)]
    pub fn index_of(
        ctx: NativeCallContext,
        array: &mut Array,
        value: Dynamic,
    ) -> Result<INT, Box<EvalAltResult>> {
        if array.is_empty() {
            Ok(-1)
        } else {
            index_of_starting_from(ctx, array, value, 0)
        }
    }
    #[rhai_fn(name = "index_of", return_raw, pure)]
    pub fn index_of_starting_from(
        ctx: NativeCallContext,
        array: &mut Array,
        value: Dynamic,
        start: INT,
    ) -> Result<INT, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(-1);
        }

        let start = if start < 0 {
            let arr_len = array.len();
            start
                .checked_abs()
                .map_or(0, |n| arr_len - (n as usize).min(arr_len))
        } else if start as usize >= array.len() {
            return Ok(-1);
        } else {
            start as usize
        };

        for (i, item) in array.iter_mut().enumerate().skip(start) {
            if ctx
                .call_fn_dynamic_raw(OP_EQUALS, true, &mut [item, &mut value.clone()])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(ref fn_sig, _)
                        if fn_sig.starts_with(OP_EQUALS) =>
                    {
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
    #[rhai_fn(name = "index_of", return_raw, pure)]
    pub fn index_of_filter(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: FnPtr,
    ) -> Result<INT, Box<EvalAltResult>> {
        if array.is_empty() {
            Ok(-1)
        } else {
            index_of_filter_starting_from(ctx, array, filter, 0)
        }
    }
    #[rhai_fn(name = "index_of", return_raw, pure)]
    pub fn index_of_filter_starting_from(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: FnPtr,
        start: INT,
    ) -> Result<INT, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(-1);
        }

        let start = if start < 0 {
            let arr_len = array.len();
            start
                .checked_abs()
                .map_or(0, |n| arr_len - (n as usize).min(arr_len))
        } else if start as usize >= array.len() {
            return Ok(-1);
        } else {
            start as usize
        };

        for (i, item) in array.iter().enumerate().skip(start) {
            if filter
                .call_dynamic(&ctx, None, [item.clone()])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(fn_sig, _)
                        if fn_sig.starts_with(filter.fn_name()) =>
                    {
                        filter.call_dynamic(&ctx, None, [item.clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(EvalAltResult::ErrorInFunctionCall(
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
    #[rhai_fn(return_raw, pure)]
    pub fn some(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: FnPtr,
    ) -> Result<bool, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(false);
        }

        for (i, item) in array.iter().enumerate() {
            if filter
                .call_dynamic(&ctx, None, [item.clone()])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(fn_sig, _)
                        if fn_sig.starts_with(filter.fn_name()) =>
                    {
                        filter.call_dynamic(&ctx, None, [item.clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(EvalAltResult::ErrorInFunctionCall(
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
    #[rhai_fn(return_raw, pure)]
    pub fn all(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: FnPtr,
    ) -> Result<bool, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(true);
        }

        for (i, item) in array.iter().enumerate() {
            if !filter
                .call_dynamic(&ctx, None, [item.clone()])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(fn_sig, _)
                        if fn_sig.starts_with(filter.fn_name()) =>
                    {
                        filter.call_dynamic(&ctx, None, [item.clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(EvalAltResult::ErrorInFunctionCall(
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
    #[rhai_fn(return_raw, pure)]
    pub fn reduce(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: FnPtr,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(Dynamic::UNIT);
        }

        let mut result = Dynamic::UNIT;

        for (i, item) in array.iter().enumerate() {
            result = reducer
                .call_dynamic(&ctx, None, [result.clone(), item.clone()])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(fn_sig, _)
                        if fn_sig.starts_with(reducer.fn_name()) =>
                    {
                        reducer.call_dynamic(&ctx, None, [result, item.clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(EvalAltResult::ErrorInFunctionCall(
                        "reduce".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?;
        }

        Ok(result)
    }
    #[rhai_fn(name = "reduce", return_raw, pure)]
    pub fn reduce_with_initial(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: FnPtr,
        initial: Dynamic,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(initial);
        }

        let mut result = initial;

        for (i, item) in array.iter().enumerate() {
            result = reducer
                .call_dynamic(&ctx, None, [result.clone(), item.clone()])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(fn_sig, _)
                        if fn_sig.starts_with(reducer.fn_name()) =>
                    {
                        reducer.call_dynamic(&ctx, None, [result, item.clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(EvalAltResult::ErrorInFunctionCall(
                        "reduce".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?;
        }

        Ok(result)
    }
    #[rhai_fn(return_raw, pure)]
    pub fn reduce_rev(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: FnPtr,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(Dynamic::UNIT);
        }

        let mut result = Dynamic::UNIT;

        for (i, item) in array.iter().enumerate().rev() {
            result = reducer
                .call_dynamic(&ctx, None, [result.clone(), item.clone()])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(fn_sig, _)
                        if fn_sig.starts_with(reducer.fn_name()) =>
                    {
                        reducer.call_dynamic(&ctx, None, [result, item.clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(EvalAltResult::ErrorInFunctionCall(
                        "reduce_rev".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?;
        }

        Ok(result)
    }
    #[rhai_fn(name = "reduce_rev", return_raw, pure)]
    pub fn reduce_rev_with_initial(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: FnPtr,
        initial: Dynamic,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(initial);
        }

        let mut result = initial;

        for (i, item) in array.iter().enumerate().rev() {
            result = reducer
                .call_dynamic(&ctx, None, [result.clone(), item.clone()])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(fn_sig, _)
                        if fn_sig.starts_with(reducer.fn_name()) =>
                    {
                        reducer.call_dynamic(&ctx, None, [result, item.clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(EvalAltResult::ErrorInFunctionCall(
                        "reduce_rev".to_string(),
                        ctx.source().unwrap_or("").to_string(),
                        err,
                        Position::NONE,
                    ))
                })?;
        }

        Ok(result)
    }
    #[rhai_fn(return_raw)]
    pub fn sort(
        ctx: NativeCallContext,
        array: &mut Array,
        comparer: FnPtr,
    ) -> Result<(), Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(());
        }

        array.sort_by(|x, y| {
            comparer
                .call_dynamic(&ctx, None, [x.clone(), y.clone()])
                .ok()
                .and_then(|v| v.as_int().ok())
                .map(|v| match v {
                    v if v > 0 => Ordering::Greater,
                    v if v < 0 => Ordering::Less,
                    0 => Ordering::Equal,
                    _ => unreachable!(),
                })
                .unwrap_or_else(|| x.type_id().cmp(&y.type_id()))
        });

        Ok(())
    }
    #[rhai_fn(return_raw)]
    pub fn drain(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: FnPtr,
    ) -> Result<Array, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(Array::new());
        }

        let mut drained = Array::with_capacity(array.len());

        let mut i = 0;
        let mut x = 0;

        while x < array.len() {
            if filter
                .call_dynamic(&ctx, None, [array[x].clone()])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(fn_sig, _)
                        if fn_sig.starts_with(filter.fn_name()) =>
                    {
                        filter.call_dynamic(&ctx, None, [array[x].clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(EvalAltResult::ErrorInFunctionCall(
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
    #[rhai_fn(name = "drain")]
    pub fn drain_range(array: &mut Array, start: INT, len: INT) -> Array {
        if array.is_empty() || len <= 0 {
            return Array::new();
        }

        let start = if start < 0 {
            let arr_len = array.len();
            start
                .checked_abs()
                .map_or(0, |n| arr_len - (n as usize).min(arr_len))
        } else if start as usize >= array.len() {
            return Array::new();
        } else {
            start as usize
        };

        let len = if len <= 0 {
            0
        } else if len as usize > array.len() - start {
            array.len() - start
        } else {
            len as usize
        };

        array.drain(start..start + len).collect()
    }
    #[rhai_fn(return_raw)]
    pub fn retain(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: FnPtr,
    ) -> Result<Array, Box<EvalAltResult>> {
        if array.is_empty() {
            return Ok(Array::new());
        }

        let mut drained = Array::new();

        let mut i = 0;
        let mut x = 0;

        while x < array.len() {
            if !filter
                .call_dynamic(&ctx, None, [array[x].clone()])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(fn_sig, _)
                        if fn_sig.starts_with(filter.fn_name()) =>
                    {
                        filter.call_dynamic(&ctx, None, [array[x].clone(), (i as INT).into()])
                    }
                    _ => Err(err),
                })
                .map_err(|err| {
                    Box::new(EvalAltResult::ErrorInFunctionCall(
                        "retain".to_string(),
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
    #[rhai_fn(name = "retain")]
    pub fn retain_range(array: &mut Array, start: INT, len: INT) -> Array {
        if array.is_empty() || len <= 0 {
            return Array::new();
        }

        let start = if start < 0 {
            let arr_len = array.len();
            start
                .checked_abs()
                .map_or(0, |n| arr_len - (n as usize).min(arr_len))
        } else if start as usize >= array.len() {
            return mem::take(array);
        } else {
            start as usize
        };

        let len = if len < 0 {
            0
        } else if len as usize > array.len() - start {
            array.len() - start
        } else {
            len as usize
        };

        let mut drained: Array = array.drain(..start).collect();
        drained.extend(array.drain(len..));

        drained
    }
    #[rhai_fn(name = "==", return_raw, pure)]
    pub fn equals(
        ctx: NativeCallContext,
        array1: &mut Array,
        array2: Array,
    ) -> Result<bool, Box<EvalAltResult>> {
        if array1.len() != array2.len() {
            return Ok(false);
        }
        if array1.is_empty() {
            return Ok(true);
        }

        let mut array2 = array2;

        for (a1, a2) in array1.iter_mut().zip(array2.iter_mut()) {
            if !ctx
                .call_fn_dynamic_raw(OP_EQUALS, true, &mut [a1, a2])
                .or_else(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(ref fn_sig, _)
                        if fn_sig.starts_with(OP_EQUALS) =>
                    {
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
    #[rhai_fn(name = "!=", return_raw, pure)]
    pub fn not_equals(
        ctx: NativeCallContext,
        array1: &mut Array,
        array2: Array,
    ) -> Result<bool, Box<EvalAltResult>> {
        equals(ctx, array1, array2).map(|r| !r)
    }
}
