//! Types to support chaining operations (i.e. indexing and dotting).
#![cfg(any(not(feature = "no_index"), not(feature = "no_object")))]

use super::{Caches, GlobalRuntimeState, Target};
use crate::ast::{ASTFlags, BinaryExpr, Expr, OpAssignment};
use crate::{Dynamic, Engine, FnArgsVec, RhaiResult, RhaiResultOf, Scope, ERR};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{convert::TryInto, hash::Hash};

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
            (_, ChainType::Indexing) if rhs.get_literal_value(Some(global)).is_some() => {
                idx_values.push(rhs.get_literal_value(Some(global)).unwrap())
            }
            // Short-circuit for simple method call: {expr}.func()
            #[cfg(not(feature = "no_object"))]
            (Expr::MethodCall(x, ..), ChainType::Dotting) if x.args.is_empty() => (),
            // All other patterns - evaluate the arguments chain
            _ => {
                let this_ptr = this_ptr.as_deref_mut();
                self.eval_dot_index_chain_arguments(
                    global, caches, scope, this_ptr, expr, rhs, idx_values,
                )?
            }
        }

        match (lhs, new_val) {
            // this.??? or this[???]
            (Expr::ThisPtr(var_pos), new_val) => {
                self.track_operation(global, *var_pos)?;

                #[cfg(feature = "debugging")]
                self.dbg(global, caches, scope, this_ptr.as_deref_mut(), lhs)?;

                this_ptr.map_or_else(
                    || Err(ERR::ErrorUnboundThis(*var_pos).into()),
                    |this_ptr| {
                        let target = &mut this_ptr.try_into()?;
                        let scope = Some(scope);
                        self.eval_dot_index_chain_raw(
                            global, caches, scope, None, lhs, expr, target, rhs, idx_values,
                            new_val,
                        )
                        .and_then(|(value, changed)| {
                            if changed {
                                self.check_data_size(target.source(), expr.position())
                                    .map(|_| (value, changed))
                            } else {
                                Ok((value, changed))
                            }
                        })
                    },
                )
            }
            // id.??? or id[???]
            (Expr::Variable(.., var_pos), new_val) => {
                self.track_operation(global, *var_pos)?;

                #[cfg(feature = "debugging")]
                self.dbg(global, caches, scope, this_ptr.as_deref_mut(), lhs)?;

                let target = &mut self.search_namespace(global, caches, scope, this_ptr, lhs)?;
                self.eval_dot_index_chain_raw(
                    global, caches, None, None, lhs, expr, target, rhs, idx_values, new_val,
                )
                .and_then(|(value, changed)| {
                    if changed {
                        self.check_data_size(target.source(), expr.position())
                            .map(|_| (value, changed))
                    } else {
                        Ok((value, changed))
                    }
                })
            }
            // {expr}.??? = ??? or {expr}[???] = ???
            (_, Some(..)) => unreachable!("cannot assign to an expression"),
            // {expr}.??? or {expr}[???]
            (lhs_expr, None) => {
                let value = self
                    .eval_expr(global, caches, scope, this_ptr.as_deref_mut(), lhs_expr)?
                    .flatten();
                let item_ptr = &mut value.into();
                let scope = Some(scope);
                self.eval_dot_index_chain_raw(
                    global, caches, scope, this_ptr, lhs_expr, expr, item_ptr, rhs, idx_values,
                    None,
                )
                .and_then(|(value, changed)| {
                    if changed {
                        self.check_data_size(item_ptr.source(), expr.position())
                            .map(|_| (value, changed))
                    } else {
                        Ok((value, changed))
                    }
                })
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
                #[cfg(not(feature = "no_module"))]
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
                        #[cfg(not(feature = "no_module"))]
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
        scope: Option<&mut Scope>,
        mut this_ptr: Option<&mut Dynamic>,
        root: &Expr,
        parent: &Expr,
        target: &mut Target,
        rhs: &Expr,
        idx_values: &mut FnArgsVec<Dynamic>,
        new_val: Option<(Dynamic, &OpAssignment)>,
    ) -> RhaiResultOf<(Dynamic, bool)> {
        let mut b;
        let mut s = scope;

        macro_rules! x {
            ($var:ident, $base:ident) => {{
                if $var.is_none() {
                    $base = Scope::new();
                    $var = Some(&mut $base);
                }
                $var.as_deref_mut().unwrap()
            }};
        }

        let is_ref_mut = target.is_ref();
        let op_pos = parent.position();

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
                        self.dbg(global, caches, x!(s, b), this_ptr.as_deref_mut(), parent)?;

                        let idx_val = &mut idx_values.pop().unwrap();
                        let mut idx_val_for_setter = idx_val.clone();
                        let idx_pos = x.lhs.start_position();

                        let (try_setter, result) = {
                            let obj = target.as_mut();

                            #[cfg(not(feature = "no_closure"))]
                            let mut target_guard;
                            #[cfg(not(feature = "no_closure"))]
                            let obj = if obj.is_shared() {
                                target_guard = obj.write_lock::<Dynamic>().unwrap();
                                &mut *target_guard
                            } else {
                                obj
                            };

                            let tp = this_ptr.as_deref_mut();
                            let new_scope = x!(s, b);
                            let mut item = self.get_indexed_mut(
                                global, caches, new_scope, tp, obj, idx_val, idx_pos, op_pos,
                                false, true,
                            )?;
                            let is_item_temp_val = item.is_temp_value();
                            let item_ptr = &mut item;

                            match self.eval_dot_index_chain_raw(
                                global, caches, s, this_ptr, root, rhs, item_ptr, &x.rhs,
                                idx_values, new_val,
                            ) {
                                Ok((result, true)) if is_item_temp_val => {
                                    (Some(item.take_or_clone()), (result, true))
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
                        self.dbg(global, caches, x!(s, b), this_ptr.as_deref_mut(), parent)?;

                        let obj = target.as_mut();

                        #[cfg(not(feature = "no_closure"))]
                        let mut target_guard;
                        #[cfg(not(feature = "no_closure"))]
                        let obj = if obj.is_shared() {
                            target_guard = obj.write_lock::<Dynamic>().unwrap();
                            &mut *target_guard
                        } else {
                            obj
                        };

                        let new_scope = x!(s, b);
                        let idx_val = &mut idx_values.pop().unwrap();
                        let idx = &mut idx_val.clone();
                        let try_setter = match self.get_indexed_mut(
                            global, caches, new_scope, this_ptr, obj, idx, pos, op_pos, true, false,
                        ) {
                            // Indexed value is not a temp value - update directly
                            Ok(ref mut item_ptr) => {
                                self.eval_op_assignment(
                                    global, caches, op_info, root, item_ptr, new_val,
                                )?;
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
                                    self.call_indexer_get(global, caches, obj, idx, op_pos)
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
                                global, caches, obj, idx_val, new_val, is_ref_mut, op_pos,
                            )?;
                        }

                        Ok((Dynamic::UNIT, true))
                    }
                    // xxx[rhs]
                    (_, None) => {
                        #[cfg(feature = "debugging")]
                        self.dbg(global, caches, x!(s, b), this_ptr.as_deref_mut(), parent)?;

                        let obj = target.as_mut();

                        #[cfg(not(feature = "no_closure"))]
                        let mut target_guard;
                        #[cfg(not(feature = "no_closure"))]
                        let obj = if obj.is_shared() {
                            target_guard = obj.write_lock::<Dynamic>().unwrap();
                            &mut *target_guard
                        } else {
                            obj
                        };

                        let new_scope = x!(s, b);
                        let idx_val = &mut idx_values.pop().unwrap();

                        self.get_indexed_mut(
                            global, caches, new_scope, this_ptr, obj, idx_val, pos, op_pos, false,
                            true,
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
                        #[cfg(not(feature = "no_module"))]
                        debug_assert!(
                            !x.is_qualified(),
                            "method call in dot chain should not be namespace-qualified"
                        );

                        #[cfg(feature = "debugging")]
                        let reset = self.dbg_reset(global, caches, x!(s, b), this_ptr, rhs)?;
                        #[cfg(feature = "debugging")]
                        defer! { global if Some(reset) => move |g| g.debugger_mut().reset_status(reset) }

                        let crate::ast::FnCallExpr {
                            name, hashes, args, ..
                        } = &**x;

                        // Truncate the index values upon exit
                        defer! { idx_values => truncate; let offset = idx_values.len() - args.len(); }

                        let call_args = &mut idx_values[offset..];
                        let arg1_pos = args.first().map_or(crate::Position::NONE, Expr::position);

                        self.make_method_call(
                            global, caches, name, *hashes, target, call_args, arg1_pos, *pos,
                        )
                    }
                    // {xxx:map}.id op= ???
                    (Expr::Property(x, pos), Some((new_val, op_info)), true) => {
                        #[cfg(feature = "debugging")]
                        self.dbg(global, caches, x!(s, b), this_ptr.as_deref_mut(), rhs)?;

                        let index = &mut x.2.clone().into();
                        {
                            let obj = target.as_mut();

                            #[cfg(not(feature = "no_closure"))]
                            let mut target_guard;
                            #[cfg(not(feature = "no_closure"))]
                            let obj = if obj.is_shared() {
                                target_guard = obj.write_lock::<Dynamic>().unwrap();
                                &mut *target_guard
                            } else {
                                obj
                            };

                            let new_scope = x!(s, b);

                            let item = &mut self.get_indexed_mut(
                                global, caches, new_scope, this_ptr, obj, index, *pos, op_pos,
                                true, false,
                            )?;
                            self.eval_op_assignment(global, caches, op_info, root, item, new_val)?;
                        }
                        Ok((Dynamic::UNIT, true))
                    }
                    // {xxx:map}.id
                    (Expr::Property(x, pos), None, true) => {
                        #[cfg(feature = "debugging")]
                        self.dbg(global, caches, x!(s, b), this_ptr.as_deref_mut(), rhs)?;

                        let obj = target.as_mut();

                        #[cfg(not(feature = "no_closure"))]
                        let mut target_guard;
                        #[cfg(not(feature = "no_closure"))]
                        let obj = if obj.is_shared() {
                            target_guard = obj.write_lock::<Dynamic>().unwrap();
                            &mut *target_guard
                        } else {
                            obj
                        };

                        let index = &mut x.2.clone().into();
                        let new_scope = x!(s, b);

                        let item = self.get_indexed_mut(
                            global, caches, new_scope, this_ptr, obj, index, *pos, op_pos, false,
                            false,
                        )?;
                        Ok((item.take_or_clone(), false))
                    }
                    // xxx.id op= ???
                    (Expr::Property(x, pos), Some((mut new_val, op_info)), false) => {
                        #[cfg(feature = "debugging")]
                        self.dbg(global, caches, x!(s, b), this_ptr, rhs)?;

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
                                let orig_val = &mut (&mut orig_val).try_into()?;

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
                        self.dbg(global, caches, x!(s, b), this_ptr, rhs)?;

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
                        let mut _tp = this_ptr.as_deref_mut();
                        let mut changed = false;
                        #[cfg(not(feature = "no_closure"))]
                        let mut target_guard;

                        let item = &mut match x.lhs {
                            Expr::Property(ref p, pos) => {
                                #[cfg(feature = "debugging")]
                                self.dbg(global, caches, x!(s, b), _tp.as_deref_mut(), _node)?;

                                let obj = target.as_mut();

                                #[cfg(not(feature = "no_closure"))]
                                let obj = if obj.is_shared() {
                                    target_guard = obj.write_lock::<Dynamic>().unwrap();
                                    &mut *target_guard
                                } else {
                                    obj
                                };

                                let new_scope = x!(s, b);
                                let index = &mut p.2.clone().into();

                                self.get_indexed_mut(
                                    global, caches, new_scope, _tp, obj, index, pos, op_pos, false,
                                    true,
                                )?
                            }
                            // {xxx:map}.fn_name(arg_expr_list)[expr] | {xxx:map}.fn_name(arg_expr_list).expr
                            Expr::MethodCall(ref x, pos) => {
                                #[cfg(not(feature = "no_module"))]
                                debug_assert!(
                                    !x.is_qualified(),
                                    "method call in dot chain should not be namespace-qualified"
                                );

                                #[cfg(feature = "debugging")]
                                let reset = self.dbg_reset(global, caches, x!(s, b), _tp, _node)?;
                                #[cfg(feature = "debugging")]
                                defer! { global if Some(reset) => move |g| g.debugger_mut().reset_status(reset) }

                                let crate::ast::FnCallExpr {
                                    name, hashes, args, ..
                                } = &**x;

                                // Truncate the index values upon exit
                                defer! { idx_values => truncate; let offset = idx_values.len() - args.len(); }

                                let call_args = &mut idx_values[offset..];
                                let arg1_pos =
                                    args.first().map_or(crate::Position::NONE, Expr::position);

                                let (value, updated) = self.make_method_call(
                                    global, caches, name, *hashes, target, call_args, arg1_pos, pos,
                                )?;
                                changed = updated;
                                value.into()
                            }
                            // Others - syntax error
                            ref expr => unreachable!("invalid dot expression: {:?}", expr),
                        };

                        self.eval_dot_index_chain_raw(
                            global, caches, s, this_ptr, root, rhs, item, &x.rhs, idx_values,
                            new_val,
                        )
                        .map(|(result, updated)| (result, changed || updated))
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
                                self.dbg(global, caches, x!(s, b), _tp, _node)?;

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

                                let val = &mut (&mut val).try_into()?;

                                let (result, may_be_changed) = self.eval_dot_index_chain_raw(
                                    global, caches, s, _this_ptr, root, rhs, val, &x.rhs,
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
                                #[cfg(not(feature = "no_module"))]
                                debug_assert!(
                                    !f.is_qualified(),
                                    "method call in dot chain should not be namespace-qualified"
                                );

                                let (val, changed) = {
                                    #[cfg(feature = "debugging")]
                                    let reset =
                                        self.dbg_reset(global, caches, x!(s, b), _tp, _node)?;
                                    #[cfg(feature = "debugging")]
                                    defer! { global if Some(reset) => move |g| g.debugger_mut().reset_status(reset) }

                                    let crate::ast::FnCallExpr {
                                        name, hashes, args, ..
                                    } = &**f;

                                    // Truncate the index values upon exit
                                    defer! { idx_values => truncate; let offset = idx_values.len() - args.len(); }

                                    let call_args = &mut idx_values[offset..];
                                    let pos1 =
                                        args.first().map_or(crate::Position::NONE, Expr::position);

                                    self.make_method_call(
                                        global, caches, name, *hashes, target, call_args, pos1, pos,
                                    )?
                                };

                                let val = &mut val.into();

                                self.eval_dot_index_chain_raw(
                                    global, caches, s, _this_ptr, root, rhs, val, &x.rhs,
                                    idx_values, new_val,
                                )
                                .map(|(result, updated)| (result, changed || updated))
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
