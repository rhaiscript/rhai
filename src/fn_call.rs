//! Implement function-calling mechanism for `Engine`.

use crate::any::Dynamic;
use crate::calc_fn_hash;
use crate::engine::{
    search_imports, search_namespace, search_scope_only, Engine, Imports, State, KEYWORD_DEBUG,
    KEYWORD_EVAL, KEYWORD_FN_PTR, KEYWORD_FN_PTR_CALL, KEYWORD_FN_PTR_CURRY, KEYWORD_PRINT,
    KEYWORD_TYPE_OF, KEYWORD_SHARED
};
use crate::error::ParseErrorType;
use crate::fn_native::{FnCallArgs, FnPtr};
use crate::module::{Module, ModuleRef};
use crate::optimize::OptimizationLevel;
use crate::parser::{Expr, ImmutableString, AST, INT};
use crate::result::EvalAltResult;
use crate::scope::Scope;
use crate::token::Position;
use crate::utils::StaticVec;
use crate::stdlib::ops::Deref;

#[cfg(not(feature = "no_function"))]
use crate::{
    parser::ScriptFnDef, r#unsafe::unsafe_cast_var_name_to_lifetime,
    scope::EntryType as ScopeEntryType,
};

#[cfg(not(feature = "no_float"))]
use crate::parser::FLOAT;

#[cfg(not(feature = "no_index"))]
use crate::engine::{FN_IDX_GET, FN_IDX_SET};

#[cfg(not(feature = "no_object"))]
use crate::engine::{Map, Target, FN_GET, FN_SET};

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

/// This function replaces the first argument of a method call with a clone copy.
/// This is to prevent a pure function unintentionally consuming the first argument.
fn normalize_first_arg<'a>(
    normalize: bool,
    this_copy: &mut Dynamic,
    old_this_ptr: &mut Option<&'a mut Dynamic>,
    args: &mut FnCallArgs<'a>,
) {
    // Only do it for method calls with arguments.
    if !normalize || args.is_empty() {
        return;
    }

    // Clone the original value.
    *this_copy = args[0].clone();

    // Replace the first reference with a reference to the clone, force-casting the lifetime.
    // Keep the original reference.  Must remember to restore it later with `restore_first_arg_of_method_call`.
    //
    // # Safety
    //
    // Blindly casting a a reference to another lifetime saves on allocations and string cloning,
    // but must be used with the utmost care.
    //
    // We can do this here because, at the end of this scope, we'd restore the original reference
    // with `restore_first_arg_of_method_call`. Therefore this shorter lifetime does not get "out".
    let this_pointer = mem::replace(args.get_mut(0).unwrap(), unsafe {
        mem::transmute(this_copy)
    });

    *old_this_ptr = Some(this_pointer);
}

/// This function restores the first argument that was replaced by `normalize_first_arg_of_method_call`.
fn restore_first_arg<'a>(old_this_ptr: Option<&'a mut Dynamic>, args: &mut FnCallArgs<'a>) {
    if let Some(this_pointer) = old_this_ptr {
        args[0] = this_pointer;
    }
}

impl Engine {
    /// Universal method for calling functions either registered with the `Engine` or written in Rhai.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    ///
    /// ## WARNING
    ///
    /// Function call arguments be _consumed_ when the function requires them to be passed by value.
    /// All function arguments not in the first position are always passed by value and thus consumed.
    /// **DO NOT** reuse the argument values unless for the first `&mut` argument - all others are silently replaced by `()`!
    pub(crate) fn call_fn_raw(
        &self,
        _scope: &mut Scope,
        _mods: &mut Imports,
        state: &mut State,
        lib: &Module,
        fn_name: &str,
        (hash_fn, hash_script): (u64, u64),
        args: &mut FnCallArgs,
        is_ref: bool,
        _is_method: bool,
        def_val: Option<bool>,
        _level: usize,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        self.inc_operations(state)?;

        let native_only = hash_script == 0;

        // Check for stack overflow
        #[cfg(not(feature = "no_function"))]
        #[cfg(not(feature = "unchecked"))]
        if _level > self.limits.max_call_stack_depth {
            return Err(Box::new(
                EvalAltResult::ErrorStackOverflow(Position::none()),
            ));
        }

        let mut this_copy: Dynamic = Default::default();
        let mut old_this_ptr: Option<&mut Dynamic> = None;

        // Search for the function
        // First search in script-defined functions (can override built-in)
        // Then search registered native functions (can override packages)
        // Then search packages
        // NOTE: We skip script functions for global_module and packages, and native functions for lib
        let func = if !native_only {
            lib.get_fn(hash_script) //.or_else(|| lib.get_fn(hash_fn))
        } else {
            None
        }
        //.or_else(|| self.global_module.get_fn(hash_script))
        .or_else(|| self.global_module.get_fn(hash_fn))
        //.or_else(|| self.packages.get_fn(hash_script))
        .or_else(|| self.packages.get_fn(hash_fn));

        if let Some(func) = func {
            #[cfg(not(feature = "no_function"))]
            let need_normalize = is_ref && (func.is_pure() || (func.is_script() && !_is_method));
            #[cfg(feature = "no_function")]
            let need_normalize = is_ref && func.is_pure();

            // Calling pure function but the first argument is a reference?
            normalize_first_arg(need_normalize, &mut this_copy, &mut old_this_ptr, args);

            #[cfg(not(feature = "no_function"))]
            if func.is_script() {
                // Run scripted function
                let fn_def = func.get_fn_def();

                // Method call of script function - map first argument to `this`
                return if _is_method {
                    let (first, rest) = args.split_at_mut(1);
                    Ok((
                        self.call_script_fn(
                            _scope,
                            _mods,
                            state,
                            lib,
                            &mut Some(first[0]),
                            fn_name,
                            fn_def,
                            rest,
                            _level,
                        )?,
                        false,
                    ))
                } else {
                    let result = self.call_script_fn(
                        _scope, _mods, state, lib, &mut None, fn_name, fn_def, args, _level,
                    )?;

                    // Restore the original reference
                    restore_first_arg(old_this_ptr, args);

                    Ok((result, false))
                };
            }

            // Run external function
            let result = func.get_native_fn()(self, lib, args)?;

            // Restore the original reference
            restore_first_arg(old_this_ptr, args);

            // See if the function match print/debug (which requires special processing)
            return Ok(match fn_name {
                KEYWORD_PRINT => (
                    (self.print)(result.as_str().map_err(|typ| {
                        Box::new(EvalAltResult::ErrorMismatchOutputType(
                            self.map_type_name(type_name::<ImmutableString>()).into(),
                            typ.into(),
                            Position::none(),
                        ))
                    })?)
                    .into(),
                    false,
                ),
                KEYWORD_DEBUG => (
                    (self.debug)(result.as_str().map_err(|typ| {
                        Box::new(EvalAltResult::ErrorMismatchOutputType(
                            self.map_type_name(type_name::<ImmutableString>()).into(),
                            typ.into(),
                            Position::none(),
                        ))
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
            return Err(Box::new(EvalAltResult::ErrorDotExpr(
                format!("- property '{}' unknown or write-only", prop),
                Position::none(),
            )));
        }

        // Setter function not found?
        if let Some(prop) = extract_prop_from_setter(fn_name) {
            return Err(Box::new(EvalAltResult::ErrorDotExpr(
                format!("- property '{}' unknown or read-only", prop),
                Position::none(),
            )));
        }

        // index getter function not found?
        #[cfg(not(feature = "no_index"))]
        if fn_name == FN_IDX_GET && args.len() == 2 {
            return Err(Box::new(EvalAltResult::ErrorFunctionNotFound(
                format!(
                    "{} [{}]",
                    self.map_type_name(args[0].type_name()),
                    self.map_type_name(args[1].type_name()),
                ),
                Position::none(),
            )));
        }

        // index setter function not found?
        #[cfg(not(feature = "no_index"))]
        if fn_name == FN_IDX_SET {
            return Err(Box::new(EvalAltResult::ErrorFunctionNotFound(
                format!(
                    "{} [{}]=",
                    self.map_type_name(args[0].type_name()),
                    self.map_type_name(args[1].type_name()),
                ),
                Position::none(),
            )));
        }

        // Raise error
        Err(Box::new(EvalAltResult::ErrorFunctionNotFound(
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
        )))
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
                    Err(Box::new(EvalAltResult::ErrorInFunctionCall(
                        format!("{} > {}", fn_name, name),
                        err,
                        Position::none(),
                    )))
                }
                _ => Err(Box::new(EvalAltResult::ErrorInFunctionCall(
                    fn_name.to_string(),
                    err,
                    Position::none(),
                ))),
            });

        // Remove all local variables
        scope.rewind(prev_scope_len);
        mods.truncate(prev_mods_len);
        state.scope_level = orig_scope_level;

        result
    }

    // Has a system function an override?
    fn has_override(&self, lib: &Module, hash_fn: u64, hash_script: u64) -> bool {
        // NOTE: We skip script functions for global_module and packages, and native functions for lib

        // First check script-defined functions
        lib.contains_fn(hash_script)
        //|| lib.contains_fn(hash_fn)
        // Then check registered functions
        //|| self.global_module.contains_fn(hash_script)
        || self.global_module.contains_fn(hash_fn)
        // Then check packages
        //|| self.packages.contains_fn(hash_script)
        || self.packages.contains_fn(hash_fn)
    }

    /// Perform an actual function call, taking care of special functions
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
        native_only: bool,
        hash_script: u64,
        args: &mut FnCallArgs,
        is_ref: bool,
        is_method: bool,
        def_val: Option<bool>,
        level: usize,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        // Qualifiers (none) + function name + number of arguments + argument `TypeId`'s.
        let arg_types = args.iter().map(|a| a.type_id());
        let hash_fn = calc_fn_hash(empty(), fn_name, args.len(), arg_types);
        let hashes = (hash_fn, if native_only { 0 } else { hash_script });

        match fn_name {
            // type_of
            KEYWORD_TYPE_OF if args.len() == 1 && !self.has_override(lib, hashes.0, hashes.1) => {
                Ok((
                    self.map_type_name(args[0].type_name()).to_string().into(),
                    false,
                ))
            }

            // Fn
            KEYWORD_FN_PTR if args.len() == 1 && !self.has_override(lib, hashes.0, hashes.1) => {
                Err(Box::new(EvalAltResult::ErrorRuntime(
                    "'Fn' should not be called in method style. Try Fn(...);".into(),
                    Position::none(),
                )))
            }

            // eval - reaching this point it must be a method-style call
            KEYWORD_EVAL if args.len() == 1 && !self.has_override(lib, hashes.0, hashes.1) => {
                Err(Box::new(EvalAltResult::ErrorRuntime(
                    "'eval' should not be called in method style. Try eval(...);".into(),
                    Position::none(),
                )))
            }

            // Normal function call
            _ => {
                let mut scope = Scope::new();
                let mut mods = Imports::new();
                self.call_fn_raw(
                    &mut scope, &mut mods, state, lib, fn_name, hashes, args, is_ref, is_method,
                    def_val, level,
                )
            }
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
        script: &Dynamic,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let script = script.as_str().map_err(|typ| {
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
    #[cfg(not(feature = "no_object"))]
    pub(crate) fn make_method_call(
        &self,
        state: &mut State,
        lib: &Module,
        target: &mut Target,
        expr: &Expr,
        idx_val: Dynamic,
        level: usize,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        let ((name, native, pos), _, hash, _, def_val) = match expr {
            Expr::FnCall(x) => x.as_ref(),
            _ => unreachable!(),
        };

        let is_ref = target.is_ref();
        let is_value = target.is_value();

        // Get a reference to the mutation target Dynamic
        let obj = target.as_mut();
        let mut idx = idx_val.cast::<StaticVec<Dynamic>>();
        let mut _fn_name = name.as_ref();

        let (result, updated) = if _fn_name == KEYWORD_FN_PTR_CALL && obj.is::<FnPtr>() {
            // FnPtr call
            let fn_ptr = obj.read_lock::<FnPtr>().unwrap();
            let mut curry = fn_ptr.curry().iter().cloned().collect::<StaticVec<_>>();
            // Redirect function name
            let fn_name = fn_ptr.fn_name();
            // Recalculate hash
            let hash = calc_fn_hash(empty(), fn_name, curry.len() + idx.len(), empty());
            // Arguments are passed as-is, adding the curried arguments
            let mut arg_values = curry
                .iter_mut()
                .chain(idx.iter_mut())
                .collect::<StaticVec<_>>();
            let args = arg_values.as_mut();

            // Map it to name(args) in function-call style
            self.exec_fn_call(
                state, lib, fn_name, *native, hash, args, false, false, *def_val, level,
            )
        } else if _fn_name == KEYWORD_FN_PTR_CALL && idx.len() > 0 && idx[0].is::<FnPtr>() {
            // FnPtr call on object
            let fn_ptr = idx.remove(0).cast::<FnPtr>();
            let mut curry = fn_ptr.curry().iter().cloned().collect::<StaticVec<_>>();
            // Redirect function name
            let fn_name = fn_ptr.get_fn_name().clone();
            // Recalculate hash
            let hash = calc_fn_hash(empty(), &fn_name, curry.len() + idx.len(), empty());
            // Replace the first argument with the object pointer, adding the curried arguments
            let mut arg_values = once(obj)
                .chain(curry.iter_mut())
                .chain(idx.iter_mut())
                .collect::<StaticVec<_>>();
            let args = arg_values.as_mut();

            // Map it to name(args) in function-call style
            self.exec_fn_call(
                state, lib, &fn_name, *native, hash, args, is_ref, true, *def_val, level,
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
        } else {
            #[cfg(not(feature = "no_object"))]
            let redirected;
            let mut _hash = *hash;

            // Check if it is a map method call in OOP style
            #[cfg(not(feature = "no_object"))]
            if let Some(map) = obj.read_lock::<Map>() {
                if let Some(val) = map.get(_fn_name) {
                    if let Some(f) = val.read_lock::<FnPtr>() {
                        // Remap the function name
                        redirected = f.get_fn_name().clone();
                        _fn_name = &redirected;
                        // Recalculate the hash based on the new function name
                        _hash = calc_fn_hash(empty(), _fn_name, idx.len(), empty());
                    }
                }
            };

            // Attached object pointer in front of the arguments
            let mut arg_values = once(obj).chain(idx.iter_mut()).collect::<StaticVec<_>>();
            let args = arg_values.as_mut();

            self.exec_fn_call(
                state, lib, _fn_name, *native, _hash, args, is_ref, true, *def_val, level,
            )
        }
        .map_err(|err| err.new_position(*pos))?;

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
        mut hash: u64,
        native: bool,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        // Handle Fn()
        if name == KEYWORD_FN_PTR && args_expr.len() == 1 {
            let hash_fn = calc_fn_hash(empty(), name, 1, once(TypeId::of::<ImmutableString>()));

            if !self.has_override(lib, hash_fn, hash) {
                // Fn - only in function call style
                let expr = args_expr.get(0).unwrap();
                let arg_value = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;

                return arg_value
                    .take_immutable_string()
                    .map_err(|typ| {
                        Box::new(EvalAltResult::ErrorMismatchOutputType(
                            self.map_type_name(type_name::<ImmutableString>()).into(),
                            typ.into(),
                            expr.position(),
                        ))
                    })
                    .and_then(|s| FnPtr::try_from(s))
                    .map(Into::<Dynamic>::into);
            }
        }

        // Handle curry()
        if name == KEYWORD_FN_PTR_CURRY && args_expr.len() > 1 {
            let expr = args_expr.get(0).unwrap();
            let fn_ptr = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;

            if !fn_ptr.is::<FnPtr>() {
                return Err(Box::new(EvalAltResult::ErrorMismatchOutputType(
                    self.map_type_name(type_name::<FnPtr>()).into(),
                    self.map_type_name(fn_ptr.type_name()).into(),
                    expr.position(),
                )));
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

        // Handle shared()
        #[cfg(not(feature = "no_shared"))]
        if name == KEYWORD_SHARED && args_expr.len() == 1 {
            let expr = args_expr.get(0).unwrap();
            let value = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;

            return Ok(value.into_shared());
        }

        // Handle eval()
        if name == KEYWORD_EVAL && args_expr.len() == 1 {
            let hash_fn = calc_fn_hash(empty(), name, 1, once(TypeId::of::<ImmutableString>()));

            if !self.has_override(lib, hash_fn, hash) {
                // eval - only in function call style
                let prev_len = scope.len();
                let expr = args_expr.get(0).unwrap();
                let script = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;
                let result = self
                    .eval_script_expr(scope, mods, state, lib, &script)
                    .map_err(|err| err.new_position(expr.position()));

                if scope.len() != prev_len {
                    // IMPORTANT! If the eval defines new variables in the current scope,
                    //            all variable offsets from this point on will be mis-aligned.
                    state.always_search = true;
                }

                return result;
            }
        }

        // Handle call() - Redirect function call
        let redirected;
        let mut args_expr = args_expr.as_ref();
        let mut curry: StaticVec<_> = Default::default();
        let mut name = name;

        if name == KEYWORD_FN_PTR_CALL && args_expr.len() >= 1 && !self.has_override(lib, 0, hash) {
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
                hash = calc_fn_hash(empty(), name, curry.len() + args_expr.len(), empty());
            } else {
                return Err(Box::new(EvalAltResult::ErrorMismatchOutputType(
                    self.map_type_name(type_name::<FnPtr>()).into(),
                    fn_name.type_name().into(),
                    expr.position(),
                )));
            }
        }

        // Normal function call - except for Fn and eval (handled above)
        let mut arg_values: StaticVec<_>;
        let mut args: StaticVec<_>;
        let mut is_ref = false;

        if args_expr.is_empty() && curry.is_empty() {
            // No arguments
            args = Default::default();
        } else {
            // See if the first argument is a variable, if so, convert to method-call style
            // in order to leverage potential &mut first argument and avoid cloning the value
            match args_expr.get(0).unwrap() {
                // func(x, ...) -> x.func(...)
                lhs @ Expr::Variable(_) => {
                    arg_values = args_expr
                        .iter()
                        .skip(1)
                        .map(|expr| self.eval_expr(scope, mods, state, lib, this_ptr, expr, level))
                        .collect::<Result<_, _>>()?;

                    let (target, _, _, pos) = search_namespace(scope, mods, state, this_ptr, lhs)?;

                    self.inc_operations(state)
                        .map_err(|err| err.new_position(pos))?;

                    args = once(target)
                        .chain(curry.iter_mut())
                        .chain(arg_values.iter_mut())
                        .collect();

                    is_ref = true;
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

        let args = args.as_mut();
        self.exec_fn_call(
            state, lib, name, native, hash, args, is_ref, false, def_val, level,
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
            Err(err) if matches!(*err, EvalAltResult::ErrorFunctionNotFound(_, _)) => {
                // Then search in Rust functions
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
            Ok(f) if f.is_script() => {
                let args = args.as_mut();
                let fn_def = f.get_fn_def();
                let mut scope = Scope::new();
                let mut mods = Imports::new();
                self.call_script_fn(
                    &mut scope, &mut mods, state, lib, &mut None, name, fn_def, args, level,
                )
            }
            Ok(f) => f.get_native_fn()(self, lib, args.as_mut()),
            Err(err) => match *err {
                EvalAltResult::ErrorFunctionNotFound(_, _) if def_val.is_some() => {
                    Ok(def_val.unwrap().into())
                }
                EvalAltResult::ErrorFunctionNotFound(_, pos) => {
                    Err(Box::new(EvalAltResult::ErrorFunctionNotFound(
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
                        pos,
                    )))
                }
                _ => Err(err),
            },
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
        let x = *x.read_lock::<INT>().unwrap();
        let y = *y.read_lock::<INT>().unwrap();

        #[cfg(not(feature = "unchecked"))]
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

        #[cfg(feature = "unchecked")]
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
        let x = *x.read_lock::<bool>().unwrap();
        let y = *y.read_lock::<bool>().unwrap();

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
        let x = *x.read_lock::<char>().unwrap();
        let y = *y.read_lock::<char>().unwrap();

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
        let x = *x.read_lock::<FLOAT>().unwrap();
        let y = *y.read_lock::<FLOAT>().unwrap();

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
        let mut x = x.write_lock::<INT>().unwrap();
        let y = *y.read_lock::<INT>().unwrap();

        #[cfg(not(feature = "unchecked"))]
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

        #[cfg(feature = "unchecked")]
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

        match op {
            "&=" => return Ok(Some(*x &= y)),
            "|=" => return Ok(Some(*x |= y)),
            "^=" => return Ok(Some(*x ^= y)),
            _ => (),
        }
    } else if args_type == TypeId::of::<bool>() {
        let mut x = x.write_lock::<bool>().unwrap();
        let y = *y.read_lock::<bool>().unwrap();

        match op {
            "&=" => return Ok(Some(*x = *x && y)),
            "|=" => return Ok(Some(*x = *x || y)),
            _ => (),
        }
    } else if args_type == TypeId::of::<ImmutableString>() {
        let mut x = x.write_lock::<ImmutableString>().unwrap();
        let y = y.read_lock::<ImmutableString>().unwrap();

        match op {
            "+=" => return Ok(Some(*x += y.deref())),
            _ => (),
        }
    }

    #[cfg(not(feature = "no_float"))]
    if args_type == TypeId::of::<FLOAT>() {
        let mut x = x.write_lock::<FLOAT>().unwrap();
        let y = *y.read_lock::<FLOAT>().unwrap();

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
