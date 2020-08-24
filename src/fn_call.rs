//! Implement function-calling mechanism for `Engine`.

use crate::any::Dynamic;
use crate::calc_fn_hash;
use crate::engine::{
    search_imports, search_namespace, search_scope_only, Engine, Imports, State, KEYWORD_DEBUG,
    KEYWORD_EVAL, KEYWORD_FN_PTR, KEYWORD_FN_PTR_CALL, KEYWORD_FN_PTR_CURRY, KEYWORD_IS_SHARED,
    KEYWORD_PRINT, KEYWORD_TYPE_OF,
};
use crate::error::ParseErrorType;
use crate::fn_native::{FnCallArgs, FnPtr};
use crate::module::{Module, ModuleRef};
use crate::optimize::OptimizationLevel;
use crate::parser::{Expr, ImmutableString, AST, INT};
use crate::result::EvalAltResult;
use crate::scope::Scope;
use crate::stdlib::ops::Deref;
use crate::token::Position;
use crate::utils::StaticVec;

#[cfg(not(feature = "no_function"))]
use crate::{
    parser::ScriptFnDef, r#unsafe::unsafe_cast_var_name_to_lifetime,
    scope::EntryType as ScopeEntryType,
};

#[cfg(not(feature = "no_float"))]
use crate::parser::FLOAT;

#[cfg(not(feature = "no_decimal"))]
use rust_decimal::Decimal;

#[cfg(not(feature = "no_index"))]
use crate::engine::{FN_IDX_GET, FN_IDX_SET};

#[cfg(not(feature = "no_object"))]
use crate::engine::{Map, Target, FN_GET, FN_SET};

#[cfg(not(feature = "no_closure"))]
#[cfg(not(feature = "no_function"))]
use crate::scope::Entry as ScopeEntry;

use crate::stdlib::{
    any::{type_name, TypeId},
    boxed::Box,
    convert::TryFrom,
    format,
    iter::{empty, once},
    mem,
    string::ToString,
    vec::Vec,
};

#[cfg(not(feature = "no_closure"))]
#[cfg(not(feature = "no_function"))]
use crate::stdlib::{collections::HashSet, string::String};

/// Extract the property name from a getter function name.
#[inline(always)]
fn extract_prop_from_getter(_fn_name: &str) -> Option<&str> {
    #[cfg(not(feature = "no_object"))]
    if _fn_name.starts_with(FN_GET) {
        return Some(&_fn_name[FN_GET.len()..]);
    }

    None
}

/// Extract the property name from a setter function name.
#[inline(always)]
fn extract_prop_from_setter(_fn_name: &str) -> Option<&str> {
    #[cfg(not(feature = "no_object"))]
    if _fn_name.starts_with(FN_SET) {
        return Some(&_fn_name[FN_SET.len()..]);
    }

    None
}

/// A type that temporarily stores a mutable reference to a `Dynamic`,
/// replacing it with a cloned copy.
#[derive(Debug, Default)]
struct ArgBackup<'a> {
    orig_mut: Option<&'a mut Dynamic>,
    value_copy: Dynamic,
}

impl<'a> ArgBackup<'a> {
    /// This function replaces the first argument of a method call with a clone copy.
    /// This is to prevent a pure function unintentionally consuming the first argument.
    ///
    /// `restore_first_arg` must be called before the end of the scope to prevent the shorter lifetime from leaking.
    ///
    /// # Safety
    ///
    /// This method blindly casts a reference to another lifetime, which saves allocation and string cloning.
    ///
    /// If `restore_first_arg` is called before the end of the scope, the shorter lifetime will not leak.
    fn change_first_arg_to_copy(&mut self, normalize: bool, args: &mut FnCallArgs<'a>) {
        // Only do it for method calls with arguments.
        if !normalize || args.is_empty() {
            return;
        }

        // Clone the original value.
        self.value_copy = args[0].clone();

        // Replace the first reference with a reference to the clone, force-casting the lifetime.
        // Must remember to restore it later with `restore_first_arg`.
        //
        // # Safety
        //
        // Blindly casting a reference to another lifetime saves allocation and string cloning,
        // but must be used with the utmost care.
        //
        // We can do this here because, before the end of this scope, we'd restore the original reference
        // via `restore_first_arg`. Therefore this shorter lifetime does not leak.
        self.orig_mut = Some(mem::replace(args.get_mut(0).unwrap(), unsafe {
            mem::transmute(&mut self.value_copy)
        }));
    }

    /// This function restores the first argument that was replaced by `change_first_arg_to_copy`.
    ///
    /// # Safety
    ///
    /// If `change_first_arg_to_copy` has been called, this function **MUST** be called _BEFORE_ exiting
    /// the current scope.  Otherwise it is undefined behavior as the shorter lifetime will leak.
    fn restore_first_arg(&mut self, args: &mut FnCallArgs<'a>) {
        if let Some(this_pointer) = self.orig_mut.take() {
            args[0] = this_pointer;
        }
    }
}

impl Drop for ArgBackup<'_> {
    fn drop(&mut self) {
        // Panic if the shorter lifetime leaks.
        assert!(
            self.orig_mut.is_none(),
            "MutBackup::restore has not been called prior to existing this scope"
        );
    }
}

// Add captured variables into scope
#[cfg(not(feature = "no_closure"))]
#[cfg(not(feature = "no_function"))]
fn add_captured_variables_into_scope<'s>(
    externals: &HashSet<String>,
    captured: Scope<'s>,
    scope: &mut Scope<'s>,
) {
    captured
        .into_iter()
        .filter(|ScopeEntry { name, .. }| externals.contains(name.as_ref()))
        .for_each(
            |ScopeEntry {
                 name, typ, value, ..
             }| {
                match typ {
                    ScopeEntryType::Normal => scope.push(name, value),
                    ScopeEntryType::Constant => scope.push_constant(name, value),
                };
            },
        );
}

#[inline(always)]
pub fn ensure_no_data_race(
    fn_name: &str,
    args: &FnCallArgs,
    is_ref: bool,
) -> Result<(), Box<EvalAltResult>> {
    if cfg!(not(feature = "no_closure")) {
        let skip = if is_ref { 1 } else { 0 };

        if let Some((n, _)) = args
            .iter()
            .skip(skip)
            .enumerate()
            .find(|(_, a)| a.is_locked())
        {
            return EvalAltResult::ErrorDataRace(
                format!("argument #{} of function '{}'", n + 1 + skip, fn_name),
                Position::none(),
            )
            .into();
        }
    }

    Ok(())
}

impl Engine {
    /// Call a native Rust function registered with the `Engine`.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    ///
    /// ## WARNING
    ///
    /// Function call arguments be _consumed_ when the function requires them to be passed by value.
    /// All function arguments not in the first position are always passed by value and thus consumed.
    /// **DO NOT** reuse the argument values unless for the first `&mut` argument - all others are silently replaced by `()`!
    pub(crate) fn call_native_fn(
        &self,
        state: &mut State,
        lib: &Module,
        fn_name: &str,
        hash_fn: u64,
        args: &mut FnCallArgs,
        is_ref: bool,
        pub_only: bool,
        def_val: Option<bool>,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        self.inc_operations(state)?;

        // Search for the native function
        // First search registered functions (can override packages)
        // Then search packages
        let func = self
            .global_module
            .get_fn(hash_fn, pub_only)
            .or_else(|| self.packages.get_fn(hash_fn, pub_only));

        if let Some(func) = func {
            assert!(func.is_native());

            // Calling pure function but the first argument is a reference?
            let mut backup: ArgBackup = Default::default();
            backup.change_first_arg_to_copy(is_ref && func.is_pure(), args);

            // Run external function
            let result = func.get_native_fn()(self, lib, args);

            // Restore the original reference
            backup.restore_first_arg(args);

            let result = result?;

            // See if the function match print/debug (which requires special processing)
            return Ok(match fn_name {
                KEYWORD_PRINT => (
                    (self.print)(result.as_str().map_err(|typ| {
                        EvalAltResult::ErrorMismatchOutputType(
                            self.map_type_name(type_name::<ImmutableString>()).into(),
                            typ.into(),
                            Position::none(),
                        )
                    })?)
                    .into(),
                    false,
                ),
                KEYWORD_DEBUG => (
                    (self.debug)(result.as_str().map_err(|typ| {
                        EvalAltResult::ErrorMismatchOutputType(
                            self.map_type_name(type_name::<ImmutableString>()).into(),
                            typ.into(),
                            Position::none(),
                        )
                    })?)
                    .into(),
                    false,
                ),
                _ => (result, func.is_method()),
            });
        }

        // See if it is built in.
        if args.len() == 2 {
            match run_builtin_binary_op(fn_name, args[0], args[1])? {
                Some(v) => return Ok((v, false)),
                None => (),
            }
        }

        // Return default value (if any)
        if let Some(val) = def_val {
            return Ok((val.into(), false));
        }

        // Getter function not found?
        if let Some(prop) = extract_prop_from_getter(fn_name) {
            return EvalAltResult::ErrorDotExpr(
                format!(
                    "Unknown property '{}' for {}, or it is write-only",
                    prop,
                    self.map_type_name(args[0].type_name())
                ),
                Position::none(),
            )
            .into();
        }

        // Setter function not found?
        if let Some(prop) = extract_prop_from_setter(fn_name) {
            return EvalAltResult::ErrorDotExpr(
                format!(
                    "Unknown property '{}' for {}, or it is read-only",
                    prop,
                    self.map_type_name(args[0].type_name())
                ),
                Position::none(),
            )
            .into();
        }

        // index getter function not found?
        #[cfg(not(feature = "no_index"))]
        if fn_name == FN_IDX_GET && args.len() == 2 {
            return EvalAltResult::ErrorFunctionNotFound(
                format!(
                    "{} [{}]",
                    self.map_type_name(args[0].type_name()),
                    self.map_type_name(args[1].type_name()),
                ),
                Position::none(),
            )
            .into();
        }

        // index setter function not found?
        #[cfg(not(feature = "no_index"))]
        if fn_name == FN_IDX_SET {
            return EvalAltResult::ErrorFunctionNotFound(
                format!(
                    "{} [{}]=",
                    self.map_type_name(args[0].type_name()),
                    self.map_type_name(args[1].type_name()),
                ),
                Position::none(),
            )
            .into();
        }

        // Raise error
        EvalAltResult::ErrorFunctionNotFound(
            format!(
                "{} ({})",
                fn_name,
                args.iter()
                    .map(|name| if name.is::<ImmutableString>() {
                        "&str | ImmutableString | String"
                    } else {
                        self.map_type_name((*name).type_name())
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Position::none(),
        )
        .into()
    }

    /// Call a script-defined function.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    ///
    /// ## WARNING
    ///
    /// Function call arguments may be _consumed_ when the function requires them to be passed by value.
    /// All function arguments not in the first position are always passed by value and thus consumed.
    /// **DO NOT** reuse the argument values unless for the first `&mut` argument - all others are silently replaced by `()`!
    #[cfg(not(feature = "no_function"))]
    pub(crate) fn call_script_fn(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &Module,
        this_ptr: &mut Option<&mut Dynamic>,
        fn_name: &str,
        fn_def: &ScriptFnDef,
        args: &mut FnCallArgs,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.inc_operations(state)?;

        // Check for stack overflow
        #[cfg(not(feature = "no_function"))]
        #[cfg(not(feature = "unchecked"))]
        if level > self.limits.max_call_stack_depth {
            return Err(Box::new(
                EvalAltResult::ErrorStackOverflow(Position::none()),
            ));
        }

        let orig_scope_level = state.scope_level;
        state.scope_level += 1;

        let prev_scope_len = scope.len();
        let prev_mods_len = mods.len();

        // Put arguments into scope as variables
        // Actually consume the arguments instead of cloning them
        scope.extend(
            fn_def
                .params
                .iter()
                .zip(args.iter_mut().map(|v| mem::take(*v)))
                .map(|(name, value)| {
                    let var_name = unsafe_cast_var_name_to_lifetime(name.as_str(), state);
                    (var_name, ScopeEntryType::Normal, value)
                }),
        );

        // Evaluate the function at one higher level of call depth
        let result = self
            .eval_stmt(scope, mods, state, lib, this_ptr, &fn_def.body, level + 1)
            .or_else(|err| match *err {
                // Convert return statement to return value
                EvalAltResult::Return(x, _) => Ok(x),
                EvalAltResult::ErrorInFunctionCall(name, err, _) => {
                    EvalAltResult::ErrorInFunctionCall(
                        format!("{} > {}", fn_name, name),
                        err,
                        Position::none(),
                    )
                    .into()
                }
                _ => EvalAltResult::ErrorInFunctionCall(fn_name.to_string(), err, Position::none())
                    .into(),
            });

        // Remove all local variables
        scope.rewind(prev_scope_len);
        mods.truncate(prev_mods_len);
        state.scope_level = orig_scope_level;

        result
    }

    // Has a system function an override?
    fn has_override(&self, lib: &Module, hash_fn: u64, hash_script: u64, pub_only: bool) -> bool {
        // NOTE: We skip script functions for global_module and packages, and native functions for lib

        // First check script-defined functions
        lib.contains_fn(hash_script, pub_only)
            //|| lib.contains_fn(hash_fn, pub_only)
            // Then check registered functions
            //|| self.global_module.contains_fn(hash_script, pub_only)
            || self.global_module.contains_fn(hash_fn, pub_only)
            // Then check packages
            //|| self.packages.contains_fn(hash_script, pub_only)
            || self.packages.contains_fn(hash_fn, pub_only)
    }

    /// Perform an actual function call, native Rust or scripted, taking care of special functions.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    ///
    /// ## WARNING
    ///
    /// Function call arguments may be _consumed_ when the function requires them to be passed by value.
    /// All function arguments not in the first position are always passed by value and thus consumed.
    /// **DO NOT** reuse the argument values unless for the first `&mut` argument - all others are silently replaced by `()`!
    pub(crate) fn exec_fn_call(
        &self,
        state: &mut State,
        lib: &Module,
        fn_name: &str,
        hash_script: u64,
        args: &mut FnCallArgs,
        is_ref: bool,
        _is_method: bool,
        pub_only: bool,
        _capture: Option<Scope>,
        def_val: Option<bool>,
        _level: usize,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        // Check for data race.
        if cfg!(not(feature = "no_closure")) {
            ensure_no_data_race(fn_name, args, is_ref)?;
        }

        // Qualifiers (none) + function name + number of arguments + argument `TypeId`'s.
        let arg_types = args.iter().map(|a| a.type_id());
        let hash_fn = calc_fn_hash(empty(), fn_name, args.len(), arg_types);

        match fn_name {
            // type_of
            KEYWORD_TYPE_OF
                if args.len() == 1 && !self.has_override(lib, hash_fn, hash_script, pub_only) =>
            {
                Ok((
                    self.map_type_name(args[0].type_name()).to_string().into(),
                    false,
                ))
            }

            // Fn
            KEYWORD_FN_PTR
                if args.len() == 1 && !self.has_override(lib, hash_fn, hash_script, pub_only) =>
            {
                EvalAltResult::ErrorRuntime(
                    "'Fn' should not be called in method style. Try Fn(...);".into(),
                    Position::none(),
                )
                .into()
            }

            // eval - reaching this point it must be a method-style call
            KEYWORD_EVAL
                if args.len() == 1 && !self.has_override(lib, hash_fn, hash_script, pub_only) =>
            {
                EvalAltResult::ErrorRuntime(
                    "'eval' should not be called in method style. Try eval(...);".into(),
                    Position::none(),
                )
                .into()
            }

            // Normal script function call
            #[cfg(not(feature = "no_function"))]
            _ if hash_script > 0 && lib.contains_fn(hash_script, pub_only) => {
                // Get scripted function
                let func = lib.get_fn(hash_script, pub_only).unwrap().get_fn_def();

                let scope = &mut Scope::new();
                let mods = &mut Imports::new();

                // Add captured variables into scope
                #[cfg(not(feature = "no_closure"))]
                if let Some(captured) = _capture {
                    add_captured_variables_into_scope(&func.externals, captured, scope);
                }

                let result = if _is_method {
                    // Method call of script function - map first argument to `this`
                    let (first, rest) = args.split_at_mut(1);
                    self.call_script_fn(
                        scope,
                        mods,
                        state,
                        lib,
                        &mut Some(first[0]),
                        fn_name,
                        func,
                        rest,
                        _level,
                    )?
                } else {
                    // Normal call of script function - map first argument to `this`
                    // The first argument is a reference?
                    let mut backup: ArgBackup = Default::default();
                    backup.change_first_arg_to_copy(is_ref, args);

                    let result = self.call_script_fn(
                        scope, mods, state, lib, &mut None, fn_name, func, args, _level,
                    );

                    // Restore the original reference
                    backup.restore_first_arg(args);

                    result?
                };

                Ok((result, false))
            }
            // Normal native function call
            _ => self.call_native_fn(
                state, lib, fn_name, hash_fn, args, is_ref, pub_only, def_val,
            ),
        }
    }

    /// Evaluate a text string as a script - used primarily for 'eval'.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    fn eval_script_expr(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &Module,
        script_expr: &Dynamic,
        _level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.inc_operations(state)?;

        // Check for stack overflow
        #[cfg(not(feature = "no_function"))]
        #[cfg(not(feature = "unchecked"))]
        if _level > self.limits.max_call_stack_depth {
            return Err(Box::new(
                EvalAltResult::ErrorStackOverflow(Position::none()),
            ));
        }

        let script = script_expr.as_str().map_err(|typ| {
            EvalAltResult::ErrorMismatchOutputType(
                self.map_type_name(type_name::<ImmutableString>()).into(),
                typ.into(),
                Position::none(),
            )
        })?;

        // Compile the script text
        // No optimizations because we only run it once
        let mut ast = self.compile_with_scope_and_optimization_level(
            &Scope::new(),
            &[script],
            OptimizationLevel::None,
        )?;

        // If new functions are defined within the eval string, it is an error
        if ast.lib().num_fn() != 0 {
            return Err(ParseErrorType::WrongFnDefinition.into());
        }

        let statements = mem::take(ast.statements_mut());
        let ast = AST::new(statements, lib.clone());

        // Evaluate the AST
        let (result, operations) = self.eval_ast_with_scope_raw(scope, mods, &ast)?;

        state.operations += operations;
        self.inc_operations(state)?;

        return Ok(result);
    }

    /// Call a dot method.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    #[cfg(not(feature = "no_object"))]
    pub(crate) fn make_method_call(
        &self,
        state: &mut State,
        lib: &Module,
        name: &str,
        hash_script: u64,
        target: &mut Target,
        idx_val: Dynamic,
        def_val: Option<bool>,
        native: bool,
        pub_only: bool,
        level: usize,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        let is_ref = target.is_ref();
        let is_value = target.is_value();

        // Get a reference to the mutation target Dynamic
        let obj = target.as_mut();
        let mut idx = idx_val.cast::<StaticVec<Dynamic>>();
        let mut _fn_name = name;

        let (result, updated) = if _fn_name == KEYWORD_FN_PTR_CALL && obj.is::<FnPtr>() {
            // FnPtr call
            let fn_ptr = obj.read_lock::<FnPtr>().unwrap();
            let mut curry = fn_ptr.curry().iter().cloned().collect::<StaticVec<_>>();
            // Redirect function name
            let fn_name = fn_ptr.fn_name();
            // Recalculate hash
            let hash = if native {
                0
            } else {
                calc_fn_hash(empty(), fn_name, curry.len() + idx.len(), empty())
            };
            // Arguments are passed as-is, adding the curried arguments
            let mut arg_values = curry
                .iter_mut()
                .chain(idx.iter_mut())
                .collect::<StaticVec<_>>();
            let args = arg_values.as_mut();

            // Map it to name(args) in function-call style
            self.exec_fn_call(
                state, lib, fn_name, hash, args, false, false, pub_only, None, def_val, level,
            )
        } else if _fn_name == KEYWORD_FN_PTR_CALL && idx.len() > 0 && idx[0].is::<FnPtr>() {
            // FnPtr call on object
            let fn_ptr = idx.remove(0).cast::<FnPtr>();
            let mut curry = fn_ptr.curry().iter().cloned().collect::<StaticVec<_>>();
            // Redirect function name
            let fn_name = fn_ptr.get_fn_name().clone();
            // Recalculate hash
            let hash = if native {
                0
            } else {
                calc_fn_hash(empty(), &fn_name, curry.len() + idx.len(), empty())
            };
            // Replace the first argument with the object pointer, adding the curried arguments
            let mut arg_values = once(obj)
                .chain(curry.iter_mut())
                .chain(idx.iter_mut())
                .collect::<StaticVec<_>>();
            let args = arg_values.as_mut();

            // Map it to name(args) in function-call style
            self.exec_fn_call(
                state, lib, &fn_name, hash, args, is_ref, true, pub_only, None, def_val, level,
            )
        } else if _fn_name == KEYWORD_FN_PTR_CURRY && obj.is::<FnPtr>() {
            // Curry call
            let fn_ptr = obj.read_lock::<FnPtr>().unwrap();
            Ok((
                FnPtr::new_unchecked(
                    fn_ptr.get_fn_name().clone(),
                    fn_ptr
                        .curry()
                        .iter()
                        .cloned()
                        .chain(idx.into_iter())
                        .collect(),
                )
                .into(),
                false,
            ))
        } else if cfg!(not(feature = "no_closure"))
            && _fn_name == KEYWORD_IS_SHARED
            && idx.is_empty()
        {
            // is_shared call
            Ok((target.is_shared().into(), false))
        } else {
            #[cfg(not(feature = "no_object"))]
            let redirected;
            let mut hash = hash_script;

            // Check if it is a map method call in OOP style
            #[cfg(not(feature = "no_object"))]
            if let Some(map) = obj.read_lock::<Map>() {
                if let Some(val) = map.get(_fn_name) {
                    if let Some(fn_ptr) = val.read_lock::<FnPtr>() {
                        // Remap the function name
                        redirected = fn_ptr.get_fn_name().clone();
                        _fn_name = &redirected;
                        // Add curried arguments
                        if !fn_ptr.curry().is_empty() {
                            fn_ptr
                                .curry()
                                .iter()
                                .cloned()
                                .enumerate()
                                .for_each(|(i, v)| idx.insert(i, v));
                        }
                        // Recalculate the hash based on the new function name and new arguments
                        hash = if native {
                            0
                        } else {
                            calc_fn_hash(empty(), _fn_name, idx.len(), empty())
                        };
                    }
                }
            };

            if native {
                hash = 0;
            }

            // Attached object pointer in front of the arguments
            let mut arg_values = once(obj).chain(idx.iter_mut()).collect::<StaticVec<_>>();
            let args = arg_values.as_mut();

            self.exec_fn_call(
                state, lib, _fn_name, hash, args, is_ref, true, pub_only, None, def_val, level,
            )
        }?;

        // Feed the changed temp value back
        if updated && !is_ref && !is_value {
            let new_val = target.as_mut().clone();
            target.set_value(new_val)?;
        }

        Ok((result, updated))
    }

    /// Call a function in normal function-call style.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    pub(crate) fn make_function_call(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &Module,
        this_ptr: &mut Option<&mut Dynamic>,
        name: &str,
        args_expr: &[Expr],
        def_val: Option<bool>,
        mut hash_script: u64,
        native: bool,
        pub_only: bool,
        capture: bool,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        // Handle Fn()
        if name == KEYWORD_FN_PTR && args_expr.len() == 1 {
            let hash_fn = calc_fn_hash(empty(), name, 1, once(TypeId::of::<ImmutableString>()));

            if !self.has_override(lib, hash_fn, hash_script, pub_only) {
                // Fn - only in function call style
                let expr = args_expr.get(0).unwrap();
                let arg_value = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;

                return arg_value
                    .take_immutable_string()
                    .map_err(|typ| {
                        EvalAltResult::ErrorMismatchOutputType(
                            self.map_type_name(type_name::<ImmutableString>()).into(),
                            typ.into(),
                            expr.position(),
                        )
                        .into()
                    })
                    .and_then(|s| FnPtr::try_from(s))
                    .map(Into::<Dynamic>::into)
                    .map_err(|err| err.new_position(expr.position()));
            }
        }

        // Handle curry()
        if name == KEYWORD_FN_PTR_CURRY && args_expr.len() > 1 {
            let expr = args_expr.get(0).unwrap();
            let fn_ptr = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;

            if !fn_ptr.is::<FnPtr>() {
                return EvalAltResult::ErrorMismatchOutputType(
                    self.map_type_name(type_name::<FnPtr>()).into(),
                    self.map_type_name(fn_ptr.type_name()).into(),
                    expr.position(),
                )
                .into();
            }

            let (fn_name, fn_curry) = fn_ptr.cast::<FnPtr>().take_data();

            let curry: StaticVec<_> = args_expr
                .iter()
                .skip(1)
                .map(|expr| self.eval_expr(scope, mods, state, lib, this_ptr, expr, level))
                .collect::<Result<_, _>>()?;

            return Ok(FnPtr::new_unchecked(
                fn_name,
                fn_curry.into_iter().chain(curry.into_iter()).collect(),
            )
            .into());
        }

        // Handle is_shared()
        if cfg!(not(feature = "no_closure")) && name == KEYWORD_IS_SHARED && args_expr.len() == 1 {
            let expr = args_expr.get(0).unwrap();
            let value = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;

            return Ok(value.is_shared().into());
        }

        // Handle call() - Redirect function call
        let redirected;
        let mut args_expr = args_expr.as_ref();
        let mut curry: StaticVec<_> = Default::default();
        let mut name = name;

        if name == KEYWORD_FN_PTR_CALL
            && args_expr.len() >= 1
            && !self.has_override(lib, 0, hash_script, pub_only)
        {
            let expr = args_expr.get(0).unwrap();
            let fn_name = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;

            if fn_name.is::<FnPtr>() {
                let fn_ptr = fn_name.cast::<FnPtr>();
                curry = fn_ptr.curry().iter().cloned().collect();
                // Redirect function name
                redirected = fn_ptr.take_data().0;
                name = &redirected;
                // Skip the first argument
                args_expr = &args_expr.as_ref()[1..];
                // Recalculate hash
                hash_script = calc_fn_hash(empty(), name, curry.len() + args_expr.len(), empty());
            } else {
                return EvalAltResult::ErrorMismatchOutputType(
                    self.map_type_name(type_name::<FnPtr>()).into(),
                    fn_name.type_name().into(),
                    expr.position(),
                )
                .into();
            }
        }

        // Handle eval()
        if name == KEYWORD_EVAL && args_expr.len() == 1 {
            let hash_fn = calc_fn_hash(empty(), name, 1, once(TypeId::of::<ImmutableString>()));

            if !self.has_override(lib, hash_fn, hash_script, pub_only) {
                // eval - only in function call style
                let prev_len = scope.len();
                let expr = args_expr.get(0).unwrap();
                let script = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;
                let result = self
                    .eval_script_expr(scope, mods, state, lib, &script, level + 1)
                    .map_err(|err| err.new_position(expr.position()));

                // IMPORTANT! If the eval defines new variables in the current scope,
                //            all variable offsets from this point on will be mis-aligned.
                if scope.len() != prev_len {
                    state.always_search = true;
                }

                return result;
            }
        }

        // Normal function call - except for Fn, curry, call and eval (handled above)
        let mut arg_values: StaticVec<_>;
        let mut args: StaticVec<_>;
        let mut is_ref = false;
        let capture = if cfg!(not(feature = "no_closure")) && capture && !scope.is_empty() {
            Some(scope.flatten_clone())
        } else {
            None
        };

        if args_expr.is_empty() && curry.is_empty() {
            // No arguments
            args = Default::default();
        } else {
            // If the first argument is a variable, and there is no curried arguments, convert to method-call style
            // in order to leverage potential &mut first argument and avoid cloning the value
            match args_expr.get(0).unwrap() {
                // func(x, ...) -> x.func(...)
                lhs @ Expr::Variable(_) if curry.is_empty() => {
                    arg_values = args_expr
                        .iter()
                        .skip(1)
                        .map(|expr| self.eval_expr(scope, mods, state, lib, this_ptr, expr, level))
                        .collect::<Result<_, _>>()?;

                    let (target, _, _, pos) = search_namespace(scope, mods, state, this_ptr, lhs)?;

                    self.inc_operations(state)
                        .map_err(|err| err.new_position(pos))?;

                    args = if target.is_shared() {
                        arg_values.insert(0, target.flatten_clone());
                        arg_values.iter_mut().collect()
                    } else {
                        // Turn it into a method call only if the object is not shared
                        is_ref = true;
                        once(target).chain(arg_values.iter_mut()).collect()
                    };
                }
                // func(..., ...)
                _ => {
                    arg_values = args_expr
                        .iter()
                        .map(|expr| self.eval_expr(scope, mods, state, lib, this_ptr, expr, level))
                        .collect::<Result<_, _>>()?;

                    args = curry.iter_mut().chain(arg_values.iter_mut()).collect();
                }
            }
        }

        let hash = if native { 0 } else { hash_script };
        let args = args.as_mut();

        self.exec_fn_call(
            state, lib, name, hash, args, is_ref, false, pub_only, capture, def_val, level,
        )
        .map(|(v, _)| v)
    }

    /// Call a module-qualified function in normal function-call style.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    pub(crate) fn make_qualified_function_call(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &Module,
        this_ptr: &mut Option<&mut Dynamic>,
        modules: &Option<Box<ModuleRef>>,
        name: &str,
        args_expr: &[Expr],
        def_val: Option<bool>,
        hash_script: u64,
        _capture: bool,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let modules = modules.as_ref().unwrap();
        let mut arg_values: StaticVec<_>;
        let mut args: StaticVec<_>;

        if args_expr.is_empty() {
            // No arguments
            args = Default::default();
        } else {
            // See if the first argument is a variable (not module-qualified).
            // If so, convert to method-call style in order to leverage potential
            // &mut first argument and avoid cloning the value
            match args_expr.get(0).unwrap() {
                // func(x, ...) -> x.func(...)
                Expr::Variable(x) if x.1.is_none() => {
                    arg_values = args_expr
                        .iter()
                        .skip(1)
                        .map(|expr| self.eval_expr(scope, mods, state, lib, this_ptr, expr, level))
                        .collect::<Result<_, _>>()?;

                    let (target, _, _, pos) =
                        search_scope_only(scope, state, this_ptr, args_expr.get(0).unwrap())?;

                    self.inc_operations(state)
                        .map_err(|err| err.new_position(pos))?;

                    args = once(target).chain(arg_values.iter_mut()).collect();
                }
                // func(..., ...) or func(mod::x, ...)
                _ => {
                    arg_values = args_expr
                        .iter()
                        .map(|expr| self.eval_expr(scope, mods, state, lib, this_ptr, expr, level))
                        .collect::<Result<_, _>>()?;

                    args = arg_values.iter_mut().collect();
                }
            }
        }

        let module = search_imports(mods, state, modules)?;

        // First search in script-defined functions (can override built-in)
        let func = match module.get_qualified_fn(hash_script) {
            // Then search in Rust functions
            None => {
                self.inc_operations(state)?;

                // Qualified Rust functions are indexed in two steps:
                // 1) Calculate a hash in a similar manner to script-defined functions,
                //    i.e. qualifiers + function name + number of arguments.
                // 2) Calculate a second hash with no qualifiers, empty function name,
                //    zero number of arguments, and the actual list of argument `TypeId`'.s
                let hash_fn_args = calc_fn_hash(empty(), "", 0, args.iter().map(|a| a.type_id()));
                // 3) The final hash is the XOR of the two hashes.
                let hash_qualified_fn = hash_script ^ hash_fn_args;

                module.get_qualified_fn(hash_qualified_fn)
            }
            r => r,
        };

        match func {
            #[cfg(not(feature = "no_function"))]
            Some(f) if f.is_script() => {
                let args = args.as_mut();
                let func = f.get_fn_def();

                let scope = &mut Scope::new();
                let mods = &mut Imports::new();

                // Add captured variables into scope
                #[cfg(not(feature = "no_closure"))]
                if _capture && !scope.is_empty() {
                    add_captured_variables_into_scope(
                        &func.externals,
                        scope.flatten_clone(),
                        scope,
                    );
                }

                self.call_script_fn(scope, mods, state, lib, &mut None, name, func, args, level)
            }
            Some(f) => f.get_native_fn()(self, lib, args.as_mut()),
            None if def_val.is_some() => Ok(def_val.unwrap().into()),
            None => EvalAltResult::ErrorFunctionNotFound(
                format!(
                    "{}{} ({})",
                    modules,
                    name,
                    args.iter()
                        .map(|a| if a.is::<ImmutableString>() {
                            "&str | ImmutableString | String"
                        } else {
                            self.map_type_name((*a).type_name())
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Position::none(),
            )
            .into(),
        }
    }
}

/// Build in common binary operator implementations to avoid the cost of calling a registered function.
pub fn run_builtin_binary_op(
    op: &str,
    x: &Dynamic,
    y: &Dynamic,
) -> Result<Option<Dynamic>, Box<EvalAltResult>> {
    use crate::packages::arithmetic::*;

    let args_type = x.type_id();

    if y.type_id() != args_type {
        return Ok(None);
    }

    if args_type == TypeId::of::<INT>() {
        let x = x.clone().cast::<INT>();
        let y = y.clone().cast::<INT>();

        if cfg!(not(feature = "unchecked")) {
            match op {
                "+" => return add(x, y).map(Into::into).map(Some),
                "-" => return sub(x, y).map(Into::into).map(Some),
                "*" => return mul(x, y).map(Into::into).map(Some),
                "/" => return div(x, y).map(Into::into).map(Some),
                "%" => return modulo(x, y).map(Into::into).map(Some),
                "~" => return pow_i_i(x, y).map(Into::into).map(Some),
                ">>" => return shr(x, y).map(Into::into).map(Some),
                "<<" => return shl(x, y).map(Into::into).map(Some),
                _ => (),
            }
        } else {
            match op {
                "+" => return Ok(Some((x + y).into())),
                "-" => return Ok(Some((x - y).into())),
                "*" => return Ok(Some((x * y).into())),
                "/" => return Ok(Some((x / y).into())),
                "%" => return Ok(Some((x % y).into())),
                "~" => return pow_i_i_u(x, y).map(Into::into).map(Some),
                ">>" => return shr_u(x, y).map(Into::into).map(Some),
                "<<" => return shl_u(x, y).map(Into::into).map(Some),
                _ => (),
            }
        }

        match op {
            "==" => return Ok(Some((x == y).into())),
            "!=" => return Ok(Some((x != y).into())),
            ">" => return Ok(Some((x > y).into())),
            ">=" => return Ok(Some((x >= y).into())),
            "<" => return Ok(Some((x < y).into())),
            "<=" => return Ok(Some((x <= y).into())),
            "&" => return Ok(Some((x & y).into())),
            "|" => return Ok(Some((x | y).into())),
            "^" => return Ok(Some((x ^ y).into())),
            _ => (),
        }
    } else if args_type == TypeId::of::<bool>() {
        let x = x.clone().cast::<bool>();
        let y = y.clone().cast::<bool>();

        match op {
            "&" => return Ok(Some((x && y).into())),
            "|" => return Ok(Some((x || y).into())),
            "^" => return Ok(Some((x ^ y).into())),
            "==" => return Ok(Some((x == y).into())),
            "!=" => return Ok(Some((x != y).into())),
            _ => (),
        }
    } else if args_type == TypeId::of::<ImmutableString>() {
        let x = &*x.read_lock::<ImmutableString>().unwrap();
        let y = &*y.read_lock::<ImmutableString>().unwrap();

        match op {
            "+" => return Ok(Some((x + y).into())),
            "==" => return Ok(Some((x == y).into())),
            "!=" => return Ok(Some((x != y).into())),
            ">" => return Ok(Some((x > y).into())),
            ">=" => return Ok(Some((x >= y).into())),
            "<" => return Ok(Some((x < y).into())),
            "<=" => return Ok(Some((x <= y).into())),
            _ => (),
        }
    } else if args_type == TypeId::of::<char>() {
        let x = x.clone().cast::<char>();
        let y = y.clone().cast::<char>();

        match op {
            "==" => return Ok(Some((x == y).into())),
            "!=" => return Ok(Some((x != y).into())),
            ">" => return Ok(Some((x > y).into())),
            ">=" => return Ok(Some((x >= y).into())),
            "<" => return Ok(Some((x < y).into())),
            "<=" => return Ok(Some((x <= y).into())),
            _ => (),
        }
    } else if args_type == TypeId::of::<()>() {
        match op {
            "==" => return Ok(Some(true.into())),
            "!=" | ">" | ">=" | "<" | "<=" => return Ok(Some(false.into())),
            _ => (),
        }
    }

    #[cfg(not(feature = "no_float"))]
    if args_type == TypeId::of::<FLOAT>() {
        let x = x.clone().cast::<FLOAT>();
        let y = y.clone().cast::<FLOAT>();

        match op {
            "+" => return Ok(Some((x + y).into())),
            "-" => return Ok(Some((x - y).into())),
            "*" => return Ok(Some((x * y).into())),
            "/" => return Ok(Some((x / y).into())),
            "%" => return Ok(Some((x % y).into())),
            "~" => return pow_f_f(x, y).map(Into::into).map(Some),
            "==" => return Ok(Some((x == y).into())),
            "!=" => return Ok(Some((x != y).into())),
            ">" => return Ok(Some((x > y).into())),
            ">=" => return Ok(Some((x >= y).into())),
            "<" => return Ok(Some((x < y).into())),
            "<=" => return Ok(Some((x <= y).into())),
            _ => (),
        }
    }

    #[cfg(not(feature = "no_decimal"))]
    if args_type == TypeId::of::<Decimal>() {
        let x = x.clone().cast::<Decimal>();
        let y = y.clone().cast::<Decimal>();

        match op {
            "+" => return Ok(Some((x + y).into())),
            "-" => return Ok(Some((x - y).into())),
            "*" => return Ok(Some((x * y).into())),
            "/" => return Ok(Some((x / y).into())),
            "%" => return Ok(Some((x % y).into())),
            "==" => return Ok(Some((x == y).into())),
            "!=" => return Ok(Some((x != y).into())),
            ">" => return Ok(Some((x > y).into())),
            ">=" => return Ok(Some((x >= y).into())),
            "<" => return Ok(Some((x < y).into())),
            "<=" => return Ok(Some((x <= y).into())),
            _ => (),
        }
    }

    Ok(None)
}

/// Build in common operator assignment implementations to avoid the cost of calling a registered function.
pub fn run_builtin_op_assignment(
    op: &str,
    x: &mut Dynamic,
    y: &Dynamic,
) -> Result<Option<()>, Box<EvalAltResult>> {
    use crate::packages::arithmetic::*;

    let args_type = x.type_id();

    if y.type_id() != args_type {
        return Ok(None);
    }

    if args_type == TypeId::of::<INT>() {
        let y = y.clone().cast::<INT>();
        let mut x = x.write_lock::<INT>().unwrap();

        if cfg!(not(feature = "unchecked")) {
            match op {
                "+=" => return Ok(Some(*x = add(*x, y)?)),
                "-=" => return Ok(Some(*x = sub(*x, y)?)),
                "*=" => return Ok(Some(*x = mul(*x, y)?)),
                "/=" => return Ok(Some(*x = div(*x, y)?)),
                "%=" => return Ok(Some(*x = modulo(*x, y)?)),
                "~=" => return Ok(Some(*x = pow_i_i(*x, y)?)),
                ">>=" => return Ok(Some(*x = shr(*x, y)?)),
                "<<=" => return Ok(Some(*x = shl(*x, y)?)),
                _ => (),
            }
        } else {
            match op {
                "+=" => return Ok(Some(*x += y)),
                "-=" => return Ok(Some(*x -= y)),
                "*=" => return Ok(Some(*x *= y)),
                "/=" => return Ok(Some(*x /= y)),
                "%=" => return Ok(Some(*x %= y)),
                "~=" => return Ok(Some(*x = pow_i_i_u(*x, y)?)),
                ">>=" => return Ok(Some(*x = shr_u(*x, y)?)),
                "<<=" => return Ok(Some(*x = shl_u(*x, y)?)),
                _ => (),
            }
        }

        match op {
            "&=" => return Ok(Some(*x &= y)),
            "|=" => return Ok(Some(*x |= y)),
            "^=" => return Ok(Some(*x ^= y)),
            _ => (),
        }
    } else if args_type == TypeId::of::<bool>() {
        let y = y.clone().cast::<bool>();
        let mut x = x.write_lock::<bool>().unwrap();

        match op {
            "&=" => return Ok(Some(*x = *x && y)),
            "|=" => return Ok(Some(*x = *x || y)),
            _ => (),
        }
    } else if args_type == TypeId::of::<ImmutableString>() {
        let y = y.read_lock::<ImmutableString>().unwrap().deref().clone();
        let mut x = x.write_lock::<ImmutableString>().unwrap();

        match op {
            "+=" => return Ok(Some(*x += y)),
            _ => (),
        }
    }

    #[cfg(not(feature = "no_float"))]
    if args_type == TypeId::of::<FLOAT>() {
        let y = y.clone().cast::<FLOAT>();
        let mut x = x.write_lock::<FLOAT>().unwrap();

        match op {
            "+=" => return Ok(Some(*x += y)),
            "-=" => return Ok(Some(*x -= y)),
            "*=" => return Ok(Some(*x *= y)),
            "/=" => return Ok(Some(*x /= y)),
            "%=" => return Ok(Some(*x %= y)),
            "~=" => return Ok(Some(*x = pow_f_f(*x, y)?)),
            _ => (),
        }
    }

    Ok(None)
}
