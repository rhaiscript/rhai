//! Module defining functions for evaluating a statement.

use super::{Caches, EvalContext, GlobalRuntimeState, Target};
use crate::ast::{
    ASTFlags, BinaryExpr, Expr, FlowControl, OpAssignment, Stmt, SwitchCasesCollection,
};
use crate::eval::search_namespace;
use crate::func::{get_builtin_op_assignment_fn, get_hasher};
use crate::tokenizer::Token;
use crate::types::dynamic::{AccessMode, Union};
use crate::{Dynamic, Engine, RhaiResult, RhaiResultOf, Scope, VarDefInfo, ERR, INT};
use std::hash::{Hash, Hasher};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// If the value is a string, intern it.
#[inline(always)]
fn intern_string(value: Dynamic, engine: &Engine) -> Dynamic {
    match value.0 {
        Union::Str(s, ..) => engine.get_interned_string(s).into(),
        _ => value,
    }
}

impl Engine {
    /// Evaluate a statements block.
    pub(crate) fn eval_stmt_block(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        mut this_ptr: Option<&mut Dynamic>,
        statements: &[Stmt],
        restore_orig_state: bool,
    ) -> RhaiResult {
        if statements.is_empty() {
            return Ok(Dynamic::UNIT);
        }

        // Restore scope at end of block if necessary
        defer! { scope if restore_orig_state => rewind; let orig_scope_len = scope.len(); }

        // Restore global state at end of block if necessary
        let orig_always_search_scope = global.always_search_scope;
        #[cfg(not(feature = "no_module"))]
        let orig_imports_len = global.num_imports();

        if restore_orig_state {
            global.scope_level += 1;
        }

        defer! { global if restore_orig_state => move |g| {
            g.scope_level -= 1;

            #[cfg(not(feature = "no_module"))]
            g.truncate_imports(orig_imports_len);

            // The impact of new local variables goes away at the end of a block
            // because any new variables introduced will go out of scope
            g.always_search_scope = orig_always_search_scope;
        }}

        // Pop new function resolution caches at end of block
        defer! {
            caches => rewind_fn_resolution_caches;
            let orig_fn_resolution_caches_len = caches.fn_resolution_caches_len();
        }

        // Run the statements
        statements.iter().try_fold(Dynamic::UNIT, |_, stmt| {
            let this_ptr = this_ptr.as_deref_mut();

            #[cfg(not(feature = "no_module"))]
            let orig_imports_len = global.num_imports();

            let result =
                self.eval_stmt(global, caches, scope, this_ptr, stmt, restore_orig_state)?;

            #[cfg(not(feature = "no_module"))]
            if matches!(stmt, Stmt::Import(..)) {
                // Get the extra modules - see if any functions are marked global.
                // Without global functions, the extra modules never affect function resolution.
                if global
                    .scan_imports_raw()
                    .skip(orig_imports_len)
                    .any(|(.., m)| m.contains_indexed_global_functions())
                {
                    // Different scenarios where the cache must be cleared - notice that this is
                    // expensive as all function resolutions must start again
                    if caches.fn_resolution_caches_len() > orig_fn_resolution_caches_len {
                        // When new module is imported with global functions and there is already
                        // a new cache, just clear it
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

            Ok(result)
        })
    }

    /// Evaluate an op-assignment statement.
    pub(crate) fn eval_op_assignment(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        op_info: &OpAssignment,
        root: &Expr,
        target: &mut Target,
        mut new_val: Dynamic,
    ) -> RhaiResultOf<()> {
        // Assignment to constant variable?
        if target.as_ref().is_read_only() {
            let name = root.get_variable_name(false).unwrap_or_default();
            let pos = root.start_position();
            return Err(ERR::ErrorAssignmentToConstant(name.to_string(), pos).into());
        }

        let pos = op_info.position();

        if let Some((hash_x, hash, op_x, op_x_str, op, op_str)) = op_info.get_op_assignment_info() {
            let mut lock_guard = target.as_mut().write_lock::<Dynamic>().unwrap();
            let mut done = false;

            // Short-circuit built-in op-assignments if under Fast Operators mode
            if self.fast_operators() {
                #[allow(clippy::wildcard_imports)]
                use Token::*;

                done = true;

                // For extremely simple primary data operations, do it directly
                // to avoid the overhead of calling a function.
                match (&mut lock_guard.0, &mut new_val.0) {
                    (Union::Bool(b1, ..), Union::Bool(b2, ..)) => match op_x {
                        AndAssign => *b1 = *b1 && *b2,
                        OrAssign => *b1 = *b1 || *b2,
                        XOrAssign => *b1 ^= *b2,
                        _ => done = false,
                    },
                    (Union::Int(n1, ..), Union::Int(n2, ..)) => {
                        #[cfg(not(feature = "unchecked"))]
                        #[allow(clippy::wildcard_imports)]
                        use crate::packages::arithmetic::arith_basic::INT::functions::*;

                        #[cfg(not(feature = "unchecked"))]
                        match op_x {
                            PlusAssign => {
                                *n1 = add(*n1, *n2).map_err(|err| err.fill_position(pos))?
                            }
                            MinusAssign => {
                                *n1 = subtract(*n1, *n2).map_err(|err| err.fill_position(pos))?
                            }
                            MultiplyAssign => {
                                *n1 = multiply(*n1, *n2).map_err(|err| err.fill_position(pos))?
                            }
                            DivideAssign => {
                                *n1 = divide(*n1, *n2).map_err(|err| err.fill_position(pos))?
                            }
                            ModuloAssign => {
                                *n1 = modulo(*n1, *n2).map_err(|err| err.fill_position(pos))?
                            }
                            _ => done = false,
                        }
                        #[cfg(feature = "unchecked")]
                        match op_x {
                            PlusAssign => *n1 += *n2,
                            MinusAssign => *n1 -= *n2,
                            MultiplyAssign => *n1 *= *n2,
                            DivideAssign => *n1 /= *n2,
                            ModuloAssign => *n1 %= *n2,
                            _ => done = false,
                        }
                    }
                    #[cfg(not(feature = "no_float"))]
                    (Union::Float(f1, ..), Union::Float(f2, ..)) => match op_x {
                        PlusAssign => **f1 += **f2,
                        MinusAssign => **f1 -= **f2,
                        MultiplyAssign => **f1 *= **f2,
                        DivideAssign => **f1 /= **f2,
                        ModuloAssign => **f1 %= **f2,
                        _ => done = false,
                    },
                    #[cfg(not(feature = "no_float"))]
                    (Union::Float(f1, ..), Union::Int(n2, ..)) => match op_x {
                        PlusAssign => **f1 += *n2 as crate::FLOAT,
                        MinusAssign => **f1 -= *n2 as crate::FLOAT,
                        MultiplyAssign => **f1 *= *n2 as crate::FLOAT,
                        DivideAssign => **f1 /= *n2 as crate::FLOAT,
                        ModuloAssign => **f1 %= *n2 as crate::FLOAT,
                        _ => done = false,
                    },
                    _ => done = false,
                }

                if !done {
                    if let Some((func, need_context)) =
                        get_builtin_op_assignment_fn(op_x, &lock_guard, &new_val)
                    {
                        // We may not need to bump the level because built-in's do not need it.
                        //defer! { let orig_level = global.level; global.level += 1 }

                        let args = &mut [&mut *lock_guard, &mut new_val];
                        let context = need_context
                            .then(|| (self, op_x_str, global.source(), &*global, pos).into());
                        let _ = func(context, args).map_err(|err| err.fill_position(pos))?;
                        done = true;
                    }
                }
            }

            if !done {
                let opx = Some(op_x);
                let args = &mut [&mut *lock_guard, &mut new_val];

                match self.exec_native_fn_call(
                    global, caches, op_x_str, opx, hash_x, args, true, false, pos,
                ) {
                    Ok(_) => (),
                    Err(err) if matches!(*err, ERR::ErrorFunctionNotFound(ref f, ..) if f.starts_with(op_x_str)) =>
                    {
                        // Expand to `var = var op rhs`
                        let op = Some(op);

                        *args[0] = self
                            .exec_native_fn_call(
                                global, caches, op_str, op, hash, args, true, false, pos,
                            )?
                            .0;
                    }
                    Err(err) => return Err(err),
                }

                self.check_data_size(&*args[0], root.position())?;
            }
        } else {
            // Normal assignment
            match target {
                // Lock it again just in case it is shared
                Target::RefMut(_) | Target::TempValue(_) => {
                    *target.as_mut().write_lock::<Dynamic>().unwrap() = new_val
                }
                #[allow(unreachable_patterns)]
                _ => *target.as_mut() = new_val,
            }
        }

        target.propagate_changed_value(pos)
    }

    /// Evaluate a statement.
    pub(crate) fn eval_stmt(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        mut this_ptr: Option<&mut Dynamic>,
        stmt: &Stmt,
        rewind_scope: bool,
    ) -> RhaiResult {
        self.track_operation(global, stmt.position())?;

        #[cfg(feature = "debugging")]
        let reset =
            self.run_debugger_with_reset(global, caches, scope, this_ptr.as_deref_mut(), stmt)?;
        #[cfg(feature = "debugging")]
        defer! { global if Some(reset) => move |g| g.debugger_mut().reset_status(reset) }

        match stmt {
            // No-op
            Stmt::Noop(..) => Ok(Dynamic::UNIT),

            // Expression as statement
            Stmt::Expr(expr) => self
                .eval_expr(global, caches, scope, this_ptr, expr)
                .map(Dynamic::flatten),

            // Block scope
            Stmt::Block(stmts, ..) => {
                if stmts.is_empty() {
                    Ok(Dynamic::UNIT)
                } else {
                    self.eval_stmt_block(global, caches, scope, this_ptr, stmts.statements(), true)
                }
            }

            // Function call
            Stmt::FnCall(x, pos) => {
                self.eval_fn_call_expr(global, caches, scope, this_ptr, x, *pos)
            }

            // Assignment
            Stmt::Assignment(x, ..) => {
                let (op_info, BinaryExpr { lhs, rhs }) = &**x;

                if let Expr::ThisPtr(..) = lhs {
                    if this_ptr.is_none() {
                        return Err(ERR::ErrorUnboundThis(lhs.position()).into());
                    }

                    #[cfg(not(feature = "no_function"))]
                    {
                        let rhs_val = self
                            .eval_expr(global, caches, scope, this_ptr.as_deref_mut(), rhs)?
                            .flatten();

                        self.track_operation(global, lhs.position())?;

                        let target = &mut this_ptr.unwrap().into();

                        self.eval_op_assignment(global, caches, op_info, lhs, target, rhs_val)?;
                    }
                    #[cfg(feature = "no_function")]
                    unreachable!();
                } else if let Expr::Variable(x, ..) = lhs {
                    let rhs_val = self
                        .eval_expr(global, caches, scope, this_ptr.as_deref_mut(), rhs)?
                        .flatten();

                    self.track_operation(global, lhs.position())?;

                    let mut target = search_namespace(self, global, caches, scope, this_ptr, lhs)?;

                    let is_temp_result = !target.is_ref();

                    #[cfg(not(feature = "no_closure"))]
                    // Also handle case where target is a `Dynamic` shared value
                    // (returned by a variable resolver, for example)
                    let is_temp_result = is_temp_result && !target.is_shared();

                    // Cannot assign to temp result from expression
                    if is_temp_result {
                        return Err(ERR::ErrorAssignmentToConstant(
                            x.3.to_string(),
                            lhs.position(),
                        )
                        .into());
                    }

                    self.eval_op_assignment(global, caches, op_info, lhs, &mut target, rhs_val)?;
                } else {
                    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
                    {
                        let rhs_val = self
                            .eval_expr(global, caches, scope, this_ptr.as_deref_mut(), rhs)?
                            .flatten();
                        let _new_val = Some((intern_string(rhs_val, self), op_info));

                        // Must be either `var[index] op= val` or `var.prop op= val`.
                        // The return value of any op-assignment (should be `()`) is thrown away and not used.
                        let _ = match lhs {
                            // this op= rhs (handled above)
                            Expr::ThisPtr(..) => {
                                unreachable!("Expr::ThisPtr case is already handled")
                            }
                            // name op= rhs (handled above)
                            Expr::Variable(..) => {
                                unreachable!("Expr::Variable case is already handled")
                            }
                            // idx_lhs[idx_expr] op= rhs
                            #[cfg(not(feature = "no_index"))]
                            Expr::Index(..) => self.eval_dot_index_chain(
                                global, caches, scope, this_ptr, lhs, _new_val,
                            ),
                            // dot_lhs.dot_rhs op= rhs
                            #[cfg(not(feature = "no_object"))]
                            Expr::Dot(..) => self.eval_dot_index_chain(
                                global, caches, scope, this_ptr, lhs, _new_val,
                            ),
                            _ => unreachable!("cannot assign to expression: {:?}", lhs),
                        }?;
                    }
                }

                Ok(Dynamic::UNIT)
            }

            // Variable definition
            Stmt::Var(x, options, pos) => {
                if !self.allow_shadowing() && scope.contains(x.0.as_str()) {
                    return Err(ERR::ErrorVariableExists(x.0.as_str().to_string(), *pos).into());
                }

                // Let/const statement
                let (var_name, expr, index) = &**x;

                let access = if options.intersects(ASTFlags::CONSTANT) {
                    AccessMode::ReadOnly
                } else {
                    AccessMode::ReadWrite
                };
                let export = options.intersects(ASTFlags::EXPORTED);

                // Check variable definition filter
                if let Some(ref filter) = self.def_var_filter {
                    let will_shadow = scope.contains(var_name.as_str());
                    let is_const = access == AccessMode::ReadOnly;
                    let info = VarDefInfo::new(
                        var_name.as_str(),
                        is_const,
                        global.scope_level,
                        will_shadow,
                    );
                    let orig_scope_len = scope.len();
                    let context =
                        EvalContext::new(self, global, caches, scope, this_ptr.as_deref_mut());
                    let filter_result = filter(true, info, context);

                    if orig_scope_len != scope.len() {
                        // The scope is changed, always search from now on
                        global.always_search_scope = true;
                    }

                    if !filter_result? {
                        return Err(ERR::ErrorForbiddenVariable(
                            var_name.as_str().to_string(),
                            *pos,
                        )
                        .into());
                    }
                }

                // Guard against too many variables
                #[cfg(not(feature = "unchecked"))]
                if index.is_none() && scope.len() >= self.max_variables() {
                    return Err(ERR::ErrorTooManyVariables(*pos).into());
                }

                // Evaluate initial value
                let value = self
                    .eval_expr(global, caches, scope, this_ptr, expr)?
                    .flatten();
                let mut value = intern_string(value, self);

                let _alias = if !rewind_scope {
                    // Put global constants into global module
                    #[cfg(not(feature = "no_function"))]
                    #[cfg(not(feature = "no_module"))]
                    if global.scope_level == 0
                        && access == AccessMode::ReadOnly
                        && global.lib.iter().any(|m| !m.is_empty())
                    {
                        crate::func::locked_write(global.constants.get_or_insert_with(|| {
                            crate::Shared::new(
                                crate::Locked::new(std::collections::BTreeMap::new()),
                            )
                        }))
                        .insert(var_name.name.clone(), value.clone());
                    }

                    export.then_some(var_name)
                } else if !export {
                    None
                } else {
                    unreachable!("exported variable not on global level");
                };

                match index {
                    Some(index) => {
                        value.set_access_mode(access);
                        *scope.get_mut_by_index(scope.len() - index.get()) = value;
                    }
                    _ => {
                        scope.push_entry(var_name.name.clone(), access, value);
                    }
                }

                #[cfg(not(feature = "no_module"))]
                if let Some(alias) = _alias {
                    scope.add_alias_by_index(scope.len() - 1, alias.as_str().into());
                }

                Ok(Dynamic::UNIT)
            }

            // If statement
            Stmt::If(x, ..) => {
                let FlowControl { expr, body, branch } = &**x;

                let guard_val = self
                    .eval_expr(global, caches, scope, this_ptr.as_deref_mut(), expr)?
                    .as_bool()
                    .map_err(|typ| self.make_type_mismatch_err::<bool>(typ, expr.position()))?;

                if guard_val && !body.is_empty() {
                    self.eval_stmt_block(global, caches, scope, this_ptr, body.statements(), true)
                } else if !guard_val && !branch.is_empty() {
                    self.eval_stmt_block(global, caches, scope, this_ptr, branch.statements(), true)
                } else {
                    Ok(Dynamic::UNIT)
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

                let mut result = None;

                let value = self.eval_expr(global, caches, scope, this_ptr.as_deref_mut(), expr)?;

                if value.is_hashable() {
                    let hasher = &mut get_hasher();
                    value.hash(hasher);
                    let hash = hasher.finish();

                    // First check hashes
                    if let Some(case_blocks_list) = cases.get(&hash) {
                        debug_assert!(!case_blocks_list.is_empty());

                        for &index in case_blocks_list {
                            let block = &expressions[index];

                            let cond_result = match block.lhs {
                                Expr::BoolConstant(b, ..) => b,
                                ref c => self
                                    .eval_expr(global, caches, scope, this_ptr.as_deref_mut(), c)?
                                    .as_bool()
                                    .map_err(|typ| {
                                        self.make_type_mismatch_err::<bool>(typ, c.position())
                                    })?,
                            };

                            if cond_result {
                                result = Some(&block.rhs);
                                break;
                            }
                        }
                    } else if !ranges.is_empty() {
                        // Then check integer ranges
                        for r in ranges.iter().filter(|r| r.contains(&value)) {
                            let BinaryExpr { lhs, rhs } = &expressions[r.index()];

                            let cond_result = match lhs {
                                Expr::BoolConstant(b, ..) => *b,
                                c => self
                                    .eval_expr(global, caches, scope, this_ptr.as_deref_mut(), c)?
                                    .as_bool()
                                    .map_err(|typ| {
                                        self.make_type_mismatch_err::<bool>(typ, c.position())
                                    })?,
                            };

                            if cond_result {
                                result = Some(rhs);
                                break;
                            }
                        }
                    }
                }

                result
                    .or_else(|| def_case.as_ref().map(|&index| &expressions[index].rhs))
                    .map_or(Ok(Dynamic::UNIT), |expr| {
                        self.eval_expr(global, caches, scope, this_ptr, expr)
                    })
            }

            // Loop
            Stmt::While(x, ..)
                if matches!(x.expr, Expr::Unit(..) | Expr::BoolConstant(true, ..)) =>
            {
                let FlowControl { body, .. } = &**x;

                if body.is_empty() {
                    loop {
                        self.track_operation(global, body.position())?;
                    }
                }

                loop {
                    let this_ptr = this_ptr.as_deref_mut();
                    let statements = body.statements();

                    match self.eval_stmt_block(global, caches, scope, this_ptr, statements, true) {
                        Ok(..) => (),
                        Err(err) => match *err {
                            ERR::LoopBreak(false, ..) => (),
                            ERR::LoopBreak(true, value, ..) => break Ok(value),
                            _ => break Err(err),
                        },
                    }
                }
            }

            // While loop
            Stmt::While(x, ..) => {
                let FlowControl { expr, body, .. } = &**x;

                loop {
                    let condition = self
                        .eval_expr(global, caches, scope, this_ptr.as_deref_mut(), expr)?
                        .as_bool()
                        .map_err(|typ| self.make_type_mismatch_err::<bool>(typ, expr.position()))?;

                    if !condition {
                        break Ok(Dynamic::UNIT);
                    }

                    if body.is_empty() {
                        continue;
                    }

                    let this_ptr = this_ptr.as_deref_mut();
                    let statements = body.statements();

                    match self.eval_stmt_block(global, caches, scope, this_ptr, statements, true) {
                        Ok(..) => (),
                        Err(err) => match *err {
                            ERR::LoopBreak(false, ..) => (),
                            ERR::LoopBreak(true, value, ..) => break Ok(value),
                            _ => break Err(err),
                        },
                    }
                }
            }

            // Do loop
            Stmt::Do(x, options, ..) => {
                let FlowControl { expr, body, .. } = &**x;
                let is_while = !options.intersects(ASTFlags::NEGATED);

                loop {
                    if !body.is_empty() {
                        let this_ptr = this_ptr.as_deref_mut();
                        let statements = body.statements();

                        match self
                            .eval_stmt_block(global, caches, scope, this_ptr, statements, true)
                        {
                            Ok(..) => (),
                            Err(err) => match *err {
                                ERR::LoopBreak(false, ..) => continue,
                                ERR::LoopBreak(true, value, ..) => break Ok(value),
                                _ => break Err(err),
                            },
                        }
                    }

                    let condition = self
                        .eval_expr(global, caches, scope, this_ptr.as_deref_mut(), expr)?
                        .as_bool()
                        .map_err(|typ| self.make_type_mismatch_err::<bool>(typ, expr.position()))?;

                    if condition ^ is_while {
                        break Ok(Dynamic::UNIT);
                    }
                }
            }

            // For loop
            Stmt::For(x, ..) => {
                let (var_name, counter, FlowControl { expr, body, .. }) = &**x;

                // Guard against too many variables
                #[cfg(not(feature = "unchecked"))]
                if scope.len() >= self.max_variables() - usize::from(counter.is_some()) {
                    return Err(ERR::ErrorTooManyVariables(var_name.pos).into());
                }

                let iter_obj = self
                    .eval_expr(global, caches, scope, this_ptr.as_deref_mut(), expr)?
                    .flatten();

                let iter_type = iter_obj.type_id();

                // lib should only contain scripts, so technically they cannot have iterators

                // Search order:
                // 1) Global namespace - functions registered via Engine::register_XXX
                // 2) Global modules - packages
                // 3) Imported modules - functions marked with global namespace
                // 4) Global sub-modules - functions marked with global namespace
                let iter_func = self
                    .global_modules
                    .iter()
                    .find_map(|m| m.get_iter(iter_type));

                #[cfg(not(feature = "no_module"))]
                let iter_func = iter_func
                    .or_else(|| global.get_iter(iter_type))
                    .or_else(|| {
                        self.global_sub_modules
                            .values()
                            .find_map(|m| m.get_qualified_iter(iter_type))
                    });

                let iter_func = iter_func.ok_or_else(|| ERR::ErrorFor(expr.start_position()))?;

                // Restore scope at end of statement
                defer! { scope => rewind; let orig_scope_len = scope.len(); }

                // Add the loop variables
                let counter_index = counter.as_ref().map(|counter| {
                    scope.push(counter.name.clone(), 0 as INT);
                    scope.len() - 1
                });

                scope.push(var_name.name.clone(), ());
                let index = scope.len() - 1;

                let mut result = Dynamic::UNIT;

                if body.is_empty() {
                    for _ in iter_func(iter_obj) {
                        self.track_operation(global, body.position())?;
                    }
                } else {
                    for (i, iter_value) in iter_func(iter_obj).enumerate() {
                        // Increment counter
                        if let Some(counter_index) = counter_index {
                            // As the variable increments from 0, this should always work
                            // since any overflow will first be caught below.
                            let index_value = i as INT;

                            #[cfg(not(feature = "unchecked"))]
                            #[allow(clippy::absurd_extreme_comparisons)]
                            if index_value > crate::MAX_USIZE_INT {
                                return Err(ERR::ErrorArithmetic(
                                    format!("for-loop counter overflow: {i}"),
                                    counter.as_ref().unwrap().pos,
                                )
                                .into());
                            }

                            *scope.get_mut_by_index(counter_index).write_lock().unwrap() =
                                Dynamic::from_int(index_value);
                        }

                        // Set loop value
                        let value = iter_value
                            .map_err(|err| err.fill_position(expr.position()))?
                            .flatten();

                        *scope.get_mut_by_index(index).write_lock().unwrap() = value;

                        // Run block
                        let this_ptr = this_ptr.as_deref_mut();
                        let statements = body.statements();

                        match self
                            .eval_stmt_block(global, caches, scope, this_ptr, statements, true)
                        {
                            Ok(_) => (),
                            Err(err) => match *err {
                                ERR::LoopBreak(false, ..) => (),
                                ERR::LoopBreak(true, value, ..) => {
                                    result = value;
                                    break;
                                }
                                _ => return Err(err),
                            },
                        }
                    }
                }

                Ok(result)
            }

            // Continue/Break statement
            Stmt::BreakLoop(expr, options, pos) => {
                let is_break = options.intersects(ASTFlags::BREAK);

                let value = match expr {
                    Some(ref expr) => self.eval_expr(global, caches, scope, this_ptr, expr)?,
                    None => Dynamic::UNIT,
                };

                Err(ERR::LoopBreak(is_break, value, *pos).into())
            }

            // Try/Catch statement
            Stmt::TryCatch(x, ..) => {
                let FlowControl {
                    body,
                    expr: catch_var,
                    branch,
                } = &**x;

                match self.eval_stmt_block(
                    global,
                    caches,
                    scope,
                    this_ptr.as_deref_mut(),
                    body.statements(),
                    true,
                ) {
                    r @ Ok(_) => r,
                    Err(err) if err.is_pseudo_error() => Err(err),
                    Err(err) if !err.is_catchable() => Err(err),
                    Err(mut err) => {
                        let err_value = match err.unwrap_inner() {
                            // No error variable
                            _ if catch_var.is_unit() => Dynamic::UNIT,

                            ERR::ErrorRuntime(x, ..) => x.clone(),

                            #[cfg(feature = "no_object")]
                            _ => {
                                let _ = err.take_position();
                                err.to_string().into()
                            }
                            #[cfg(not(feature = "no_object"))]
                            _ => {
                                let mut err_map = crate::Map::new();
                                let err_pos = err.take_position();

                                err_map.insert("message".into(), err.to_string().into());

                                if let Some(ref source) = global.source {
                                    err_map.insert("source".into(), source.into());
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

                        // Restore scope at end of block
                        defer! { scope if !catch_var.is_unit() => rewind; let orig_scope_len = scope.len(); }

                        if let Expr::Variable(x, ..) = catch_var {
                            // Guard against too many variables
                            #[cfg(not(feature = "unchecked"))]
                            if scope.len() >= self.max_variables() {
                                return Err(ERR::ErrorTooManyVariables(catch_var.position()).into());
                            }
                            scope.push(x.3.clone(), err_value);
                        }

                        let this_ptr = this_ptr.as_deref_mut();
                        let statements = branch.statements();

                        self.eval_stmt_block(global, caches, scope, this_ptr, statements, true)
                            .map(|_| Dynamic::UNIT)
                            .map_err(|result_err| match *result_err {
                                // Re-throw exception
                                ERR::ErrorRuntime(v, pos) if v.is_unit() => {
                                    err.set_position(pos);
                                    err
                                }
                                _ => result_err,
                            })
                    }
                }
            }

            // Throw value
            Stmt::Return(Some(expr), options, pos) if options.intersects(ASTFlags::BREAK) => self
                .eval_expr(global, caches, scope, this_ptr, expr)
                .and_then(|v| Err(ERR::ErrorRuntime(v.flatten(), *pos).into())),

            // Empty throw
            Stmt::Return(None, options, pos) if options.intersects(ASTFlags::BREAK) => {
                Err(ERR::ErrorRuntime(Dynamic::UNIT, *pos).into())
            }

            // Return value
            Stmt::Return(Some(expr), .., pos) => self
                .eval_expr(global, caches, scope, this_ptr, expr)
                .and_then(|v| Err(ERR::Return(v.flatten(), *pos).into())),

            // Empty return
            Stmt::Return(None, .., pos) => Err(ERR::Return(Dynamic::UNIT, *pos).into()),

            // Import statement
            #[cfg(not(feature = "no_module"))]
            Stmt::Import(x, _pos) => {
                use crate::ModuleResolver;

                let (expr, export) = &**x;

                // Guard against too many modules
                #[cfg(not(feature = "unchecked"))]
                if global.num_modules_loaded >= self.max_modules() {
                    return Err(ERR::ErrorTooManyModules(*_pos).into());
                }

                let v = self.eval_expr(global, caches, scope, this_ptr, expr)?;

                let path = v.try_cast_raw::<crate::ImmutableString>().map_err(|v| {
                    self.make_type_mismatch_err::<crate::ImmutableString>(
                        v.type_name(),
                        expr.position(),
                    )
                })?;

                let path_pos = expr.start_position();

                let resolver = global.embedded_module_resolver.clone();

                let module = resolver
                    .as_ref()
                    .and_then(
                        |r| match r.resolve_raw(self, global, scope, &path, path_pos) {
                            Err(err) if matches!(*err, ERR::ErrorModuleNotFound(..)) => None,
                            result => Some(result),
                        },
                    )
                    .or_else(|| {
                        Some(
                            self.module_resolver()
                                .resolve_raw(self, global, scope, &path, path_pos),
                        )
                    })
                    .unwrap_or_else(|| {
                        Err(ERR::ErrorModuleNotFound(path.to_string(), path_pos).into())
                    })?;

                let (export, must_be_indexed) = if export.is_empty() {
                    (self.const_empty_string(), false)
                } else {
                    (export.name.clone(), true)
                };

                if !must_be_indexed || module.is_indexed() {
                    global.push_import(export, module);
                } else {
                    // Index the module (making a clone copy if necessary) if it is not indexed
                    let mut m = crate::func::shared_take_or_clone(module);
                    m.build_index();
                    global.push_import(export, m);
                }

                global.num_modules_loaded += 1;

                Ok(Dynamic::UNIT)
            }

            // Export statement
            #[cfg(not(feature = "no_module"))]
            Stmt::Export(x, ..) => {
                use crate::ast::Ident;
                let (Ident { name, pos, .. }, Ident { name: alias, .. }) = &**x;
                // Mark scope variables as public
                scope.search(name).map_or_else(
                    || Err(ERR::ErrorVariableNotFound(name.to_string(), *pos).into()),
                    |index| {
                        let alias = if alias.is_empty() { name } else { alias };
                        scope.add_alias_by_index(index, alias.clone());
                        Ok(Dynamic::UNIT)
                    },
                )
            }

            // Share statement
            #[cfg(not(feature = "no_closure"))]
            Stmt::Share(x) => {
                for (var, index) in &**x {
                    let index = index
                        .map(|n| scope.len() - n.get())
                        .or_else(|| scope.search(&var.name))
                        .ok_or_else(|| {
                            Box::new(ERR::ErrorVariableNotFound(var.name.to_string(), var.pos))
                        })?;

                    let val = scope.get_mut_by_index(index);

                    if !val.is_shared() {
                        // Replace the variable with a shared value.
                        *val = val.take().into_shared();
                    }
                }

                Ok(Dynamic::UNIT)
            }

            #[allow(unreachable_patterns)]
            _ => unreachable!("statement cannot be evaluated: {:?}", stmt),
        }
    }

    /// Evaluate a list of statements with no `this` pointer.
    /// This is commonly used to evaluate a list of statements in an [`AST`][crate::AST] or a script function body.
    #[inline(always)]
    pub(crate) fn eval_global_statements(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        statements: &[Stmt],
        map_exit_to_return_value: bool,
    ) -> RhaiResult {
        self.eval_stmt_block(global, caches, scope, None, statements, false)
            .or_else(|err| match *err {
                ERR::Return(out, ..) => Ok(out),
                ERR::Exit(out, ..) if map_exit_to_return_value => Ok(out),
                ERR::LoopBreak(..) => {
                    unreachable!("no outer loop scope to break out of")
                }
                _ => Err(err),
            })
    }
}
