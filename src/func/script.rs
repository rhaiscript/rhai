//! Implement script function-calling mechanism for [`Engine`].
#![cfg(not(feature = "no_function"))]

use super::func_call::FnCallArgs;
use super::FnAccess;
use crate::eval::{Caches, GlobalRef, GlobalRuntimeState};
use crate::{Dynamic, Engine, FnArgsVec, ImmutableString, Position, RhaiResult, Scope, ERR};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{fmt, hash::Hash};

/// A type containing the body of a script-defined function.
#[derive(Debug, Clone)]
pub enum ScriptFuncPayload {
    /// Normal statements block.
    #[cfg(not(feature = "no_ast"))]
    Statements(crate::ast::StmtBlock),
    /// _(grain)_ A Rhai Grain VM to run the function body.
    #[cfg(feature = "grain")]
    GrainVM {
        program: crate::grain::SharedProgram,
        params: FnArgsVec<u32>,
        chunk: crate::grain::bytecode::Chunk,
        span: crate::types::Span,
    },
}

impl ScriptFuncPayload {
    /// Get the start position.
    #[inline(always)]
    #[must_use]
    pub const fn start_position(&self) -> Position {
        match self {
            #[cfg(not(feature = "no_ast"))]
            Self::Statements(block) => block.position(),
            #[cfg(feature = "grain")]
            ScriptFuncPayload::GrainVM { span, .. } => span.start(),
            // `no_ast` and no `grain` -- no payload whatsoever
            #[cfg(feature = "no_ast")]
            #[cfg(not(feature = "grain"))]
            _ => Position::NONE,
        }
    }
    /// Get the end position.
    #[inline(always)]
    #[must_use]
    pub const fn end_position(&self) -> Position {
        match self {
            #[cfg(not(feature = "no_ast"))]
            Self::Statements(block) => block.end_position(),
            #[cfg(feature = "grain")]
            ScriptFuncPayload::GrainVM { span, .. } => span.end(),
            #[cfg(feature = "no_ast")]
            #[cfg(not(feature = "grain"))]
            _ => unreachable!(),
        }
    }
}

/// _(internals)_ A type containing information on a script-defined function.
/// Exported under the `internals` feature only.
#[derive(Debug, Clone)]
pub struct ScriptFuncDef {
    /// Function body.
    pub body: ScriptFuncPayload,
    /// Function name.
    pub name: ImmutableString,
    /// Function access mode.
    pub access: FnAccess,
    /// Type of `this` pointer, if any.
    /// Not available under `no_object`.
    #[cfg(not(feature = "no_object"))]
    pub this_type: Option<ImmutableString>,
    /// Names of function parameters.
    pub params: FnArgsVec<ImmutableString>,
    /// _(metadata)_ Function doc-comments (if any). Exported under the `metadata` feature only.
    ///
    /// Doc-comments are comment lines beginning with `///` or comment blocks beginning with `/**`,
    /// placed immediately before a function definition.
    ///
    /// Block doc-comments are kept in a single string with line-breaks within.
    ///
    /// Line doc-comments are merged, with line-breaks, into a single string without a termination line-break.
    ///
    /// Leading white-spaces are stripped, and each string always starts with the corresponding
    /// doc-comment leader: `///` or `/**`.
    ///
    /// Each line in non-block doc-comments starts with `///`.
    #[cfg(feature = "metadata")]
    pub comments: crate::StaticVec<crate::SmartString>,
}

impl ScriptFuncDef {
    /// Clone this [`ScriptFuncDef`] but with only signature-related info.
    ///
    /// The body of the function is removed, as well as comments (if any).
    #[cfg(not(feature = "no_ast"))]
    #[allow(dead_code)]
    pub(crate) fn clone_function_signatures(&self) -> Self {
        Self {
            name: self.name.clone(),
            access: self.access,
            body: ScriptFuncPayload::Statements(Default::default()),
            #[cfg(not(feature = "no_object"))]
            this_type: self.this_type.clone(),
            params: self.params.clone(),
            #[cfg(feature = "metadata")]
            comments: <_>::default(),
        }
    }
}

impl fmt::Display for ScriptFuncDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(not(feature = "no_object"))]
        let this_type = self
            .this_type
            .as_ref()
            .map_or(String::new(), |s| format!("{s:?}."));

        #[cfg(feature = "no_object")]
        let this_type = "";

        write!(
            f,
            "{}{}{}({})",
            match self.access {
                FnAccess::Public => "",
                FnAccess::Private => "private ",
            },
            this_type,
            self.name,
            self.params
                .iter()
                .map(ImmutableString::as_str)
                .collect::<FnArgsVec<_>>()
                .join(", ")
        )
    }
}

/// A type containing the metadata of a script-defined function.
///
/// Not available under `no_function`.
///
/// Created by [`AST::iter_functions`][crate::AST::iter_functions].
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
#[non_exhaustive]
pub struct ScriptFnMetadata<'a> {
    /// Function name.
    pub name: &'a str,
    /// Function parameters (if any).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub params: Vec<&'a str>,
    /// Function access mode.
    pub access: FnAccess,
    /// Type of `this` pointer, if any.
    /// Not available under `no_object`.
    #[cfg(not(feature = "no_object"))]
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub this_type: Option<&'a str>,
    /// _(metadata)_ Function doc-comments (if any).
    /// Exported under the `metadata` feature only.
    ///
    /// Doc-comments are comment lines beginning with `///` or comment blocks beginning with `/**`,
    /// placed immediately before a function definition.
    ///
    /// Block doc-comments are kept in a single string slice with line-breaks within.
    ///
    /// Line doc-comments are merged, with line-breaks, into a single string slice without a termination line-break.
    ///
    /// Leading white-spaces are stripped, and each string slice always starts with the
    /// corresponding doc-comment leader: `///` or `/**`.
    ///
    /// Each line in non-block doc-comments starts with `///`.
    #[cfg(feature = "metadata")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<&'a str>,
}

impl fmt::Display for ScriptFnMetadata<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(not(feature = "no_object"))]
        let this_type = self
            .this_type
            .as_ref()
            .map_or(String::new(), |s| format!("{s:?}."));

        #[cfg(feature = "no_object")]
        let this_type = "";

        write!(
            f,
            "{}{}{}({})",
            match self.access {
                FnAccess::Public => "",
                FnAccess::Private => "private ",
            },
            this_type,
            self.name,
            self.params
                .iter()
                .copied()
                .collect::<FnArgsVec<_>>()
                .join(", ")
        )
    }
}

impl<'a> From<&'a ScriptFuncDef> for ScriptFnMetadata<'a> {
    #[inline]
    fn from(value: &'a ScriptFuncDef) -> Self {
        Self {
            name: &value.name,
            params: value.params.iter().map(ImmutableString::as_str).collect(),
            access: value.access,
            #[cfg(not(feature = "no_object"))]
            this_type: value.this_type.as_deref(),
            #[cfg(feature = "metadata")]
            comments: value.comments.iter().map(<_>::as_ref).collect(),
        }
    }
}

/// _(internals)_ Encapsulated environment.
/// Exported under the `internals` feature only.
///
/// Not available under `no_function`.
///
/// 1) stack of scripted functions defined
/// 2) the stack of imported [modules][crate::Module]
/// 3) global constants
#[derive(Debug, Clone)]
pub struct EncapsulatedEnviron {
    /// Stack of loaded [modules][crate::Module] containing script-defined functions.
    pub lib: crate::StaticVec<crate::SharedModule>,
    /// Imported [modules][crate::Module].
    #[cfg(not(feature = "no_module"))]
    pub imports: crate::ThinVec<(ImmutableString, crate::SharedModule)>,
    /// Globally-defined constants.
    #[cfg(not(feature = "no_module"))]
    pub constants: Option<crate::eval::SharedGlobalConstants>,
}

impl From<&crate::eval::GlobalRuntimeState> for EncapsulatedEnviron {
    fn from(value: &crate::eval::GlobalRuntimeState) -> Self {
        Self {
            lib: value.lib.clone(),
            #[cfg(not(feature = "no_module"))]
            imports: value
                .iter_imports_raw()
                .map(|(n, m)| (n.clone(), m.clone()))
                .collect(),
            #[cfg(not(feature = "no_module"))]
            constants: value.constants.clone(),
        }
    }
}

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
        mut global_ref: GlobalRef,
        caches: &mut Caches,
        scope: &mut Scope,
        mut this_ptr: Option<&mut Dynamic>,
        _env: Option<&EncapsulatedEnviron>,
        fn_def: &ScriptFuncDef,
        args: &mut FnCallArgs,
        rewind_scope: bool,
        pos: Position,
    ) -> RhaiResult {
        debug_assert_eq!(fn_def.params.len(), args.len());

        let global = global_ref.as_mut_level_up();

        self.track_operation(global, pos)?;

        // Check for stack overflow
        #[cfg(not(feature = "unchecked"))]
        if global.level > self.max_call_levels() {
            return Err(ERR::ErrorStackOverflow(pos).into());
        }

        // Guard against too many variables
        #[cfg(not(feature = "unchecked"))]
        if scope.len() + fn_def.params.len() > self.max_variables() {
            return Err(ERR::ErrorTooManyVariables(pos).into());
        }

        // Short-circuit empty function body
        match fn_def.body {
            #[cfg(not(feature = "no_ast"))]
            ScriptFuncPayload::Statements(ref block) => {
                let is_empty = block.is_empty();
                #[cfg(feature = "debugging")]
                let is_empty = is_empty && self.debugger_interface.is_none();

                if is_empty {
                    return Ok(Dynamic::UNIT);
                }
            }
            // We don't know about Grain functions, so call it anyway
            #[cfg(feature = "grain")]
            ScriptFuncPayload::GrainVM { .. } => (),
        }

        // Save the original state
        let orig_scope_len = scope.len();
        let orig_lib_len = global.lib.len();
        #[cfg(not(feature = "no_module"))]
        let orig_imports_len = global.num_imports();

        #[cfg(feature = "debugging")]
        let orig_call_stack_len = global
            .debugger
            .as_ref()
            .map_or(0, |dbg| dbg.call_stack().len());

        // Put collect function call arguments.
        // Actually consume the arguments instead of cloning them.
        let arg_values = args.iter_mut().map(|v| v.take()).collect::<FnArgsVec<_>>();

        // Push a new call stack frame
        #[cfg(feature = "debugging")]
        if self.is_debugger_registered() {
            let fn_name = fn_def.name.clone();
            let args = arg_values
                .iter()
                .map(Dynamic::flatten_clone)
                .collect::<FnArgsVec<_>>();
            let source = global.source.clone();

            global
                .debugger_mut()
                .push_call_stack_frame(fn_name, args, source, pos);
        }

        // Merge in encapsulated environment, if any
        let orig_fn_resolution_caches_len = caches.fn_resolution_caches_len();

        #[cfg(not(feature = "no_module"))]
        let orig_constants = _env.map(
            |EncapsulatedEnviron {
                 lib,
                 imports,
                 constants,
             }| {
                imports
                    .iter()
                    .cloned()
                    .for_each(|(n, m)| global.push_import(n, m));

                global.lib.extend(lib.clone());

                std::mem::replace(&mut global.constants, constants.clone())
            },
        );

        let mut _arg_slots = 0;

        // Evaluate the function
        let mut _result: RhaiResult = match fn_def.body {
            // Normal statements block
            #[cfg(not(feature = "no_ast"))]
            ScriptFuncPayload::Statements(ref body) => {
                // Put arguments into scope as variables
                scope.extend(fn_def.params.iter().cloned().zip(arg_values));
                _arg_slots = fn_def.params.len();

                #[cfg(feature = "debugging")]
                if self.is_debugger_registered() {
                    let node = crate::ast::Stmt::Noop(fn_def.body.start_position());
                    self.dbg(global, caches, scope, this_ptr.as_deref_mut(), &node)?;
                }

                self.eval_stmt_block(
                    global,
                    caches,
                    scope,
                    this_ptr.as_deref_mut(),
                    body.statements(),
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
                        _env.and_then(|env| env.lib.last())
                            .and_then(|m| m.id())
                            .unwrap_or_else(|| global.source().unwrap_or(""))
                            .to_string(),
                        #[cfg(feature = "no_module")]
                        global.source().unwrap_or("").to_string(),
                        err,
                        pos,
                    )
                    .into()),
                })
            }
            // Rhai Grain VM
            #[cfg(feature = "grain")]
            ScriptFuncPayload::GrainVM {
                ref program,
                ref params,
                chunk,
                ..
            } => {
                // The Grain VM needs a whole `GlobalRuntimeState` and not a reference.
                // So if `global_mut` is `Some`, then it needs to be cloned.
                let global = global_ref.take_with_level_up();
                let level = global.level;

                let mut vm = crate::grain::Vm::with_global_state(self, global);

                // The value in the `this` pointer is cloned
                let (result, new_this_ptr) = vm.call_function_with_this(
                    program,
                    fn_def.name.as_str(),
                    params,
                    chunk,
                    arg_values,
                    level,
                    scope,
                    rewind_scope,
                    pos,
                    this_ptr.as_deref_mut().cloned(),
                );

                // Write back new value for the `this` pointer
                if let Some(this_ptr) = this_ptr.as_deref_mut() {
                    if let Some(new_this_ptr) = new_this_ptr {
                        *this_ptr = new_this_ptr;
                    }
                }

                let global = vm.into_global_state();

                // Update number of operations, into both potential sources
                #[cfg(atomic_ops_count)]
                global_ref.as_ref().num_operations.store(
                    global.num_operations(),
                    std::sync::atomic::Ordering::Relaxed,
                );
                // Without 64-bit atomics only a mutable original can be updated.
                // An immutable one gets the count through the owned instance put back below.
                #[cfg(not(atomic_ops_count))]
                if let GlobalRef::Mut(g) = &mut global_ref {
                    g.num_operations = global.num_operations();
                }

                // Put back the instance
                global_ref.put_back(global);

                result
            }
        };

        let global = global_ref.as_mut_level_up();

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
                match self.dbg_raw(global, caches, scope, this_ptr, node, event) {
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
        } else if _arg_slots > 0 {
            // Remove arguments only, leaving new variables in the scope
            scope.remove_range(orig_scope_len, _arg_slots);
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
        let result = global.lib.iter().any(|m| m.contains_fn(hash_script))
            // Then check the global namespace and packages
            || self.global_modules.iter().any(|m| m.contains_fn(hash_script));

        #[cfg(not(feature = "no_module"))]
        let result = result ||
            // Then check imported modules
            global.contains_qualified_fn(hash_script)
            // Then check sub-modules
            || self.global_sub_modules.values().any(|m| m.contains_qualified_fn(hash_script));

        if !result && !cache.bloom_filter.is_absent_and_set(hash_script) {
            // Do not cache "one-hit wonders"
            cache.dict.insert(hash_script, None);
        }

        result
    }
}
