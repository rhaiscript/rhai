//! Data size checks during evaluation.
#![cfg(not(feature = "unchecked"))]

use super::GlobalRuntimeState;
use crate::types::dynamic::Union;
use crate::{Dynamic, Engine, Position, RhaiResultOf, ERR};
use std::borrow::Borrow;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Recursively calculate the sizes of an array.
///
/// Sizes returned are `(` [`Array`][crate::Array], [`Map`][crate::Map] and [`String`] `)`.
///
/// # Panics
///
/// Panics if any interior data is shared (should never happen).
#[cfg(not(feature = "no_index"))]
#[inline]
pub fn calc_array_sizes(array: &crate::Array) -> (usize, usize, usize) {
    let (mut ax, mut mx, mut sx) = (0, 0, 0);

    for value in array {
        ax += 1;

        match value.0 {
            Union::Array(ref a, ..) => {
                let (a, m, s) = calc_array_sizes(a);
                ax += a;
                mx += m;
                sx += s;
            }
            Union::Blob(ref a, ..) => ax += 1 + a.len(),
            #[cfg(not(feature = "no_object"))]
            Union::Map(ref m, ..) => {
                let (a, m, s) = calc_map_sizes(m);
                ax += a;
                mx += m;
                sx += s;
            }
            Union::Str(ref s, ..) => sx += s.len(),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(..) => {
                unreachable!("shared values discovered within data")
            }
            _ => (),
        }
    }

    (ax, mx, sx)
}
/// Recursively calculate the sizes of a map.
///
/// Sizes returned are `(` [`Array`][crate::Array], [`Map`][crate::Map] and [`String`] `)`.
///
/// # Panics
///
/// Panics if any interior data is shared (should never happen).
#[cfg(not(feature = "no_object"))]
#[inline]
pub fn calc_map_sizes(map: &crate::Map) -> (usize, usize, usize) {
    let (mut ax, mut mx, mut sx) = (0, 0, 0);

    for value in map.values() {
        mx += 1;

        match value.0 {
            #[cfg(not(feature = "no_index"))]
            Union::Array(ref a, ..) => {
                let (a, m, s) = calc_array_sizes(a);
                ax += a;
                mx += m;
                sx += s;
            }
            #[cfg(not(feature = "no_index"))]
            Union::Blob(ref a, ..) => ax += 1 + a.len(),
            Union::Map(ref m, ..) => {
                let (a, m, s) = calc_map_sizes(m);
                ax += a;
                mx += m;
                sx += s;
            }
            Union::Str(ref s, ..) => sx += s.len(),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(..) => {
                unreachable!("shared values discovered within data")
            }
            _ => (),
        }
    }

    (ax, mx, sx)
}

/// Recursively calculate the sizes of a value.
///
/// Sizes returned are `(` [`Array`][crate::Array], [`Map`][crate::Map] and [`String`] `)`.
///
/// # Panics
///
/// Panics if any interior data is shared (should never happen).
#[inline]
pub fn calc_data_sizes(value: &Dynamic, _top: bool) -> (usize, usize, usize) {
    match value.0 {
        #[cfg(not(feature = "no_index"))]
        Union::Array(ref arr, ..) => calc_array_sizes(arr),
        #[cfg(not(feature = "no_index"))]
        Union::Blob(ref blob, ..) => (blob.len(), 0, 0),
        #[cfg(not(feature = "no_object"))]
        Union::Map(ref map, ..) => calc_map_sizes(map),
        Union::Str(ref s, ..) => (0, 0, s.len()),
        #[cfg(not(feature = "no_closure"))]
        Union::Shared(..) if _top => calc_data_sizes(&value.read_lock::<Dynamic>().unwrap(), true),
        #[cfg(not(feature = "no_closure"))]
        Union::Shared(..) => {
            unreachable!("shared values discovered within data: {}", value)
        }
        _ => (0, 0, 0),
    }
}

impl Engine {
    /// Raise an error if any data size exceeds limit.
    ///
    /// [`Position`] in [`EvalAltResult`][crate::EvalAltResult] is always [`NONE`][Position::NONE]
    /// and should be set afterwards.
    #[cfg(not(feature = "unchecked"))]
    pub(crate) fn throw_on_size(&self, (_arr, _map, s): (usize, usize, usize)) -> RhaiResultOf<()> {
        if self.limits.string_len.map_or(false, |max| s > max.get()) {
            return Err(
                ERR::ErrorDataTooLarge("Length of string".to_string(), Position::NONE).into(),
            );
        }

        #[cfg(not(feature = "no_index"))]
        if self.limits.array_size.map_or(false, |max| _arr > max.get()) {
            return Err(
                ERR::ErrorDataTooLarge("Size of array/BLOB".to_string(), Position::NONE).into(),
            );
        }

        #[cfg(not(feature = "no_object"))]
        if self.limits.map_size.map_or(false, |max| _map > max.get()) {
            return Err(
                ERR::ErrorDataTooLarge("Size of object map".to_string(), Position::NONE).into(),
            );
        }

        Ok(())
    }

    /// Check whether the size of a [`Dynamic`] is within limits.
    #[cfg(not(feature = "unchecked"))]
    #[inline]
    pub(crate) fn check_data_size<T: Borrow<Dynamic>>(
        &self,
        value: T,
        pos: Position,
    ) -> RhaiResultOf<T> {
        // If no data size limits, just return
        if !self.has_data_size_limit() {
            return Ok(value);
        }

        let sizes = calc_data_sizes(value.borrow(), true);

        self.throw_on_size(sizes)
            .map_err(|err| err.fill_position(pos))?;

        Ok(value)
    }

    /// Raise an error if the size of a [`Dynamic`] is out of limits (if any).
    ///
    /// Not available under `unchecked`.
    #[cfg(not(feature = "unchecked"))]
    #[inline(always)]
    pub fn ensure_data_size_within_limits(&self, value: &Dynamic) -> RhaiResultOf<()> {
        self.check_data_size(value, Position::NONE).map(|_| ())
    }

    /// Check if the number of operations stay within limit.
    #[inline(always)]
    pub(crate) fn track_operation(
        &self,
        global: &mut GlobalRuntimeState,
        pos: Position,
    ) -> RhaiResultOf<()> {
        global.num_operations += 1;

        // Guard against too many operations
        #[cfg(not(feature = "unchecked"))]
        if self.max_operations() > 0 && global.num_operations > self.max_operations() {
            return Err(ERR::ErrorTooManyOperations(pos).into());
        }

        self.progress
            .as_ref()
            .and_then(|progress| {
                progress(global.num_operations)
                    .map(|token| Err(ERR::ErrorTerminated(token, pos).into()))
            })
            .unwrap_or(Ok(()))
    }
}
