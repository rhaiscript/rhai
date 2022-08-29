//! Implement script function-calling mechanism for [`Engine`].
#![cfg(not(feature = "no_function"))]

use super::call::FnCallArgs;
use crate::ast::ScriptFnDef;
use crate::eval::{Caches, GlobalRuntimeState};
use crate::{Dynamic, Engine, Module, Position, RhaiError, RhaiResult, Scope, ERR};
use std::mem;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

impl Engine {
    /// # Main Entry-Point
    ///
    /// Call a script-defined function.
    ///
    /// If `rewind_scope` is `false`, arguments are removed from the scope but new variables are not.
    ///
    /// # WARNING
    ///
    /// Function call arguments may be _consumed_ when the function requires them to be passed by value.
    /// All function arguments not in the first position are always passed by value and thus consumed.
    ///
    /// **DO NOT** reuse the argument values unless for the first `&mut` argument - all others are silently replaced by `()`!
    pub(crate) fn call_script_fn(
        &self,
        scope: &mut Scope,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        fn_def: &ScriptFnDef,
        args: &mut FnCallArgs,
        rewind_scope: bool,
        pos: Position,
        level: usize,
    ) -> RhaiResult {
        #[inline(never)]
        fn make_error(
            name: String,
            _fn_def: &ScriptFnDef,
            global: &GlobalRuntimeState,
            err: RhaiError,
            pos: Position,
        ) -> RhaiResult {
            #[cfg(not(feature = "no_module"))]
            let source = _fn_def
                .environ
                .as_ref()
                .and_then(|environ| environ.lib.id().map(str::to_string));
            #[cfg(feature = "no_module")]
            let source = None;

            Err(ERR::ErrorInFunctionCall(
                name,
                source.unwrap_or_else(|| global.source.to_string()),
                err,
                pos,
            )
            .into())
        }

        assert!(fn_def.params.len() == args.len());

        #[cfg(not(feature = "unchecked"))]
        self.inc_operations(&mut global.num_operations, pos)?;

        // Check for stack overflow
        #[cfg(not(feature = "unchecked"))]
        if level > self.max_call_levels() {
            return Err(ERR::ErrorStackOverflow(pos).into());
        }

        #[cfg(feature = "debugging")]
        if self.debugger.is_none() && fn_def.body.is_empty() {
            return Ok(Dynamic::UNIT);
        }
        #[cfg(not(feature = "debugging"))]
        if fn_def.body.is_empty() {
            return Ok(Dynamic::UNIT);
        }

        let orig_scope_len = scope.len();
        #[cfg(not(feature = "no_module"))]
        let orig_imports_len = global.num_imports();

        #[cfg(feature = "debugging")]
        let orig_call_stack_len = global.debugger.call_stack().len();

        // Put arguments into scope as variables
        scope.extend(fn_def.params.iter().cloned().zip(args.iter_mut().map(|v| {
            // Actually consume the arguments instead of cloning them
            mem::take(*v)
        })));

        // Push a new call stack frame
        #[cfg(feature = "debugging")]
        if self.debugger.is_some() {
            global.debugger.push_call_stack_frame(
                fn_def.name.clone(),
                scope.iter().skip(orig_scope_len).map(|(.., v)| v).collect(),
                global.source.clone(),
                pos,
            );
        }

        // Merge in encapsulated environment, if any
        let orig_fn_resolution_caches_len = caches.fn_resolution_caches_len();

        #[cfg(not(feature = "no_module"))]
        let mut lib_merged = crate::StaticVec::with_capacity(lib.len() + 1);

        #[cfg(not(feature = "no_module"))]
        let (lib, constants) = if let Some(ref environ) = fn_def.environ {
            let crate::ast::EncapsulatedEnviron {
                lib: fn_lib,
                imports,
                constants,
            } = environ.as_ref();

            imports
                .iter()
                .cloned()
                .for_each(|(n, m)| global.push_import(n, m));

            (
                if fn_lib.is_empty() {
                    lib
                } else {
                    caches.push_fn_resolution_cache();
                    lib_merged.push(&**fn_lib);
                    lib_merged.extend(lib.iter().copied());
                    &lib_merged
                },
                Some(mem::replace(&mut global.constants, constants.clone())),
            )
        } else {
            (lib, None)
        };

        #[cfg(feature = "debugging")]
        {
            let node = crate::ast::Stmt::Noop(fn_def.body.position());
            self.run_debugger(scope, global, lib, this_ptr, &node, level)?;
        }

        // Evaluate the function
        let mut _result = self
            .eval_stmt_block(
                scope,
                global,
                caches,
                lib,
                this_ptr,
                &fn_def.body,
                rewind_scope,
                level,
            )
            .or_else(|err| match *err {
                // Convert return statement to return value
                ERR::Return(x, ..) => Ok(x),
                // Error in sub function call
                ERR::ErrorInFunctionCall(name, src, err, ..) => {
                    let fn_name = if src.is_empty() {
                        format!("{name} < {}", fn_def.name)
                    } else {
                        format!("{name} @ '{src}' < {}", fn_def.name)
                    };
                    make_error(fn_name, fn_def, global, err, pos)
                }
                // System errors are passed straight-through
                mut err if err.is_system_exception() => {
                    err.set_position(pos);
                    Err(err.into())
                }
                // Other errors are wrapped in `ErrorInFunctionCall`
                _ => make_error(fn_def.name.to_string(), fn_def, global, err, pos),
            });

        #[cfg(feature = "debugging")]
        {
            let trigger = match global.debugger.status {
                crate::eval::DebuggerStatus::FunctionExit(n) => n >= level,
                crate::eval::DebuggerStatus::Next(.., true) => true,
                _ => false,
            };
            if trigger {
                let node = crate::ast::Stmt::Noop(fn_def.body.end_position().or_else(pos));
                let node = (&node).into();
                let event = match _result {
                    Ok(ref r) => crate::eval::DebuggerEvent::FunctionExitWithValue(r),
                    Err(ref err) => crate::eval::DebuggerEvent::FunctionExitWithError(err),
                };
                match self.run_debugger_raw(scope, global, lib, this_ptr, node, event, level) {
                    Ok(_) => (),
                    Err(err) => _result = Err(err),
                }
            }

            // Pop the call stack
            global.debugger.rewind_call_stack(orig_call_stack_len);
        }

        // Remove all local variables and imported modules
        if rewind_scope {
            scope.rewind(orig_scope_len);
        } else if !args.is_empty() {
            // Remove arguments only, leaving new variables in the scope
            scope.remove_range(orig_scope_len, args.len());
        }
        #[cfg(not(feature = "no_module"))]
        global.truncate_imports(orig_imports_len);

        // Restore constants
        #[cfg(not(feature = "no_module"))]
        if let Some(constants) = constants {
            global.constants = constants;
        }

        // Restore state
        caches.rewind_fn_resolution_caches(orig_fn_resolution_caches_len);

        _result
    }

    // Does a script-defined function exist?
    #[must_use]
    pub(crate) fn has_script_fn(
        &self,
        _global: Option<&GlobalRuntimeState>,
        caches: &mut Caches,
        lib: &[&Module],
        hash_script: u64,
    ) -> bool {
        let cache = caches.fn_resolution_cache_mut();

        if let Some(result) = cache.get(&hash_script).map(Option::is_some) {
            return result;
        }

        // First check script-defined functions
        let result = lib.iter().any(|&m| m.contains_fn(hash_script))
            // Then check the global namespace and packages
            || self.global_modules.iter().any(|m| m.contains_fn(hash_script));

        #[cfg(not(feature = "no_module"))]
        let result = result ||
            // Then check imported modules
            _global.map_or(false, |m| m.contains_qualified_fn(hash_script))
            // Then check sub-modules
            || self.global_sub_modules.values().any(|m| m.contains_qualified_fn(hash_script));

        if !result {
            cache.insert(hash_script, None);
        }

        result
    }
}
