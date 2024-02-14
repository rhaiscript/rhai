//! Types to support chaining operations (i.e. indexing and dotting).
#![cfg(any(not(feature = "no_index"), not(feature = "no_object")))]

use super::{Caches, GlobalRuntimeState, Target};
use crate::ast::{ASTFlags, BinaryExpr, Expr, OpAssignment};
use crate::engine::{FN_IDX_GET, FN_IDX_SET};
use crate::eval::search_namespace;
use crate::types::dynamic::Union;
use crate::{
    calc_fn_hash, Dynamic, Engine, FnArgsVec, OnceCell, Position, RhaiResult, RhaiResultOf, Scope,
    ERR,
};
use std::hash::Hash;
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

/// Method of chaining.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ChainType {
    /// Indexing.
    #[cfg(not(feature = "no_index"))]
    Indexing,
    /// Dotting.
    #[cfg(not(feature = "no_object"))]
    Dotting,
}

impl From<&Expr> for ChainType {
    #[inline(always)]
    fn from(expr: &Expr) -> Self {
        match expr {
            #[cfg(not(feature = "no_index"))]
            Expr::Index(..) => Self::Indexing,
            #[cfg(not(feature = "no_object"))]
            Expr::Dot(..) => Self::Dotting,
            expr => unreachable!("Expr::Index or Expr::Dot expected but gets {:?}", expr),
        }
    }
}

impl Engine {
    /// Call a get indexer.
    #[inline]
    fn call_indexer_get(
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
    fn call_indexer_set(
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

    /// Get the value at the indexed position of a base type.
    fn get_indexed_mut<'t>(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
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
                let arr_idx = super::calc_index(len, index, true, || {
                    ERR::ErrorArrayBounds(len, index, idx_pos).into()
                })?;

                Ok(arr.get_mut(arr_idx).map(Target::from).unwrap())
            }

            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Blob(arr, ..)) => {
                // val_blob[idx]
                let index = idx
                    .as_int()
                    .map_err(|typ| self.make_type_mismatch_err::<crate::INT>(typ, idx_pos))?;
                let len = arr.len();
                let arr_idx = super::calc_index(len, index, true, || {
                    ERR::ErrorArrayBounds(len, index, idx_pos).into()
                })?;

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

                if _add_if_not_found && (map.is_empty() || !map.contains_key(index.as_str())) {
                    map.insert(index.clone().into(), Dynamic::UNIT);
                }

                map.get_mut(index.as_str()).map_or_else(
                    || {
                        if self.fail_on_invalid_map_property() {
                            Err(ERR::ErrorPropertyNotFound(index.to_string(), idx_pos).into())
                        } else {
                            Ok(Target::from(Dynamic::UNIT))
                        }
                    },
                    |value| Ok(Target::from(value)),
                )
            }

            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Int(value, ..))
                if idx.is::<crate::ExclusiveRange>() || idx.is::<crate::InclusiveRange>() =>
            {
                // val_int[range]
                let (shift, mask) = if let Some(range) = idx.read_lock::<crate::ExclusiveRange>() {
                    let start = range.start;
                    let end = range.end;

                    let start = super::calc_index(crate::INT_BITS, start, false, || {
                        ERR::ErrorBitFieldBounds(crate::INT_BITS, start, idx_pos).into()
                    })?;
                    let end = super::calc_index(crate::INT_BITS, end, false, || {
                        ERR::ErrorBitFieldBounds(crate::INT_BITS, end, idx_pos).into()
                    })?;

                    #[allow(clippy::cast_possible_truncation)]
                    if end <= start {
                        (0, 0)
                    } else if end == crate::INT_BITS && start == 0 {
                        // -1 = all bits set
                        (0, -1)
                    } else {
                        (
                            start as u8,
                            // 2^bits - 1
                            (((2 as crate::UNSIGNED_INT).pow((end - start) as u32) - 1)
                                as crate::INT)
                                << start,
                        )
                    }
                } else if let Some(range) = idx.read_lock::<crate::InclusiveRange>() {
                    let start = *range.start();
                    let end = *range.end();

                    let start = super::calc_index(crate::INT_BITS, start, false, || {
                        ERR::ErrorBitFieldBounds(crate::INT_BITS, start, idx_pos).into()
                    })?;
                    let end = super::calc_index(crate::INT_BITS, end, false, || {
                        ERR::ErrorBitFieldBounds(crate::INT_BITS, end, idx_pos).into()
                    })?;

                    #[allow(clippy::cast_possible_truncation)]
                    if end < start {
                        (0, 0)
                    } else if end == crate::INT_BITS - 1 && start == 0 {
                        // -1 = all bits set
                        (0, -1)
                    } else {
                        (
                            start as u8,
                            // 2^bits - 1
                            (((2 as crate::UNSIGNED_INT).pow((end - start + 1) as u32) - 1)
                                as crate::INT)
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

                let bit = super::calc_index(crate::INT_BITS, index, true, || {
                    ERR::ErrorBitFieldBounds(crate::INT_BITS, index, idx_pos).into()
                })?;

                let bit_value = (*value & (1 << bit)) != 0;
                #[allow(clippy::cast_possible_truncation)]
                let bit = bit as u8;

                Ok(Target::Bit {
                    source: target,
                    value: bit_value.into(),
                    bit,
                })
            }

            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Str(s, ..)) => {
                // val_string[idx]
                let index = idx
                    .as_int()
                    .map_err(|typ| self.make_type_mismatch_err::<crate::INT>(typ, idx_pos))?;

                let (ch, offset) = if index >= 0 {
                    #[allow(clippy::absurd_extreme_comparisons)]
                    if index >= crate::MAX_USIZE_INT {
                        return Err(
                            ERR::ErrorStringBounds(s.chars().count(), index, idx_pos).into()
                        );
                    }

                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let offset = index as usize;
                    (
                        s.chars().nth(offset).ok_or_else(|| {
                            ERR::ErrorStringBounds(s.chars().count(), index, idx_pos)
                        })?,
                        offset,
                    )
                } else {
                    let abs_index = index.unsigned_abs();

                    #[allow(clippy::unnecessary_cast)]
                    if abs_index as u64 > usize::MAX as u64 {
                        return Err(
                            ERR::ErrorStringBounds(s.chars().count(), index, idx_pos).into()
                        );
                    }

                    #[allow(clippy::cast_possible_truncation)]
                    let offset = abs_index as usize;
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

    /// Evaluate a dot/index chain.
    pub(crate) fn eval_dot_index_chain(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        mut this_ptr: Option<&mut Dynamic>,
        expr: &Expr,
        new_val: Option<(Dynamic, &OpAssignment)>,
    ) -> RhaiResult {
        let BinaryExpr { lhs, rhs } = match expr {
            #[cfg(not(feature = "no_index"))]
            Expr::Index(x, ..) => &**x,
            #[cfg(not(feature = "no_object"))]
            Expr::Dot(x, ..) => &**x,
            expr => unreachable!("Expr::Index or Expr::Dot expected but gets {:?}", expr),
        };

        let idx_values = &mut FnArgsVec::new_const();

        match (rhs, ChainType::from(expr)) {
            // Short-circuit for simple property access: {expr}.prop
            #[cfg(not(feature = "no_object"))]
            (Expr::Property(..), ChainType::Dotting) => (),
            #[cfg(not(feature = "no_object"))]
            #[cfg(not(feature = "no_index"))]
            (Expr::Property(..), ChainType::Indexing) => {
                unreachable!("unexpected Expr::Property for indexing")
            }
            // Short-circuit for indexing with literal: {expr}[1]
            #[cfg(not(feature = "no_index"))]
            (_, ChainType::Indexing) if rhs.is_constant() => {
                idx_values.push(rhs.get_literal_value().unwrap())
            }
            // Short-circuit for simple method call: {expr}.func()
            #[cfg(not(feature = "no_object"))]
            (Expr::MethodCall(x, ..), ChainType::Dotting) if x.args.is_empty() => (),
            // All other patterns - evaluate the arguments chain
            _ => self.eval_dot_index_chain_arguments(
                global,
                caches,
                scope,
                this_ptr.as_deref_mut(),
                expr,
                rhs,
                idx_values,
            )?,
        }

        #[cfg(feature = "debugging")]
        let scope2 = &mut Scope::new();
        #[cfg(not(feature = "debugging"))]
        let scope2 = ();

        match (lhs, new_val) {
            // this.??? or this[???]
            (Expr::ThisPtr(var_pos), new_val) => {
                self.track_operation(global, *var_pos)?;

                #[cfg(feature = "debugging")]
                self.run_debugger(global, caches, scope, this_ptr.as_deref_mut(), lhs)?;

                this_ptr.map_or_else(
                    || Err(ERR::ErrorUnboundThis(*var_pos).into()),
                    |this_ptr| {
                        let target = &mut this_ptr.into();

                        self.eval_dot_index_chain_raw(
                            global, caches, scope2, None, lhs, expr, target, rhs, idx_values,
                            new_val,
                        )
                    },
                )
            }
            // id.??? or id[???]
            (Expr::Variable(.., var_pos), new_val) => {
                self.track_operation(global, *var_pos)?;

                #[cfg(feature = "debugging")]
                self.run_debugger(global, caches, scope, this_ptr.as_deref_mut(), lhs)?;

                let target = &mut search_namespace(self, global, caches, scope, this_ptr, lhs)?;

                self.eval_dot_index_chain_raw(
                    global, caches, scope2, None, lhs, expr, target, rhs, idx_values, new_val,
                )
            }
            // {expr}.??? = ??? or {expr}[???] = ???
            (_, Some(..)) => unreachable!("cannot assign to an expression"),
            // {expr}.??? or {expr}[???]
            (lhs_expr, None) => {
                let value = self
                    .eval_expr(global, caches, scope, this_ptr.as_deref_mut(), lhs_expr)?
                    .flatten();
                let obj_ptr = &mut value.into();

                self.eval_dot_index_chain_raw(
                    global, caches, scope2, this_ptr, lhs_expr, expr, obj_ptr, rhs, idx_values,
                    None,
                )
            }
        }
        .map(|(v, ..)| v)
    }

    /// Evaluate a chain of indexes and store the results in a [`FnArgsVec`].
    fn eval_dot_index_chain_arguments(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        mut this_ptr: Option<&mut Dynamic>,
        parent: &Expr,
        expr: &Expr,
        idx_values: &mut FnArgsVec<Dynamic>,
    ) -> RhaiResultOf<()> {
        self.track_operation(global, expr.position())?;

        match (expr, ChainType::from(parent)) {
            #[cfg(not(feature = "no_object"))]
            (Expr::MethodCall(x, ..), ChainType::Dotting) => {
                debug_assert!(
                    !x.is_qualified(),
                    "method call in dot chain should not be namespace-qualified"
                );

                for expr in &*x.args {
                    let arg_value =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    idx_values.push(arg_value.0.flatten());
                }
            }

            #[cfg(not(feature = "no_object"))]
            (Expr::Property(..), ChainType::Dotting) => (),

            (Expr::Index(x, ..) | Expr::Dot(x, ..), chain_type)
                if !parent.options().intersects(ASTFlags::BREAK) =>
            {
                let BinaryExpr { lhs, rhs, .. } = &**x;

                let mut _arg_values = FnArgsVec::new_const();

                // Evaluate in left-to-right order
                match (lhs, chain_type) {
                    #[cfg(not(feature = "no_object"))]
                    (Expr::Property(..), ChainType::Dotting) => (),

                    #[cfg(not(feature = "no_object"))]
                    (Expr::MethodCall(x, ..), ChainType::Dotting) => {
                        debug_assert!(
                            !x.is_qualified(),
                            "method call in dot chain should not be namespace-qualified"
                        );

                        for expr in &*x.args {
                            let tp = this_ptr.as_deref_mut();
                            let arg_value = self.get_arg_value(global, caches, scope, tp, expr)?;
                            _arg_values.push(arg_value.0.flatten());
                        }
                    }
                    #[cfg(not(feature = "no_index"))]
                    (_, ChainType::Indexing) => {
                        _arg_values.push(
                            self.eval_expr(global, caches, scope, this_ptr.as_deref_mut(), lhs)?
                                .flatten(),
                        );
                    }
                    #[allow(unreachable_patterns)]
                    (expr, chain_type) => {
                        unreachable!("unknown {:?} expression: {:?}", chain_type, expr)
                    }
                }

                // Push in reverse order
                self.eval_dot_index_chain_arguments(
                    global, caches, scope, this_ptr, expr, rhs, idx_values,
                )?;

                idx_values.extend(_arg_values);
            }

            #[cfg(not(feature = "no_index"))]
            (_, ChainType::Indexing) => idx_values.push(
                self.eval_expr(global, caches, scope, this_ptr, expr)?
                    .flatten(),
            ),
            #[allow(unreachable_patterns)]
            (expr, chain_type) => unreachable!("unknown {:?} expression: {:?}", chain_type, expr),
        }

        Ok(())
    }

    /// Chain-evaluate a dot/index chain.
    fn eval_dot_index_chain_raw(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        #[cfg(feature = "debugging")] _scope: &mut Scope,
        #[cfg(not(feature = "debugging"))] _scope: (),
        this_ptr: Option<&mut Dynamic>,
        root: &Expr,
        parent: &Expr,
        target: &mut Target,
        rhs: &Expr,
        idx_values: &mut FnArgsVec<Dynamic>,
        new_val: Option<(Dynamic, &OpAssignment)>,
    ) -> RhaiResultOf<(Dynamic, bool)> {
        let is_ref_mut = target.is_ref();
        let op_pos = parent.position();

        #[cfg(feature = "debugging")]
        #[allow(unused_mut)]
        let mut this_ptr = this_ptr;

        match ChainType::from(parent) {
            #[cfg(not(feature = "no_index"))]
            ChainType::Indexing => {
                // Check for existence with the null conditional operator
                if parent.options().intersects(ASTFlags::NEGATED) && target.as_ref().is_unit() {
                    return Ok((Dynamic::UNIT, false));
                }

                let pos = rhs.start_position();

                match (rhs, new_val) {
                    // xxx[idx].expr... | xxx[idx][expr]...
                    (Expr::Dot(x, ..) | Expr::Index(x, ..), new_val)
                        if !parent.options().intersects(ASTFlags::BREAK) =>
                    {
                        #[cfg(feature = "debugging")]
                        self.run_debugger(global, caches, _scope, this_ptr.as_deref_mut(), parent)?;

                        let idx_val = &mut idx_values.pop().unwrap();
                        let mut idx_val_for_setter = idx_val.clone();
                        let idx_pos = x.lhs.start_position();

                        let (try_setter, result) = {
                            let target = target.as_mut();
                            let mut obj = self.get_indexed_mut(
                                global, caches, target, idx_val, idx_pos, op_pos, false, true,
                            )?;
                            let is_obj_temp_val = obj.is_temp_value();
                            let obj_ptr = &mut obj;

                            match self.eval_dot_index_chain_raw(
                                global, caches, _scope, this_ptr, root, rhs, obj_ptr, &x.rhs,
                                idx_values, new_val,
                            ) {
                                Ok((result, true)) if is_obj_temp_val => {
                                    (Some(obj.take_or_clone()), (result, true))
                                }
                                Ok(result) => (None, result),
                                Err(err) => return Err(err),
                            }
                        };

                        if let Some(mut new_val) = try_setter {
                            // Try to call index setter if value is changed
                            let target = target.as_mut();
                            let idx = &mut idx_val_for_setter;
                            let new_val = &mut new_val;
                            // The return value of a indexer setter (usually `()`) is thrown away and not used.
                            let _ = self
                                .call_indexer_set(
                                    global, caches, target, idx, new_val, is_ref_mut, op_pos,
                                )
                                .or_else(|e| match *e {
                                    ERR::ErrorIndexingType(..) => Ok((Dynamic::UNIT, false)),
                                    _ => Err(e),
                                })?;
                        }

                        Ok(result)
                    }
                    // xxx[rhs] op= new_val
                    (_, Some((new_val, op_info))) => {
                        #[cfg(feature = "debugging")]
                        self.run_debugger(global, caches, _scope, this_ptr, parent)?;

                        let target = target.as_mut();
                        let idx_val = &mut idx_values.pop().unwrap();
                        let idx = &mut idx_val.clone();

                        let try_setter = match self
                            .get_indexed_mut(global, caches, target, idx, pos, op_pos, true, false)
                        {
                            // Indexed value is not a temp value - update directly
                            Ok(ref mut obj_ptr) => {
                                self.eval_op_assignment(
                                    global, caches, op_info, root, obj_ptr, new_val,
                                )?;
                                self.check_data_size(obj_ptr.as_ref(), op_info.position())?;
                                None
                            }
                            // Indexed value cannot be referenced - use indexer
                            #[cfg(not(feature = "no_index"))]
                            Err(err) if matches!(*err, ERR::ErrorIndexingType(..)) => Some(new_val),
                            // Any other error
                            Err(err) => return Err(err),
                        };

                        if let Some(mut new_val) = try_setter {
                            // Is this an op-assignment?
                            if op_info.is_op_assignment() {
                                let idx = &mut idx_val.clone();

                                // Call the index getter to get the current value
                                if let Ok(val) =
                                    self.call_indexer_get(global, caches, target, idx, op_pos)
                                {
                                    let mut val = val.into();
                                    // Run the op-assignment
                                    self.eval_op_assignment(
                                        global, caches, op_info, root, &mut val, new_val,
                                    )?;
                                    // Replace new value
                                    new_val = val.take_or_clone();
                                    self.check_data_size(&new_val, op_info.position())?;
                                }
                            }

                            // Try to call index setter
                            let new_val = &mut new_val;
                            // The return value of a indexer setter (usually `()`) is thrown away and not used.
                            let _ = self.call_indexer_set(
                                global, caches, target, idx_val, new_val, is_ref_mut, op_pos,
                            )?;
                        }

                        Ok((Dynamic::UNIT, true))
                    }
                    // xxx[rhs]
                    (_, None) => {
                        #[cfg(feature = "debugging")]
                        self.run_debugger(global, caches, _scope, this_ptr, parent)?;

                        let target = target.as_mut();
                        let idx_val = &mut idx_values.pop().unwrap();

                        self.get_indexed_mut(
                            global, caches, target, idx_val, pos, op_pos, false, true,
                        )
                        .map(|v| (v.take_or_clone(), false))
                    }
                }
            }

            #[cfg(not(feature = "no_object"))]
            ChainType::Dotting => {
                // Check for existence with the Elvis operator
                if parent.options().intersects(ASTFlags::NEGATED) && target.as_ref().is_unit() {
                    return Ok((Dynamic::UNIT, false));
                }

                match (rhs, new_val, target.as_ref().is_map()) {
                    // xxx.fn_name(...) = ???
                    (Expr::MethodCall(..), Some(..), ..) => {
                        unreachable!("method call cannot be assigned to")
                    }
                    // xxx.fn_name(arg_expr_list)
                    (Expr::MethodCall(x, pos), None, ..) => {
                        debug_assert!(
                            !x.is_qualified(),
                            "method call in dot chain should not be namespace-qualified"
                        );

                        #[cfg(feature = "debugging")]
                        let reset =
                            self.run_debugger_with_reset(global, caches, _scope, this_ptr, rhs)?;
                        #[cfg(feature = "debugging")]
                        defer! { global if Some(reset) => move |g| g.debugger_mut().reset_status(reset) }

                        let crate::ast::FnCallExpr {
                            name, hashes, args, ..
                        } = &**x;

                        // Truncate the index values upon exit
                        defer! { idx_values => truncate; let offset = idx_values.len() - args.len(); }

                        let call_args = &mut idx_values[offset..];
                        let arg1_pos = args.first().map_or(Position::NONE, Expr::position);

                        self.make_method_call(
                            global, caches, name, *hashes, target, call_args, arg1_pos, *pos,
                        )
                    }
                    // {xxx:map}.id op= ???
                    (Expr::Property(x, pos), Some((new_val, op_info)), true) => {
                        #[cfg(feature = "debugging")]
                        self.run_debugger(global, caches, _scope, this_ptr, rhs)?;

                        let index = &mut x.2.clone().into();
                        {
                            let target = target.as_mut();
                            let val_target = &mut self.get_indexed_mut(
                                global, caches, target, index, *pos, op_pos, true, false,
                            )?;
                            self.eval_op_assignment(
                                global, caches, op_info, root, val_target, new_val,
                            )?;
                        }
                        self.check_data_size(target.source(), op_info.position())?;
                        Ok((Dynamic::UNIT, true))
                    }
                    // {xxx:map}.id
                    (Expr::Property(x, pos), None, true) => {
                        #[cfg(feature = "debugging")]
                        self.run_debugger(global, caches, _scope, this_ptr, rhs)?;

                        let target = target.as_mut();
                        let index = &mut x.2.clone().into();
                        let val = self.get_indexed_mut(
                            global, caches, target, index, *pos, op_pos, false, false,
                        )?;
                        Ok((val.take_or_clone(), false))
                    }
                    // xxx.id op= ???
                    (Expr::Property(x, pos), Some((mut new_val, op_info)), false) => {
                        #[cfg(feature = "debugging")]
                        self.run_debugger(global, caches, _scope, this_ptr, rhs)?;

                        let ((getter, hash_get), (setter, hash_set), name) = &**x;

                        if op_info.is_op_assignment() {
                            let args = &mut [target.as_mut()];

                            let (mut orig_val, ..) = self
                                .exec_native_fn_call(
                                    global, caches, getter, None, *hash_get, args, is_ref_mut,
                                    false, *pos,
                                )
                                .or_else(|err| match *err {
                                    // Try an indexer if property does not exist
                                    ERR::ErrorDotExpr(..) => {
                                        let target = target.as_mut();
                                        let mut prop = name.into();
                                        self.call_indexer_get(
                                            global, caches, target, &mut prop, op_pos,
                                        )
                                        .map(|r| (r, false))
                                        .map_err(|e| {
                                            match *e {
                                                ERR::ErrorIndexingType(..) => err,
                                                _ => e,
                                            }
                                        })
                                    }
                                    _ => Err(err),
                                })?;

                            {
                                let orig_val = &mut (&mut orig_val).into();

                                self.eval_op_assignment(
                                    global, caches, op_info, root, orig_val, new_val,
                                )?;
                            }

                            new_val = orig_val;
                        }

                        let args = &mut [target.as_mut(), &mut new_val];

                        self.exec_native_fn_call(
                            global, caches, setter, None, *hash_set, args, is_ref_mut, false, *pos,
                        )
                        .or_else(|err| match *err {
                            // Try an indexer if property does not exist
                            ERR::ErrorDotExpr(..) => {
                                let target = target.as_mut();
                                let idx = &mut name.into();
                                let new_val = &mut new_val;
                                self.call_indexer_set(
                                    global, caches, target, idx, new_val, is_ref_mut, op_pos,
                                )
                                .map_err(|e| match *e {
                                    ERR::ErrorIndexingType(..) => err,
                                    _ => e,
                                })
                            }
                            _ => Err(err),
                        })
                    }
                    // xxx.id
                    (Expr::Property(x, pos), None, false) => {
                        #[cfg(feature = "debugging")]
                        self.run_debugger(global, caches, _scope, this_ptr, rhs)?;

                        let ((getter, hash_get), _, name) = &**x;
                        let args = &mut [target.as_mut()];

                        self.exec_native_fn_call(
                            global, caches, getter, None, *hash_get, args, is_ref_mut, false, *pos,
                        )
                        .map_or_else(
                            |err| match *err {
                                // Try an indexer if property does not exist
                                ERR::ErrorDotExpr(..) => {
                                    let target = target.as_mut();
                                    let mut prop = name.into();
                                    self.call_indexer_get(global, caches, target, &mut prop, op_pos)
                                        .map(|r| (r, false))
                                        .map_err(|e| match *e {
                                            ERR::ErrorIndexingType(..) => err,
                                            _ => e,
                                        })
                                }
                                _ => Err(err),
                            },
                            // Assume getters are always pure
                            |(v, ..)| Ok((v, false)),
                        )
                    }
                    // {xxx:map}.sub_lhs[expr] | {xxx:map}.sub_lhs.expr
                    (Expr::Index(x, ..) | Expr::Dot(x, ..), new_val, true) => {
                        let _node = &x.lhs;
                        let mut _this_ptr = this_ptr;
                        let _tp = _this_ptr.as_deref_mut();

                        let val_target = &mut match x.lhs {
                            Expr::Property(ref p, pos) => {
                                #[cfg(feature = "debugging")]
                                self.run_debugger(global, caches, _scope, _tp, _node)?;

                                let target = target.as_mut();
                                let index = &mut p.2.clone().into();
                                self.get_indexed_mut(
                                    global, caches, target, index, pos, op_pos, false, true,
                                )?
                            }
                            // {xxx:map}.fn_name(arg_expr_list)[expr] | {xxx:map}.fn_name(arg_expr_list).expr
                            Expr::MethodCall(ref x, pos) => {
                                debug_assert!(
                                    !x.is_qualified(),
                                    "method call in dot chain should not be namespace-qualified"
                                );

                                #[cfg(feature = "debugging")]
                                let reset = self
                                    .run_debugger_with_reset(global, caches, _scope, _tp, _node)?;
                                #[cfg(feature = "debugging")]
                                defer! { global if Some(reset) => move |g| g.debugger_mut().reset_status(reset) }

                                let crate::ast::FnCallExpr {
                                    name, hashes, args, ..
                                } = &**x;

                                // Truncate the index values upon exit
                                defer! { idx_values => truncate; let offset = idx_values.len() - args.len(); }

                                let call_args = &mut idx_values[offset..];
                                let arg1_pos = args.first().map_or(Position::NONE, Expr::position);

                                self.make_method_call(
                                    global, caches, name, *hashes, target, call_args, arg1_pos, pos,
                                )?
                                .0
                                .into()
                            }
                            // Others - syntax error
                            ref expr => unreachable!("invalid dot expression: {:?}", expr),
                        };

                        self.eval_dot_index_chain_raw(
                            global, caches, _scope, _this_ptr, root, rhs, val_target, &x.rhs,
                            idx_values, new_val,
                        )
                    }
                    // xxx.sub_lhs[expr] | xxx.sub_lhs.expr
                    (Expr::Index(x, ..) | Expr::Dot(x, ..), new_val, ..) => {
                        let _node = &x.lhs;
                        let mut _this_ptr = this_ptr;
                        let _tp = _this_ptr.as_deref_mut();

                        match x.lhs {
                            // xxx.prop[expr] | xxx.prop.expr
                            Expr::Property(ref p, pos) => {
                                #[cfg(feature = "debugging")]
                                self.run_debugger(global, caches, _scope, _tp, _node)?;

                                let ((getter, hash_get), (setter, hash_set), name) = &**p;
                                let args = &mut [target.as_mut()];

                                // Assume getters are always pure
                                let (mut val, ..) = self
                                    .exec_native_fn_call(
                                        global, caches, getter, None, *hash_get, args, is_ref_mut,
                                        false, pos,
                                    )
                                    .or_else(|err| match *err {
                                        // Try an indexer if property does not exist
                                        ERR::ErrorDotExpr(..) => {
                                            let target = target.as_mut();
                                            let mut prop = name.into();
                                            self.call_indexer_get(
                                                global, caches, target, &mut prop, op_pos,
                                            )
                                            .map(|r| (r, false))
                                            .map_err(
                                                |e| match *e {
                                                    ERR::ErrorIndexingType(..) => err,
                                                    _ => e,
                                                },
                                            )
                                        }
                                        _ => Err(err),
                                    })?;

                                let val = &mut (&mut val).into();

                                let (result, may_be_changed) = self.eval_dot_index_chain_raw(
                                    global, caches, _scope, _this_ptr, root, rhs, val, &x.rhs,
                                    idx_values, new_val,
                                )?;

                                // Feed the value back via a setter just in case it has been updated
                                if may_be_changed {
                                    // Re-use args because the first &mut parameter will not be consumed
                                    let args = &mut [target.as_mut(), val.as_mut()];

                                    // The return value is thrown away and not used.
                                    let _ = self
                                        .exec_native_fn_call(
                                            global, caches, setter, None, *hash_set, args,
                                            is_ref_mut, false, pos,
                                        )
                                        .or_else(|err| match *err {
                                            // Try an indexer if property does not exist
                                            ERR::ErrorDotExpr(..) => {
                                                let target = target.as_mut();
                                                let idx = &mut name.into();
                                                let new_val = val.as_mut();
                                                self.call_indexer_set(
                                                    global, caches, target, idx, new_val,
                                                    is_ref_mut, op_pos,
                                                )
                                                .or_else(|e| match *e {
                                                    // If there is no setter, no need to feed it
                                                    // back because the property is read-only
                                                    ERR::ErrorIndexingType(..) => {
                                                        Ok((Dynamic::UNIT, false))
                                                    }
                                                    _ => Err(e),
                                                })
                                            }
                                            _ => Err(err),
                                        })?;
                                }

                                Ok((result, may_be_changed))
                            }
                            // xxx.fn_name(arg_expr_list)[expr] | xxx.fn_name(arg_expr_list).expr
                            Expr::MethodCall(ref f, pos) => {
                                debug_assert!(
                                    !f.is_qualified(),
                                    "method call in dot chain should not be namespace-qualified"
                                );

                                let val = {
                                    #[cfg(feature = "debugging")]
                                    let reset = self.run_debugger_with_reset(
                                        global, caches, _scope, _tp, _node,
                                    )?;
                                    #[cfg(feature = "debugging")]
                                    defer! { global if Some(reset) => move |g| g.debugger_mut().reset_status(reset) }

                                    let crate::ast::FnCallExpr {
                                        name, hashes, args, ..
                                    } = &**f;

                                    // Truncate the index values upon exit
                                    defer! { idx_values => truncate; let offset = idx_values.len() - args.len(); }

                                    let call_args = &mut idx_values[offset..];
                                    let pos1 = args.first().map_or(Position::NONE, Expr::position);

                                    self.make_method_call(
                                        global, caches, name, *hashes, target, call_args, pos1, pos,
                                    )?
                                    .0
                                };

                                let val = &mut val.into();

                                self.eval_dot_index_chain_raw(
                                    global, caches, _scope, _this_ptr, root, rhs, val, &x.rhs,
                                    idx_values, new_val,
                                )
                            }
                            // Others - syntax error
                            ref expr => unreachable!("invalid dot expression: {:?}", expr),
                        }
                    }
                    // Syntax error
                    (expr, ..) => unreachable!("invalid chaining expression: {:?}", expr),
                }
            }
        }
    }
}
