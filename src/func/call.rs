//! Implement function-calling mechanism for [`Engine`].

use super::{get_builtin_binary_op_fn, get_builtin_op_assignment_fn, RhaiFunc};
use crate::api::default_limits::MAX_DYNAMIC_PARAMETERS;
use crate::ast::{Expr, FnCallExpr, FnCallHashes};
#[cfg(feature = "default-parameters")]
use crate::ast::ScriptFuncDef;
use crate::engine::{
    KEYWORD_DEBUG, KEYWORD_EVAL, KEYWORD_FN_PTR, KEYWORD_FN_PTR_CALL, KEYWORD_FN_PTR_CURRY,
    KEYWORD_IS_DEF_VAR, KEYWORD_PRINT, KEYWORD_TYPE_OF,
};
use crate::eval::{Caches, FnResolutionCacheEntry, GlobalRuntimeState};
use crate::tokenizer::{is_valid_function_name, Token};
use crate::types::{dynamic::Union, fn_ptr::FnPtrType};
use crate::{
    calc_fn_hash, calc_fn_hash_full, Dynamic, Engine, FnArgsVec, FnPtr, ImmutableString, Position,
    RhaiResult, RhaiResultOf, Scope, Shared, ERR,
};
#[cfg(feature = "no_std")]
use hashbrown::hash_map::Entry;
#[cfg(not(feature = "no_std"))]
use std::collections::hash_map::Entry;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::{type_name, TypeId},
    convert::TryFrom,
    mem,
};

#[cfg(not(feature = "no_float"))]
#[cfg(feature = "no_std")]
use num_traits::Float;

/// Arguments to a function call, which is a list of [`&mut Dynamic`][Dynamic].
pub type FnCallArgs<'a> = [&'a mut Dynamic];

/// Describes how to provide a value for each parameter.
#[cfg(all(not(feature = "no_function"), feature = "default-parameters"))]
#[derive(Debug)]
pub(crate) enum ArgSource {
    /// Use the provided value (already evaluated).
    Provided(Dynamic),
    /// Evaluate the default expression in the callee context.
    DefaultExpr,
}

/// Plan for how to construct the complete argument list.
#[cfg(all(not(feature = "no_function"), feature = "default-parameters"))]
#[derive(Debug)]
pub(crate) struct ArgPlan {
    pub(crate) sources: FnArgsVec<ArgSource>,
}

/// Build an argument plan from positional args, defaults, and named args.
/// Returns a plan describing how to construct the complete argument list.
#[cfg(all(not(feature = "no_function"), feature = "default-parameters"))]
fn build_arg_plan(
    fn_def: &ScriptFuncDef,
    positional_args: &[Dynamic],
    named_args: &[(ImmutableString, Dynamic)],
    pos: Position,
) -> RhaiResultOf<ArgPlan> {
    let num_params = fn_def.params.len();
    let num_positional = positional_args.len();

    // Validate: can't have more positional args than parameters
    if num_positional > num_params {
        return Err(ERR::ErrorFunctionNotFound(
            format!(
                "{} (expected at most {} arguments, got {})",
                fn_def.name, num_params, num_positional
            ),
            pos,
        )
        .into());
    }

    // Build a map of named arguments for quick lookup
    let mut named_map = std::collections::HashMap::new();
    for (param_name, value) in named_args {
        // Check for duplicate named arguments
        if named_map
            .insert(param_name.clone(), value.clone())
            .is_some()
        {
            return Err(ERR::ErrorFunctionNotFound(
                format!(
                    "{} (duplicate named argument '{}')",
                    fn_def.name, param_name
                ),
                pos,
            )
            .into());
        }
    }

    // Validate: check for arguments provided both positionally and by name
    for (param_name, _) in named_args {
        if let Some(pos_idx) = fn_def.params.iter().position(|p| p == param_name) {
            if pos_idx < num_positional {
                return Err(ERR::ErrorFunctionNotFound(
                    format!(
                        "{} (argument '{}' provided both positionally and by name)",
                        fn_def.name, param_name
                    ),
                    pos,
                )
                .into());
            }
        }
    }

    // Build argument plan
    let mut sources = FnArgsVec::with_capacity(num_params);

    // First, add positional arguments
    for arg in positional_args.iter() {
        sources.push(ArgSource::Provided(arg.clone()));
    }

    // Then, fill remaining positions with named args or defaults
    for i in num_positional..num_params {
        let param_name = &fn_def.params[i];

        // Check if this parameter has a named argument
        if let Some(value) = named_map.remove(param_name) {
            sources.push(ArgSource::Provided(value));
        } else if fn_def.defaults[i].is_some() {
            // Mark that we need to evaluate the default expression
            sources.push(ArgSource::DefaultExpr);
        } else {
            // Missing required argument
            return Err(ERR::ErrorFunctionNotFound(
                format!(
                    "{} (missing required argument '{}', position {})",
                    fn_def.name,
                    param_name,
                    i + 1
                ),
                pos,
            )
            .into());
        }
    }

    // Check for named arguments that don't match any parameter
    if !named_map.is_empty() {
        let unknown_param = named_map.keys().next().unwrap();
        return Err(ERR::ErrorFunctionNotFound(
            format!(
                "{} (unknown named argument '{}')",
                fn_def.name, unknown_param
            ),
            pos,
        )
        .into());
    }

    Ok(ArgPlan { sources })
}

/// A type that temporarily stores a mutable reference to a `Dynamic`,
/// replacing it with a cloned copy.
#[derive(Debug)]
struct ArgBackup<'a> {
    orig_mut: Option<&'a mut Dynamic>,
    value_copy: Dynamic,
}

impl<'a> ArgBackup<'a> {
    /// Create a new `ArgBackup`.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            orig_mut: None,
            value_copy: Dynamic::UNIT,
        }
    }
    /// This function replaces the first argument of a method call with a clone copy.
    /// This is to prevent a pure function unintentionally consuming the first argument.
    ///
    /// `restore_first_arg` must be called before the end of the scope to prevent the shorter
    /// lifetime from leaking.
    ///
    /// # Safety
    ///
    /// This method blindly casts a reference to another lifetime, which saves allocation and
    /// string cloning.
    ///
    /// As long as `restore_first_arg` is called before the end of the scope, the shorter lifetime
    /// will not leak.
    ///
    /// # Panics
    ///
    /// Panics when `args` is empty.
    #[inline(always)]
    pub fn change_first_arg_to_copy(&mut self, args: &mut FnCallArgs<'a>) {
        // Clone the original value.
        self.value_copy = args[0].clone();

        // Replace the first reference with a reference to the clone, force-casting the lifetime.
        // Must remember to restore it later with `restore_first_arg`.
        //
        // SAFETY:
        //
        // Blindly casting a reference to another lifetime saves allocation and string cloning,
        // but must be used with the utmost care.
        //
        // We can do this here because, before the end of this scope, we'd restore the original
        // reference via `restore_first_arg`. Therefore this shorter lifetime does not leak.
        self.orig_mut = Some(mem::replace(&mut args[0], unsafe {
            mem::transmute(&mut self.value_copy)
        }));
    }
    /// This function restores the first argument that was replaced by `change_first_arg_to_copy`.
    ///
    /// # Safety
    ///
    /// If `change_first_arg_to_copy` has been called, this function **MUST** be called _BEFORE_
    /// exiting the current scope.  Otherwise it is undefined behavior as the shorter lifetime will leak.
    #[inline(always)]
    pub fn restore_first_arg(&mut self, args: &mut FnCallArgs<'a>) {
        args[0] = self.orig_mut.take().unwrap();
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

// Ensure no data races in function call arguments.
#[cfg(not(feature = "no_closure"))]
#[inline]
pub fn ensure_no_data_race(fn_name: &str, args: &FnCallArgs, is_ref_mut: bool) -> RhaiResultOf<()> {
    args.iter()
        .skip(usize::from(is_ref_mut))
        .position(|a| a.is_locked())
        .map_or(Ok(()), |n| {
            Err(ERR::ErrorDataRace(
                format!("argument #{} of function '{fn_name}'", n + 1),
                Position::NONE,
            )
            .into())
        })
}

/// Is a function name an anonymous function?
#[cfg(not(feature = "no_function"))]
#[inline]
#[must_use]
pub fn is_anonymous_fn(name: &str) -> bool {
    name.starts_with(crate::engine::FN_ANONYMOUS)
}

impl Engine {
    /// Generate the signature for a function call.
    #[inline]
    #[must_use]
    fn gen_fn_call_signature(&self, fn_name: &str, args: &[&mut Dynamic]) -> String {
        format!(
            "{fn_name} ({})",
            args.iter()
                .map(|a| if a.is_string() {
                    "&str | ImmutableString | String"
                } else {
                    self.map_type_name(a.type_name())
                })
                .collect::<FnArgsVec<_>>()
                .join(", ")
        )
    }

    /// Resolve a normal (non-qualified) function call.
    ///
    /// Search order:
    /// 1) AST - script functions in the AST
    /// 2) Global namespace - functions registered via `Engine::register_XXX`
    /// 3) Global registered modules - packages
    /// 4) Imported modules - functions marked with global namespace
    /// 5) Static registered modules
    #[must_use]
    fn resolve_fn<'s>(
        &self,
        _global: &GlobalRuntimeState,
        caches: &'s mut Caches,
        local_entry: &'s mut Option<FnResolutionCacheEntry>,
        op_token: Option<&Token>,
        hash_base: u64,
        args: Option<&mut FnCallArgs>,
        allow_dynamic: bool,
    ) -> Option<&'s FnResolutionCacheEntry> {
        let mut hash = args.as_deref().map_or(hash_base, |args| {
            calc_fn_hash_full(hash_base, args.iter().map(|a| a.type_id()))
        });

        let cache = caches.fn_resolution_cache_mut();

        match cache.dict.entry(hash) {
            Entry::Occupied(entry) => entry.into_mut().as_ref(),
            Entry::Vacant(entry) => {
                let num_args = args.as_deref().map_or(0, FnCallArgs::len);
                let mut max_bitmask = 0; // One above maximum bitmask based on number of parameters.
                                         // Set later when a specific matching function is not found.
                let mut bitmask = 1usize; // Bitmask of which parameter to replace with `Dynamic`

                loop {
                    // First check scripted functions in the AST or embedded environments
                    #[cfg(not(feature = "no_function"))]
                    let func = _global
                        .lib
                        .iter()
                        .rev()
                        .find_map(|m| m.get_fn(hash).map(|f| (f, m.id_raw())));
                    #[cfg(feature = "no_function")]
                    let func = None;

                    // Then check the global namespace
                    let func = func.or_else(|| {
                        self.global_modules
                            .iter()
                            .find_map(|m| m.get_fn(hash).map(|f| (f, m.id_raw())))
                    });

                    // Then check imported modules for global functions, then global sub-modules for global functions
                    #[cfg(not(feature = "no_module"))]
                    let func = func
                        .or_else(|| _global.get_qualified_fn(hash, true))
                        .or_else(|| {
                            self.global_sub_modules
                                .values()
                                .filter(|m| m.contains_indexed_global_functions())
                                .find_map(|m| m.get_qualified_fn(hash).map(|f| (f, m.id_raw())))
                        });

                    if let Some((f, s)) = func {
                        // Specific version found
                        let new_entry = FnResolutionCacheEntry {
                            func: f.clone(),
                            source: s.cloned(),
                        };
                        return if cache.bloom_filter.is_absent_and_set(hash) {
                            // Do not cache "one-hit wonders"
                            *local_entry = Some(new_entry);
                            local_entry.as_ref()
                        } else {
                            // Cache entry
                            entry.insert(Some(new_entry)).as_ref()
                        };
                    }

                    // Check `Dynamic` parameters for functions with parameters
                    let max_dynamic_count = usize::min(num_args, MAX_DYNAMIC_PARAMETERS);

                    if allow_dynamic && max_bitmask == 0 && num_args > 0 {
                        let has_dynamic = self
                            .global_modules
                            .iter()
                            .any(|m| m.may_contain_dynamic_fn(hash_base));

                        #[cfg(not(feature = "no_function"))]
                        let has_dynamic = has_dynamic
                            || _global
                                .lib
                                .iter()
                                .any(|m| m.may_contain_dynamic_fn(hash_base));

                        #[cfg(not(feature = "no_module"))]
                        let has_dynamic = has_dynamic
                            || _global.may_contain_dynamic_fn(hash_base)
                            || self
                                .global_sub_modules
                                .values()
                                .any(|m| m.may_contain_dynamic_fn(hash_base));

                        // Set maximum bitmask when there are dynamic versions of the function
                        if has_dynamic {
                            max_bitmask = 1usize << max_dynamic_count;
                        }
                    }

                    // Stop when all permutations are exhausted
                    if bitmask >= max_bitmask {
                        // With proper registration, functions with defaults are registered for all valid
                        // argument counts (min_args to num_params). If we reach here, either:
                        // 1. No function exists for this arg count (initial lookup failed)
                        // 2. All Dynamic permutations failed (if allow_dynamic was true)
                        //
                        // For named arguments that skip parameters (e.g., `func(a, c=2)` needing a 3-param
                        // function), the caller (exec_fn_call_with_named) handles searching for functions
                        // with more parameters and validating they have the required named parameters.
                        //
                        // No need to guess upward here - either we found it or it doesn't exist.

                        if num_args != 2 {
                            return None;
                        }

                        // Try to find a built-in version
                        let builtin =
                            args.and_then(|args| match op_token {
                                None => None,
                                Some(token) if token.is_op_assignment() => {
                                    let (first_arg, rest_args) = args.split_first().unwrap();

                                    get_builtin_op_assignment_fn(token, first_arg, rest_args[0])
                                        .map(|(f, has_context)| FnResolutionCacheEntry {
                                            func: RhaiFunc::Method {
                                                func: Shared::new(f),
                                                has_context,
                                                is_pure: false,
                                                is_volatile: false,
                                            },
                                            source: None,
                                        })
                                }
                                Some(token) => get_builtin_binary_op_fn(token, args[0], args[1])
                                    .map(|(f, has_context)| FnResolutionCacheEntry {
                                        func: RhaiFunc::Method {
                                            func: Shared::new(f),
                                            has_context,
                                            is_pure: true,
                                            is_volatile: false,
                                        },
                                        source: None,
                                    }),
                            });

                        return if cache.bloom_filter.is_absent_and_set(hash) {
                            // Do not cache "one-hit wonders"
                            *local_entry = builtin;
                            local_entry.as_ref()
                        } else {
                            // Cache entry
                            entry.insert(builtin).as_ref()
                        };
                    }

                    // Try all permutations with `Dynamic` wildcards
                    hash = calc_fn_hash_full(
                        hash_base,
                        args.as_ref().unwrap().iter().enumerate().map(|(i, a)| {
                            if i < max_dynamic_count
                                && bitmask & (1usize << (max_dynamic_count - i - 1)) != 0
                            {
                                // Replace with `Dynamic`
                                TypeId::of::<Dynamic>()
                            } else {
                                a.type_id()
                            }
                        }),
                    );

                    bitmask += 1;
                }
            }
        }
    }

    /// # Main Entry-Point (Native by Name)
    ///
    /// Call a native Rust function registered with the [`Engine`] by name.
    ///
    /// # WARNING
    ///
    /// Function call arguments be _consumed_ when the function requires them to be passed by value.
    /// All function arguments not in the first position are always passed by value and thus consumed.
    ///
    /// **DO NOT** reuse the argument values except for the first `&mut` argument - all others are silently replaced by `()`!
    pub(crate) fn exec_native_fn_call(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        name: &str,
        op_token: Option<&Token>,
        hash: u64,
        args: &mut FnCallArgs,
        is_ref_mut: bool,
        non_volatile_only: bool,
        pos: Position,
    ) -> RhaiResultOf<(Dynamic, bool)> {
        self.track_operation(global, pos)?;

        // Check if function access already in the cache
        let local_entry = &mut None;
        let a = Some(&mut *args);
        let func = self.resolve_fn(global, caches, local_entry, op_token, hash, a, true);

        if let Some(FnResolutionCacheEntry { func, source }) = func {
            debug_assert!(func.is_native());

            if non_volatile_only && func.is_volatile() {
                let gen_fn_call_signature = self.gen_fn_call_signature(name, args);
                return Err(ERR::ErrorFunctionNotFound(gen_fn_call_signature, pos).into());
            }

            let is_method = func.is_method();

            // Push a new call stack frame
            #[cfg(feature = "debugging")]
            let orig_call_stack_len = global
                .debugger
                .as_ref()
                .map_or(0, |dbg| dbg.call_stack().len());

            let backup = &mut ArgBackup::new();

            // Calling non-method function but the first argument is a reference?
            let swap = is_ref_mut && !is_method && !args.is_empty();

            if swap {
                // Clone the first argument
                backup.change_first_arg_to_copy(args);
            }

            #[cfg(feature = "debugging")]
            if self.is_debugger_registered() {
                let source = source.clone().or_else(|| global.source.clone());

                global.debugger_mut().push_call_stack_frame(
                    self.get_interned_string(name),
                    args.iter().map(|v| (*v).clone()),
                    source,
                    pos,
                );
            }

            // Run external function
            let context = func
                .has_context()
                .then(|| (self, name, source.as_deref(), &*global, pos).into());

            let mut _result = match func {
                // If function is not pure, there must be at least one argument
                f if !f.is_pure() && !args.is_empty() && args[0].is_read_only() => {
                    Err(ERR::ErrorNonPureMethodCallOnConstant(name.to_string(), pos).into())
                }
                RhaiFunc::Plugin { func } => func.call(context, args),
                RhaiFunc::Pure { func, .. } | RhaiFunc::Method { func, .. } => func(context, args),
                _ => unreachable!("non-native function"),
            }
            .and_then(|r| self.check_data_size(r, pos))
            .map_err(|err| err.fill_position(pos));

            if swap {
                backup.restore_first_arg(args);
            }

            #[cfg(feature = "debugging")]
            if self.is_debugger_registered() {
                use crate::eval::{DebuggerEvent, DebuggerStatus};

                let trigger = match global.debugger().status {
                    DebuggerStatus::FunctionExit(n) => n >= global.level,
                    DebuggerStatus::Next(.., true) => true,
                    _ => false,
                };
                if trigger {
                    let scope = &mut Scope::new();
                    let node = crate::ast::Stmt::Noop(pos);
                    let node = (&node).into();
                    let event = match _result {
                        Ok(ref r) => DebuggerEvent::FunctionExitWithValue(r),
                        Err(ref err) => DebuggerEvent::FunctionExitWithError(err),
                    };

                    match self.dbg_raw(global, caches, scope, None, node, event) {
                        Ok(..) => (),
                        Err(err) => _result = Err(err),
                    }
                }

                // Pop the call stack
                global.debugger_mut().rewind_call_stack(orig_call_stack_len);
            }

            let result = _result?;

            // Check the data size of any `&mut` object, which may be changed.
            #[cfg(not(feature = "unchecked"))]
            if is_ref_mut && !args.is_empty() {
                self.check_data_size(&*args[0], pos)?;
            }

            // See if the function match print/debug (which requires special processing)
            return Ok(match name {
                KEYWORD_PRINT => {
                    if let Some(ref print) = self.print {
                        let text = result.into_immutable_string().map_err(|typ| {
                            let t = self.map_type_name(type_name::<ImmutableString>()).into();
                            ERR::ErrorMismatchOutputType(t, typ.into(), pos)
                        })?;
                        print(&text);
                    }
                    (Dynamic::UNIT, false)
                }
                KEYWORD_DEBUG => {
                    if let Some(ref debug) = self.debug {
                        let text = result.into_immutable_string().map_err(|typ| {
                            let t = self.map_type_name(type_name::<ImmutableString>()).into();
                            ERR::ErrorMismatchOutputType(t, typ.into(), pos)
                        })?;
                        debug(&text, global.source(), pos);
                    }
                    (Dynamic::UNIT, false)
                }
                _ => (result, is_method),
            });
        }

        // Error handling

        match name {
            // index getter function not found?
            #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
            crate::engine::FN_IDX_GET => {
                debug_assert_eq!(args.len(), 2);

                let t0 = self.map_type_name(args[0].type_name());
                let t1 = self.map_type_name(args[1].type_name());

                Err(ERR::ErrorIndexingType(format!("{t0} [{t1}]"), pos).into())
            }

            // index setter function not found?
            #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
            crate::engine::FN_IDX_SET => {
                debug_assert_eq!(args.len(), 3);

                let t0 = self.map_type_name(args[0].type_name());
                let t1 = self.map_type_name(args[1].type_name());
                let t2 = self.map_type_name(args[2].type_name());

                Err(ERR::ErrorIndexingType(format!("{t0} [{t1}] = {t2}"), pos).into())
            }

            // Getter function not found?
            #[cfg(not(feature = "no_object"))]
            _ if name.starts_with(crate::engine::FN_GET) => {
                debug_assert_eq!(args.len(), 1);

                let prop = &name[crate::engine::FN_GET.len()..];
                let t0 = self.map_type_name(args[0].type_name());

                Err(ERR::ErrorDotExpr(
                    format!(
                        "Unknown property '{prop}' - a getter is not registered for type '{t0}'"
                    ),
                    pos,
                )
                .into())
            }

            // Setter function not found?
            #[cfg(not(feature = "no_object"))]
            _ if name.starts_with(crate::engine::FN_SET) => {
                debug_assert_eq!(args.len(), 2);

                let prop = &name[crate::engine::FN_SET.len()..];
                let t0 = self.map_type_name(args[0].type_name());
                let t1 = self.map_type_name(args[1].type_name());

                Err(ERR::ErrorDotExpr(
                    format!(
                        "No writable property '{prop}' - a setter is not registered for type '{t0}' to handle '{t1}'"
                    ),
                    pos,
                )
                .into())
            }

            // Raise error
            _ => {
                Err(ERR::ErrorFunctionNotFound(self.gen_fn_call_signature(name, args), pos).into())
            }
        }
    }

    /// # Main Entry-Point (By Name)
    ///
    /// Perform an actual function call, native Rust or scripted, by name, taking care of special functions.
    ///
    /// # WARNING
    ///
    /// Function call arguments may be _consumed_ when the function requires them to be passed by value.
    /// All function arguments not in the first position are always passed by value and thus consumed.
    ///
    /// **DO NOT** reuse the argument values except for the first `&mut` argument - all others are silently replaced by `()`!
    pub(crate) fn exec_fn_call(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: Option<&mut Scope>,
        fn_name: &str,
        op_token: Option<&Token>,
        hashes: FnCallHashes,
        args: &mut FnCallArgs,
        is_ref_mut: bool,
        is_method_call: bool,
        pos: Position,
    ) -> RhaiResultOf<(Dynamic, bool)> {
        #[cfg(all(not(feature = "no_function"), feature = "default-parameters"))]
        {
            self.exec_fn_call_with_named(
                global,
                caches,
                scope,
                fn_name,
                op_token,
                hashes,
                args,
                is_ref_mut,
                is_method_call,
                &[],
                pos,
            )
        }
        #[cfg(all(not(feature = "no_function"), not(feature = "default-parameters")))]
        {
            // When default-parameters is disabled, use the original behavior
            self.exec_fn_call_original(
                global,
                caches,
                scope,
                fn_name,
                op_token,
                hashes,
                args,
                is_ref_mut,
                is_method_call,
                pos,
            )
        }
        #[cfg(feature = "no_function")]
        {
            // When no_function is enabled, only native functions are available
            self.exec_native_fn_call(
                global,
                caches,
                fn_name,
                op_token,
                hashes.native(),
                args,
                is_ref_mut,
                false,
                pos,
            )
        }
    }

    /// Original exec_fn_call implementation without default parameters support.
    #[cfg(all(not(feature = "no_function"), not(feature = "default-parameters")))]
    fn exec_fn_call_original(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: Option<&mut Scope>,
        fn_name: &str,
        op_token: Option<&Token>,
        hashes: FnCallHashes,
        args: &mut FnCallArgs,
        is_ref_mut: bool,
        is_method_call: bool,
        pos: Position,
    ) -> RhaiResultOf<(Dynamic, bool)> {
        // These may be redirected from method style calls.
        if hashes.is_native_only()
            && match fn_name {
                // Handle type_of()
                KEYWORD_TYPE_OF if args.len() == 1 => {
                    let typ = self.get_interned_string(self.map_type_name(args[0].type_name()));
                    return Ok((typ.into(), false));
                }

                #[cfg(not(feature = "no_closure"))]
                crate::engine::KEYWORD_IS_SHARED if args.len() == 1 => {
                    return Ok((args[0].is_shared().into(), false))
                }
                #[cfg(not(feature = "no_closure"))]
                crate::engine::KEYWORD_IS_SHARED => true,

                #[cfg(not(feature = "no_function"))]
                crate::engine::KEYWORD_IS_DEF_FN => true,

                KEYWORD_TYPE_OF | KEYWORD_FN_PTR | KEYWORD_EVAL | KEYWORD_IS_DEF_VAR
                | KEYWORD_FN_PTR_CALL | KEYWORD_FN_PTR_CURRY => true,

                _ => false,
            }
        {
            let sig = self.gen_fn_call_signature(fn_name, args);
            return Err(ERR::ErrorFunctionNotFound(sig, pos).into());
        }

        // Check for data race.
        #[cfg(not(feature = "no_closure"))]
        ensure_no_data_race(fn_name, args, is_ref_mut)?;

        defer! { let orig_level = global.level; global.level += 1 }

        // Script-defined function call?
        if !hashes.is_native_only() {
            let hash = hashes.script();
            let local_entry = &mut None;
            let mut resolved = None;

            #[cfg(not(feature = "no_object"))]
            if is_method_call && !args.is_empty() {
                let typed_hash =
                    crate::calc_typed_method_hash(hash, self.map_type_name(args[0].type_name()));
                resolved = self.resolve_fn(
                    global,
                    caches,
                    local_entry,
                    None,
                    typed_hash,
                    None,
                    false,
                );
            }

            if resolved.is_none() {
                resolved = self.resolve_fn(
                    global,
                    caches,
                    local_entry,
                    None,
                    hash,
                    None,
                    false,
                );
            }

            if let Some(FnResolutionCacheEntry { func, source }) = resolved.cloned() {
                let RhaiFunc::Script { fn_def, env } = func else {
                    unreachable!("Script function expected");
                };

                let fn_def = &*fn_def;
                let env = env.as_deref();

                if fn_def.body.is_empty() {
                    return Ok((Dynamic::UNIT, false));
                }

                let mut empty_scope;
                let scope = if let Some(scope) = scope {
                    scope
                } else {
                    empty_scope = Scope::new();
                    &mut empty_scope
                };

                let orig_source = mem::replace(&mut global.source, source);
                defer! { global => move |g| g.source = orig_source }

                return if is_method_call {
                    // Method call: first arg is the object (not in fn_def.params)
                    let (first_arg, rest_args) = args.split_first_mut().unwrap();
                    let this_ptr = Some(&mut **first_arg);

                    self.call_script_fn(
                        global, caches, scope, this_ptr, env, fn_def, rest_args, true, pos,
                    )
                } else {
                    // Normal call of script function
                    let backup = &mut ArgBackup::new();

                    // The first argument is a reference?
                    let swap = is_ref_mut && !args.is_empty();

                    if swap {
                        backup.change_first_arg_to_copy(args);
                    }

                    defer! { args = (args) if swap => move |a| backup.restore_first_arg(a) }

                    self.call_script_fn(global, caches, scope, None, env, fn_def, args, true, pos)
                }
                .map(|r| (r, false));
            }
        }

        // Native function call
        let hash = hashes.native();

        self.exec_native_fn_call(
            global, caches, fn_name, op_token, hash, args, is_ref_mut, false, pos,
        )
    }

    /// Internal function that handles named arguments and defaults.
    #[cfg(all(not(feature = "no_function"), feature = "default-parameters"))]
    fn exec_fn_call_with_named(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: Option<&mut Scope>,
        fn_name: &str,
        op_token: Option<&Token>,
        hashes: FnCallHashes,
        args: &mut FnCallArgs,
        is_ref_mut: bool,
        is_method_call: bool,
        named_args: &[(ImmutableString, Dynamic)],
        pos: Position,
    ) -> RhaiResultOf<(Dynamic, bool)> {
        // These may be redirected from method style calls.
        if hashes.is_native_only()
            && match fn_name {
                // Handle type_of()
                KEYWORD_TYPE_OF if args.len() == 1 => {
                    let typ = self.get_interned_string(self.map_type_name(args[0].type_name()));
                    return Ok((typ.into(), false));
                }

                #[cfg(not(feature = "no_closure"))]
                crate::engine::KEYWORD_IS_SHARED if args.len() == 1 => {
                    return Ok((args[0].is_shared().into(), false))
                }
                #[cfg(not(feature = "no_closure"))]
                crate::engine::KEYWORD_IS_SHARED => true,

                #[cfg(not(feature = "no_function"))]
                crate::engine::KEYWORD_IS_DEF_FN => true,

                KEYWORD_TYPE_OF | KEYWORD_FN_PTR | KEYWORD_EVAL | KEYWORD_IS_DEF_VAR
                | KEYWORD_FN_PTR_CALL | KEYWORD_FN_PTR_CURRY => true,

                _ => false,
            }
        {
            let sig = self.gen_fn_call_signature(fn_name, args);
            return Err(ERR::ErrorFunctionNotFound(sig, pos).into());
        }

        // Check for data race.
        #[cfg(not(feature = "no_closure"))]
        ensure_no_data_race(fn_name, args, is_ref_mut)?;

        defer! { let orig_level = global.level; global.level += 1 }

        // Script-defined function call?
        #[cfg(not(feature = "no_function"))]
        if !hashes.is_native_only() {
            let hash = hashes.script();
            let local_entry = &mut None;
            let mut resolved = None;

            #[cfg(not(feature = "no_object"))]
            if is_method_call && !args.is_empty() {
                let typed_hash =
                    crate::calc_typed_method_hash(hash, self.map_type_name(args[0].type_name()));
                resolved = self.resolve_fn(
                    global,
                    caches,
                    local_entry,
                    None,
                    typed_hash,
                    None,
                    false,
                );
            }

            if resolved.is_none() {
                resolved = self.resolve_fn(
                    global,
                    caches,
                    local_entry,
                    None,
                    hash,
                    None,
                    false,
                );
            }

            if let Some(FnResolutionCacheEntry { func, source }) = resolved.cloned() {
                let RhaiFunc::Script { fn_def, env } = func else {
                    unreachable!("Script function expected");
                };

                let fn_def = &*fn_def;
                let env = env.as_deref();

                // If named arguments are present, check if this function can handle them
                // If not, try to find a function with more parameters that can
                #[cfg(feature = "default-parameters")]
                if !named_args.is_empty() {
                    // Check if all named arguments exist in this function's parameters
                    let has_all_named_params = named_args.iter().all(|(name, _)| {
                        fn_def.params.iter().any(|p| p == name)
                    });

                    if !has_all_named_params {
                        // Try to find a function with more parameters that might have these named args
                        let mut found_better = None;
                        for try_arg_count in (args.len() + 1)..=(args.len() + 10) {
                            let try_hash = crate::calc_fn_hash(None, fn_name, try_arg_count);
                            let try_resolved = self.resolve_fn(
                                global,
                                caches,
                                local_entry,
                                None,
                                try_hash,
                                None,
                                false,
                            );

                            if let Some(FnResolutionCacheEntry { func: try_func, source: try_source }) = try_resolved {
                                if let RhaiFunc::Script { fn_def: try_fn_def, .. } = try_func {
                                    // Check if this function has all the named parameters
                                    let has_all = named_args.iter().all(|(name, _)| {
                                        try_fn_def.params.iter().any(|p| p == name)
                                    });
                                    if has_all {
                                        found_better = Some((try_func.clone(), try_source.clone()));
                                        break;
                                    }
                                }
                            }
                        }

                        // If we found a better match, use it
                        if let Some((better_func, better_source)) = found_better {
                            let RhaiFunc::Script { fn_def: better_fn_def, env: better_env } = better_func else {
                                unreachable!("Script function expected");
                            };
                            let fn_def = &*better_fn_def;
                            let env = better_env.as_deref();
                            let orig_source = mem::replace(&mut global.source, better_source);
                            defer! { global => move |g| g.source = orig_source }

                            let mut empty_scope;
                            let scope = if let Some(scope) = scope {
                                scope
                            } else {
                                empty_scope = Scope::new();
                                &mut empty_scope
                            };

                            return if is_method_call {
                                let (first_arg, rest_positional) = args.split_first_mut().unwrap();
                                let this_ptr = Some(&mut **first_arg);
                                let positional_args: FnArgsVec<Dynamic> = rest_positional.iter().map(|a| (*a).clone()).collect();
                                let arg_plan = build_arg_plan(fn_def, &positional_args, named_args, pos)?;

                                self.call_script_fn_with_plan(
                                    global, caches, scope, this_ptr, env, fn_def, arg_plan, true, pos,
                                )
                            } else {
                                let positional_args: FnArgsVec<Dynamic> = args.iter().map(|a| (*a).clone()).collect();
                                let arg_plan = build_arg_plan(fn_def, &positional_args, named_args, pos)?;

                                self.call_script_fn_with_plan(global, caches, scope, None, env, fn_def, arg_plan, true, pos)
                            }
                            .map(|r| (r, false));
                        }
                        // Otherwise fall through and let build_arg_plan report the error
                    }
                }

                if fn_def.body.is_empty() {
                    return Ok((Dynamic::UNIT, false));
                }

                let mut empty_scope;
                let scope = if let Some(scope) = scope {
                    scope
                } else {
                    empty_scope = Scope::new();
                    &mut empty_scope
                };

                let orig_source = mem::replace(&mut global.source, source);
                defer! { global => move |g| g.source = orig_source }

                // Build argument plan from positional args, defaults, and named args
                return if is_method_call {
                    // Method call: first arg is the object (not in fn_def.params)
                    // So we need to handle it separately
                    let (first_arg, rest_positional) = args.split_first_mut().unwrap();
                    let this_ptr = Some(&mut **first_arg);

                    // For method calls, fn_def.params doesn't include 'this'
                    // So we build args for the remaining parameters
                    let positional_args: FnArgsVec<Dynamic> = rest_positional.iter().map(|a| (*a).clone()).collect();
                    let arg_plan = build_arg_plan(fn_def, &positional_args, named_args, pos)?;

                    self.call_script_fn_with_plan(
                        global, caches, scope, this_ptr, env, fn_def, arg_plan, true, pos,
                    )
                } else {
                    // Normal call of script function
                    let positional_args: FnArgsVec<Dynamic> = args.iter().map(|a| (*a).clone()).collect();
                    let arg_plan = build_arg_plan(fn_def, &positional_args, named_args, pos)?;

                    self.call_script_fn_with_plan(global, caches, scope, None, env, fn_def, arg_plan, true, pos)
                }
                .map(|r| (r, false));
            }
        }

        // Native function call
        let hash = hashes.native();

        self.exec_native_fn_call(
            global, caches, fn_name, op_token, hash, args, is_ref_mut, false, pos,
        )
    }

    /// Evaluate an argument.
    #[inline]
    pub(crate) fn get_arg_value(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        this_ptr: Option<&mut Dynamic>,
        arg_expr: &Expr,
    ) -> RhaiResultOf<(Dynamic, Position)> {
        // Literal values
        if let Some(value) = arg_expr.get_literal_value(Some(global)) {
            self.track_operation(global, arg_expr.start_position())?;

            #[cfg(feature = "debugging")]
            self.dbg(global, caches, scope, this_ptr, arg_expr)?;

            return Ok((value, arg_expr.start_position()));
        }

        // Do not match function exit for arguments
        #[cfg(feature = "debugging")]
        let reset = global.debugger.as_deref_mut().and_then(|dbg| {
            dbg.clear_status_if(|status| {
                matches!(status, crate::eval::DebuggerStatus::FunctionExit(..))
            })
        });
        #[cfg(feature = "debugging")]
        defer! { global if Some(reset) => move |g| g.debugger_mut().reset_status(reset) }

        self.eval_expr(global, caches, scope, this_ptr, arg_expr)
            .map(|r| (r, arg_expr.start_position()))
    }

    /// Call a dot method.
    #[cfg(not(feature = "no_object"))]
    pub(crate) fn make_method_call(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        fn_name: &str,
        mut hash: FnCallHashes,
        target: &mut crate::eval::Target,
        call_args: &mut [Dynamic],
        first_arg_pos: Position,
        pos: Position,
        named_args: &[(ImmutableString, Dynamic)],
    ) -> RhaiResultOf<(Dynamic, bool)> {
        let (result, updated) = match fn_name {
            // Handle fn_ptr.call(...)
            KEYWORD_FN_PTR_CALL if target.as_ref().is_fnptr() => {
                let fn_ptr = target.as_ref().read_lock::<FnPtr>().unwrap();
                let mut curry = fn_ptr.curry().iter().cloned().collect::<FnArgsVec<_>>();

                // Arguments are passed as-is, adding the curried arguments
                let mut args = curry
                    .iter_mut()
                    .chain(call_args.iter_mut())
                    .collect::<FnArgsVec<_>>();

                match fn_ptr.typ {
                    // Linked to scripted function - short-circuit
                    #[cfg(not(feature = "no_function"))]
                    FnPtrType::Script(ref fn_def) if fn_def.params.len() == args.len() => {
                        let fn_ptr = target.as_ref().read_lock::<FnPtr>().unwrap();

                        let scope = &mut Scope::new();
                        let env = fn_ptr.env.as_ref().map(<_>::as_ref);

                        defer! { let orig_level = global.level; global.level += 1 }

                        self.call_script_fn(
                            global, caches, scope, None, env, fn_def, &mut args, true, pos,
                        )
                        .map(|v| (v, false))
                    }
                    // Native function - short-circuit
                    FnPtrType::Native(ref func) => {
                        defer! { let orig_level = global.level; global.level += 1 }

                        let context = (self, fn_name, None, &*global, pos).into();

                        func(context, &mut args)
                            .and_then(|r| self.check_data_size(r, pos))
                            .map(|v| (v, false))
                            .map_err(|err| err.fill_position(pos))
                    }
                    // Normal call
                    _ => {
                        let fn_name = fn_ptr.fn_name();

                        let _is_anon = false;
                        #[cfg(not(feature = "no_function"))]
                        let _is_anon = fn_ptr.is_anonymous();

                        // Recalculate hashes - for anonymous functions, we need to try multiple argument counts
                        // if the function has defaults
                        let new_hash = if !_is_anon && !is_valid_function_name(fn_name) {
                            FnCallHashes::from_native_only(calc_fn_hash(None, fn_name, args.len()))
                        } else {
                            // For anonymous functions, try the exact arg count first, then allow resolution
                            // to try other counts if the function has defaults
                            FnCallHashes::from_hash(calc_fn_hash(None, fn_name, args.len()))
                        };

                        // Map it to name(args) in function-call style
                        #[cfg(feature = "default-parameters")]
                        {
                            self.exec_fn_call_with_named(
                                global, caches, None, fn_name, None, new_hash, &mut args, false, false,
                                named_args, pos,
                            )
                        }
                        #[cfg(not(feature = "default-parameters"))]
                        {
                            self.exec_fn_call(
                                global, caches, None, fn_name, None, new_hash, &mut args, false, false, pos,
                            )
                        }
                    }
                }
            }

            // Handle obj.call(fn_ptr, ...)
            KEYWORD_FN_PTR_CALL => {
                if call_args.is_empty() {
                    return Err(self.make_type_mismatch_err::<FnPtr>(
                        self.map_type_name(target.as_ref().type_name()),
                        pos,
                    ));
                }

                // FnPtr call on object
                let fn_ptr = call_args[0]
                    .take()
                    .try_cast_result::<FnPtr>()
                    .map_err(|v| {
                        self.make_type_mismatch_err::<FnPtr>(
                            self.map_type_name(v.type_name()),
                            first_arg_pos,
                        )
                    })?;

                let _is_anon = false;
                #[cfg(not(feature = "no_function"))]
                let _is_anon = fn_ptr.is_anonymous();

                let FnPtr {
                    name,
                    curry,
                    #[cfg(not(feature = "no_function"))]
                    env,
                    typ,
                } = fn_ptr;

                // Adding the curried arguments and the remaining arguments
                let mut curry = curry.into_iter().collect::<FnArgsVec<_>>();
                let args = &mut FnArgsVec::with_capacity(curry.len() + call_args.len());
                args.extend(curry.iter_mut());
                args.extend(call_args.iter_mut().skip(1));

                match typ {
                    // Linked to scripted function - short-circuit
                    #[cfg(not(feature = "no_function"))]
                    FnPtrType::Script(fn_def) if fn_def.params.len() == args.len() => {
                        // Check for data race.
                        #[cfg(not(feature = "no_closure"))]
                        ensure_no_data_race(&fn_def.name, args, false)?;

                        let scope = &mut Scope::new();
                        let this_ptr = Some(target.as_mut());
                        let env = env.as_deref();

                        defer! { let orig_level = global.level; global.level += 1 }

                        self.call_script_fn(
                            global, caches, scope, this_ptr, env, &fn_def, args, true, pos,
                        )
                        .map(|v| (v, false))
                    }
                    // Native function - short-circuit
                    FnPtrType::Native(ref func) => {
                        defer! { let orig_level = global.level; global.level += 1 }

                        let context = (self, fn_name, None, &*global, pos).into();

                        // Add the first argument with the object pointer
                        args.insert(0, target.as_mut());

                        func(context, args)
                            .and_then(|r| self.check_data_size(r, pos))
                            .map(|v| (v, false))
                            .map_err(|err| err.fill_position(pos))
                    }
                    // Normal call
                    _ => {
                        let is_ref_mut = target.is_ref();

                        // Add the first argument with the object pointer
                        args.insert(0, target.as_mut());

                        // Recalculate hash
                        let num_args = args.len();

                        let new_hash = if !_is_anon && !is_valid_function_name(&name) {
                            FnCallHashes::from_native_only(calc_fn_hash(None, &name, num_args))
                        } else {
                            #[cfg(not(feature = "no_function"))]
                            {
                                FnCallHashes::from_script_and_native(
                                    calc_fn_hash(None, &name, num_args - 1),
                                    calc_fn_hash(None, &name, num_args),
                                )
                            }
                            #[cfg(feature = "no_function")]
                            {
                                FnCallHashes::from_native_only(calc_fn_hash(None, &name, num_args))
                            }
                        };

                        // Map it to name(args) in function-call style
                        #[cfg(feature = "default-parameters")]
                        {
                            self.exec_fn_call_with_named(
                                global, caches, None, &name, None, new_hash, args, is_ref_mut, true,
                                named_args, pos,
                            )
                        }
                        #[cfg(not(feature = "default-parameters"))]
                        {
                            self.exec_fn_call(
                                global, caches, None, &name, None, new_hash, args, is_ref_mut, true, pos,
                            )
                        }
                    }
                }
            }

            // Handle fn_ptr.curry(...)
            KEYWORD_FN_PTR_CURRY => {
                let typ = target.as_ref().type_name();
                let mut fn_ptr = target
                    .as_ref()
                    .read_lock::<FnPtr>()
                    .ok_or_else(|| {
                        self.make_type_mismatch_err::<FnPtr>(self.map_type_name(typ), pos)
                    })?
                    .clone();

                // Append the new curried arguments to the existing list.
                fn_ptr.extend(call_args.iter_mut().map(mem::take));

                Ok((fn_ptr.into(), false))
            }

            // Handle var.is_shared()
            #[cfg(not(feature = "no_closure"))]
            crate::engine::KEYWORD_IS_SHARED if call_args.is_empty() => {
                return Ok((target.is_shared().into(), false));
            }

            _ => {
                let mut fn_name = fn_name;
                let _redirected;
                let mut _linked = None;
                let mut _arg_values;
                let mut call_args = call_args;

                // Check if it is a map method call in OOP style
                #[cfg(not(feature = "no_object"))]
                if let Ok(map) = target.as_ref().as_map_ref() {
                    if let Some(val) = map.get(fn_name) {
                        if let Some(fn_ptr) = val.read_lock::<FnPtr>() {
                            // Remap the function name
                            _redirected = fn_ptr.fn_name_raw().clone();
                            fn_name = &_redirected;
                            // Add curried arguments
                            if fn_ptr.is_curried() {
                                _arg_values = fn_ptr
                                    .curry()
                                    .iter()
                                    .cloned()
                                    .chain(call_args.iter_mut().map(mem::take))
                                    .collect::<FnArgsVec<_>>();
                                call_args = &mut _arg_values;
                            }

                            match fn_ptr.typ {
                                // Linked to scripted function
                                #[cfg(not(feature = "no_function"))]
                                FnPtrType::Script(ref fn_def)
                                    if fn_def.params.len() == call_args.len() =>
                                {
                                    _linked = Some((Some(fn_def.clone()), None, fn_ptr.env.clone()))
                                }
                                FnPtrType::Native(ref func) => {
                                    _linked = Some((
                                        #[cfg(not(feature = "no_function"))]
                                        None,
                                        #[cfg(feature = "no_function")]
                                        Option::<()>::None,
                                        Some(func.clone()),
                                        #[cfg(not(feature = "no_function"))]
                                        fn_ptr.env.clone(),
                                        #[cfg(feature = "no_function")]
                                        Option::<()>::None,
                                    ))
                                }
                                _ => {
                                    let _is_anon = false;
                                    #[cfg(not(feature = "no_function"))]
                                    let _is_anon = fn_ptr.is_anonymous();

                                    // Recalculate the hash based on the new function name and new arguments
                                    let num_args = call_args.len() + 1;

                                    hash = if !_is_anon && !is_valid_function_name(fn_name) {
                                        FnCallHashes::from_native_only(calc_fn_hash(
                                            None, fn_name, num_args,
                                        ))
                                    } else {
                                        #[cfg(not(feature = "no_function"))]
                                        {
                                            FnCallHashes::from_script_and_native(
                                                calc_fn_hash(None, fn_name, num_args - 1),
                                                calc_fn_hash(None, fn_name, num_args),
                                            )
                                        }
                                        #[cfg(feature = "no_function")]
                                        {
                                            FnCallHashes::from_native_only(calc_fn_hash(
                                                None, fn_name, num_args,
                                            ))
                                        }
                                    };
                                }
                            }
                        }
                    }
                }

                match _linked {
                    // Linked to scripted function - short-circuit
                    #[cfg(not(feature = "no_function"))]
                    Some((Some(fn_def), None, env)) => {
                        let scope = &mut Scope::new();
                        let env = env.as_deref();
                        let this_ptr = Some(target.as_mut());

                        #[cfg(feature = "default-parameters")]
                        {
                            // Build argument plan from positional args, defaults, and named args
                            let positional_args: FnArgsVec<Dynamic> =
                                call_args.iter().map(|a| (*a).clone()).collect();
                            let arg_plan =
                                build_arg_plan(&fn_def, &positional_args, named_args, pos)?;

                            defer! { let orig_level = global.level; global.level += 1 }

                            self.call_script_fn_with_plan(
                                global, caches, scope, this_ptr, env, &fn_def, arg_plan, true, pos,
                            )
                            .map(|v| (v, false))
                        }
                        #[cfg(not(feature = "default-parameters"))]
                        {
                            // When feature is disabled, just use positional args directly
                            let args = &mut call_args.iter_mut().collect::<FnArgsVec<_>>();

                            defer! { let orig_level = global.level; global.level += 1 }

                            self.call_script_fn(
                                global, caches, scope, this_ptr, env, &fn_def, args, true, pos,
                            )
                            .map(|v| (v, false))
                        }
                    }
                    // Native function - short-circuit
                    Some((None, Some(func), ..)) => {
                        // Attached object pointer in front of the arguments
                        let args = &mut std::iter::once(target.as_mut())
                            .chain(call_args.iter_mut())
                            .collect::<FnArgsVec<_>>();

                        defer! { let orig_level = global.level; global.level += 1 }

                        let context = (self, fn_name, None, &*global, pos).into();

                        func(context, args)
                            .and_then(|r| self.check_data_size(r, pos))
                            .map(|v| (v, false))
                            .map_err(|err| err.fill_position(pos))
                    }
                    // Normal call
                    None => {
                        let is_ref_mut = target.is_ref();

                        // Attached object pointer in front of the arguments
                        let args = &mut std::iter::once(target.as_mut())
                            .chain(call_args.iter_mut())
                            .collect::<FnArgsVec<_>>();

                        #[cfg(feature = "default-parameters")]
                        {
                            self.exec_fn_call_with_named(
                                global, caches, None, fn_name, None, hash, args, is_ref_mut, true,
                                named_args, pos,
                            )
                        }
                        #[cfg(not(feature = "default-parameters"))]
                        {
                            self.exec_fn_call(
                                global, caches, None, fn_name, None, hash, args, is_ref_mut, true, pos,
                            )
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }?;

        // Propagate the changed value back to the source if necessary
        if updated {
            target.propagate_changed_value(pos)?;
        }

        Ok((result, updated))
    }

    /// Call a function in normal function-call style.
    pub(crate) fn make_function_call(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        mut this_ptr: Option<&mut Dynamic>,
        fn_name: &str,
        op_token: Option<&Token>,
        first_arg: Option<&Expr>,
        args_expr: &[Expr],
        #[cfg(feature = "default-parameters")]
        named_args: &[(ImmutableString, Expr)],
        #[cfg(not(feature = "default-parameters"))]
        _named_args: &[(ImmutableString, Expr)],
        hashes: FnCallHashes,
        capture_scope: bool,
        pos: Position,
    ) -> RhaiResult {
        let mut first_arg = first_arg;
        let mut args_expr = args_expr;
        let mut num_args = usize::from(first_arg.is_some()) + args_expr.len();
        let mut curry = FnArgsVec::new_const();
        let mut fn_name = fn_name;
        let mut hashes = hashes;
        let redirected; // Handle call() - Redirect function call

        match fn_name {
            _ if op_token.is_some() => (),

            // Handle call(fn_ptr, ...)
            KEYWORD_FN_PTR_CALL if num_args >= 1 => {
                let arg = first_arg.unwrap();
                let (first_arg_value, first_arg_pos) =
                    self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), arg)?;

                let fn_ptr = first_arg_value.try_cast_result::<FnPtr>().map_err(|v| {
                    self.make_type_mismatch_err::<FnPtr>(
                        self.map_type_name(v.type_name()),
                        first_arg_pos,
                    )
                })?;

                let _is_anon = false;
                #[cfg(not(feature = "no_function"))]
                let _is_anon = fn_ptr.is_anonymous();

                let FnPtr {
                    name,
                    curry: extra_curry,
                    #[cfg(not(feature = "no_function"))]
                    env,
                    typ,
                } = fn_ptr;

                curry.extend(extra_curry);

                match typ {
                    // Linked to scripted function - short-circuit
                    // For functions with defaults, allow fewer arguments than the full parameter count
                    #[cfg(all(not(feature = "no_function"), feature = "default-parameters"))]
                    FnPtrType::Script(fn_def) => {
                        let num_provided = curry.len() + args_expr.len();
                        let num_params = fn_def.params.len();
                        let min_args = fn_def
                            .defaults
                            .iter()
                            .position(|d| d.is_some())
                            .unwrap_or(num_params);

                        // Check if we have a valid number of arguments (between min_args and num_params)
                        if num_provided >= min_args && num_provided <= num_params {
                            // Evaluate arguments
                            let mut arg_values =
                                FnArgsVec::with_capacity(curry.len() + args_expr.len());
                            arg_values.extend(curry);
                            for expr in args_expr {
                                let this_ptr = this_ptr.as_deref_mut();
                                let (value, _) =
                                    self.get_arg_value(global, caches, scope, this_ptr, expr)?;
                                arg_values.push(value);
                            }

                            // Build argument plan with defaults
                            let positional_args: FnArgsVec<Dynamic> =
                                arg_values.iter().map(|a| (*a).clone()).collect();
                            let arg_plan =
                                build_arg_plan(&fn_def, &positional_args, &[], pos)?;

                            let scope = &mut Scope::new();
                            let env = env.as_deref();

                            defer! { let orig_level = global.level; global.level += 1 }

                            return self.call_script_fn_with_plan(
                                global, caches, scope, None, env, &fn_def, arg_plan, true, pos,
                            );
                        }
                    }
                    #[cfg(all(not(feature = "no_function"), not(feature = "default-parameters")))]
                    FnPtrType::Script(fn_def) if fn_def.params.len() == curry.len() + args_expr.len() => {
                        // When feature is disabled, only allow exact argument count match
                        // Evaluate arguments
                        let mut arg_values =
                            FnArgsVec::with_capacity(curry.len() + args_expr.len());
                        arg_values.extend(curry);
                        for expr in args_expr {
                            let this_ptr = this_ptr.as_deref_mut();
                            let (value, _) =
                                self.get_arg_value(global, caches, scope, this_ptr, expr)?;
                            arg_values.push(value);
                        }
                        let args = &mut arg_values.iter_mut().collect::<FnArgsVec<_>>();

                        let scope = &mut Scope::new();
                        let env = env.as_deref();

                        defer! { let orig_level = global.level; global.level += 1 }

                        return self.call_script_fn(
                            global, caches, scope, None, env, &fn_def, args, true, pos,
                        );
                    }
                    // Native function - short-circuit
                    FnPtrType::Native(ref func) => {
                        // Evaluate arguments
                        let mut arg_values =
                            FnArgsVec::with_capacity(curry.len() + args_expr.len());
                        arg_values.extend(curry);
                        for expr in args_expr {
                            let this_ptr = this_ptr.as_deref_mut();
                            let (value, _) =
                                self.get_arg_value(global, caches, scope, this_ptr, expr)?;
                            arg_values.push(value);
                        }
                        let args = &mut arg_values.iter_mut().collect::<FnArgsVec<_>>();

                        defer! { let orig_level = global.level; global.level += 1 }

                        let context = (self, fn_name, None, &*global, pos).into();

                        return func(context, args)
                            .and_then(|r| self.check_data_size(r, pos))
                            .map_err(|err| err.fill_position(pos));
                    }
                    // Normal function call
                    _ => (),
                }

                // Redirect function name
                redirected = name;
                fn_name = &redirected;

                // Shift the arguments
                first_arg = args_expr.first();
                if !args_expr.is_empty() {
                    args_expr = &args_expr[1..];
                }
                num_args -= 1;

                // Recalculate hash
                let args_len = num_args + curry.len();

                hashes = if !_is_anon && !is_valid_function_name(fn_name) {
                    FnCallHashes::from_native_only(calc_fn_hash(None, fn_name, args_len))
                } else {
                    FnCallHashes::from_hash(calc_fn_hash(None, fn_name, args_len))
                };
            }

            // Handle Fn(fn_name)
            KEYWORD_FN_PTR if num_args == 1 => {
                let arg = first_arg.unwrap();
                let (arg_value, arg_pos) =
                    self.get_arg_value(global, caches, scope, this_ptr, arg)?;

                // Fn - only in function call style
                return arg_value
                    .into_immutable_string()
                    .map_err(|typ| self.make_type_mismatch_err::<ImmutableString>(typ, arg_pos))
                    .and_then(FnPtr::try_from)
                    .map(Into::into)
                    .map_err(|err| err.fill_position(arg_pos));
            }

            // Handle curry(x, ...)
            KEYWORD_FN_PTR_CURRY if num_args > 1 => {
                let first = first_arg.unwrap();
                let (first_arg_value, first_arg_pos) =
                    self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), first)?;

                let mut fn_ptr = first_arg_value.try_cast_result::<FnPtr>().map_err(|v| {
                    self.make_type_mismatch_err::<FnPtr>(
                        self.map_type_name(v.type_name()),
                        first_arg_pos,
                    )
                })?;

                // Append the new curried arguments to the existing list.
                for expr in args_expr {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    fn_ptr.add_curry(value);
                }

                return Ok(fn_ptr.into());
            }

            // Handle is_shared(var)
            #[cfg(not(feature = "no_closure"))]
            crate::engine::KEYWORD_IS_SHARED if num_args == 1 => {
                let arg = first_arg.unwrap();
                let (arg_value, ..) =
                    self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), arg)?;
                return Ok(arg_value.is_shared().into());
            }

            // Handle is_def_fn(fn_name, arity)
            #[cfg(not(feature = "no_function"))]
            crate::engine::KEYWORD_IS_DEF_FN if num_args == 2 => {
                let first = first_arg.unwrap();
                let (arg_value, arg_pos) =
                    self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), first)?;

                let fn_name = arg_value
                    .into_immutable_string()
                    .map_err(|typ| self.make_type_mismatch_err::<ImmutableString>(typ, arg_pos))?;

                let (arg_value, arg_pos) =
                    self.get_arg_value(global, caches, scope, this_ptr, &args_expr[0])?;

                let num_params = arg_value
                    .as_int()
                    .map_err(|typ| self.make_type_mismatch_err::<crate::INT>(typ, arg_pos))?;

                return Ok(usize::try_from(num_params)
                    .map(|num_params| {
                        let hash_script = calc_fn_hash(None, &fn_name, num_params);
                        self.has_script_fn(global, caches, hash_script).into()
                    })
                    .unwrap_or(Dynamic::FALSE));
            }

            // Handle is_def_fn(this_type, fn_name, arity)
            #[cfg(not(feature = "no_function"))]
            #[cfg(not(feature = "no_object"))]
            crate::engine::KEYWORD_IS_DEF_FN if num_args == 3 => {
                let first = first_arg.unwrap();
                let (arg_value, arg_pos) =
                    self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), first)?;

                let this_type = arg_value
                    .into_immutable_string()
                    .map_err(|typ| self.make_type_mismatch_err::<ImmutableString>(typ, arg_pos))?;

                let (arg_value, arg_pos) = self.get_arg_value(
                    global,
                    caches,
                    scope,
                    this_ptr.as_deref_mut(),
                    &args_expr[0],
                )?;

                let fn_name = arg_value
                    .into_immutable_string()
                    .map_err(|typ| self.make_type_mismatch_err::<ImmutableString>(typ, arg_pos))?;

                let (arg_value, arg_pos) =
                    self.get_arg_value(global, caches, scope, this_ptr, &args_expr[1])?;

                let num_params = arg_value
                    .as_int()
                    .map_err(|typ| self.make_type_mismatch_err::<crate::INT>(typ, arg_pos))?;

                return Ok(usize::try_from(num_params)
                    .map(|num_params| {
                        let hash_script = crate::calc_typed_method_hash(
                            calc_fn_hash(None, &fn_name, num_params),
                            &this_type,
                        );
                        self.has_script_fn(global, caches, hash_script).into()
                    })
                    .unwrap_or(Dynamic::FALSE));
            }

            // Handle is_def_var(fn_name)
            KEYWORD_IS_DEF_VAR if num_args == 1 => {
                let arg = first_arg.unwrap();
                let (arg_value, arg_pos) =
                    self.get_arg_value(global, caches, scope, this_ptr, arg)?;
                let var_name = arg_value
                    .into_immutable_string()
                    .map_err(|typ| self.make_type_mismatch_err::<ImmutableString>(typ, arg_pos))?;
                return Ok(scope.contains(&var_name).into());
            }

            // Handle eval(script)
            KEYWORD_EVAL if num_args == 1 => {
                // eval - only in function call style
                let orig_scope_len = scope.len();
                #[cfg(not(feature = "no_module"))]
                let orig_imports_len = global.num_imports();
                let arg = first_arg.unwrap();
                let (arg_value, pos) = self.get_arg_value(global, caches, scope, this_ptr, arg)?;
                let s = &arg_value
                    .into_immutable_string()
                    .map_err(|typ| self.make_type_mismatch_err::<ImmutableString>(typ, pos))?;

                let orig_level = global.level;
                global.level += 1;

                let result = self.eval_script_expr_in_place(global, caches, scope, s, pos);

                // IMPORTANT! If the eval defines new variables in the current scope,
                //            all variable offsets from this point on will be mis-aligned.
                //            The same is true for imports.
                let scope_changed = scope.len() != orig_scope_len;
                #[cfg(not(feature = "no_module"))]
                let scope_changed = scope_changed || global.num_imports() != orig_imports_len;

                if scope_changed {
                    global.always_search_scope = true;
                }
                global.level = orig_level;

                return result.map_err(|err| match *err {
                    ERR::Exit(..) => err,
                    _ => ERR::ErrorInFunctionCall(
                        KEYWORD_EVAL.to_string(),
                        global.source().unwrap_or("").to_string(),
                        err,
                        pos,
                    )
                    .into(),
                });
            }

            _ => (),
        }

        // Normal function call - except for Fn, curry, call and eval (handled above)
        let mut arg_values = FnArgsVec::with_capacity(num_args);
        let mut args = FnArgsVec::with_capacity(num_args + curry.len());
        let mut is_ref_mut = false;

        // Evaluate named arguments
        #[cfg(feature = "default-parameters")]
        let mut named_arg_values = FnArgsVec::with_capacity(named_args.len());
        #[cfg(feature = "default-parameters")]
        for (param_name, expr) in named_args {
            let (value, ..) =
                self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
            named_arg_values.push((param_name.clone(), value.flatten()));
        }

        // Capture parent scope?
        //
        // If so, do it separately because we cannot convert the first argument (if it is a simple
        // variable access) to &mut because `scope` is needed.
        if capture_scope && !scope.is_empty() {
            for expr in first_arg.iter().copied().chain(args_expr.iter()) {
                let (value, ..) =
                    self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                arg_values.push(value.flatten());
            }
            args.extend(curry.iter_mut());
            args.extend(arg_values.iter_mut());

            // Use parent scope
            let scope = Some(scope);

            // Convert named_arg_values to the format expected by exec_fn_call_with_named
            #[cfg(feature = "default-parameters")]
            let named_args_dyn: FnArgsVec<_> = named_arg_values
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect();
            #[cfg(feature = "default-parameters")]
            let named_args_slice: &[(ImmutableString, Dynamic)] = &named_args_dyn;
            #[cfg(not(feature = "default-parameters"))]
            let named_args_slice: &[(ImmutableString, Dynamic)] = &[];

            #[cfg(all(not(feature = "no_function"), feature = "default-parameters"))]
            {
                return self
                    .exec_fn_call_with_named(
                        global,
                        caches,
                        scope,
                        fn_name,
                        op_token,
                        hashes,
                        &mut args,
                        is_ref_mut,
                        false,
                        named_args_slice,
                        pos,
                    )
                    .map(|(v, ..)| v);
            }
            #[cfg(all(not(feature = "no_function"), not(feature = "default-parameters")))]
            {
                return self
                    .exec_fn_call(
                        global,
                        caches,
                        scope,
                        fn_name,
                        op_token,
                        hashes,
                        &mut args,
                        is_ref_mut,
                        false,
                        pos,
                    )
                    .map(|(v, ..)| v);
            }
            #[cfg(feature = "no_function")]
            {
                // When no_function is enabled, named arguments are not supported
                // Just call exec_fn_call which will handle native functions
                return self
                    .exec_fn_call(
                        global,
                        caches,
                        scope,
                        fn_name,
                        op_token,
                        hashes,
                        &mut args,
                        is_ref_mut,
                        false,
                        pos,
                    )
                    .map(|(v, ..)| v);
            }
        }

        // Call with blank scope
        #[cfg(not(feature = "no_closure"))]
        let has_non_shared_this_ptr = this_ptr.as_ref().map_or(false, |v| !v.is_shared());
        #[cfg(feature = "no_closure")]
        let has_non_shared_this_ptr = this_ptr.is_some();

        // If the first argument is a variable, and there are no curried arguments,
        // convert to method-call style in order to leverage potential &mut first argument
        // and avoid cloning the value.
        match first_arg {
            Some(_first @ Expr::ThisPtr(pos)) if curry.is_empty() && has_non_shared_this_ptr => {
                // Turn it into a method call only if the object is not shared
                self.track_operation(global, *pos)?;

                #[cfg(feature = "debugging")]
                self.dbg(global, caches, scope, this_ptr.as_deref_mut(), _first)?;

                // func(x, ...) -> x.func(...)
                for expr in args_expr {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }

                is_ref_mut = true;
                args.push(this_ptr.unwrap());
            }
            Some(first @ Expr::Variable(.., pos)) if curry.is_empty() => {
                self.track_operation(global, *pos)?;

                #[cfg(feature = "debugging")]
                self.dbg(global, caches, scope, this_ptr.as_deref_mut(), first)?;

                // func(x, ...) -> x.func(...)
                for expr in args_expr {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }

                let mut target = self.search_namespace(global, caches, scope, this_ptr, first)?;

                if target.as_ref().is_read_only() {
                    target = target.into_owned();
                }

                if target.is_shared() || target.is_temp_value() {
                    arg_values.insert(0, target.take_or_clone().flatten());
                } else {
                    // Turn it into a method call only if the object is not shared and not a simple value
                    is_ref_mut = true;
                    let obj_ref = target.take_ref().unwrap();
                    args.push(obj_ref);
                }
            }
            _ => {
                // func(..., ...)
                for expr in first_arg.into_iter().chain(args_expr.iter()) {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }
                args.extend(curry.iter_mut());
            }
        }

        args.extend(arg_values.iter_mut());

        // Convert named_arg_values to the format expected by exec_fn_call_with_named
        #[cfg(feature = "default-parameters")]
        let named_args_dyn: FnArgsVec<_> = named_arg_values
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        #[cfg(feature = "default-parameters")]
        let named_args_slice: &[(ImmutableString, Dynamic)] = &named_args_dyn;
        #[cfg(not(feature = "default-parameters"))]
        let named_args_slice: &[(ImmutableString, Dynamic)] = &[];

        #[cfg(all(not(feature = "no_function"), feature = "default-parameters"))]
        {
            self.exec_fn_call_with_named(
                global,
                caches,
                None,
                fn_name,
                op_token,
                hashes,
                &mut args,
                is_ref_mut,
                false,
                named_args_slice,
                pos,
            )
            .map(|(v, ..)| v)
        }
        #[cfg(all(not(feature = "no_function"), not(feature = "default-parameters")))]
        {
            self.exec_fn_call(
                global,
                caches,
                None,
                fn_name,
                op_token,
                hashes,
                &mut args,
                is_ref_mut,
                false,
                pos,
            )
            .map(|(v, ..)| v)
        }
        #[cfg(feature = "no_function")]
        {
            // When no_function is enabled, named arguments are not supported
            // Just call exec_fn_call which will handle native functions
            self.exec_fn_call(
                global,
                caches,
                None,
                fn_name,
                op_token,
                hashes,
                &mut args,
                is_ref_mut,
                false,
                pos,
            )
            .map(|(v, ..)| v)
        }
    }

    /// Call a namespace-qualified function in normal function-call style.
    #[cfg(not(feature = "no_module"))]
    pub(crate) fn make_qualified_function_call(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        mut this_ptr: Option<&mut Dynamic>,
        namespace: &crate::ast::Namespace,
        fn_name: &str,
        args_expr: &[Expr],
        #[cfg(feature = "default-parameters")]
        named_args: &[(ImmutableString, Expr)],
        #[cfg(not(feature = "default-parameters"))]
        _named_args: &[(ImmutableString, Expr)],
        hash: u64,
        pos: Position,
    ) -> RhaiResult {
        // Evaluate named arguments first
        #[cfg(feature = "default-parameters")]
        let mut named_arg_values = FnArgsVec::with_capacity(named_args.len());
        #[cfg(feature = "default-parameters")]
        for (param_name, expr) in named_args {
            let (value, ..) =
                self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
            named_arg_values.push((param_name.clone(), value.flatten()));
        }

        let mut arg_values = FnArgsVec::with_capacity(args_expr.len());
        let args = &mut FnArgsVec::with_capacity(args_expr.len());
        let mut first_arg_value = None;

        #[cfg(not(feature = "no_closure"))]
        let has_non_shared_this_ptr = this_ptr.as_ref().map_or(false, |v| !v.is_shared());
        #[cfg(feature = "no_closure")]
        let has_non_shared_this_ptr = this_ptr.is_some();

        // See if the first argument is a variable.
        // If so, convert to method-call style in order to leverage potential
        // &mut first argument and avoid cloning the value.
        match args_expr.first() {
            Some(_first @ Expr::ThisPtr(pos)) if has_non_shared_this_ptr => {
                self.track_operation(global, *pos)?;

                #[cfg(feature = "debugging")]
                self.dbg(global, caches, scope, this_ptr.as_deref_mut(), _first)?;

                // The first value is a placeholder (for later if it needs to be cloned)
                arg_values.push(Dynamic::UNIT);

                for expr in args_expr.iter().skip(1) {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }

                // func(x, ...) -> x.func(...)
                let (first, rest) = arg_values.split_first_mut().unwrap();
                first_arg_value = Some(first);
                args.push(this_ptr.unwrap());
                args.extend(rest.iter_mut());
            }
            Some(first @ Expr::Variable(.., pos)) => {
                self.track_operation(global, *pos)?;

                #[cfg(feature = "debugging")]
                self.dbg(global, caches, scope, this_ptr.as_deref_mut(), first)?;

                // The first value is a placeholder (for later if it needs to be cloned)
                arg_values.push(Dynamic::UNIT);

                for expr in args_expr.iter().skip(1) {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }

                let target = self.search_namespace(global, caches, scope, this_ptr, first)?;

                if target.is_shared() || target.is_temp_value() {
                    arg_values[0] = target.take_or_clone().flatten();
                    args.extend(arg_values.iter_mut());
                } else {
                    // Turn it into a method call only if the object is not shared and not a simple value
                    // func(x, ...) -> x.func(...)
                    let (first, rest) = arg_values.split_first_mut().unwrap();
                    first_arg_value = Some(first);
                    let obj_ref = target.take_ref().unwrap();
                    args.push(obj_ref);
                    args.extend(rest.iter_mut());
                }
            }
            Some(_) => {
                // func(..., ...) or func(mod::x, ...)
                for expr in args_expr {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }
                args.extend(arg_values.iter_mut());
            }
            None => (),
        }

        // Search for the root namespace
        let module = self
            .search_imports(global, namespace)
            .ok_or_else(|| ERR::ErrorModuleNotFound(namespace.to_string(), namespace.position()))?;

        // First search script-defined functions in namespace (can override built-in)
        let mut func = module.get_qualified_fn(hash).or_else(|| {
            // Then search native Rust functions
            let hash_qualified_fn = calc_fn_hash_full(hash, args.iter().map(|a| a.type_id()));
            module.get_qualified_fn(hash_qualified_fn)
        });

        // Check for `Dynamic` parameters.
        //
        // Note - This is done during every function call mismatch without cache,
        //        so hopefully the number of arguments should not be too many
        //        (expected because closures cannot be qualified).
        if func.is_none() && !args.is_empty() {
            let num_args = args.len();
            let max_dynamic_count = usize::min(num_args, MAX_DYNAMIC_PARAMETERS);
            let max_bitmask = 1usize << max_dynamic_count;
            let mut bitmask = 1usize; // Bitmask of which parameter to replace with `Dynamic`

            // Try all permutations with `Dynamic` wildcards
            while bitmask < max_bitmask {
                let hash_qualified_fn = calc_fn_hash_full(
                    hash,
                    args.iter().enumerate().map(|(i, a)| {
                        if i < max_dynamic_count
                            && bitmask & (1usize << (max_dynamic_count - i - 1)) != 0
                        {
                            // Replace with `Dynamic`
                            TypeId::of::<Dynamic>()
                        } else {
                            a.type_id()
                        }
                    }),
                );

                if let Some(f) = module.get_qualified_fn(hash_qualified_fn) {
                    func = Some(f);
                    break;
                }

                bitmask += 1;
            }
        }

        // Clone first argument if the function is not a method after-all
        if !func.map_or(true, RhaiFunc::is_method) {
            if let Some(first) = first_arg_value {
                *first = args[0].clone();
                args[0] = first;
            }
        }

        defer! { let orig_level = global.level; global.level += 1 }

        match func {
            #[cfg(not(feature = "no_function"))]
            Some(RhaiFunc::Script { fn_def, env }) => {
                let env = env.as_deref();
                let scope = &mut Scope::new();

                let orig_source = mem::replace(&mut global.source, module.id_raw().cloned());
                defer! { global => move |g| g.source = orig_source }

                #[cfg(feature = "default-parameters")]
                {
                    // Check if we have named args or if the function has defaults
                    let has_named_args = !named_arg_values.is_empty();
                    let has_defaults = fn_def.defaults.iter().any(|d| d.is_some());
                    
                    if has_named_args || has_defaults {
                        // Use arg plan for script functions with named args or defaults
                        let positional_args: FnArgsVec<Dynamic> = args.iter().map(|a| (*a).clone()).collect();
                        let named_args_dyn: &[(ImmutableString, Dynamic)] = &named_arg_values;
                        let arg_plan = build_arg_plan(fn_def, &positional_args, named_args_dyn, pos)?;
                        
                        return self.call_script_fn_with_plan(
                            global, caches, scope, None, env, fn_def, arg_plan, true, pos,
                        );
                    }
                }
                
                // Fall back to regular call if no named args or defaults
                self.call_script_fn(global, caches, scope, None, env, fn_def, args, true, pos)
            }

            Some(f) if !f.is_pure() && !args.is_empty() && args[0].is_read_only() => {
                // If function is not pure, there must be at least one argument
                Err(ERR::ErrorNonPureMethodCallOnConstant(fn_name.to_string(), pos).into())
            }

            Some(RhaiFunc::Plugin { func }) => {
                // Native plugin functions don't support named arguments
                #[cfg(feature = "default-parameters")]
                if !named_arg_values.is_empty() {
                    return Err(ERR::ErrorFunctionNotFound(
                        format!(
                            "{namespace}{}{} (named arguments not supported for native functions)",
                            crate::engine::NAMESPACE_SEPARATOR,
                            fn_name
                        ),
                        pos,
                    )
                    .into());
                }
                
                let context = func
                    .has_context()
                    .then(|| (self, fn_name, module.id(), &*global, pos).into());
                func.call(context, args)
                    .and_then(|r| self.check_data_size(r, pos))
            }

            Some(
                RhaiFunc::Pure {
                    func, has_context, ..
                }
                | RhaiFunc::Method {
                    func, has_context, ..
                },
            ) => {
                // Native functions don't support named arguments
                #[cfg(feature = "default-parameters")]
                if !named_arg_values.is_empty() {
                    return Err(ERR::ErrorFunctionNotFound(
                        format!(
                            "{namespace}{}{} (named arguments not supported for native functions)",
                            crate::engine::NAMESPACE_SEPARATOR,
                            fn_name
                        ),
                        pos,
                    )
                    .into());
                }
                
                let context =
                    has_context.then(|| (self, fn_name, module.id(), &*global, pos).into());
                func(context, args).and_then(|r| self.check_data_size(r, pos))
            }

            Some(RhaiFunc::Iterator { .. }) => {
                unreachable!("iterator functions should not occur here")
            }

            None => Err(ERR::ErrorFunctionNotFound(
                if namespace.is_empty() {
                    self.gen_fn_call_signature(fn_name, args)
                } else {
                    format!(
                        "{namespace}{}{}",
                        crate::engine::NAMESPACE_SEPARATOR,
                        self.gen_fn_call_signature(fn_name, args)
                    )
                },
                pos,
            )
            .into())
        }
    }

    /// Evaluate a text script in place - used primarily for 'eval'.
    pub(crate) fn eval_script_expr_in_place(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        script: &str,
        _pos: Position,
    ) -> RhaiResult {
        self.track_operation(global, _pos)?;

        let script = script.trim();

        if script.is_empty() {
            return Ok(Dynamic::UNIT);
        }

        // Compile the script text
        // No optimizations because we only run it once
        let ast = self.compile_scripts_with_scope_raw(
            None,
            [script],
            #[cfg(not(feature = "no_optimize"))]
            crate::OptimizationLevel::None,
        )?;

        // If new functions are defined within the eval string, it is an error
        #[cfg(not(feature = "no_function"))]
        if ast.has_functions() {
            return Err(crate::PERR::WrongFnDefinition.into());
        }

        let statements = ast.statements();
        if statements.is_empty() {
            return Ok(Dynamic::UNIT);
        }

        // Evaluate the AST
        self.eval_global_statements(global, caches, scope, statements, false)
    }

    /// # Main Entry-Point (`FnCallExpr`)
    ///
    /// Evaluate a function call expression.
    ///
    /// This method tries to short-circuit function resolution under Fast Operators mode if the
    /// function call is an operator.
    pub(crate) fn eval_fn_call_expr(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        mut this_ptr: Option<&mut Dynamic>,
        expr: &FnCallExpr,
        pos: Position,
    ) -> RhaiResult {
        let FnCallExpr {
            #[cfg(not(feature = "no_module"))]
            namespace,
            name,
            hashes,
            args,
            #[cfg(feature = "default-parameters")]
            named_args,
            op_token,
            capture_parent_scope: capture,
            ..
        } = expr;
        #[cfg(not(feature = "default-parameters"))]
        let named_args = &[];

        let op_token = op_token.as_ref();

        // Short-circuit native unary operator call if under Fast Operators mode
        if self.fast_operators() && args.len() == 1 && op_token == Some(&Token::Bang) {
            let mut value = self
                .get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), &args[0])?
                .0
                .flatten();

            return if let Union::Bool(b, ..) = value.0 {
                Ok((!b).into())
            } else {
                let operand = &mut [&mut value];
                self.exec_fn_call(
                    global, caches, None, name, op_token, *hashes, operand, false, false, pos,
                )
                .map(|(v, ..)| v)
            };
        }

        // Short-circuit native binary operator call if under Fast Operators mode
        if self.fast_operators() && args.len() == 2 && op_token.is_some() {
            let mut lhs = self
                .get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), &args[0])?
                .0
                .flatten();

            let mut rhs = self
                .get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), &args[1])?
                .0
                .flatten();

            if lhs.is_variant() || rhs.is_variant() {
                // For custom types, give registered functions a chance to run first before considering the built-in fallback
            } else {
                // For other types, try to get a built-in
                if let Some((func, need_context)) =
                    get_builtin_binary_op_fn(op_token.unwrap(), &lhs, &rhs)
                {
                    // We may not need to bump the level because built-in's do not need it.
                    //defer! { let orig_level = global.level; global.level += 1 }

                    let context =
                        need_context.then(|| (self, name.as_str(), None, &*global, pos).into());
                    return func(context, &mut [&mut lhs, &mut rhs]);
                }
            }

            let operands = &mut [&mut lhs, &mut rhs];

            return self
                .exec_fn_call(
                    global, caches, None, name, op_token, *hashes, operands, false, false, pos,
                )
                .map(|(v, ..)| v);
        }

        #[cfg(not(feature = "no_module"))]
        if !namespace.is_empty() {
            // Qualified function call
            let hash = hashes.native();

            return self.make_qualified_function_call(
                global, caches, scope, this_ptr, namespace, name, args, named_args, hash, pos,
            );
        }

        // Normal function call
        // First check if the function name is a variable containing a FnPtr
        // This handles cases like: let f = |a, b = 2| { a + b }; f(1)
        if op_token.is_none() {
            // Clone the FnPtr value to avoid borrow checker issues
            let fn_ptr_opt = scope
                .get(name)
                .and_then(|v| v.read_lock::<FnPtr>().map(|fp| fp.clone()));
            if let Some(fn_ptr) = fn_ptr_opt {
                // It's a FnPtr - extract it and handle as FnPtr call

                // Evaluate arguments (now scope is not borrowed)
                let mut arg_values = FnArgsVec::with_capacity(args.len());
                for expr in args {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }

                // Evaluate named arguments
                #[cfg(feature = "default-parameters")]
                let mut named_arg_values = FnArgsVec::new();
                #[cfg(feature = "default-parameters")]
                for (param_name, expr) in named_args {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    named_arg_values.push((param_name.clone(), value.flatten()));
                }
                #[cfg(not(feature = "default-parameters"))]
                let named_arg_values: FnArgsVec<(ImmutableString, Dynamic)> = FnArgsVec::new();

                // Handle FnPtr call with defaults
                let FnPtr {
                    name: fn_ptr_name,
                    curry: extra_curry,
                    #[cfg(not(feature = "no_function"))]
                    env,
                    typ,
                } = fn_ptr;

                match typ {
                    #[cfg(all(not(feature = "no_function"), feature = "default-parameters"))]
                    FnPtrType::Script(fn_def) => {
                        let num_provided = extra_curry.len() + arg_values.len();
                        let num_params = fn_def.params.len();
                        let min_args = fn_def
                            .defaults
                            .iter()
                            .position(|d| d.is_some())
                            .unwrap_or(num_params);

                        if num_provided >= min_args && num_provided <= num_params {
                            // Combine curried args with provided args
                            let mut all_args =
                                FnArgsVec::with_capacity(extra_curry.len() + arg_values.len());
                            all_args.extend(extra_curry.iter().cloned());
                            all_args.extend(arg_values);

                            // Build argument plan with defaults
                            let arg_plan =
                                build_arg_plan(&fn_def, &all_args, &named_arg_values, pos)?;

                            let scope = &mut Scope::new();
                            let env = env.as_deref();

                            defer! { let orig_level = global.level; global.level += 1 }

                            return self.call_script_fn_with_plan(
                                global, caches, scope, None, env, &fn_def, arg_plan, true, pos,
                            );
                        }
                    }
                    #[cfg(all(not(feature = "no_function"), not(feature = "default-parameters")))]
                    FnPtrType::Script(fn_def) if fn_def.params.len() == extra_curry.len() + arg_values.len() => {
                        // When feature is disabled, only allow exact argument count match
                        // Combine curried args with provided args
                        let mut all_args =
                            FnArgsVec::with_capacity(extra_curry.len() + arg_values.len());
                        all_args.extend(extra_curry.iter().cloned());
                        all_args.extend(arg_values);
                        let args = &mut all_args.iter_mut().collect::<FnArgsVec<_>>();

                        let scope = &mut Scope::new();
                        let env = env.as_deref();

                        defer! { let orig_level = global.level; global.level += 1 }

                        return self.call_script_fn(
                            global, caches, scope, None, env, &fn_def, args, true, pos,
                        );
                    }
                    _ => {
                        // For other FnPtr types, fall through to normal handling
                        // by redirecting to the FnPtr's function name
                        let (first_arg, rest_args) = args.split_first().map_or_else(
                            || (None, args.as_ref()),
                            |(first, rest)| (Some(first), rest),
                        );

                        return self.make_function_call(
                            global,
                            caches,
                            scope,
                            this_ptr,
                            &fn_ptr_name,
                            op_token,
                            first_arg,
                            rest_args,
                            named_args,
                            FnCallHashes::from_hash(calc_fn_hash(
                                None,
                                &fn_ptr_name,
                                extra_curry.len() + args.len(),
                            )),
                            *capture,
                            pos,
                        );
                    }
                }
            }
        }

        let (first_arg, rest_args) = args.split_first().map_or_else(
            || (None, args.as_ref()),
            |(first, rest)| (Some(first), rest),
        );

        self.make_function_call(
            global, caches, scope, this_ptr, name, op_token, first_arg, rest_args, named_args,
            *hashes, *capture, pos,
        )
    }
}
