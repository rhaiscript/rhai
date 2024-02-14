//! Implement script function-calling mechanism for [`Engine`].
#![cfg(not(feature = "no_function"))]

use super::call::FnCallArgs;
use crate::ast::{EncapsulatedEnviron, ScriptFuncDef};
use crate::eval::{Caches, GlobalRuntimeState};
use crate::{Dynamic, Engine, Position, RhaiResult, Scope, ERR};
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
    /// **DO NOT** reuse the argument values except for the first `&mut` argument - all others are silently replaced by `()`!
    pub(crate) fn call_script_fn(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        mut this_ptr: Option<&mut Dynamic>,
        _environ: Option<&EncapsulatedEnviron>,
        fn_def: &ScriptFuncDef,
        args: &mut FnCallArgs,
        rewind_scope: bool,
        pos: Position,
    ) -> RhaiResult {
        debug_assert_eq!(fn_def.params.len(), args.len());

        self.track_operation(global, pos)?;

        // Check for stack overflow
        #[cfg(not(feature = "unchecked"))]
        if global.level > self.max_call_levels() {
            return Err(ERR::ErrorStackOverflow(pos).into());
        }

        #[cfg(feature = "debugging")]
        if self.debugger_interface.is_none() && fn_def.body.is_empty() {
            return Ok(Dynamic::UNIT);
        }
        #[cfg(not(feature = "debugging"))]
        if fn_def.body.is_empty() {
            return Ok(Dynamic::UNIT);
        }

        let orig_scope_len = scope.len();
        let orig_lib_len = global.lib.len();
        #[cfg(not(feature = "no_module"))]
        let orig_imports_len = global.num_imports();

        #[cfg(feature = "debugging")]
        let orig_call_stack_len = global
            .debugger
            .as_ref()
            .map_or(0, |dbg| dbg.call_stack().len());

        // Guard against too many variables
        #[cfg(not(feature = "unchecked"))]
        if scope.len() + fn_def.params.len() > self.max_variables() {
            return Err(ERR::ErrorTooManyVariables(pos).into());
        }

        // Put arguments into scope as variables
        scope.extend(fn_def.params.iter().cloned().zip(args.iter_mut().map(|v| {
            // Actually consume the arguments instead of cloning them
            v.take()
        })));

        // Push a new call stack frame
        #[cfg(feature = "debugging")]
        if self.is_debugger_registered() {
            let fn_name = fn_def.name.clone();
            let args = scope
                .iter_inner()
                .skip(orig_scope_len)
                .map(|(.., v)| v.flatten_clone());
            let source = global.source.clone();

            global
                .debugger_mut()
                .push_call_stack_frame(fn_name, args, source, pos);
        }

        // Merge in encapsulated environment, if any
        let orig_fn_resolution_caches_len = caches.fn_resolution_caches_len();

        #[cfg(not(feature = "no_module"))]
        let orig_constants = _environ.map(|environ| {
            let EncapsulatedEnviron {
                lib,
                imports,
                constants,
            } = environ;

            imports
                .iter()
                .cloned()
                .for_each(|(n, m)| global.push_import(n, m));

            global.lib.push(lib.clone());

            std::mem::replace(&mut global.constants, constants.clone())
        });

        #[cfg(feature = "debugging")]
        if self.is_debugger_registered() {
            let node = crate::ast::Stmt::Noop(fn_def.body.position());
            self.run_debugger(global, caches, scope, this_ptr.as_deref_mut(), &node)?;
        }

        // Evaluate the function
        let mut _result: RhaiResult = self
            .eval_stmt_block(
                global,
                caches,
                scope,
                this_ptr.as_deref_mut(),
                fn_def.body.statements(),
                rewind_scope,
            )
            .or_else(|err| match *err {
                // Convert return statement to return value
                ERR::Return(x, ..) => Ok(x),
                // Exit value is passed straight-through
                mut err @ ERR::Exit(..) => {
                    err.set_position(pos);
                    Err(err.into())
                }
                // System errors are passed straight-through
                mut err if err.is_system_exception() => {
                    err.set_position(pos);
                    Err(err.into())
                }
                // Other errors are wrapped in `ErrorInFunctionCall`
                _ => Err(ERR::ErrorInFunctionCall(
                    fn_def.name.to_string(),
                    #[cfg(not(feature = "no_module"))]
                    _environ
                        .and_then(|environ| environ.lib.id())
                        .unwrap_or_else(|| global.source().unwrap_or(""))
                        .to_string(),
                    #[cfg(feature = "no_module")]
                    global.source().unwrap_or("").to_string(),
                    err,
                    pos,
                )
                .into()),
            });

        #[cfg(feature = "debugging")]
        if self.is_debugger_registered() {
            let trigger = match global.debugger_mut().status {
                crate::eval::DebuggerStatus::FunctionExit(n) => n >= global.level,
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
                match self.run_debugger_raw(global, caches, scope, this_ptr, node, event) {
                    Ok(_) => (),
                    Err(err) => _result = Err(err),
                }
            }

            // Pop the call stack
            global
                .debugger
                .as_mut()
                .unwrap()
                .rewind_call_stack(orig_call_stack_len);
        }

        // Remove all local variables and imported modules
        if rewind_scope {
            scope.rewind(orig_scope_len);
        } else if !args.is_empty() {
            // Remove arguments only, leaving new variables in the scope
            scope.remove_range(orig_scope_len, args.len());
        }
        global.lib.truncate(orig_lib_len);
        #[cfg(not(feature = "no_module"))]
        global.truncate_imports(orig_imports_len);

        // Restore constants
        #[cfg(not(feature = "no_module"))]
        if let Some(constants) = orig_constants {
            global.constants = constants;
        }

        // Restore state
        caches.rewind_fn_resolution_caches(orig_fn_resolution_caches_len);

        _result
    }

    // Does a script-defined function exist?
    ///
    /// # Note
    ///
    /// If the scripted function is not found, this information is cached for future look-ups.
    #[must_use]
    pub(crate) fn has_script_fn(
        &self,
        global: &GlobalRuntimeState,
        caches: &mut Caches,
        hash_script: u64,
    ) -> bool {
        let cache = caches.fn_resolution_cache_mut();

        if let Some(result) = cache.dict.get(&hash_script).map(Option::is_some) {
            return result;
        }

        // First check script-defined functions
        let res = global.lib.iter().any(|m| m.contains_fn(hash_script))
            // Then check the global namespace and packages
            || self.global_modules.iter().any(|m| m.contains_fn(hash_script));

        #[cfg(not(feature = "no_module"))]
        let res = res ||
            // Then check imported modules
            global.contains_qualified_fn(hash_script)
            // Then check sub-modules
            || self.global_sub_modules.values().any(|m| m.contains_qualified_fn(hash_script));

        if !res && !cache.filter.is_absent_and_set(hash_script) {
            // Do not cache "one-hit wonders"
            cache.dict.insert(hash_script, None);
        }

        res
    }
}
