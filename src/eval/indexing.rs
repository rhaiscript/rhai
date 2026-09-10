//! Types to support chaining operations (i.e. indexing and dotting).
#![cfg(any(not(feature = "no_index"), not(feature = "no_object")))]

use super::{Caches, GlobalRuntimeState, Target};
use crate::engine::{FN_IDX_GET, FN_IDX_SET};
use crate::types::dynamic::Union;
use crate::{calc_fn_hash, Dynamic, Engine, OnceCell, Position, RhaiResultOf, Scope, ERR};
use std::convert::TryInto;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Function call hashes to index getters and setters.
static INDEXER_HASHES: OnceCell<(u64, u64)> = OnceCell::new();

/// Get the pre-calculated index getter/setter hashes.
#[inline]
#[must_use]
fn hash_idx() -> (u64, u64) {
    *INDEXER_HASHES.get_or_init(|| {
        #[allow(clippy::useless_conversion)]
        (
            calc_fn_hash(None, FN_IDX_GET, 2),
            calc_fn_hash(None, FN_IDX_SET, 3),
        )
            .into()
    })
}

impl Engine {
    /// Get the value at the indexed position of a base type.
    ///
    /// # Panics
    ///
    /// Panics if the target object is shared.
    ///
    /// Shared objects should be handled (dereferenced) before calling this method.
    pub(crate) fn get_indexed_mut<'t>(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        _scope: &mut Scope,
        _this_ptr: Option<&mut Dynamic>,
        target: &'t mut Dynamic,
        idx: &mut Dynamic,
        idx_pos: Position,
        op_pos: Position,
        _add_if_not_found: bool,
        use_indexers: bool,
    ) -> RhaiResultOf<Target<'t>> {
        self.track_operation(global, Position::NONE)?;

        match target {
            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Array(arr, ..)) => {
                // val_array[idx]
                let index = idx
                    .as_int()
                    .map_err(|typ| self.make_type_mismatch_err::<crate::INT>(typ, idx_pos))?;
                let len = arr.len();

                let arr_idx = match super::calc_index(len, index, true)
                    .ok_or_else(|| ERR::ErrorArrayBounds(len, index, idx_pos))
                {
                    Ok(idx) => idx,
                    Err(err) => {
                        #[cfg(not(feature = "no_index"))]
                        #[cfg(feature = "internals")]
                        if let Some(ref cb) = self.invalid_array_index {
                            let context =
                                super::EvalContext::new(self, global, caches, _scope, _this_ptr);
                            return cb(arr, index, context)
                                .map_err(|err| err.fill_position(idx_pos));
                        }
                        return Err(err.into());
                    }
                };

                arr.get_mut(arr_idx).unwrap().try_into()
            }

            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Blob(arr, ..)) => {
                // val_blob[idx]
                let index = idx
                    .as_int()
                    .map_err(|typ| self.make_type_mismatch_err::<crate::INT>(typ, idx_pos))?;
                let len = arr.len();
                let arr_idx = super::calc_index(len, index, true)
                    .ok_or_else(|| ERR::ErrorArrayBounds(len, index, idx_pos))?;

                let value = arr.get(arr_idx).map(|&v| (v as crate::INT).into()).unwrap();

                Ok(Target::BlobByte {
                    source: target,
                    value,
                    index: arr_idx,
                })
            }

            #[cfg(not(feature = "no_object"))]
            Dynamic(Union::Map(map, ..)) => {
                // val_map[idx]
                let index = idx.read_lock::<crate::ImmutableString>().ok_or_else(|| {
                    self.make_type_mismatch_err::<crate::ImmutableString>(idx.type_name(), idx_pos)
                })?;

                #[cfg(not(feature = "no_object"))]
                #[cfg(feature = "internals")]
                if let Some(ref cb) = self.missing_map_property {
                    if !map.contains_key(index.as_str()) {
                        let context =
                            super::EvalContext::new(self, global, caches, _scope, _this_ptr);
                        return cb(map, index.as_str(), context)
                            .map_err(|err| err.fill_position(idx_pos));
                    }
                }

                if _add_if_not_found && (map.is_empty() || !map.contains_key(index.as_str())) {
                    map.insert(index.clone().into(), Dynamic::UNIT);
                }

                if let Some(value) = map.get_mut(index.as_str()) {
                    value.try_into()
                } else if self.fail_on_invalid_map_property() {
                    Err(ERR::ErrorPropertyNotFound(index.to_string(), idx_pos).into())
                } else {
                    Ok(Target::from(Dynamic::UNIT))
                }
            }

            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Int(value, ..))
                if idx.is::<crate::ExclusiveRange>() || idx.is::<crate::InclusiveRange>() =>
            {
                // val_int[range]
                let (shift, mask) = if let Some(range) = idx.read_lock::<crate::ExclusiveRange>() {
                    let start = range.start;
                    let end = if range.end == crate::INT::MAX {
                        crate::INT_BITS as crate::INT
                    } else {
                        range.end
                    };

                    let start = super::calc_index(crate::INT_BITS, start, false)
                        .ok_or_else(|| ERR::ErrorBitFieldBounds(crate::INT_BITS, start, idx_pos))?;
                    let end = super::calc_index(crate::INT_BITS, end, false)
                        .or_else(|| {
                            usize::try_from(end)
                                .ok()
                                .and_then(|x| (x <= crate::INT_BITS).then_some(x))
                        })
                        .ok_or_else(|| ERR::ErrorBitFieldBounds(crate::INT_BITS, end, idx_pos))?;

                    if end <= start {
                        (0, 0)
                    } else if end == crate::INT_BITS && start == 0 {
                        // -1 = all bits set
                        (0, -1)
                    } else {
                        (
                            u8::try_from(start).unwrap(),
                            // 2^bits - 1
                            (((2 as crate::UNSIGNED_INT).pow(u32::try_from(end - start).unwrap())
                                - 1) as crate::INT)
                                << start,
                        )
                    }
                } else if let Some(range) = idx.read_lock::<crate::InclusiveRange>() {
                    let start = *range.start();
                    let end = if *range.end() == crate::INT::MAX {
                        (crate::INT_BITS - 1) as crate::INT
                    } else {
                        *range.end()
                    };

                    let start = super::calc_index(crate::INT_BITS, start, false)
                        .ok_or_else(|| ERR::ErrorBitFieldBounds(crate::INT_BITS, start, idx_pos))?;
                    let end = super::calc_index(crate::INT_BITS, end, false)
                        .ok_or_else(|| ERR::ErrorBitFieldBounds(crate::INT_BITS, end, idx_pos))?;

                    if end < start {
                        (0, 0)
                    } else if end == crate::INT_BITS - 1 && start == 0 {
                        // -1 = all bits set
                        (0, -1)
                    } else {
                        (
                            u8::try_from(start).unwrap(),
                            // 2^bits - 1
                            (((2 as crate::UNSIGNED_INT)
                                .pow(u32::try_from(end - start + 1).unwrap())
                                - 1) as crate::INT)
                                << start,
                        )
                    }
                } else {
                    unreachable!("Range or RangeInclusive expected but gets {:?}", idx);
                };

                let field_value = (*value & mask) >> shift;

                Ok(Target::BitField {
                    source: target,
                    value: field_value.into(),
                    mask,
                    shift,
                })
            }

            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Int(value, ..)) => {
                // val_int[idx]
                let index = idx
                    .as_int()
                    .map_err(|typ| self.make_type_mismatch_err::<crate::INT>(typ, idx_pos))?;

                let bit = super::calc_index(crate::INT_BITS, index, true)
                    .ok_or_else(|| ERR::ErrorBitFieldBounds(crate::INT_BITS, index, idx_pos))?;

                let bit_value = (*value & (1 << bit)) != 0;
                let bit = u8::try_from(bit).unwrap();

                Ok(Target::Bit {
                    source: target,
                    value: bit_value.into(),
                    bit,
                })
            }

            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Str(s, ..)) => {
                // Numeric index - character
                match idx.as_int() {
                    Ok(index) => {
                        let (ch, offset) = if index >= 0 {
                            let Ok(offset) = usize::try_from(index) else {
                                return Err(ERR::ErrorStringBounds(
                                    s.chars().count(),
                                    index,
                                    idx_pos,
                                )
                                .into());
                            };

                            (
                                s.chars().nth(offset).ok_or_else(|| {
                                    ERR::ErrorStringBounds(s.chars().count(), index, idx_pos)
                                })?,
                                offset,
                            )
                        } else {
                            let Ok(offset) = usize::try_from(index.unsigned_abs()) else {
                                return Err(ERR::ErrorStringBounds(
                                    s.chars().count(),
                                    index,
                                    idx_pos,
                                )
                                .into());
                            };

                            (
                                // Count from end if negative
                                s.chars().rev().nth(offset - 1).ok_or_else(|| {
                                    ERR::ErrorStringBounds(s.chars().count(), index, idx_pos)
                                })?,
                                offset,
                            )
                        };

                        Ok(Target::StringChar {
                            source: target,
                            value: ch.into(),
                            index: offset,
                        })
                    }

                    // Range index on empty string - empty slice
                    Err(typ)
                        if (typ == std::any::type_name::<crate::ExclusiveRange>()
                            || typ == std::any::type_name::<crate::InclusiveRange>())
                            && s.is_empty() =>
                    {
                        let value = s.clone().into();
                        Ok(Target::StringSlice {
                            source: target,
                            value,
                            start: 0,
                            end: 0,
                            exclusive: true,
                        })
                    }

                    // Range index - slice
                    Err(typ) if typ == std::any::type_name::<crate::ExclusiveRange>() => {
                        // val_str[range]
                        let range = idx.read_lock::<crate::ExclusiveRange>().unwrap().clone();
                        let count = s.chars().count();

                        let start = if range.start >= 0 {
                            range.start as usize
                        } else {
                            super::calc_index(count, range.start, true).ok_or_else(|| {
                                ERR::ErrorStringBounds(count, range.start, idx_pos)
                            })?
                        };
                        let end = if range.end >= 0 {
                            range.end as usize
                        } else {
                            super::calc_index(count, range.end, true)
                                .ok_or_else(|| ERR::ErrorStringBounds(count, range.end, idx_pos))
                                .unwrap_or(0)
                        };

                        let value = if start == 0 && end >= count {
                            s.clone().into()
                        } else {
                            let take = end.saturating_sub(start);
                            s.chars().skip(start).take(take).collect::<String>().into()
                        };

                        Ok(Target::StringSlice {
                            source: target,
                            value,
                            start,
                            end,
                            exclusive: true,
                        })
                    }
                    Err(typ) if typ == std::any::type_name::<crate::InclusiveRange>() => {
                        // val_str[range]
                        let range = idx.read_lock::<crate::InclusiveRange>().unwrap().clone();
                        let start = *range.start();
                        let end = *range.end();
                        let count = s.chars().count();

                        let start = if start >= 0 {
                            start as usize
                        } else {
                            super::calc_index(count, start, true)
                                .ok_or_else(|| ERR::ErrorStringBounds(count, start, idx_pos))?
                        };
                        let end = if end >= 0 {
                            end as usize
                        } else {
                            super::calc_index(count, end, true)
                                .ok_or_else(|| ERR::ErrorStringBounds(count, end, idx_pos))
                                .unwrap_or(0)
                        };

                        let value = if start == 0 && end >= count - 1 {
                            s.clone().into()
                        } else {
                            let take = if end > start { end - start + 1 } else { 0 };
                            s.chars().skip(start).take(take).collect::<String>().into()
                        };

                        Ok(Target::StringSlice {
                            source: target,
                            value,
                            start,
                            end,
                            exclusive: false,
                        })
                    }

                    // Unsupported index type
                    Err(typ) => Err(self.make_type_mismatch_err::<crate::INT>(typ, idx_pos)),
                }
            }

            #[cfg(not(feature = "no_closure"))]
            Dynamic(Union::Shared(..)) => {
                unreachable!("`get_indexed_mut` cannot handle shared values")
            }

            _ if use_indexers => self
                .call_indexer_get(global, caches, target, idx, op_pos)
                .map(Into::into),

            _ => Err(ERR::ErrorIndexingType(
                format!(
                    "{} [{}]",
                    self.map_type_name(target.type_name()),
                    self.map_type_name(idx.type_name())
                ),
                op_pos,
            )
            .into()),
        }
    }

    /// Call a get indexer.
    #[inline]
    pub(crate) fn call_indexer_get(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        target: &mut Dynamic,
        idx: &mut Dynamic,
        pos: Position,
    ) -> RhaiResultOf<Dynamic> {
        defer! { let orig_level = global.level; global.level += 1 }

        let hash = hash_idx().0;
        let args = &mut [target, idx];

        self.exec_native_fn_call(
            global, caches, FN_IDX_GET, None, hash, args, true, false, pos,
        )
        .map(|(r, ..)| r)
    }

    /// Call a set indexer.
    #[inline]
    pub(crate) fn call_indexer_set(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        target: &mut Dynamic,
        idx: &mut Dynamic,
        new_val: &mut Dynamic,
        is_ref_mut: bool,
        pos: Position,
    ) -> RhaiResultOf<(Dynamic, bool)> {
        defer! { let orig_level = global.level; global.level += 1 }

        let hash = hash_idx().1;
        let args = &mut [target, idx, new_val];

        self.exec_native_fn_call(
            global, caches, FN_IDX_SET, None, hash, args, is_ref_mut, false, pos,
        )
    }
}
