//! Implement function-calling mechanism for [`Engine`].

use super::{get_builtin_binary_op_fn, get_builtin_op_assignment_fn, RhaiFunc};
use crate::api::default_limits::MAX_DYNAMIC_PARAMETERS;
use crate::engine::{
    KEYWORD_DEBUG, KEYWORD_EVAL, KEYWORD_FN_PTR, KEYWORD_FN_PTR_CALL, KEYWORD_FN_PTR_CURRY,
    KEYWORD_IS_DEF_VAR, KEYWORD_PRINT, KEYWORD_TYPE_OF,
};
#[cfg(feature = "internals")]
use crate::eval::EvalContext;
use crate::eval::{Caches, FnResolutionCacheEntry, GlobalRuntimeState};
use crate::types::Token;
use crate::{
    calc_fn_hash_full, Dynamic, Engine, FnArgsVec, ImmutableString, Position, RhaiResultOf, Scope,
    Shared, ERR,
};
#[cfg(feature = "no_std")]
use hashbrown::hash_map::Entry;
#[cfg(not(feature = "no_std"))]
use std::collections::hash_map::Entry;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::{type_name, TypeId},
    fmt, mem,
};

/// Arguments to a function call, which is a list of [`&mut Dynamic`][Dynamic].
pub type FnCallArgs<'a> = [&'a mut Dynamic];

/// _(internals)_ A set of function call hashes. Exported under the `internals` feature only.
///
/// Two separate hashes are pre-calculated because of the following patterns:
///
/// ```rhai
/// func(a, b, c);      // Native: func(a, b, c)        - 3 parameters
///                     // Script: func(a, b, c)        - 3 parameters
///
/// a.func(b, c);       // Native: func(&mut a, b, c)   - 3 parameters
///                     // Script: func(b, c)           - 2 parameters
/// ```
///
/// For normal function calls, the native hash equals the script hash.
///
/// For method-style calls, the script hash contains one fewer parameter.
///
/// Function call hashes are used in the following manner:
///
/// * First, the script hash (if any) is tried, which contains only the called function's name plus
///   the number of parameters.
///
/// * Next, the actual types of arguments are hashed and _combined_ with the native hash, which is
///   then used to search for a native function.
///
///   In other words, a complete native function call hash always contains the called function's
///   name plus the types of the arguments.  This is due to possible function overloading for
///   different parameter types.
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub struct FnCallHashes {
    /// Pre-calculated hash for a script-defined function ([`None`] if native functions only).
    #[cfg(not(feature = "no_function"))]
    script: Option<u64>,
    /// Pre-calculated hash for a native Rust function with no parameter types.
    native: u64,
}

impl fmt::Debug for FnCallHashes {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(not(feature = "no_function"))]
        return match self.script {
            Some(script) if script == self.native => fmt::Debug::fmt(&self.native, f),
            Some(script) => write!(f, "({script}, {})", self.native),
            None => write!(f, "{} (native only)", self.native),
        };

        #[cfg(feature = "no_function")]
        return write!(f, "{}", self.native);
    }
}

impl FnCallHashes {
    /// Create a [`FnCallHashes`] from a single hash.
    #[inline]
    #[must_use]
    pub const fn from_hash(hash: u64) -> Self {
        Self {
            #[cfg(not(feature = "no_function"))]
            script: Some(hash),
            native: hash,
        }
    }
    /// Create a [`FnCallHashes`] with only the native Rust hash.
    #[cfg(any(feature = "no_function", not(feature = "no_ast")))]
    #[inline]
    #[must_use]
    pub const fn from_native_only(hash: u64) -> Self {
        Self {
            #[cfg(not(feature = "no_function"))]
            script: None,
            native: hash,
        }
    }
    /// Create a [`FnCallHashes`] with both script function and native Rust hashes.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    #[must_use]
    pub const fn from_script_and_native(script: u64, native: u64) -> Self {
        Self {
            script: Some(script),
            native,
        }
    }
    /// Is this [`FnCallHashes`] native-only?
    #[inline(always)]
    #[must_use]
    pub const fn is_native_only(&self) -> bool {
        #[cfg(not(feature = "no_function"))]
        return self.script.is_none();
        #[cfg(feature = "no_function")]
        return true;
    }
    /// Get the native hash.
    ///
    /// The hash returned is never zero.
    #[inline(always)]
    #[must_use]
    pub const fn native(&self) -> u64 {
        self.native
    }
    /// Get the script hash.
    ///
    /// The hash returned is never zero.
    ///
    /// # Panics
    ///
    /// Panics if this [`FnCallHashes`] is native-only.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    #[must_use]
    pub fn script(&self) -> u64 {
        self.script.expect("native-only hash")
    }
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
            mem::transmute::<&mut Dynamic, &'a mut Dynamic>(&mut self.value_copy)
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
#[cfg(not(feature = "unchecked"))]
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

impl Engine {
    /// Generate the signature for a function call.
    #[inline]
    #[must_use]
    pub(crate) fn gen_fn_call_signature(&self, fn_name: &str, args: &[&mut Dynamic]) -> String {
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

    /// Implement built-in syntactic functions.
    ///
    /// These are functions (e.g. `type_of`, `is_shared`) that are not registered as normal
    /// functions but provided by Rhai.
    pub(crate) fn exec_syntactic_fn_call(
        &self,
        _global: &mut GlobalRuntimeState,
        _caches: &mut Caches,
        fn_name: &str,
        args: &FnCallArgs,
        pos: Position,
    ) -> RhaiResultOf<Option<Dynamic>> {
        match fn_name {
            // Handle type_of()
            KEYWORD_TYPE_OF if args.len() == 1 => {
                let typ = self.get_interned_string(self.map_type_name(args[0].type_name()));
                return Ok(Some(typ.into()));
            }
            KEYWORD_TYPE_OF => (),

            // Handle is_shared()
            #[cfg(not(feature = "no_closure"))]
            crate::engine::KEYWORD_IS_SHARED if args.len() == 1 => {
                return Ok(Some(args[0].is_shared().into()))
            }
            #[cfg(not(feature = "no_closure"))]
            crate::engine::KEYWORD_IS_SHARED => (),

            // Handle is_def_fn()
            #[cfg(not(feature = "no_function"))]
            crate::engine::KEYWORD_IS_DEF_FN if args.len() == 2 => {
                use std::convert::TryFrom;

                let fn_name = args[0]
                    .as_immutable_string_ref()
                    .map_err(|typ| self.make_type_mismatch_err::<ImmutableString>(typ, pos))?;
                let num_params = args[1]
                    .as_int()
                    .map_err(|typ| self.make_type_mismatch_err::<crate::INT>(typ, pos))?;

                return Ok(Some(
                    usize::try_from(num_params)
                        .map(|num_params| {
                            let hash_script = crate::calc_fn_hash(None, &fn_name, num_params);
                            self.has_script_fn(_global, _caches, hash_script).into()
                        })
                        .unwrap_or(Dynamic::FALSE),
                ));
            }
            #[cfg(not(feature = "no_function"))]
            #[cfg(not(feature = "no_object"))]
            crate::engine::KEYWORD_IS_DEF_FN if args.len() == 3 => {
                let this_type = args[0]
                    .as_immutable_string_ref()
                    .map_err(|typ| self.make_type_mismatch_err::<ImmutableString>(typ, pos))?;
                let fn_name = args[1]
                    .as_immutable_string_ref()
                    .map_err(|typ| self.make_type_mismatch_err::<ImmutableString>(typ, pos))?;
                let num_params = args[2]
                    .as_int()
                    .map_err(|typ| self.make_type_mismatch_err::<crate::INT>(typ, pos))?;

                return Ok(Some(
                    usize::try_from(num_params)
                        .map(|num_params| {
                            let hash_script = crate::calc_typed_method_hash(
                                crate::calc_fn_hash(None, &fn_name, num_params),
                                &this_type,
                            );
                            self.has_script_fn(_global, _caches, hash_script).into()
                        })
                        .unwrap_or(Dynamic::FALSE),
                ));
            }
            #[cfg(not(feature = "no_function"))]
            crate::engine::KEYWORD_IS_DEF_FN => (),

            // Other syntactic functions
            KEYWORD_IS_DEF_VAR | KEYWORD_FN_PTR | KEYWORD_EVAL | KEYWORD_FN_PTR_CALL
            | KEYWORD_FN_PTR_CURRY => (),

            // Normal functions
            _ => return Ok(None),
        }

        Err(ERR::ErrorFunctionNotFound(self.gen_fn_call_signature(fn_name, args), pos).into())
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
    /// Function call arguments may be _consumed_ when the function requires them to be passed by value.
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

        // Check for data race.
        #[cfg(not(feature = "no_closure"))]
        #[cfg(not(feature = "unchecked"))]
        ensure_no_data_race(name, args, is_ref_mut)?;

        if let Some(result) = self.exec_syntactic_fn_call(global, caches, name, args, pos)? {
            return Ok((result, false));
        }

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
                f if !f.is_pure() && args.get(0).map_or(false, |v| v.is_read_only()) => {
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
        //
        // Note: we cannot use `assert!` here (e.g. to check for number of arguments) because
        //       this function may be called from the VM running a corrupted bytecodes stream!

        match name {
            // index getter function not found?
            #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
            crate::engine::FN_IDX_GET => Err(if args.len() != 2 {
                ERR::ErrorParsing(
                    crate::ParseErrorType::MalformedIndexExpr(format!(
                        "System error: {} argument(s) found for indexer (should be 2)",
                        args.len()
                    )),
                    pos,
                )
                .into()
            } else {
                let t0 = self.map_type_name(args[0].type_name());
                let t1 = self.map_type_name(args[1].type_name());
                ERR::ErrorIndexingType(format!("{t0} [{t1}]"), pos).into()
            }),

            // index setter function not found?
            #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
            crate::engine::FN_IDX_SET => Err(if args.len() != 3 {
                ERR::ErrorParsing(
                    crate::ParseErrorType::MalformedIndexExpr(format!(
                        "System error: {} argument(s) found for index setter (should be 3)",
                        args.len()
                    )),
                    pos,
                )
                .into()
            } else {
                let t0 = self.map_type_name(args[0].type_name());
                let t1 = self.map_type_name(args[1].type_name());
                let t2 = self.map_type_name(args[2].type_name());
                ERR::ErrorIndexingType(format!("{t0} [{t1}] = {t2}"), pos).into()
            }),

            // Getter function not found?
            #[cfg(not(feature = "no_object"))]
            _ if name.starts_with(crate::engine::FN_GET) => {
                let prop = &name[crate::engine::FN_SET.len()..];
                Err(if args.len() != 1 {
                    ERR::ErrorParsing(
                    crate::ParseErrorType::MalformedIndexExpr(format!(
                        "System error: {} argument(s) found for property getter '{prop}' (should be 1)",
                        args.len()
                    )),
                    pos,
                )
                .into()
                } else {
                    let t0 = self.map_type_name(args[0].type_name());
                    ERR::ErrorDotExpr(
                        format!(
                        "Unknown property '{prop}' - a getter is not registered for type '{t0}'"
                    ),
                        pos,
                    )
                    .into()
                })
            }

            // Setter function not found?
            #[cfg(not(feature = "no_object"))]
            _ if name.starts_with(crate::engine::FN_SET) => {
                let prop = &name[crate::engine::FN_SET.len()..];
                Err(if args.len() != 2 {
                    ERR::ErrorParsing(
                    crate::ParseErrorType::MalformedIndexExpr(format!(
                        "System error: {} argument(s) found for property setter '{prop}' (should be 2)",
                        args.len()
                    )),
                    pos,
                )
                .into()
                } else {
                    let t0 = self.map_type_name(args[0].type_name());
                    let t1 = self.map_type_name(args[1].type_name());
                    ERR::ErrorDotExpr(
                    format!(
                        "No writable property '{prop}' - a setter is not registered for type '{t0}' to handle '{t1}'"
                    ),
                    pos,
                )
                .into()
                })
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
        mut _scope: Option<&mut Scope>,
        fn_name: &str,
        op_token: Option<&Token>,
        hashes: FnCallHashes,
        args: &mut FnCallArgs,
        is_ref_mut: bool,
        _is_method_call: bool,
        pos: Position,
    ) -> RhaiResultOf<(Dynamic, bool)> {
        // Check for data race.
        #[cfg(not(feature = "no_closure"))]
        #[cfg(not(feature = "unchecked"))]
        ensure_no_data_race(fn_name, args, is_ref_mut)?;

        // These may be redirected from method style calls.
        if let Some(result) = self.exec_syntactic_fn_call(global, caches, fn_name, args, pos)? {
            return Ok((result, false));
        }

        defer! { let orig_level = global.level; global.level += 1 }

        // Script-defined function call?
        #[cfg(not(feature = "no_function"))]
        if !hashes.is_native_only() {
            let hash = hashes.script();
            let local_entry = &mut None;
            let mut resolved = None;

            #[cfg(not(feature = "no_object"))]
            if _is_method_call && !args.is_empty() {
                let typed_hash =
                    crate::calc_typed_method_hash(hash, self.map_type_name(args[0].type_name()));
                resolved =
                    self.resolve_fn(global, caches, local_entry, None, typed_hash, None, false);
            }

            if resolved.is_none() {
                resolved = self.resolve_fn(global, caches, local_entry, None, hash, None, false);
            }

            if let Some(FnResolutionCacheEntry { func, source }) = resolved.cloned() {
                let RhaiFunc::Script { fn_def, env } = func else {
                    unreachable!("Script function expected");
                };

                let fn_def = &*fn_def;
                let env = env.as_deref();

                // Short-circuit empty script function body
                match fn_def.body {
                    #[cfg(not(feature = "no_ast"))]
                    crate::func::ScriptFuncPayload::Statements(ref block) => {
                        if block.is_empty() {
                            return Ok((Dynamic::UNIT, false));
                        }
                    }
                    // We don't know about Grain functions, so call it anyway
                    #[cfg(feature = "grain")]
                    crate::func::ScriptFuncPayload::GrainVM { .. } => (),
                }

                // Make empty scope for function call
                let mut empty_scope;
                let scope = if let Some(scope) = _scope {
                    scope
                } else {
                    empty_scope = Scope::new();
                    &mut empty_scope
                };

                // Swap source
                let orig_source = mem::replace(&mut global.source, source);
                defer! { global => move |g| g.source = orig_source }
                let global = global.into();

                // Execute function call
                return if _is_method_call {
                    // Method call of script function - map first argument to `this`
                    let (first_arg, args) = args.split_first_mut().unwrap();
                    let this_ptr = Some(&mut **first_arg);
                    self.call_script_fn(
                        global, caches, scope, this_ptr, env, fn_def, args, true, pos,
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

        let result = self.exec_native_fn_call(
            global, caches, fn_name, op_token, hash, args, is_ref_mut, false, pos,
        );

        #[cfg(feature = "internals")]
        if result.is_err() {
            if let Some(ref callback) = self.missing_function {
                let mut empty_scope;
                let scope = match _scope {
                    Some(ref mut s) => &mut **s,
                    None => {
                        empty_scope = Scope::new();
                        &mut empty_scope
                    }
                };
                let context = EvalContext::new(self, global, caches, scope, None);
                match callback(fn_name, args, _is_method_call, context) {
                    Ok(Some(value)) => return Ok((value, false)),
                    Ok(None) => {}
                    Err(err) => return Err(err),
                }
            }
        }

        result
    }

    /// Call a namespace-qualified function directly with pre-evaluated arguments.
    #[cfg(not(feature = "no_module"))]
    #[cfg(feature = "grain")]
    pub(crate) fn exec_qualified_fn_call(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        namespace_str: &str,
        fn_name: &str,
        args: &mut [&mut Dynamic],
        hash: u64,
        pos: Position,
    ) -> crate::RhaiResult {
        let root = match namespace_str.split_once(crate::engine::NAMESPACE_SEPARATOR) {
            Some((root, _)) => root,
            None => namespace_str,
        };

        let module = global
            .find_import(root)
            .map_or_else(
                || self.global_sub_modules.get(root).cloned(),
                |offset| global.get_shared_import(offset),
            )
            .ok_or_else(|| ERR::ErrorModuleNotFound(namespace_str.to_string(), pos))?;

        let namespace = namespace_str
            .split(crate::engine::NAMESPACE_SEPARATOR)
            .collect::<crate::StaticVec<_>>();

        self.exec_qualified_fn_call_raw(
            global, caches, &module, &namespace, fn_name, None, args, hash, pos,
        )
    }

    /// Shared core logic for calling a qualified function given its module.
    #[cfg(not(feature = "no_module"))]
    pub(crate) fn exec_qualified_fn_call_raw<'a, 's>(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        module: &crate::SharedModule,
        namespace: &[&'s str],
        fn_name: &str,
        first_arg_value: Option<&'a mut Dynamic>,
        args: &mut [&'a mut Dynamic],
        hash: u64,
        pos: Position,
    ) -> crate::RhaiResult {
        use super::RhaiFunc;

        // First search script-defined functions in namespace (can override built-in)
        let mut func = module.get_qualified_fn(hash).or_else(|| {
            // Then search native Rust functions
            let hash_qualified_fn =
                super::calc_fn_hash_full(hash, args.iter().map(|a| a.type_id()));
            module.get_qualified_fn(hash_qualified_fn)
        });

        // Check for `Dynamic` parameters.
        if func.is_none() && !args.is_empty() {
            let num_args = args.len();
            let max_dynamic_count =
                usize::min(num_args, crate::api::default_limits::MAX_DYNAMIC_PARAMETERS);
            let max_bitmask = 1usize << max_dynamic_count;
            let mut bitmask = 1usize;

            while bitmask < max_bitmask {
                let hash_qualified_fn = super::calc_fn_hash_full(
                    hash,
                    args.iter().enumerate().map(|(i, a)| {
                        if i < max_dynamic_count
                            && bitmask & (1usize << (max_dynamic_count - i - 1)) != 0
                        {
                            std::any::TypeId::of::<Dynamic>()
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

                let orig_source = std::mem::replace(&mut global.source, module.id_raw().cloned());
                defer! { global => move |g| g.source = orig_source }
                let global = global.into();

                self.call_script_fn(global, caches, scope, None, env, fn_def, args, true, pos)
            }

            Some(f) if !f.is_pure() && args[0].is_read_only() => {
                Err(ERR::ErrorNonPureMethodCallOnConstant(fn_name.to_string(), pos).into())
            }

            Some(RhaiFunc::Plugin { func }) => {
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
                        "{}{}{}",
                        namespace
                            .iter()
                            .map(|&s| s)
                            .collect::<crate::StaticVec<_>>()
                            .join(crate::engine::NAMESPACE_SEPARATOR),
                        crate::engine::NAMESPACE_SEPARATOR,
                        self.gen_fn_call_signature(fn_name, args)
                    )
                },
                pos,
            )
            .into()),
        }
    }
}
