//! Implement function-calling mechanism for `Engine`.

use crate::ast::{Expr, Stmt};
use crate::dynamic::Dynamic;
use crate::engine::{
    search_imports, Engine, Imports, State, KEYWORD_DEBUG, KEYWORD_EVAL, KEYWORD_FN_PTR,
    KEYWORD_FN_PTR_CALL, KEYWORD_FN_PTR_CURRY, KEYWORD_IS_DEF_FN, KEYWORD_IS_DEF_VAR,
    KEYWORD_PRINT, KEYWORD_TYPE_OF,
};
use crate::fn_native::{FnCallArgs, FnPtr};
use crate::module::{Module, ModuleRef};
use crate::optimize::OptimizationLevel;
use crate::parse_error::ParseErrorType;
use crate::result::EvalAltResult;
use crate::scope::Scope;
use crate::stdlib::ops::Deref;
use crate::token::NO_POS;
use crate::utils::ImmutableString;
use crate::{calc_native_fn_hash, calc_script_fn_hash, StaticVec, INT};

#[cfg(not(feature = "no_function"))]
use crate::{
    ast::ScriptFnDef, r#unsafe::unsafe_cast_var_name_to_lifetime,
    scope::EntryType as ScopeEntryType,
};

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

#[cfg(not(feature = "no_index"))]
use crate::engine::{FN_IDX_GET, FN_IDX_SET};

#[cfg(not(feature = "no_object"))]
use crate::engine::{Map, Target, FN_GET, FN_SET};

#[cfg(not(feature = "no_closure"))]
use crate::engine::KEYWORD_IS_SHARED;

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

#[cfg(not(feature = "no_function"))]
use crate::stdlib::borrow::Cow;

#[cfg(feature = "no_std")]
#[cfg(not(feature = "no_float"))]
use num_traits::float::Float;

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
                NO_POS,
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
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        fn_name: &str,
        hash_fn: u64,
        args: &mut FnCallArgs,
        is_ref: bool,
        pub_only: bool,
        def_val: Option<Dynamic>,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        self.inc_operations(state)?;

        // Search for the native function
        // First search registered functions (can override packages)
        // Then search packages
        let func = //lib.get_fn(hash_fn, pub_only)
            self.global_module.get_fn(hash_fn, pub_only)
                .or_else(|| self.packages.get_fn(hash_fn, pub_only));

        if let Some(func) = func {
            assert!(func.is_native());

            // Calling pure function but the first argument is a reference?
            let mut backup: ArgBackup = Default::default();
            backup.change_first_arg_to_copy(is_ref && func.is_pure(), args);

            // Run external function
            let result = if func.is_plugin_fn() {
                func.get_plugin_fn().call((self, mods, lib).into(), args)
            } else {
                func.get_native_fn()((self, mods, lib).into(), args)
            };

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
                            NO_POS,
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
                            NO_POS,
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
            return Ok((val, false));
        }

        // Getter function not found?
        if let Some(prop) = extract_prop_from_getter(fn_name) {
            return EvalAltResult::ErrorDotExpr(
                format!(
                    "Failed to get property '{}' of '{}' - the property may not exist, or it may be write-only",
                    prop,
                    self.map_type_name(args[0].type_name())
                ),
                NO_POS,
            )
            .into();
        }

        // Setter function not found?
        if let Some(prop) = extract_prop_from_setter(fn_name) {
            return EvalAltResult::ErrorDotExpr(
                format!(
                    "Failed to set property '{}' of '{}' - the property may not exist, may be read-only, or '{}' is the wrong type",
                    prop,
                    self.map_type_name(args[0].type_name()),
                    self.map_type_name(args[1].type_name()),
                ),
                NO_POS,
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
                NO_POS,
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
                NO_POS,
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
            NO_POS,
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
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        fn_def: &ScriptFnDef,
        args: &mut FnCallArgs,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.inc_operations(state)?;

        // Check for stack overflow
        #[cfg(not(feature = "no_function"))]
        #[cfg(not(feature = "unchecked"))]
        if level > self.max_call_levels() {
            return Err(Box::new(EvalAltResult::ErrorStackOverflow(NO_POS)));
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
                    let var_name: Cow<'_, str> = unsafe_cast_var_name_to_lifetime(name).into();
                    (var_name, ScopeEntryType::Normal, value)
                }),
        );

        // Merge in encapsulated environment, if any
        let mut lib_merged: StaticVec<_>;

        let unified_lib = if let Some(ref env_lib) = fn_def.lib {
            lib_merged = Default::default();
            lib_merged.push(env_lib.as_ref());
            lib_merged.extend(lib.iter().cloned());
            lib_merged.as_ref()
        } else {
            lib
        };

        // Evaluate the function at one higher level of call depth
        let stmt = &fn_def.body;

        let result =
            self.eval_stmt(scope, mods, state, unified_lib, this_ptr, stmt, level + 1)
                .or_else(|err| match *err {
                    // Convert return statement to return value
                    EvalAltResult::Return(x, _) => Ok(x),
                    EvalAltResult::ErrorInFunctionCall(name, err, _) => {
                        EvalAltResult::ErrorInFunctionCall(
                            format!("{} > {}", fn_def.name, name),
                            err,
                            NO_POS,
                        )
                        .into()
                    }
                    // System errors are passed straight-through
                    err if err.is_system_exception() => Err(Box::new(err)),
                    // Other errors are wrapped in `ErrorInFunctionCall`
                    _ => EvalAltResult::ErrorInFunctionCall(fn_def.name.to_string(), err, NO_POS)
                        .into(),
                });

        // Remove all local variables
        scope.rewind(prev_scope_len);
        mods.truncate(prev_mods_len);
        state.scope_level = orig_scope_level;

        result
    }

    // Has a system function an override?
    #[inline]
    pub(crate) fn has_override_by_name_and_arguments(
        &self,
        lib: &[&Module],
        name: &str,
        arg_types: impl AsRef<[TypeId]>,
        pub_only: bool,
    ) -> bool {
        let arg_types = arg_types.as_ref();
        let hash_fn = calc_native_fn_hash(empty(), name, arg_types.iter().cloned());
        let hash_script = calc_script_fn_hash(empty(), name, arg_types.len());

        self.has_override(lib, hash_fn, hash_script, pub_only)
    }

    // Has a system function an override?
    #[inline(always)]
    pub(crate) fn has_override(
        &self,
        lib: &[&Module],
        hash_fn: u64,
        hash_script: u64,
        pub_only: bool,
    ) -> bool {
        // NOTE: We skip script functions for global_module and packages, and native functions for lib

        // First check script-defined functions
        lib.iter().any(|&m| m.contains_fn(hash_script, pub_only))
            //|| lib.iter().any(|&m| m.contains_fn(hash_fn, pub_only))
            // Then check registered functions
            //|| self.global_module.contains_fn(hash_script, pub_only)
            || self.global_module.contains_fn(hash_fn, pub_only)
            // Then check packages
            || self.packages.contains_fn(hash_script, pub_only)
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
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        fn_name: &str,
        hash_script: u64,
        args: &mut FnCallArgs,
        is_ref: bool,
        _is_method: bool,
        pub_only: bool,
        _capture_scope: Option<Scope>,
        def_val: Option<Dynamic>,
        _level: usize,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        // Check for data race.
        if cfg!(not(feature = "no_closure")) {
            ensure_no_data_race(fn_name, args, is_ref)?;
        }

        // Qualifiers (none) + function name + number of arguments + argument `TypeId`'s.
        let arg_types = args.iter().map(|a| a.type_id());
        let hash_fn = calc_native_fn_hash(empty(), fn_name, arg_types);

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

            // Fn/eval - reaching this point it must be a method-style call, mostly like redirected
            //           by a function pointer so it isn't caught at parse time.
            KEYWORD_FN_PTR | KEYWORD_EVAL
                if args.len() == 1 && !self.has_override(lib, hash_fn, hash_script, pub_only) =>
            {
                EvalAltResult::ErrorRuntime(
                    format!(
                        "'{}' should not be called in method style. Try {}(...);",
                        fn_name, fn_name
                    )
                    .into(),
                    NO_POS,
                )
                .into()
            }

            // Script-like function found
            #[cfg(not(feature = "no_function"))]
            _ if lib.iter().any(|&m| m.contains_fn(hash_script, pub_only))
                //|| self.global_module.contains_fn(hash_script, pub_only)
                || self.packages.contains_fn(hash_script, pub_only) =>
            {
                // Get function
                let func = lib
                    .iter()
                    .find_map(|&m| m.get_fn(hash_script, pub_only))
                    //.or_else(|| self.global_module.get_fn(hash_script, pub_only))
                    .or_else(|| self.packages.get_fn(hash_script, pub_only))
                    .unwrap();

                if func.is_script() {
                    let func = func.get_fn_def();

                    let scope: &mut Scope = &mut Default::default();

                    // Move captured variables into scope
                    #[cfg(not(feature = "no_closure"))]
                    if let Some(captured) = _capture_scope {
                        if let Some(ref externals) = func.externals {
                            captured
                                .into_iter()
                                .filter(|(name, _, _, _)| externals.contains(name.as_ref()))
                                .for_each(|(name, typ, value, _)| {
                                    // Consume the scope values.
                                    match typ {
                                        ScopeEntryType::Normal => scope.push(name, value),
                                        ScopeEntryType::Constant => {
                                            scope.push_constant(name, value)
                                        }
                                    };
                                });
                        }
                    }

                    let result = if _is_method {
                        // Method call of script function - map first argument to `this`
                        let (first, rest) = args.split_first_mut().unwrap();
                        self.call_script_fn(
                            scope,
                            mods,
                            state,
                            lib,
                            &mut Some(*first),
                            func,
                            rest,
                            _level,
                        )?
                    } else {
                        // Normal call of script function - map first argument to `this`
                        // The first argument is a reference?
                        let mut backup: ArgBackup = Default::default();
                        backup.change_first_arg_to_copy(is_ref, args);

                        let result = self
                            .call_script_fn(scope, mods, state, lib, &mut None, func, args, _level);

                        // Restore the original reference
                        backup.restore_first_arg(args);

                        result?
                    };

                    Ok((result, false))
                } else {
                    // If it is a native function, redirect it
                    self.call_native_fn(
                        mods,
                        state,
                        lib,
                        fn_name,
                        hash_script,
                        args,
                        is_ref,
                        pub_only,
                        def_val,
                    )
                }
            }

            // Normal native function call
            _ => self.call_native_fn(
                mods, state, lib, fn_name, hash_fn, args, is_ref, pub_only, def_val,
            ),
        }
    }

    /// Evaluate a list of statements with an empty state and no `this` pointer.
    /// This is commonly used to evaluate a list of statements in an `AST` or a script function body.
    #[inline]
    pub(crate) fn eval_statements_raw<'a>(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        statements: impl IntoIterator<Item = &'a Stmt>,
        lib: &[&Module],
    ) -> Result<(Dynamic, u64), Box<EvalAltResult>> {
        let mut state = Default::default();

        statements
            .into_iter()
            .try_fold(().into(), |_, stmt| {
                self.eval_stmt(scope, mods, &mut state, lib, &mut None, stmt, 0)
            })
            .or_else(|err| match *err {
                EvalAltResult::Return(out, _) => Ok(out),
                EvalAltResult::LoopBreak(_, _) => unreachable!(),
                _ => Err(err),
            })
            .map(|v| (v, state.operations))
    }

    /// Evaluate a text string as a script - used primarily for 'eval'.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    fn eval_script_expr(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        script: &str,
        _level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.inc_operations(state)?;

        let script = script.trim();
        if script.is_empty() {
            return Ok(().into());
        }

        // Check for stack overflow
        #[cfg(not(feature = "no_function"))]
        #[cfg(not(feature = "unchecked"))]
        if _level > self.max_call_levels() {
            return Err(Box::new(EvalAltResult::ErrorStackOverflow(NO_POS)));
        }

        // Compile the script text
        // No optimizations because we only run it once
        let ast = self.compile_with_scope_and_optimization_level(
            &Default::default(),
            &[script],
            OptimizationLevel::None,
        )?;

        // If new functions are defined within the eval string, it is an error
        if ast.lib().count().0 != 0 {
            return Err(ParseErrorType::WrongFnDefinition.into());
        }

        // Evaluate the AST
        let (result, operations) = self.eval_statements_raw(scope, mods, ast.statements(), lib)?;

        state.operations += operations;
        self.inc_operations(state)?;

        return Ok(result);
    }

    /// Call a dot method.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    #[cfg(not(feature = "no_object"))]
    pub(crate) fn make_method_call(
        &self,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        name: &str,
        hash_script: u64,
        target: &mut Target,
        mut call_args: StaticVec<Dynamic>,
        def_val: Option<Dynamic>,
        native: bool,
        pub_only: bool,
        level: usize,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        let is_ref = target.is_ref();

        // Get a reference to the mutation target Dynamic
        let obj = target.as_mut();
        let mut _fn_name = name;

        let (result, updated) = if _fn_name == KEYWORD_FN_PTR_CALL && obj.is::<FnPtr>() {
            // FnPtr call
            let fn_ptr = obj.read_lock::<FnPtr>().unwrap();
            // Redirect function name
            let fn_name = fn_ptr.fn_name();
            let args_len = call_args.len() + fn_ptr.curry().len();
            // Recalculate hash
            let hash = if native {
                0
            } else {
                calc_script_fn_hash(empty(), fn_name, args_len)
            };
            // Arguments are passed as-is, adding the curried arguments
            let mut curry = fn_ptr.curry().iter().cloned().collect::<StaticVec<_>>();
            let mut arg_values = curry
                .iter_mut()
                .chain(call_args.iter_mut())
                .collect::<StaticVec<_>>();
            let args = arg_values.as_mut();

            // Map it to name(args) in function-call style
            self.exec_fn_call(
                mods, state, lib, fn_name, hash, args, false, false, pub_only, None, def_val, level,
            )
        } else if _fn_name == KEYWORD_FN_PTR_CALL
            && call_args.len() > 0
            && call_args[0].is::<FnPtr>()
        {
            // FnPtr call on object
            let fn_ptr = call_args.remove(0).cast::<FnPtr>();
            // Redirect function name
            let fn_name = fn_ptr.fn_name();
            let args_len = call_args.len() + fn_ptr.curry().len();
            // Recalculate hash
            let hash = if native {
                0
            } else {
                calc_script_fn_hash(empty(), fn_name, args_len)
            };
            // Replace the first argument with the object pointer, adding the curried arguments
            let mut curry = fn_ptr.curry().iter().cloned().collect::<StaticVec<_>>();
            let mut arg_values = once(obj)
                .chain(curry.iter_mut())
                .chain(call_args.iter_mut())
                .collect::<StaticVec<_>>();
            let args = arg_values.as_mut();

            // Map it to name(args) in function-call style
            self.exec_fn_call(
                mods, state, lib, fn_name, hash, args, is_ref, true, pub_only, None, def_val, level,
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
                        .chain(call_args.into_iter())
                        .collect(),
                )
                .into(),
                false,
            ))
        } else if {
            #[cfg(not(feature = "no_closure"))]
            {
                _fn_name == KEYWORD_IS_SHARED && call_args.is_empty()
            }
            #[cfg(feature = "no_closure")]
            false
        } {
            // is_shared call
            Ok((target.is_shared().into(), false))
        } else {
            let _redirected;
            let mut hash = hash_script;

            // Check if it is a map method call in OOP style
            #[cfg(not(feature = "no_object"))]
            if let Some(map) = obj.read_lock::<Map>() {
                if let Some(val) = map.get(_fn_name) {
                    if let Some(fn_ptr) = val.read_lock::<FnPtr>() {
                        // Remap the function name
                        _redirected = fn_ptr.get_fn_name().clone();
                        _fn_name = &_redirected;
                        // Add curried arguments
                        fn_ptr
                            .curry()
                            .iter()
                            .cloned()
                            .enumerate()
                            .for_each(|(i, v)| call_args.insert(i, v));
                        // Recalculate the hash based on the new function name and new arguments
                        hash = if native {
                            0
                        } else {
                            calc_script_fn_hash(empty(), _fn_name, call_args.len())
                        };
                    }
                }
            };

            if native {
                hash = 0;
            }

            // Attached object pointer in front of the arguments
            let mut arg_values = once(obj)
                .chain(call_args.iter_mut())
                .collect::<StaticVec<_>>();
            let args = arg_values.as_mut();

            self.exec_fn_call(
                mods, state, lib, _fn_name, hash, args, is_ref, true, pub_only, None, def_val,
                level,
            )
        }?;

        // Propagate the changed value back to the source if necessary
        if updated {
            target.propagate_changed_value();
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
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        name: &str,
        args_expr: impl AsRef<[Expr]>,
        def_val: Option<Dynamic>,
        mut hash_script: u64,
        native: bool,
        pub_only: bool,
        capture_scope: bool,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let args_expr = args_expr.as_ref();

        // Handle Fn()
        if name == KEYWORD_FN_PTR && args_expr.len() == 1 {
            let hash_fn = calc_native_fn_hash(empty(), name, once(TypeId::of::<ImmutableString>()));

            if !self.has_override(lib, hash_fn, hash_script, pub_only) {
                // Fn - only in function call style
                return self
                    .eval_expr(scope, mods, state, lib, this_ptr, &args_expr[0], level)?
                    .take_immutable_string()
                    .map_err(|typ| {
                        self.make_type_mismatch_err::<ImmutableString>(typ, args_expr[0].position())
                    })
                    .and_then(|s| FnPtr::try_from(s))
                    .map(Into::<Dynamic>::into)
                    .map_err(|err| err.fill_position(args_expr[0].position()));
            }
        }

        // Handle curry()
        if name == KEYWORD_FN_PTR_CURRY && args_expr.len() > 1 {
            let fn_ptr = self.eval_expr(scope, mods, state, lib, this_ptr, &args_expr[0], level)?;

            if !fn_ptr.is::<FnPtr>() {
                return Err(self.make_type_mismatch_err::<FnPtr>(
                    self.map_type_name(fn_ptr.type_name()),
                    args_expr[0].position(),
                ));
            }

            let (fn_name, mut fn_curry) = fn_ptr.cast::<FnPtr>().take_data();

            // Append the new curried arguments to the existing list.

            args_expr
                .iter()
                .skip(1)
                .try_for_each(|expr| -> Result<(), Box<EvalAltResult>> {
                    fn_curry.push(self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?);
                    Ok(())
                })?;

            return Ok(FnPtr::new_unchecked(fn_name, fn_curry).into());
        }

        // Handle is_shared()
        #[cfg(not(feature = "no_closure"))]
        if name == KEYWORD_IS_SHARED && args_expr.len() == 1 {
            let value = self.eval_expr(scope, mods, state, lib, this_ptr, &args_expr[0], level)?;

            return Ok(value.is_shared().into());
        }

        // Handle call() - Redirect function call
        let redirected;
        let mut args_expr = args_expr.as_ref();
        let mut curry = StaticVec::new();
        let mut name = name;

        if name == KEYWORD_FN_PTR_CALL
            && args_expr.len() >= 1
            && !self.has_override(lib, 0, hash_script, pub_only)
        {
            let fn_ptr = self.eval_expr(scope, mods, state, lib, this_ptr, &args_expr[0], level)?;

            if !fn_ptr.is::<FnPtr>() {
                return Err(self.make_type_mismatch_err::<FnPtr>(
                    self.map_type_name(fn_ptr.type_name()),
                    args_expr[0].position(),
                ));
            }

            let fn_ptr = fn_ptr.cast::<FnPtr>();
            curry.extend(fn_ptr.curry().iter().cloned());

            // Redirect function name
            redirected = fn_ptr.take_data().0;
            name = &redirected;

            // Skip the first argument
            args_expr = &args_expr.as_ref()[1..];

            // Recalculate hash
            let args_len = args_expr.len() + curry.len();
            hash_script = calc_script_fn_hash(empty(), name, args_len);
        }

        // Handle is_def_var()
        if name == KEYWORD_IS_DEF_VAR && args_expr.len() == 1 {
            let hash_fn = calc_native_fn_hash(empty(), name, once(TypeId::of::<ImmutableString>()));

            if !self.has_override(lib, hash_fn, hash_script, pub_only) {
                let var_name =
                    self.eval_expr(scope, mods, state, lib, this_ptr, &args_expr[0], level)?;
                let var_name = var_name.as_str().map_err(|err| {
                    self.make_type_mismatch_err::<ImmutableString>(err, args_expr[0].position())
                })?;
                return Ok(scope.contains(var_name).into());
            }
        }

        // Handle is_def_fn()
        if name == KEYWORD_IS_DEF_FN && args_expr.len() == 2 {
            let hash_fn = calc_native_fn_hash(
                empty(),
                name,
                [TypeId::of::<ImmutableString>(), TypeId::of::<INT>()]
                    .iter()
                    .cloned(),
            );

            if !self.has_override(lib, hash_fn, hash_script, pub_only) {
                let fn_name =
                    self.eval_expr(scope, mods, state, lib, this_ptr, &args_expr[0], level)?;
                let num_params =
                    self.eval_expr(scope, mods, state, lib, this_ptr, &args_expr[1], level)?;

                let fn_name = fn_name.as_str().map_err(|err| {
                    self.make_type_mismatch_err::<ImmutableString>(err, args_expr[0].position())
                })?;
                let num_params = num_params.as_int().map_err(|err| {
                    self.make_type_mismatch_err::<INT>(err, args_expr[1].position())
                })?;

                return Ok(if num_params < 0 {
                    false
                } else {
                    let hash = calc_script_fn_hash(empty(), fn_name, num_params as usize);
                    lib.iter().any(|&m| m.contains_fn(hash, false))
                }
                .into());
            }
        }

        // Handle eval()
        if name == KEYWORD_EVAL && args_expr.len() == 1 {
            let hash_fn = calc_native_fn_hash(empty(), name, once(TypeId::of::<ImmutableString>()));

            if !self.has_override(lib, hash_fn, hash_script, pub_only) {
                // eval - only in function call style
                let prev_len = scope.len();
                let script =
                    self.eval_expr(scope, mods, state, lib, this_ptr, &args_expr[0], level)?;
                let script = script.as_str().map_err(|typ| {
                    self.make_type_mismatch_err::<ImmutableString>(typ, args_expr[0].position())
                })?;
                let result = self
                    .eval_script_expr(scope, mods, state, lib, script, level + 1)
                    .map_err(|err| err.fill_position(args_expr[0].position()));

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
        let capture = if capture_scope && !scope.is_empty() {
            Some(scope.clone_visible())
        } else {
            None
        };

        if args_expr.is_empty() && curry.is_empty() {
            // No arguments
            args = Default::default();
        } else {
            // If the first argument is a variable, and there is no curried arguments, convert to method-call style
            // in order to leverage potential &mut first argument and avoid cloning the value
            if curry.is_empty() && args_expr[0].get_variable_access(false).is_some() {
                // func(x, ...) -> x.func(...)
                arg_values = args_expr
                    .iter()
                    .skip(1)
                    .map(|expr| self.eval_expr(scope, mods, state, lib, this_ptr, expr, level))
                    .collect::<Result<_, _>>()?;

                let (target, _, _, pos) =
                    self.search_namespace(scope, mods, state, lib, this_ptr, &args_expr[0])?;

                self.inc_operations(state)
                    .map_err(|err| err.fill_position(pos))?;

                args = if target.is_shared() || target.is_value() {
                    arg_values.insert(0, target.take_or_clone().flatten());
                    arg_values.iter_mut().collect()
                } else {
                    // Turn it into a method call only if the object is not shared and not a simple value
                    is_ref = true;
                    once(target.take_ref().unwrap())
                        .chain(arg_values.iter_mut())
                        .collect()
                };
            } else {
                // func(..., ...)
                arg_values = args_expr
                    .iter()
                    .map(|expr| self.eval_expr(scope, mods, state, lib, this_ptr, expr, level))
                    .collect::<Result<_, _>>()?;

                args = curry.iter_mut().chain(arg_values.iter_mut()).collect();
            }
        }

        let hash = if native { 0 } else { hash_script };
        let args = args.as_mut();

        self.exec_fn_call(
            mods, state, lib, name, hash, args, is_ref, false, pub_only, capture, def_val, level,
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
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        modules: Option<&ModuleRef>,
        name: &str,
        args_expr: impl AsRef<[Expr]>,
        def_val: Option<bool>,
        hash_script: u64,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let args_expr = args_expr.as_ref();

        let modules = modules.as_ref().unwrap();
        let mut arg_values: StaticVec<_>;
        let mut first_arg_value = None;
        let mut args: StaticVec<_>;

        if args_expr.is_empty() {
            // No arguments
            args = Default::default();
        } else {
            // See if the first argument is a variable (not module-qualified).
            // If so, convert to method-call style in order to leverage potential
            // &mut first argument and avoid cloning the value
            if args_expr[0].get_variable_access(true).is_some() {
                // func(x, ...) -> x.func(...)
                arg_values = args_expr
                    .iter()
                    .enumerate()
                    .map(|(i, expr)| {
                        // Skip the first argument
                        if i == 0 {
                            Ok(Default::default())
                        } else {
                            self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)
                        }
                    })
                    .collect::<Result<_, _>>()?;

                // Get target reference to first argument
                let (target, _, _, pos) =
                    self.search_scope_only(scope, mods, state, lib, this_ptr, &args_expr[0])?;

                self.inc_operations(state)
                    .map_err(|err| err.fill_position(pos))?;

                if target.is_shared() || target.is_value() {
                    arg_values[0] = target.take_or_clone().flatten();
                    args = arg_values.iter_mut().collect();
                } else {
                    let (first, rest) = arg_values.split_first_mut().unwrap();
                    first_arg_value = Some(first);
                    args = once(target.take_ref().unwrap())
                        .chain(rest.iter_mut())
                        .collect();
                }
            } else {
                // func(..., ...) or func(mod::x, ...)
                arg_values = args_expr
                    .iter()
                    .map(|expr| self.eval_expr(scope, mods, state, lib, this_ptr, expr, level))
                    .collect::<Result<_, _>>()?;

                args = arg_values.iter_mut().collect();
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
                //    and the actual list of argument `TypeId`'.s
                let hash_fn_args =
                    calc_native_fn_hash(empty(), "", args.iter().map(|a| a.type_id()));
                // 3) The final hash is the XOR of the two hashes.
                let hash_qualified_fn = hash_script ^ hash_fn_args;

                module.get_qualified_fn(hash_qualified_fn)
            }
            r => r,
        };

        match func {
            #[cfg(not(feature = "no_function"))]
            Some(f) if f.is_script() => {
                // Clone first argument
                if let Some(first) = first_arg_value {
                    let first_val = args[0].clone();
                    args[0] = first;
                    *args[0] = first_val;
                }

                let args = args.as_mut();
                let new_scope = &mut Default::default();
                let fn_def = f.get_fn_def().clone();
                self.call_script_fn(new_scope, mods, state, lib, &mut None, &fn_def, args, level)
            }
            Some(f) if f.is_plugin_fn() => f
                .get_plugin_fn()
                .clone()
                .call((self, mods, lib).into(), args.as_mut()),
            Some(f) if f.is_native() => {
                if !f.is_method() {
                    // Clone first argument
                    if let Some(first) = first_arg_value {
                        let first_val = args[0].clone();
                        args[0] = first;
                        *args[0] = first_val;
                    }
                }

                f.get_native_fn().clone()((self, mods, lib).into(), args.as_mut())
            }
            Some(_) => unreachable!(),
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
                NO_POS,
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
    use crate::packages::arithmetic::arith_basic::INT::functions::*;

    let args_type = x.type_id();
    let second_type = y.type_id();

    if second_type != args_type {
        if args_type == TypeId::of::<char>() && second_type == TypeId::of::<ImmutableString>() {
            let x = x.clone().cast::<char>();
            let y = &*y.read_lock::<ImmutableString>().unwrap();

            match op {
                "+" => return Ok(Some(format!("{}{}", x, y).into())),
                _ => (),
            }
        } else if args_type == TypeId::of::<ImmutableString>()
            && second_type == TypeId::of::<char>()
        {
            let x = &*x.read_lock::<ImmutableString>().unwrap();
            let y = y.clone().cast::<char>();

            match op {
                "+" => return Ok(Some((x + y).into())),
                _ => (),
            }
        }
        return Ok(None);
    }

    if args_type == TypeId::of::<INT>() {
        let x = x.clone().cast::<INT>();
        let y = y.clone().cast::<INT>();

        if cfg!(not(feature = "unchecked")) {
            match op {
                "+" => return add(x, y).map(Some),
                "-" => return subtract(x, y).map(Some),
                "*" => return multiply(x, y).map(Some),
                "/" => return divide(x, y).map(Some),
                "%" => return modulo(x, y).map(Some),
                "~" => return power(x, y).map(Some),
                ">>" => return shift_right(x, y).map(Some),
                "<<" => return shift_left(x, y).map(Some),
                _ => (),
            }
        } else {
            match op {
                "+" => return Ok(Some((x + y).into())),
                "-" => return Ok(Some((x - y).into())),
                "*" => return Ok(Some((x * y).into())),
                "/" => return Ok(Some((x / y).into())),
                "%" => return Ok(Some((x % y).into())),
                "~" => return Ok(Some(x.pow(y as u32).into())),
                ">>" => return Ok(Some((x >> y).into())),
                "<<" => return Ok(Some((x << y).into())),
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
            "+" => return Ok(Some(format!("{}{}", x, y).into())),
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
            "~" => return Ok(Some(x.powf(y).into())),
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
    use crate::packages::arithmetic::arith_basic::INT::functions::*;

    let args_type = x.type_id();
    let second_type = y.type_id();

    if second_type != args_type {
        if args_type == TypeId::of::<ImmutableString>() && second_type == TypeId::of::<char>() {
            let y = y.read_lock::<char>().unwrap().deref().clone();
            let mut x = x.write_lock::<ImmutableString>().unwrap();

            match op {
                "+=" => return Ok(Some(*x += y)),
                _ => (),
            }
        }

        return Ok(None);
    }

    if args_type == TypeId::of::<INT>() {
        let y = y.clone().cast::<INT>();
        let mut x = x.write_lock::<INT>().unwrap();

        if cfg!(not(feature = "unchecked")) {
            match op {
                "+=" => return Ok(Some(*x = add(*x, y)?.as_int().unwrap())),
                "-=" => return Ok(Some(*x = subtract(*x, y)?.as_int().unwrap())),
                "*=" => return Ok(Some(*x = multiply(*x, y)?.as_int().unwrap())),
                "/=" => return Ok(Some(*x = divide(*x, y)?.as_int().unwrap())),
                "%=" => return Ok(Some(*x = modulo(*x, y)?.as_int().unwrap())),
                "~=" => return Ok(Some(*x = power(*x, y)?.as_int().unwrap())),
                ">>=" => return Ok(Some(*x = shift_right(*x, y)?.as_int().unwrap())),
                "<<=" => return Ok(Some(*x = shift_left(*x, y)?.as_int().unwrap())),
                _ => (),
            }
        } else {
            match op {
                "+=" => return Ok(Some(*x += y)),
                "-=" => return Ok(Some(*x -= y)),
                "*=" => return Ok(Some(*x *= y)),
                "/=" => return Ok(Some(*x /= y)),
                "%=" => return Ok(Some(*x %= y)),
                "~=" => return Ok(Some(*x = x.pow(y as u32))),
                ">>=" => return Ok(Some(*x = *x >> y)),
                "<<=" => return Ok(Some(*x = *x << y)),
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
    } else if args_type == TypeId::of::<char>() {
        let y = y.read_lock::<char>().unwrap().deref().clone();
        let mut x = x.write_lock::<Dynamic>().unwrap();

        match op {
            "+=" => return Ok(Some(*x = format!("{}{}", *x, y).into())),
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
            "~=" => return Ok(Some(*x = x.powf(y))),
            _ => (),
        }
    }

    Ok(None)
}
