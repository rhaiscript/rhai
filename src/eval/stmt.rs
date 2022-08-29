//! Module defining functions for evaluating a statement.

use super::{Caches, EvalContext, GlobalRuntimeState, Target};
use crate::api::events::VarDefInfo;
use crate::ast::{
    ASTFlags, BinaryExpr, Expr, Ident, OpAssignment, Stmt, SwitchCasesCollection, TryCatchBlock,
};
use crate::func::get_hasher;
use crate::types::dynamic::{AccessMode, Union};
use crate::{
    Dynamic, Engine, ImmutableString, Module, Position, RhaiResult, RhaiResultOf, Scope, ERR, INT,
};
use std::hash::{Hash, Hasher};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

impl Engine {
    /// Evaluate a statements block.
    //
    // # Implementation Notes
    //
    // Do not use the `?` operator within the main body as it makes this function return early,
    // possibly by-passing important cleanup tasks at the end.
    //
    // Errors that are not recoverable, such as system errors or safety errors, can use `?`.
    pub(crate) fn eval_stmt_block(
        &self,
        scope: &mut Scope,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        statements: &[Stmt],
        restore_orig_state: bool,
        level: usize,
    ) -> RhaiResult {
        if statements.is_empty() {
            return Ok(Dynamic::UNIT);
        }

        let orig_always_search_scope = global.always_search_scope;
        let orig_scope_len = scope.len();
        #[cfg(not(feature = "no_module"))]
        let orig_imports_len = global.num_imports();
        let orig_fn_resolution_caches_len = caches.fn_resolution_caches_len();

        if restore_orig_state {
            global.scope_level += 1;
        }

        let mut result = Ok(Dynamic::UNIT);

        for stmt in statements {
            #[cfg(not(feature = "no_module"))]
            let imports_len = global.num_imports();

            result = self.eval_stmt(
                scope,
                global,
                caches,
                lib,
                this_ptr,
                stmt,
                restore_orig_state,
                level,
            );

            if result.is_err() {
                break;
            }

            #[cfg(not(feature = "no_module"))]
            if matches!(stmt, Stmt::Import(..)) {
                // Get the extra modules - see if any functions are marked global.
                // Without global functions, the extra modules never affect function resolution.
                if global
                    .scan_imports_raw()
                    .skip(imports_len)
                    .any(|(.., m)| m.contains_indexed_global_functions())
                {
                    if caches.fn_resolution_caches_len() > orig_fn_resolution_caches_len {
                        // When new module is imported with global functions and there is already
                        // a new cache, clear it - notice that this is expensive as all function
                        // resolutions must start again
                        caches.fn_resolution_cache_mut().clear();
                    } else if restore_orig_state {
                        // When new module is imported with global functions, push a new cache
                        caches.push_fn_resolution_cache();
                    } else {
                        // When the block is to be evaluated in-place, just clear the current cache
                        caches.fn_resolution_cache_mut().clear();
                    }
                }
            }
        }

        // If imports list is modified, pop the functions lookup cache
        caches.rewind_fn_resolution_caches(orig_fn_resolution_caches_len);

        if restore_orig_state {
            scope.rewind(orig_scope_len);
            global.scope_level -= 1;
            #[cfg(not(feature = "no_module"))]
            global.truncate_imports(orig_imports_len);

            // The impact of new local variables goes away at the end of a block
            // because any new variables introduced will go out of scope
            global.always_search_scope = orig_always_search_scope;
        }

        result
    }

    /// Evaluate an op-assignment statement.
    pub(crate) fn eval_op_assignment(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        lib: &[&Module],
        op_info: OpAssignment,
        target: &mut Target,
        root: (&str, Position),
        new_val: Dynamic,
        level: usize,
    ) -> RhaiResultOf<()> {
        if target.is_read_only() {
            // Assignment to constant variable
            return Err(ERR::ErrorAssignmentToConstant(root.0.to_string(), root.1).into());
        }

        let mut new_val = new_val;

        if op_info.is_op_assignment() {
            let OpAssignment {
                hash_op_assign,
                hash_op,
                op_assign,
                op,
                pos: op_pos,
            } = op_info;

            let mut lock_guard = target.write_lock::<Dynamic>().unwrap();

            let hash = hash_op_assign;
            let args = &mut [&mut *lock_guard, &mut new_val];
            let level = level + 1;

            match self.call_native_fn(
                global, caches, lib, op_assign, hash, args, true, true, op_pos, level,
            ) {
                Ok(_) => {
                    #[cfg(not(feature = "unchecked"))]
                    self.check_data_size(args[0], root.1)?;
                }
                Err(err) if matches!(*err, ERR::ErrorFunctionNotFound(ref f, ..) if f.starts_with(op_assign)) =>
                {
                    // Expand to `var = var op rhs`
                    let (value, ..) = self
                        .call_native_fn(
                            global, caches, lib, op, hash_op, args, true, false, op_pos, level,
                        )
                        .map_err(|err| err.fill_position(op_info.pos))?;

                    #[cfg(not(feature = "unchecked"))]
                    self.check_data_size(&value, root.1)?;

                    *args[0] = value.flatten();
                }
                Err(err) => return Err(err),
            }
        } else {
            // Normal assignment
            *target.write_lock::<Dynamic>().unwrap() = new_val;
        }

        /*
        if let Some(mut guard) = target.write_lock::<Dynamic>() {
            if guard.is::<ImmutableString>() {
                let s = std::mem::take(&mut *guard).cast::<ImmutableString>();
                *guard = self.get_interned_string(s).into();
            }
        }
        */

        target.propagate_changed_value(op_info.pos)
    }

    /// Evaluate a statement.
    //
    // # Implementation Notes
    //
    // Do not use the `?` operator within the main body as it makes this function return early,
    // possibly by-passing important cleanup tasks at the end.
    //
    // Errors that are not recoverable, such as system errors or safety errors, can use `?`.
    pub(crate) fn eval_stmt(
        &self,
        scope: &mut Scope,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        stmt: &Stmt,
        rewind_scope: bool,
        level: usize,
    ) -> RhaiResult {
        #[cfg(feature = "debugging")]
        let reset_debugger =
            self.run_debugger_with_reset(scope, global, lib, this_ptr, stmt, level)?;

        // Coded this way for better branch prediction.
        // Popular branches are lifted out of the `match` statement into their own branches.

        // Function calls should account for a relatively larger portion of statements.
        if let Stmt::FnCall(x, ..) = stmt {
            #[cfg(not(feature = "unchecked"))]
            self.inc_operations(&mut global.num_operations, stmt.position())?;

            let result =
                self.eval_fn_call_expr(scope, global, caches, lib, this_ptr, x, x.pos, level);

            #[cfg(feature = "debugging")]
            global.debugger.reset_status(reset_debugger);

            return result;
        }

        // Then assignments.
        // We shouldn't do this for too many variants because, soon or later, the added comparisons
        // will cost more than the mis-predicted `match` branch.
        if let Stmt::Assignment(x, ..) = stmt {
            let (op_info, BinaryExpr { lhs, rhs }) = &**x;

            #[cfg(not(feature = "unchecked"))]
            self.inc_operations(&mut global.num_operations, stmt.position())?;

            let result = if let Expr::Variable(x, ..) = lhs {
                let rhs_result = self
                    .eval_expr(scope, global, caches, lib, this_ptr, rhs, level)
                    .map(Dynamic::flatten);

                if let Ok(rhs_val) = rhs_result {
                    let search_result =
                        self.search_namespace(scope, global, lib, this_ptr, lhs, level);

                    if let Ok(search_val) = search_result {
                        let (mut lhs_ptr, pos) = search_val;

                        let var_name = x.3.as_str();

                        #[cfg(not(feature = "no_closure"))]
                        // Also handle case where target is a `Dynamic` shared value
                        // (returned by a variable resolver, for example)
                        let is_temp_result = !lhs_ptr.is_ref() && !lhs_ptr.is_shared();
                        #[cfg(feature = "no_closure")]
                        let is_temp_result = !lhs_ptr.is_ref();

                        // Cannot assign to temp result from expression
                        if is_temp_result {
                            return Err(
                                ERR::ErrorAssignmentToConstant(var_name.to_string(), pos).into()
                            );
                        }

                        #[cfg(not(feature = "unchecked"))]
                        self.inc_operations(&mut global.num_operations, pos)?;

                        let root = (var_name, pos);
                        let lhs_ptr = &mut lhs_ptr;

                        self.eval_op_assignment(
                            global, caches, lib, *op_info, lhs_ptr, root, rhs_val, level,
                        )
                        .map(|_| Dynamic::UNIT)
                    } else {
                        search_result.map(|_| Dynamic::UNIT)
                    }
                } else {
                    rhs_result
                }
            } else {
                let (op_info, BinaryExpr { lhs, rhs }) = &**x;

                let rhs_result = self
                    .eval_expr(scope, global, caches, lib, this_ptr, rhs, level)
                    .map(Dynamic::flatten);

                if let Ok(rhs_val) = rhs_result {
                    let rhs_val = if rhs_val.is::<ImmutableString>() {
                        self.get_interned_string(rhs_val.cast::<ImmutableString>())
                            .into()
                    } else {
                        rhs_val
                    };

                    let _new_val = Some((rhs_val, *op_info));

                    // Must be either `var[index] op= val` or `var.prop op= val`
                    match lhs {
                        // name op= rhs (handled above)
                        Expr::Variable(..) => {
                            unreachable!("Expr::Variable case is already handled")
                        }
                        // idx_lhs[idx_expr] op= rhs
                        #[cfg(not(feature = "no_index"))]
                        Expr::Index(..) => self
                            .eval_dot_index_chain(
                                scope, global, caches, lib, this_ptr, lhs, level, _new_val,
                            )
                            .map(|_| Dynamic::UNIT),
                        // dot_lhs.dot_rhs op= rhs
                        #[cfg(not(feature = "no_object"))]
                        Expr::Dot(..) => self
                            .eval_dot_index_chain(
                                scope, global, caches, lib, this_ptr, lhs, level, _new_val,
                            )
                            .map(|_| Dynamic::UNIT),
                        _ => unreachable!("cannot assign to expression: {:?}", lhs),
                    }
                } else {
                    rhs_result
                }
            };

            #[cfg(feature = "debugging")]
            global.debugger.reset_status(reset_debugger);

            return result;
        }

        #[cfg(not(feature = "unchecked"))]
        self.inc_operations(&mut global.num_operations, stmt.position())?;

        let result = match stmt {
            // No-op
            Stmt::Noop(..) => Ok(Dynamic::UNIT),

            // Expression as statement
            Stmt::Expr(expr) => self
                .eval_expr(scope, global, caches, lib, this_ptr, expr, level)
                .map(Dynamic::flatten),

            // Block scope
            Stmt::Block(statements, ..) if statements.is_empty() => Ok(Dynamic::UNIT),
            Stmt::Block(statements, ..) => self.eval_stmt_block(
                scope, global, caches, lib, this_ptr, statements, true, level,
            ),

            // If statement
            Stmt::If(x, ..) => {
                let (expr, if_block, else_block) = &**x;

                let guard_val = self
                    .eval_expr(scope, global, caches, lib, this_ptr, expr, level)
                    .and_then(|v| {
                        v.as_bool().map_err(|typ| {
                            self.make_type_mismatch_err::<bool>(typ, expr.position())
                        })
                    });

                match guard_val {
                    Ok(true) => {
                        if if_block.is_empty() {
                            Ok(Dynamic::UNIT)
                        } else {
                            self.eval_stmt_block(
                                scope, global, caches, lib, this_ptr, if_block, true, level,
                            )
                        }
                    }
                    Ok(false) => {
                        if else_block.is_empty() {
                            Ok(Dynamic::UNIT)
                        } else {
                            self.eval_stmt_block(
                                scope, global, caches, lib, this_ptr, else_block, true, level,
                            )
                        }
                    }
                    err => err.map(Into::into),
                }
            }

            // Switch statement
            Stmt::Switch(x, ..) => {
                let (
                    expr,
                    SwitchCasesCollection {
                        expressions,
                        cases,
                        def_case,
                        ranges,
                    },
                ) = &**x;

                let value_result =
                    self.eval_expr(scope, global, caches, lib, this_ptr, expr, level);

                if let Ok(value) = value_result {
                    let expr_result = if value.is_hashable() {
                        let hasher = &mut get_hasher();
                        value.hash(hasher);
                        let hash = hasher.finish();

                        // First check hashes
                        if let Some(case_blocks_list) = cases.get(&hash) {
                            assert!(!case_blocks_list.is_empty());

                            let mut result = Ok(None);

                            for &index in case_blocks_list {
                                let block = &expressions[index];

                                let cond_result = match block.condition {
                                    Expr::BoolConstant(b, ..) => Ok(b),
                                    ref c => self
                                        .eval_expr(scope, global, caches, lib, this_ptr, c, level)
                                        .and_then(|v| {
                                            v.as_bool().map_err(|typ| {
                                                self.make_type_mismatch_err::<bool>(
                                                    typ,
                                                    c.position(),
                                                )
                                            })
                                        }),
                                };

                                match cond_result {
                                    Ok(true) => {
                                        result = Ok(Some(&block.expr));
                                        break;
                                    }
                                    Ok(false) => (),
                                    _ => {
                                        result = cond_result.map(|_| None);
                                        break;
                                    }
                                }
                            }

                            result
                        } else if value.is::<INT>() && !ranges.is_empty() {
                            // Then check integer ranges
                            let value = value.as_int().expect("`INT`");
                            let mut result = Ok(None);

                            for r in ranges.iter().filter(|r| r.contains(value)) {
                                let block = &expressions[r.index()];

                                let cond_result = match block.condition {
                                    Expr::BoolConstant(b, ..) => Ok(b),
                                    ref c => self
                                        .eval_expr(scope, global, caches, lib, this_ptr, c, level)
                                        .and_then(|v| {
                                            v.as_bool().map_err(|typ| {
                                                self.make_type_mismatch_err::<bool>(
                                                    typ,
                                                    c.position(),
                                                )
                                            })
                                        }),
                                };

                                match cond_result {
                                    Ok(true) => result = Ok(Some(&block.expr)),
                                    Ok(false) => continue,
                                    _ => result = cond_result.map(|_| None),
                                }

                                break;
                            }

                            result
                        } else {
                            // Nothing matches
                            Ok(None)
                        }
                    } else {
                        // Non-hashable
                        Ok(None)
                    };

                    if let Ok(Some(expr)) = expr_result {
                        self.eval_expr(scope, global, caches, lib, this_ptr, expr, level)
                    } else if let Ok(None) = expr_result {
                        // Default match clause
                        if let Some(index) = def_case {
                            let def_expr = &expressions[*index].expr;
                            self.eval_expr(scope, global, caches, lib, this_ptr, def_expr, level)
                        } else {
                            Ok(Dynamic::UNIT)
                        }
                    } else {
                        expr_result.map(|_| Dynamic::UNIT)
                    }
                } else {
                    value_result
                }
            }

            // Loop
            Stmt::While(x, ..) if matches!(x.0, Expr::Unit(..)) => loop {
                let (.., body) = &**x;

                if body.is_empty() {
                    #[cfg(not(feature = "unchecked"))]
                    self.inc_operations(&mut global.num_operations, body.position())?;
                } else {
                    match self
                        .eval_stmt_block(scope, global, caches, lib, this_ptr, body, true, level)
                    {
                        Ok(_) => (),
                        Err(err) => match *err {
                            ERR::LoopBreak(false, ..) => (),
                            ERR::LoopBreak(true, ..) => break Ok(Dynamic::UNIT),
                            _ => break Err(err),
                        },
                    }
                }
            },

            // While loop
            Stmt::While(x, ..) => loop {
                let (expr, body) = &**x;

                let condition = self
                    .eval_expr(scope, global, caches, lib, this_ptr, expr, level)
                    .and_then(|v| {
                        v.as_bool().map_err(|typ| {
                            self.make_type_mismatch_err::<bool>(typ, expr.position())
                        })
                    });

                match condition {
                    Ok(false) => break Ok(Dynamic::UNIT),
                    Ok(true) if body.is_empty() => (),
                    Ok(true) => {
                        match self.eval_stmt_block(
                            scope, global, caches, lib, this_ptr, body, true, level,
                        ) {
                            Ok(_) => (),
                            Err(err) => match *err {
                                ERR::LoopBreak(false, ..) => (),
                                ERR::LoopBreak(true, ..) => break Ok(Dynamic::UNIT),
                                _ => break Err(err),
                            },
                        }
                    }
                    err => break err.map(|_| Dynamic::UNIT),
                }
            },

            // Do loop
            Stmt::Do(x, options, ..) => loop {
                let (expr, body) = &**x;
                let is_while = !options.contains(ASTFlags::NEGATED);

                if !body.is_empty() {
                    match self
                        .eval_stmt_block(scope, global, caches, lib, this_ptr, body, true, level)
                    {
                        Ok(_) => (),
                        Err(err) => match *err {
                            ERR::LoopBreak(false, ..) => continue,
                            ERR::LoopBreak(true, ..) => break Ok(Dynamic::UNIT),
                            _ => break Err(err),
                        },
                    }
                }

                let condition = self
                    .eval_expr(scope, global, caches, lib, this_ptr, expr, level)
                    .and_then(|v| {
                        v.as_bool().map_err(|typ| {
                            self.make_type_mismatch_err::<bool>(typ, expr.position())
                        })
                    });

                match condition {
                    Ok(condition) if condition ^ is_while => break Ok(Dynamic::UNIT),
                    Ok(_) => (),
                    err => break err.map(|_| Dynamic::UNIT),
                }
            },

            // For loop
            Stmt::For(x, ..) => {
                let (var_name, counter, expr, statements) = &**x;

                let iter_result = self
                    .eval_expr(scope, global, caches, lib, this_ptr, expr, level)
                    .map(Dynamic::flatten);

                if let Ok(iter_obj) = iter_result {
                    let iter_type = iter_obj.type_id();

                    // lib should only contain scripts, so technically they cannot have iterators

                    // Search order:
                    // 1) Global namespace - functions registered via Engine::register_XXX
                    // 2) Global modules - packages
                    // 3) Imported modules - functions marked with global namespace
                    // 4) Global sub-modules - functions marked with global namespace
                    let func = self
                        .global_modules
                        .iter()
                        .find_map(|m| m.get_iter(iter_type));

                    #[cfg(not(feature = "no_module"))]
                    let func = func.or_else(|| global.get_iter(iter_type)).or_else(|| {
                        self.global_sub_modules
                            .values()
                            .find_map(|m| m.get_qualified_iter(iter_type))
                    });

                    if let Some(func) = func {
                        // Add the loop variables
                        let orig_scope_len = scope.len();
                        let counter_index = if counter.is_empty() {
                            usize::MAX
                        } else {
                            scope.push(counter.name.clone(), 0 as INT);
                            scope.len() - 1
                        };

                        scope.push(var_name.name.clone(), ());
                        let index = scope.len() - 1;

                        let mut loop_result = Ok(Dynamic::UNIT);

                        for (x, iter_value) in func(iter_obj).enumerate() {
                            // Increment counter
                            if counter_index < usize::MAX {
                                #[cfg(not(feature = "unchecked"))]
                                if x > INT::MAX as usize {
                                    loop_result = Err(ERR::ErrorArithmetic(
                                        format!("for-loop counter overflow: {x}"),
                                        counter.pos,
                                    )
                                    .into());
                                    break;
                                }

                                let index_value = Dynamic::from(x as INT);

                                *scope.get_mut_by_index(counter_index).write_lock().unwrap() =
                                    index_value;
                            }

                            let value = match iter_value {
                                Ok(v) => v.flatten(),
                                Err(err) => {
                                    loop_result = Err(err.fill_position(expr.position()));
                                    break;
                                }
                            };

                            *scope.get_mut_by_index(index).write_lock().unwrap() = value;

                            #[cfg(not(feature = "unchecked"))]
                            if let Err(err) = self
                                .inc_operations(&mut global.num_operations, statements.position())
                            {
                                loop_result = Err(err);
                                break;
                            }

                            if statements.is_empty() {
                                continue;
                            }

                            let result = self.eval_stmt_block(
                                scope, global, caches, lib, this_ptr, statements, true, level,
                            );

                            match result {
                                Ok(_) => (),
                                Err(err) => match *err {
                                    ERR::LoopBreak(false, ..) => (),
                                    ERR::LoopBreak(true, ..) => break,
                                    _ => {
                                        loop_result = Err(err);
                                        break;
                                    }
                                },
                            }
                        }

                        scope.rewind(orig_scope_len);

                        loop_result
                    } else {
                        Err(ERR::ErrorFor(expr.start_position()).into())
                    }
                } else {
                    iter_result
                }
            }

            // Continue/Break statement
            Stmt::BreakLoop(options, pos) => {
                Err(ERR::LoopBreak(options.contains(ASTFlags::BREAK), *pos).into())
            }

            // Try/Catch statement
            Stmt::TryCatch(x, ..) => {
                let TryCatchBlock {
                    try_block,
                    catch_var:
                        Ident {
                            name: catch_var, ..
                        },
                    catch_block,
                } = &**x;

                let result = self
                    .eval_stmt_block(scope, global, caches, lib, this_ptr, try_block, true, level)
                    .map(|_| Dynamic::UNIT);

                match result {
                    Ok(_) => result,
                    Err(err) if err.is_pseudo_error() => Err(err),
                    Err(err) if !err.is_catchable() => Err(err),
                    Err(mut err) => {
                        let err_value = match err.unwrap_inner() {
                            ERR::ErrorRuntime(x, ..) => x.clone(),

                            #[cfg(feature = "no_object")]
                            _ => {
                                err.take_position();
                                err.to_string().into()
                            }
                            #[cfg(not(feature = "no_object"))]
                            _ => {
                                let mut err_map = crate::Map::new();
                                let err_pos = err.take_position();

                                err_map.insert("message".into(), err.to_string().into());

                                if !global.source.is_empty() {
                                    err_map.insert("source".into(), global.source.clone().into());
                                }

                                if !err_pos.is_none() {
                                    err_map.insert(
                                        "line".into(),
                                        (err_pos.line().unwrap() as INT).into(),
                                    );
                                    err_map.insert(
                                        "position".into(),
                                        (err_pos.position().unwrap_or(0) as INT).into(),
                                    );
                                }

                                err.dump_fields(&mut err_map);
                                err_map.into()
                            }
                        };

                        let orig_scope_len = scope.len();

                        if !catch_var.is_empty() {
                            scope.push(catch_var.clone(), err_value);
                        }

                        let result = self.eval_stmt_block(
                            scope,
                            global,
                            caches,
                            lib,
                            this_ptr,
                            catch_block,
                            true,
                            level,
                        );

                        scope.rewind(orig_scope_len);

                        match result {
                            Ok(_) => Ok(Dynamic::UNIT),
                            Err(result_err) => match *result_err {
                                // Re-throw exception
                                ERR::ErrorRuntime(Dynamic(Union::Unit(..)), pos) => {
                                    err.set_position(pos);
                                    Err(err)
                                }
                                _ => Err(result_err),
                            },
                        }
                    }
                }
            }

            // Throw value
            Stmt::Return(Some(expr), options, pos) if options.contains(ASTFlags::BREAK) => self
                .eval_expr(scope, global, caches, lib, this_ptr, expr, level)
                .and_then(|v| Err(ERR::ErrorRuntime(v.flatten(), *pos).into())),

            // Empty throw
            Stmt::Return(None, options, pos) if options.contains(ASTFlags::BREAK) => {
                Err(ERR::ErrorRuntime(Dynamic::UNIT, *pos).into())
            }

            // Return value
            Stmt::Return(Some(expr), .., pos) => self
                .eval_expr(scope, global, caches, lib, this_ptr, expr, level)
                .and_then(|v| Err(ERR::Return(v.flatten(), *pos).into())),

            // Empty return
            Stmt::Return(None, .., pos) => Err(ERR::Return(Dynamic::UNIT, *pos).into()),

            // Let/const statement - shadowing disallowed
            Stmt::Var(x, .., pos) if !self.allow_shadowing() && scope.contains(&x.0) => {
                Err(ERR::ErrorVariableExists(x.0.to_string(), *pos).into())
            }
            // Let/const statement
            Stmt::Var(x, options, pos) => {
                let (var_name, expr, index) = &**x;

                let access = if options.contains(ASTFlags::CONSTANT) {
                    AccessMode::ReadOnly
                } else {
                    AccessMode::ReadWrite
                };
                let export = options.contains(ASTFlags::EXPORTED);

                // Check variable definition filter
                let result = if let Some(ref filter) = self.def_var_filter {
                    let will_shadow = scope.contains(var_name);
                    let nesting_level = global.scope_level;
                    let is_const = access == AccessMode::ReadOnly;
                    let info = VarDefInfo {
                        name: var_name,
                        is_const,
                        nesting_level,
                        will_shadow,
                    };
                    let context = EvalContext::new(self, scope, global, None, lib, this_ptr, level);

                    match filter(true, info, context) {
                        Ok(true) => None,
                        Ok(false) => {
                            Some(Err(
                                ERR::ErrorForbiddenVariable(var_name.to_string(), *pos).into()
                            ))
                        }
                        err @ Err(_) => Some(err),
                    }
                } else {
                    None
                };

                if let Some(result) = result {
                    result.map(|_| Dynamic::UNIT)
                } else {
                    // Evaluate initial value
                    let value_result = self
                        .eval_expr(scope, global, caches, lib, this_ptr, expr, level)
                        .map(Dynamic::flatten);

                    if let Ok(mut value) = value_result {
                        let _alias = if !rewind_scope {
                            // Put global constants into global module
                            #[cfg(not(feature = "no_function"))]
                            #[cfg(not(feature = "no_module"))]
                            if global.scope_level == 0
                                && access == AccessMode::ReadOnly
                                && lib.iter().any(|&m| !m.is_empty())
                            {
                                if global.constants.is_none() {
                                    global.constants = Some(crate::Shared::new(
                                        crate::Locked::new(std::collections::BTreeMap::new()),
                                    ));
                                }
                                crate::func::locked_write(global.constants.as_ref().unwrap())
                                    .insert(var_name.name.clone(), value.clone());
                            }

                            if export {
                                Some(var_name)
                            } else {
                                None
                            }
                        } else if export {
                            unreachable!("exported variable not on global level");
                        } else {
                            None
                        };

                        if let Some(index) = index {
                            value.set_access_mode(access);
                            *scope.get_mut_by_index(scope.len() - index.get()) = value;
                        } else {
                            scope.push_entry(var_name.name.clone(), access, value);
                        }

                        #[cfg(not(feature = "no_module"))]
                        if let Some(alias) = _alias {
                            scope.add_alias_by_index(scope.len() - 1, alias.name.as_str().into());
                        }

                        Ok(Dynamic::UNIT)
                    } else {
                        value_result
                    }
                }
            }

            // Import statement
            #[cfg(not(feature = "no_module"))]
            Stmt::Import(x, _pos) => {
                let (expr, export) = &**x;

                // Guard against too many modules
                #[cfg(not(feature = "unchecked"))]
                if global.num_modules_loaded >= self.max_modules() {
                    return Err(ERR::ErrorTooManyModules(*_pos).into());
                }

                let path_result = self
                    .eval_expr(scope, global, caches, lib, this_ptr, expr, level)
                    .and_then(|v| {
                        let typ = v.type_name();
                        v.try_cast::<crate::ImmutableString>().ok_or_else(|| {
                            self.make_type_mismatch_err::<crate::ImmutableString>(
                                typ,
                                expr.position(),
                            )
                        })
                    });

                if let Ok(path) = path_result {
                    use crate::ModuleResolver;

                    let path_pos = expr.start_position();

                    let resolver = global.embedded_module_resolver.clone();

                    let module_result = resolver
                        .as_ref()
                        .and_then(|r| match r.resolve_raw(self, global, &path, path_pos) {
                            Err(err) if matches!(*err, ERR::ErrorModuleNotFound(..)) => None,
                            result => Some(result),
                        })
                        .or_else(|| {
                            Some(
                                self.module_resolver
                                    .resolve_raw(self, global, &path, path_pos),
                            )
                        })
                        .unwrap_or_else(|| {
                            Err(ERR::ErrorModuleNotFound(path.to_string(), path_pos).into())
                        });

                    if let Ok(module) = module_result {
                        if !export.is_empty() {
                            if module.is_indexed() {
                                global.push_import(export.name.clone(), module);
                            } else {
                                // Index the module (making a clone copy if necessary) if it is not indexed
                                let mut m = crate::func::shared_take_or_clone(module);
                                m.build_index();
                                global.push_import(export.name.clone(), m);
                            }
                        }

                        global.num_modules_loaded += 1;

                        Ok(Dynamic::UNIT)
                    } else {
                        module_result.map(|_| Dynamic::UNIT)
                    }
                } else {
                    path_result.map(|_| Dynamic::UNIT)
                }
            }

            // Export statement
            #[cfg(not(feature = "no_module"))]
            Stmt::Export(x, ..) => {
                let (Ident { name, pos, .. }, Ident { name: alias, .. }) = &**x;
                // Mark scope variables as public
                if let Some((index, ..)) = scope.get_index(name) {
                    let alias = if alias.is_empty() { name } else { alias }.clone();
                    scope.add_alias_by_index(index, alias.into());
                    Ok(Dynamic::UNIT)
                } else {
                    Err(ERR::ErrorVariableNotFound(name.to_string(), *pos).into())
                }
            }

            // Share statement
            #[cfg(not(feature = "no_closure"))]
            Stmt::Share(name, pos) => {
                if let Some((index, ..)) = scope.get_index(name) {
                    let val = scope.get_mut_by_index(index);

                    if !val.is_shared() {
                        // Replace the variable with a shared value.
                        *val = std::mem::take(val).into_shared();
                    }
                    Ok(Dynamic::UNIT)
                } else {
                    Err(ERR::ErrorVariableNotFound(name.to_string(), *pos).into())
                }
            }

            _ => unreachable!("statement cannot be evaluated: {:?}", stmt),
        };

        #[cfg(feature = "debugging")]
        global.debugger.reset_status(reset_debugger);

        result
    }
}
