//! Implement function-calling mechanism for [`Engine`].

use crate::ast::{Expr, Stmt};
use crate::engine::{
    search_imports, Imports, State, KEYWORD_DEBUG, KEYWORD_EVAL, KEYWORD_FN_PTR,
    KEYWORD_FN_PTR_CALL, KEYWORD_FN_PTR_CURRY, KEYWORD_IS_DEF_VAR, KEYWORD_PRINT, KEYWORD_TYPE_OF,
};
use crate::fn_native::FnCallArgs;
use crate::module::NamespaceRef;
use crate::optimize::OptimizationLevel;
use crate::stdlib::{
    any::{type_name, TypeId},
    boxed::Box,
    convert::TryFrom,
    format,
    iter::{empty, once},
    mem,
    num::NonZeroU64,
    ops::Deref,
    string::ToString,
    vec::Vec,
};
use crate::utils::combine_hashes;
use crate::{
    calc_native_fn_hash, calc_script_fn_hash, Dynamic, Engine, EvalAltResult, FnPtr,
    ImmutableString, Module, ParseErrorType, Position, Scope, StaticVec, INT,
};

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

#[cfg(not(feature = "no_object"))]
use crate::Map;

#[cfg(feature = "no_std")]
#[cfg(not(feature = "no_float"))]
use num_traits::float::Float;

/// Extract the property name from a getter function name.
#[cfg(not(feature = "no_object"))]
#[inline(always)]
fn extract_prop_from_getter(_fn_name: &str) -> Option<&str> {
    if _fn_name.starts_with(crate::engine::FN_GET) {
        Some(&_fn_name[crate::engine::FN_GET.len()..])
    } else {
        None
    }
}

/// Extract the property name from a setter function name.
#[cfg(not(feature = "no_object"))]
#[inline(always)]
fn extract_prop_from_setter(_fn_name: &str) -> Option<&str> {
    if _fn_name.starts_with(crate::engine::FN_SET) {
        Some(&_fn_name[crate::engine::FN_SET.len()..])
    } else {
        None
    }
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
    #[inline(always)]
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
    #[inline(always)]
    fn restore_first_arg(&mut self, args: &mut FnCallArgs<'a>) {
        if let Some(this_pointer) = self.orig_mut.take() {
            args[0] = this_pointer;
        }
    }
}

impl Drop for ArgBackup<'_> {
    #[inline(always)]
    fn drop(&mut self) {
        // Panic if the shorter lifetime leaks.
        assert!(
            self.orig_mut.is_none(),
            "ArgBackup::restore_first_arg has not been called prior to existing this scope"
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
                Position::NONE,
            )
            .into();
        }
    }

    Ok(())
}

impl Engine {
    /// Call a native Rust function registered with the [`Engine`].
    ///
    /// # WARNING
    ///
    /// Function call arguments be _consumed_ when the function requires them to be passed by value.
    /// All function arguments not in the first position are always passed by value and thus consumed.
    /// **DO NOT** reuse the argument values unless for the first `&mut` argument - all others are silently replaced by `()`!
    pub(crate) fn call_native_fn(
        &self,
        mods: &Imports,
        state: &mut State,
        lib: &[&Module],
        fn_name: &str,
        hash_fn: NonZeroU64,
        args: &mut FnCallArgs,
        is_ref: bool,
        pub_only: bool,
        pos: Position,
        def_val: Option<&Dynamic>,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        self.inc_operations(state, pos)?;

        let source = state.source.clone();

        // Check if function access already in the cache
        let func = &*state
            .fn_resolution_cache_mut()
            .entry(hash_fn)
            .or_insert_with(|| {
                // Search for the native function
                // First search registered functions (can override packages)
                // Then search packages
                // Finally search modules

                //lib.get_fn(hash_fn, pub_only)
                self.global_namespace
                    .get_fn(hash_fn, pub_only)
                    .cloned()
                    .map(|f| (f, None))
                    .or_else(|| {
                        self.global_modules.iter().find_map(|m| {
                            m.get_fn(hash_fn, false)
                                .map(|f| (f.clone(), m.id_raw().cloned()))
                        })
                    })
                    .or_else(|| {
                        mods.get_fn(hash_fn)
                            .map(|(f, source)| (f.clone(), source.cloned()))
                    })
            });

        if let Some((func, src)) = func {
            assert!(func.is_native());

            // Calling pure function but the first argument is a reference?
            let mut backup: ArgBackup = Default::default();
            backup.change_first_arg_to_copy(is_ref && func.is_pure(), args);

            // Run external function
            let source = src.as_ref().or_else(|| source.as_ref()).map(|s| s.as_str());
            let result = if func.is_plugin_fn() {
                func.get_plugin_fn()
                    .call((self, fn_name, source, mods, lib).into(), args)
            } else {
                func.get_native_fn()((self, fn_name, source, mods, lib).into(), args)
            };

            // Restore the original reference
            backup.restore_first_arg(args);

            let result = result?;

            // See if the function match print/debug (which requires special processing)
            return Ok(match fn_name {
                KEYWORD_PRINT => {
                    let text = result.as_str().map_err(|typ| {
                        EvalAltResult::ErrorMismatchOutputType(
                            self.map_type_name(type_name::<ImmutableString>()).into(),
                            typ.into(),
                            pos,
                        )
                    })?;
                    ((self.print)(text).into(), false)
                }
                KEYWORD_DEBUG => {
                    let text = result.as_str().map_err(|typ| {
                        EvalAltResult::ErrorMismatchOutputType(
                            self.map_type_name(type_name::<ImmutableString>()).into(),
                            typ.into(),
                            pos,
                        )
                    })?;
                    let source = state.source.as_ref().map(|s| s.as_str());
                    ((self.debug)(text, source, pos).into(), false)
                }
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
            return Ok((val.clone(), false));
        }

        // Getter function not found?
        #[cfg(not(feature = "no_object"))]
        if let Some(prop) = extract_prop_from_getter(fn_name) {
            return EvalAltResult::ErrorDotExpr(
                format!(
                    "Unknown property '{}' - a getter is not registered for type '{}'",
                    prop,
                    self.map_type_name(args[0].type_name())
                ),
                pos,
            )
            .into();
        }

        // Setter function not found?
        #[cfg(not(feature = "no_object"))]
        if let Some(prop) = extract_prop_from_setter(fn_name) {
            return EvalAltResult::ErrorDotExpr(
                format!(
                    "No writable property '{}' - a setter is not registered for type '{}' to handle '{}'",
                    prop,
                    self.map_type_name(args[0].type_name()),
                    self.map_type_name(args[1].type_name()),
                ),
                pos,
            )
            .into();
        }

        // index getter function not found?
        #[cfg(not(feature = "no_index"))]
        if fn_name == crate::engine::FN_IDX_GET && args.len() == 2 {
            return EvalAltResult::ErrorFunctionNotFound(
                format!(
                    "{} [{}]",
                    self.map_type_name(args[0].type_name()),
                    self.map_type_name(args[1].type_name()),
                ),
                pos,
            )
            .into();
        }

        // index setter function not found?
        #[cfg(not(feature = "no_index"))]
        if fn_name == crate::engine::FN_IDX_SET {
            return EvalAltResult::ErrorFunctionNotFound(
                format!(
                    "{} [{}]=",
                    self.map_type_name(args[0].type_name()),
                    self.map_type_name(args[1].type_name()),
                ),
                pos,
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
            pos,
        )
        .into()
    }

    /// Call a script-defined function.
    ///
    /// # WARNING
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
        fn_def: &crate::ast::ScriptFnDef,
        args: &mut FnCallArgs,
        pos: Position,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.inc_operations(state, pos)?;

        // Check for stack overflow
        #[cfg(not(feature = "no_function"))]
        #[cfg(not(feature = "unchecked"))]
        if level > self.max_call_levels() {
            return Err(Box::new(EvalAltResult::ErrorStackOverflow(pos)));
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
                    let var_name: crate::stdlib::borrow::Cow<'_, str> =
                        crate::r#unsafe::unsafe_cast_var_name_to_lifetime(name).into();
                    (var_name, value)
                }),
        );

        // Merge in encapsulated environment, if any
        let mut lib_merged: StaticVec<_>;
        let mut unified = false;

        let unified_lib = if let Some(ref env_lib) = fn_def.lib {
            unified = true;
            state.push_fn_resolution_cache();
            lib_merged = Default::default();
            lib_merged.push(env_lib.as_ref());
            lib_merged.extend(lib.iter().cloned());
            lib_merged.as_ref()
        } else {
            lib
        };

        #[cfg(not(feature = "no_module"))]
        if !fn_def.mods.is_empty() {
            mods.extend(fn_def.mods.iter_raw().map(|(n, m)| (n.clone(), m.clone())));
        }

        // Evaluate the function
        let stmt = &fn_def.body;

        let result = self
            .eval_stmt(scope, mods, state, unified_lib, this_ptr, stmt, level)
            .or_else(|err| match *err {
                // Convert return statement to return value
                EvalAltResult::Return(x, _) => Ok(x),
                EvalAltResult::ErrorInFunctionCall(name, src, err, _) => {
                    EvalAltResult::ErrorInFunctionCall(
                        format!(
                            "{}{} < {}",
                            name,
                            if src.is_empty() {
                                "".to_string()
                            } else {
                                format!(" @ '{}'", src)
                            },
                            fn_def.name
                        ),
                        fn_def
                            .lib
                            .as_ref()
                            .map(|m| m.id())
                            .flatten()
                            .or_else(|| state.source.as_ref().map(|s| s.as_str()))
                            .unwrap_or("")
                            .to_string(),
                        err,
                        pos,
                    )
                    .into()
                }
                // System errors are passed straight-through
                err if err.is_system_exception() => Err(Box::new(err)),
                // Other errors are wrapped in `ErrorInFunctionCall`
                _ => EvalAltResult::ErrorInFunctionCall(
                    fn_def.name.to_string(),
                    fn_def
                        .lib
                        .as_ref()
                        .map(|m| m.id())
                        .flatten()
                        .or_else(|| state.source.as_ref().map(|s| s.as_str()))
                        .unwrap_or("")
                        .to_string(),
                    err,
                    pos,
                )
                .into(),
            });

        // Remove all local variables
        scope.rewind(prev_scope_len);
        mods.truncate(prev_mods_len);
        state.scope_level = orig_scope_level;

        if unified {
            state.pop_fn_resolution_cache();
        }

        result
    }

    // Has a system function an override?
    #[inline(always)]
    pub(crate) fn has_override_by_name_and_arguments(
        &self,
        mods: Option<&Imports>,
        state: Option<&mut State>,
        lib: &[&Module],
        fn_name: &str,
        arg_types: impl AsRef<[TypeId]>,
        pub_only: bool,
    ) -> bool {
        let arg_types = arg_types.as_ref();
        let hash_fn = calc_native_fn_hash(empty(), fn_name, arg_types.iter().cloned());
        let hash_script = calc_script_fn_hash(empty(), fn_name, arg_types.len());

        self.has_override(mods, state, lib, hash_fn, hash_script, pub_only)
    }

    // Has a system function an override?
    #[inline(always)]
    pub(crate) fn has_override(
        &self,
        mods: Option<&Imports>,
        mut state: Option<&mut State>,
        lib: &[&Module],
        hash_fn: Option<NonZeroU64>,
        hash_script: Option<NonZeroU64>,
        pub_only: bool,
    ) -> bool {
        // Check if it is already in the cache
        if let Some(state) = state.as_mut() {
            if let Some(hash) = hash_script {
                match state.fn_resolution_cache().map_or(None, |c| c.get(&hash)) {
                    Some(v) => return v.is_some(),
                    None => (),
                }
            }
            if let Some(hash) = hash_fn {
                match state.fn_resolution_cache().map_or(None, |c| c.get(&hash)) {
                    Some(v) => return v.is_some(),
                    None => (),
                }
            }
        }

        // First check script-defined functions
        let r = hash_script.map_or(false, |hash| lib.iter().any(|&m| m.contains_fn(hash, pub_only)))
            //|| hash_fn.map_or(false, |hash| lib.iter().any(|&m| m.contains_fn(hash, pub_only)))
            // Then check registered functions
            //|| hash_script.map_or(false, |hash| self.global_namespace.contains_fn(hash, pub_only))
            || hash_fn.map_or(false, |hash| self.global_namespace.contains_fn(hash, false))
            // Then check packages
            || hash_script.map_or(false, |hash| self.global_modules.iter().any(|m| m.contains_fn(hash, false)))
            || hash_fn.map_or(false, |hash| self.global_modules.iter().any(|m| m.contains_fn(hash, false)))
            // Then check imported modules
            || hash_script.map_or(false, |hash| mods.map_or(false, |m| m.contains_fn(hash)))
            || hash_fn.map_or(false, |hash| mods.map_or(false, |m| m.contains_fn(hash)));

        // If there is no override, put that information into the cache
        if !r {
            if let Some(state) = state.as_mut() {
                if let Some(hash) = hash_script {
                    state.fn_resolution_cache_mut().insert(hash, None);
                }
                if let Some(hash) = hash_fn {
                    state.fn_resolution_cache_mut().insert(hash, None);
                }
            }
        }

        r
    }

    /// Perform an actual function call, native Rust or scripted, taking care of special functions.
    ///
    /// # WARNING
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
        hash_script: Option<NonZeroU64>,
        args: &mut FnCallArgs,
        is_ref: bool,
        _is_method: bool,
        pub_only: bool,
        pos: Position,
        _capture_scope: Option<Scope>,
        def_val: Option<&Dynamic>,
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
                if args.len() == 1
                    && !self.has_override(
                        Some(mods),
                        Some(state),
                        lib,
                        hash_fn,
                        hash_script,
                        pub_only,
                    ) =>
            {
                Ok((
                    self.map_type_name(args[0].type_name()).to_string().into(),
                    false,
                ))
            }

            // Fn/eval - reaching this point it must be a method-style call, mostly like redirected
            //           by a function pointer so it isn't caught at parse time.
            KEYWORD_FN_PTR | KEYWORD_EVAL
                if args.len() == 1
                    && !self.has_override(
                        Some(mods),
                        Some(state),
                        lib,
                        hash_fn,
                        hash_script,
                        pub_only,
                    ) =>
            {
                EvalAltResult::ErrorRuntime(
                    format!(
                        "'{}' should not be called in method style. Try {}(...);",
                        fn_name, fn_name
                    )
                    .into(),
                    pos,
                )
                .into()
            }

            // Script-like function found
            #[cfg(not(feature = "no_function"))]
            _ if hash_script.is_some()
                && self.has_override(Some(mods), Some(state), lib, None, hash_script, pub_only) =>
            {
                let hash_script = hash_script.unwrap();

                // Check if function access already in the cache
                let (func, source) = state
                    .fn_resolution_cache_mut()
                    .entry(hash_script)
                    .or_insert_with(|| {
                        lib.iter()
                            .find_map(|&m| {
                                m.get_fn(hash_script, pub_only)
                                    .map(|f| (f.clone(), m.id_raw().cloned()))
                            })
                            //.or_else(|| self.global_namespace.get_fn(hash_script, pub_only))
                            .or_else(|| {
                                self.global_modules.iter().find_map(|m| {
                                    m.get_fn(hash_script, false)
                                        .map(|f| (f.clone(), m.id_raw().cloned()))
                                })
                            })
                        //.or_else(|| mods.iter().find_map(|(_, m)| m.get_qualified_fn(hash_script).map(|f| (f, m.id_raw().clone()))))
                    })
                    .as_ref()
                    .map(|(f, s)| (f.clone(), s.clone()))
                    .unwrap();

                assert!(func.is_script());

                let func = func.get_fn_def();

                let scope: &mut Scope = &mut Default::default();

                // Move captured variables into scope
                #[cfg(not(feature = "no_closure"))]
                if let Some(captured) = _capture_scope {
                    if !func.externals.is_empty() {
                        captured
                            .into_iter()
                            .filter(|(name, _, _)| func.externals.iter().any(|ex| ex == name))
                            .for_each(|(name, value, _)| {
                                // Consume the scope values.
                                scope.push_dynamic(name, value);
                            });
                    }
                }

                let result = if _is_method {
                    // Method call of script function - map first argument to `this`
                    let (first, rest) = args.split_first_mut().unwrap();

                    let orig_source = mem::take(&mut state.source);
                    state.source = source;

                    let level = _level + 1;

                    let result = self.call_script_fn(
                        scope,
                        mods,
                        state,
                        lib,
                        &mut Some(*first),
                        func,
                        rest,
                        pos,
                        level,
                    );

                    // Restore the original source
                    state.source = orig_source;

                    result?
                } else {
                    // Normal call of script function
                    // The first argument is a reference?
                    let mut backup: ArgBackup = Default::default();
                    backup.change_first_arg_to_copy(is_ref, args);

                    let orig_source = mem::take(&mut state.source);
                    state.source = source;

                    let level = _level + 1;

                    let result = self
                        .call_script_fn(scope, mods, state, lib, &mut None, func, args, pos, level);

                    // Restore the original source
                    state.source = orig_source;

                    // Restore the original reference
                    backup.restore_first_arg(args);

                    result?
                };

                Ok((result, false))
            }

            // Normal native function call
            _ => self.call_native_fn(
                mods,
                state,
                lib,
                fn_name,
                hash_fn.unwrap(),
                args,
                is_ref,
                pub_only,
                pos,
                def_val,
            ),
        }
    }

    /// Evaluate a list of statements with no `this` pointer.
    /// This is commonly used to evaluate a list of statements in an [`AST`] or a script function body.
    #[inline]
    pub(crate) fn eval_global_statements<'a>(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        statements: impl IntoIterator<Item = &'a Stmt>,
        lib: &[&Module],
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.eval_stmt_block(scope, mods, state, lib, &mut None, statements, false, level)
            .or_else(|err| match *err {
                EvalAltResult::Return(out, _) => Ok(out),
                EvalAltResult::LoopBreak(_, _) => {
                    unreachable!("no outer loop scope to break out of")
                }
                _ => Err(err),
            })
    }

    /// Evaluate a text script in place - used primarily for 'eval'.
    fn eval_script_expr_in_place(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        script: &str,
        pos: Position,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.inc_operations(state, pos)?;

        let script = script.trim();
        if script.is_empty() {
            return Ok(Dynamic::UNIT);
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
            return Err(ParseErrorType::FnWrongDefinition.into());
        }

        // Evaluate the AST
        let mut new_state: State = Default::default();
        new_state.source = state.source.clone();
        new_state.operations = state.operations;

        let result =
            self.eval_global_statements(scope, mods, &mut new_state, ast.statements(), lib, level);

        state.operations = new_state.operations;
        result
    }

    /// Call a dot method.
    #[cfg(not(feature = "no_object"))]
    pub(crate) fn make_method_call(
        &self,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        fn_name: &str,
        hash_script: Option<NonZeroU64>,
        target: &mut crate::engine::Target,
        mut call_args: StaticVec<Dynamic>,
        def_val: Option<&Dynamic>,
        pub_only: bool,
        pos: Position,
        level: usize,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        let is_ref = target.is_ref();

        // Get a reference to the mutation target Dynamic
        let obj = target.as_mut();
        let mut fn_name = fn_name;

        let (result, updated) = if fn_name == KEYWORD_FN_PTR_CALL && obj.is::<FnPtr>() {
            // FnPtr call
            let fn_ptr = obj.read_lock::<FnPtr>().unwrap();
            // Redirect function name
            let fn_name = fn_ptr.fn_name();
            let args_len = call_args.len() + fn_ptr.curry().len();
            // Recalculate hash
            let hash = hash_script.and_then(|_| calc_script_fn_hash(empty(), fn_name, args_len));
            // Arguments are passed as-is, adding the curried arguments
            let mut curry = fn_ptr.curry().iter().cloned().collect::<StaticVec<_>>();
            let mut arg_values = curry
                .iter_mut()
                .chain(call_args.iter_mut())
                .collect::<StaticVec<_>>();
            let args = arg_values.as_mut();

            // Map it to name(args) in function-call style
            self.exec_fn_call(
                mods, state, lib, fn_name, hash, args, false, false, pub_only, pos, None, def_val,
                level,
            )
        } else if fn_name == KEYWORD_FN_PTR_CALL
            && call_args.len() > 0
            && call_args[0].is::<FnPtr>()
        {
            // FnPtr call on object
            let fn_ptr = call_args.remove(0).cast::<FnPtr>();
            // Redirect function name
            let fn_name = fn_ptr.fn_name();
            let args_len = call_args.len() + fn_ptr.curry().len();
            // Recalculate hash
            let hash = hash_script.and_then(|_| calc_script_fn_hash(empty(), fn_name, args_len));
            // Replace the first argument with the object pointer, adding the curried arguments
            let mut curry = fn_ptr.curry().iter().cloned().collect::<StaticVec<_>>();
            let mut arg_values = once(obj)
                .chain(curry.iter_mut())
                .chain(call_args.iter_mut())
                .collect::<StaticVec<_>>();
            let args = arg_values.as_mut();

            // Map it to name(args) in function-call style
            self.exec_fn_call(
                mods, state, lib, fn_name, hash, args, is_ref, true, pub_only, pos, None, def_val,
                level,
            )
        } else if fn_name == KEYWORD_FN_PTR_CURRY && obj.is::<FnPtr>() {
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
                fn_name == crate::engine::KEYWORD_IS_SHARED && call_args.is_empty()
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
                if let Some(val) = map.get(fn_name) {
                    if let Some(fn_ptr) = val.read_lock::<FnPtr>() {
                        // Remap the function name
                        _redirected = fn_ptr.get_fn_name().clone();
                        fn_name = &_redirected;
                        // Add curried arguments
                        fn_ptr
                            .curry()
                            .iter()
                            .cloned()
                            .enumerate()
                            .for_each(|(i, v)| call_args.insert(i, v));
                        // Recalculate the hash based on the new function name and new arguments
                        hash = hash_script
                            .and_then(|_| calc_script_fn_hash(empty(), fn_name, call_args.len()));
                    }
                }
            };

            if hash_script.is_none() {
                hash = None;
            }

            // Attached object pointer in front of the arguments
            let mut arg_values = once(obj)
                .chain(call_args.iter_mut())
                .collect::<StaticVec<_>>();
            let args = arg_values.as_mut();

            self.exec_fn_call(
                mods, state, lib, fn_name, hash, args, is_ref, true, pub_only, pos, None, def_val,
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
    pub(crate) fn make_function_call(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        fn_name: &str,
        args_expr: impl AsRef<[Expr]>,
        def_val: Option<&Dynamic>,
        mut hash_script: Option<NonZeroU64>,
        pub_only: bool,
        pos: Position,
        capture_scope: bool,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let args_expr = args_expr.as_ref();

        // Handle call() - Redirect function call
        let redirected;
        let mut args_expr = args_expr.as_ref();
        let mut curry = StaticVec::new();
        let mut name = fn_name;

        if name == KEYWORD_FN_PTR_CALL
            && args_expr.len() >= 1
            && !self.has_override(Some(mods), Some(state), lib, None, hash_script, pub_only)
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

        // Handle Fn()
        if name == KEYWORD_FN_PTR && args_expr.len() == 1 {
            let hash_fn = calc_native_fn_hash(empty(), name, once(TypeId::of::<ImmutableString>()));

            if !self.has_override(Some(mods), Some(state), lib, hash_fn, hash_script, pub_only) {
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

            let (name, mut fn_curry) = fn_ptr.cast::<FnPtr>().take_data();

            // Append the new curried arguments to the existing list.

            args_expr
                .iter()
                .skip(1)
                .try_for_each(|expr| -> Result<(), Box<EvalAltResult>> {
                    fn_curry.push(self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?);
                    Ok(())
                })?;

            return Ok(FnPtr::new_unchecked(name, fn_curry).into());
        }

        // Handle is_shared()
        #[cfg(not(feature = "no_closure"))]
        if name == crate::engine::KEYWORD_IS_SHARED && args_expr.len() == 1 {
            let value = self.eval_expr(scope, mods, state, lib, this_ptr, &args_expr[0], level)?;

            return Ok(value.is_shared().into());
        }

        // Handle is_def_var()
        if name == KEYWORD_IS_DEF_VAR && args_expr.len() == 1 {
            let hash_fn = calc_native_fn_hash(empty(), name, once(TypeId::of::<ImmutableString>()));

            if !self.has_override(Some(mods), Some(state), lib, hash_fn, hash_script, pub_only) {
                let var_name =
                    self.eval_expr(scope, mods, state, lib, this_ptr, &args_expr[0], level)?;
                let var_name = var_name.as_str().map_err(|err| {
                    self.make_type_mismatch_err::<ImmutableString>(err, args_expr[0].position())
                })?;
                return Ok(scope.contains(var_name).into());
            }
        }

        // Handle eval()
        if name == KEYWORD_EVAL && args_expr.len() == 1 {
            let hash_fn = calc_native_fn_hash(empty(), name, once(TypeId::of::<ImmutableString>()));

            if !self.has_override(Some(mods), Some(state), lib, hash_fn, hash_script, pub_only) {
                // eval - only in function call style
                let prev_len = scope.len();
                let script =
                    self.eval_expr(scope, mods, state, lib, this_ptr, &args_expr[0], level)?;
                let script = script.as_str().map_err(|typ| {
                    self.make_type_mismatch_err::<ImmutableString>(typ, args_expr[0].position())
                })?;
                let pos = args_expr[0].position();
                let result =
                    self.eval_script_expr_in_place(scope, mods, state, lib, script, pos, level + 1);

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
            // If the first argument is a variable, and there is no curried arguments,
            // convert to method-call style in order to leverage potential &mut first argument and
            // avoid cloning the value
            if curry.is_empty() && args_expr[0].get_variable_access(false).is_some() {
                // func(x, ...) -> x.func(...)
                arg_values = args_expr
                    .iter()
                    .skip(1)
                    .map(|expr| self.eval_expr(scope, mods, state, lib, this_ptr, expr, level))
                    .collect::<Result<_, _>>()?;

                let (mut target, pos) =
                    self.search_namespace(scope, mods, state, lib, this_ptr, &args_expr[0])?;

                if target.as_ref().is_read_only() {
                    target = target.into_owned();
                }

                self.inc_operations(state, pos)?;

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

        let args = args.as_mut();

        self.exec_fn_call(
            mods,
            state,
            lib,
            name,
            hash_script,
            args,
            is_ref,
            false,
            pub_only,
            pos,
            capture,
            def_val,
            level,
        )
        .map(|(v, _)| v)
    }

    /// Call a namespace-qualified function in normal function-call style.
    pub(crate) fn make_qualified_function_call(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        namespace: Option<&NamespaceRef>,
        fn_name: &str,
        args_expr: impl AsRef<[Expr]>,
        def_val: Option<&Dynamic>,
        hash_script: NonZeroU64,
        pos: Position,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let args_expr = args_expr.as_ref();

        let namespace = namespace.as_ref().unwrap();
        let mut arg_values: StaticVec<_>;
        let mut first_arg_value = None;
        let mut args: StaticVec<_>;

        if args_expr.is_empty() {
            // No arguments
            args = Default::default();
        } else {
            // See if the first argument is a variable (not namespace-qualified).
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
                let (target, pos) =
                    self.search_scope_only(scope, mods, state, lib, this_ptr, &args_expr[0])?;

                self.inc_operations(state, pos)?;

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

        let module = search_imports(mods, state, namespace)?;

        // First search in script-defined functions (can override built-in)
        let func = match module.get_qualified_fn(hash_script) {
            // Then search in Rust functions
            None => {
                self.inc_operations(state, pos)?;

                // Namespace-qualified Rust functions are indexed in two steps:
                // 1) Calculate a hash in a similar manner to script-defined functions,
                //    i.e. qualifiers + function name + number of arguments.
                // 2) Calculate a second hash with no qualifiers, empty function name,
                //    and the actual list of argument `TypeId`'.s
                let hash_fn_args =
                    calc_native_fn_hash(empty(), "", args.iter().map(|a| a.type_id())).unwrap();
                // 3) The two hashes are combined.
                let hash_qualified_fn = combine_hashes(hash_script, hash_fn_args);

                module.get_qualified_fn(hash_qualified_fn)
            }
            r => r,
        };

        // Clone first argument if the function is not a method after-all
        if let Some(first) = first_arg_value {
            if !func.map(|f| f.is_method()).unwrap_or(true) {
                let first_val = args[0].clone();
                args[0] = first;
                *args[0] = first_val;
            }
        }

        match func {
            #[cfg(not(feature = "no_function"))]
            Some(f) if f.is_script() => {
                let args = args.as_mut();
                let new_scope = &mut Default::default();
                let fn_def = f.get_fn_def().clone();

                let mut source = module.id_raw().cloned();
                mem::swap(&mut state.source, &mut source);

                let level = level + 1;

                let result = self.call_script_fn(
                    new_scope, mods, state, lib, &mut None, &fn_def, args, pos, level,
                );

                state.source = source;

                result
            }
            Some(f) if f.is_plugin_fn() => f.get_plugin_fn().clone().call(
                (self, fn_name, module.id(), &*mods, lib).into(),
                args.as_mut(),
            ),
            Some(f) if f.is_native() => f.get_native_fn()(
                (self, fn_name, module.id(), &*mods, lib).into(),
                args.as_mut(),
            ),
            Some(f) => unreachable!("unknown function type: {:?}", f),
            None if def_val.is_some() => Ok(def_val.unwrap().clone()),
            None => EvalAltResult::ErrorFunctionNotFound(
                format!(
                    "{}{} ({})",
                    namespace,
                    fn_name,
                    args.iter()
                        .map(|a| if a.is::<ImmutableString>() {
                            "&str | ImmutableString | String"
                        } else {
                            self.map_type_name((*a).type_name())
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                pos,
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
                "**" => return power(x, y).map(Some),
                "e" => return scinot(x, y).map(Some),
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
                "**" => return Ok(Some(x.pow(y as u32).into())),
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
            "**" => return Ok(Some(x.powf(y).into())),
            "e" => return Ok(Some((x*(10.0 as FLOAT).powf(y)).into())),
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
