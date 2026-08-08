//! Evaluation context.

use super::{Caches, GlobalRuntimeState};
use crate::ast::FnCallHashes;
use crate::tokenizer::{is_valid_function_name, Token};
use crate::types::dynamic::Variant;
use crate::{
    calc_fn_hash, expose_under_internals, Dynamic, Engine, FnArgsVec, FuncArgs, ImmutableString,
    Position, RhaiResult, RhaiResultOf, Scope, StaticVec, ERR,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::type_name,
    mem,
    ops::{Deref, DerefMut},
};

/// Context of a script evaluation process.
#[allow(dead_code)]
pub struct EvalContext<'a, 's, 'ps, 'g, 'c, 't> {
    /// The current [`Engine`].
    engine: &'a Engine,
    /// The current [`GlobalRuntimeState`].
    global: &'g mut GlobalRuntimeState,
    /// The current [caches][Caches], if available.
    caches: &'c mut Caches,
    /// The current [`Scope`].
    scope: &'s mut Scope<'ps>,
    /// The current bound `this` pointer, if any.
    this_ptr: Option<&'t mut Dynamic>,
}

impl<'a, 's, 'ps, 'g, 'c, 't> EvalContext<'a, 's, 'ps, 'g, 'c, 't> {
    /// Create a new [`EvalContext`].
    #[expose_under_internals]
    #[inline(always)]
    #[must_use]
    fn new(
        engine: &'a Engine,
        global: &'g mut GlobalRuntimeState,
        caches: &'c mut Caches,
        scope: &'s mut Scope<'ps>,
        this_ptr: Option<&'t mut Dynamic>,
    ) -> Self {
        Self {
            engine,
            global,
            caches,
            scope,
            this_ptr,
        }
    }
    /// The current [`Engine`].
    #[inline(always)]
    #[must_use]
    pub const fn engine(&self) -> &'a Engine {
        self.engine
    }
    /// The current source.
    #[inline(always)]
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        self.global.source()
    }
    /// Get a mutable reference to the current source.
    #[inline(always)]
    #[must_use]
    pub fn source_mut(&mut self) -> &mut Option<ImmutableString> {
        &mut self.global.source
    }

    /// The current [`Scope`].
    #[inline(always)]
    #[must_use]
    pub const fn scope(&self) -> &Scope<'ps> {
        self.scope
    }
    /// Get a mutable reference to the current [`Scope`].
    #[inline(always)]
    #[must_use]
    pub fn scope_mut(&mut self) -> &mut Scope<'ps> {
        self.scope
    }
    /// Get an iterator over the current set of modules imported via `import` statements,
    /// in reverse order (i.e. modules imported last come first).
    #[cfg(not(feature = "no_module"))]
    #[inline(always)]
    pub fn iter_imports(&self) -> impl Iterator<Item = (&str, &crate::Module)> {
        self.global.iter_imports()
    }
    /// Custom state kept in a [`Dynamic`].
    #[inline(always)]
    pub const fn tag(&self) -> &Dynamic {
        &self.global.tag
    }
    /// Mutable reference to the custom state kept in a [`Dynamic`].
    #[inline(always)]
    pub fn tag_mut(&mut self) -> &mut Dynamic {
        &mut self.global.tag
    }
    /// _(internals)_ The current [`GlobalRuntimeState`].
    /// Exported under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub const fn global_runtime_state(&self) -> &GlobalRuntimeState {
        self.global
    }
    /// _(internals)_ Get a mutable reference to the current [`GlobalRuntimeState`].
    /// Exported under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn global_runtime_state_mut(&mut self) -> &mut GlobalRuntimeState {
        self.global
    }
    /// Get an iterator over the namespaces containing definition of all script-defined functions.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub fn iter_namespaces(&self) -> impl Iterator<Item = &crate::Module> {
        self.global.lib.iter().map(<_>::as_ref)
    }
    /// _(internals)_ The current set of namespaces containing definitions of all script-defined functions.
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn namespaces(&self) -> &[crate::SharedModule] {
        &self.global.lib
    }
    /// _(internals)_ Mutable reference to the current set of namespaces containing definitions of all script-defined functions.
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn namespaces_mut(&mut self) -> &mut [crate::SharedModule] {
        self.global.lib.as_mut()
    }
    /// The current bound `this` pointer, if any.
    #[inline(always)]
    #[must_use]
    pub fn this_ptr(&self) -> Option<&Dynamic> {
        self.this_ptr.as_deref()
    }
    /// Mutable reference to the current bound `this` pointer, if any.
    #[inline(always)]
    #[must_use]
    pub fn this_ptr_mut(&mut self) -> Option<&mut Dynamic> {
        self.this_ptr.as_deref_mut()
    }
    /// The current nesting level of function calls.
    #[inline(always)]
    #[must_use]
    pub const fn call_level(&self) -> usize {
        self.global.level
    }
    /// _(internals, debugging)_ Debugging interface.
    /// Exported under the `debugging` and `internals` features only.
    #[cfg(feature = "debugging")]
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn debugger(&self) -> Option<&crate::debugger::Debugger> {
        self.global.debugger.as_deref()
    }
    /// _(internals, debugging)_ Mutable reference to the debugging interface.
    /// Exported under the `debugging` and `internals` features only.
    #[cfg(feature = "debugging")]
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn debugger_mut(&mut self) -> Option<&mut crate::debugger::Debugger> {
        self.global.debugger.as_deref_mut()
    }

    /// Evaluate an [expression tree][crate::Expression] within this [evaluation context][`EvalContext`].
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.  It evaluates an expression from an [`AST`][crate::AST].
    #[cfg(not(feature = "no_custom_syntax"))]
    #[inline(always)]
    pub fn eval_expression_tree(&mut self, expr: &crate::Expression) -> crate::RhaiResult {
        #[allow(deprecated)]
        self.eval_expression_tree_raw(expr, true)
    }
    /// Evaluate an [expression tree][crate::Expression] within this [evaluation context][`EvalContext`].
    ///
    /// The following option is available:
    ///
    /// * whether to rewind the [`Scope`] after evaluation if the expression is a [`StmtBlock`][crate::ast::StmtBlock]
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is _extremely_ low level.  It evaluates an expression from an [`AST`][crate::AST].
    #[cfg(not(feature = "no_custom_syntax"))]
    #[deprecated = "This API is NOT deprecated, but it is considered volatile and may change in the future."]
    #[inline]
    pub fn eval_expression_tree_raw(
        &mut self,
        expr: &crate::Expression,
        rewind_scope: bool,
    ) -> crate::RhaiResult {
        let expr: &crate::ast::Expr = expr;
        let this_ptr = self.this_ptr.as_deref_mut();

        match expr {
            crate::ast::Expr::Stmt(stmts) => self.engine.eval_stmt_block(
                self.global,
                self.caches,
                self.scope,
                this_ptr,
                stmts.statements(),
                rewind_scope,
            ),
            _ => self
                .engine
                .eval_expr(self.global, self.caches, self.scope, this_ptr, expr),
        }
    }

    /// Call a function inside the [evaluation context][`EvalContext`] with the provided arguments.
    pub fn call_fn<T: Variant + Clone>(
        &mut self,
        fn_name: impl AsRef<str>,
        args: impl FuncArgs,
    ) -> RhaiResultOf<T> {
        let engine = self.engine();

        let mut arg_values = StaticVec::new_const();
        args.parse(&mut arg_values);

        let args = &mut arg_values.iter_mut().collect::<FnArgsVec<_>>();

        let is_ref_mut = if let Some(this_ptr) = self.this_ptr.as_deref_mut() {
            args.insert(0, this_ptr);
            true
        } else {
            false
        };

        _call_fn_raw(
            engine,
            self.global,
            self.caches,
            self.scope,
            fn_name,
            args,
            false,
            is_ref_mut,
            false,
        )
        .and_then(|result| {
            result.try_cast_result().map_err(|r| {
                let result_type = engine.map_type_name(r.type_name());
                let cast_type = match type_name::<T>() {
                    typ if typ.contains("::") => engine.map_type_name(typ),
                    typ => typ,
                };
                ERR::ErrorMismatchOutputType(cast_type.into(), result_type.into(), Position::NONE)
                    .into()
            })
        })
    }
    /// Call a registered native Rust function inside the [evaluation context][`EvalContext`] with
    /// the provided arguments.
    ///
    /// This is often useful because Rust functions typically only want to cross-call other
    /// registered Rust functions and not have to worry about scripted functions hijacking the
    /// process unknowingly (or deliberately).
    pub fn call_native_fn<T: Variant + Clone>(
        &mut self,
        fn_name: impl AsRef<str>,
        args: impl FuncArgs,
    ) -> RhaiResultOf<T> {
        let engine = self.engine();

        let mut arg_values = StaticVec::new_const();
        args.parse(&mut arg_values);

        let args = &mut arg_values.iter_mut().collect::<FnArgsVec<_>>();

        let is_ref_mut = self.this_ptr.as_deref_mut().map_or(false, |this_ptr| {
            args.insert(0, this_ptr);
            true
        });

        _call_fn_raw(
            engine,
            self.global,
            self.caches,
            self.scope,
            fn_name,
            args,
            true,
            is_ref_mut,
            false,
        )
        .and_then(|result| {
            result.try_cast_result().map_err(|r| {
                let result_type = engine.map_type_name(r.type_name());
                let cast_type = match type_name::<T>() {
                    typ if typ.contains("::") => engine.map_type_name(typ),
                    typ => typ,
                };
                ERR::ErrorMismatchOutputType(cast_type.into(), result_type.into(), Position::NONE)
                    .into()
            })
        })
    }
    /// Call a function (native Rust or scripted) inside the [evaluation context][`EvalContext`].
    ///
    /// If `is_method_call` is [`true`], the first argument is assumed to be the `this` pointer for
    /// a script-defined function (or the object of a method call).
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.
    ///
    /// # Arguments
    ///
    /// All arguments may be _consumed_, meaning that they may be replaced by `()`. This is to avoid
    /// unnecessarily cloning the arguments.
    ///
    /// **DO NOT** reuse the arguments after this call. If they are needed afterwards, clone them
    /// _before_ calling this function.
    ///
    /// If `is_ref_mut` is [`true`], the first argument is assumed to be passed by reference and is
    /// not consumed.
    #[inline(always)]
    pub fn call_fn_raw(
        &mut self,
        fn_name: impl AsRef<str>,
        is_ref_mut: bool,
        is_method_call: bool,
        args: &mut [&mut Dynamic],
    ) -> RhaiResult {
        let name = fn_name.as_ref();
        let native_only = !is_valid_function_name(name);
        #[cfg(not(feature = "no_function"))]
        let native_only = native_only && !crate::parser::is_anonymous_fn(name);

        _call_fn_raw(
            self.engine(),
            self.global,
            self.caches,
            self.scope,
            fn_name,
            args,
            native_only,
            is_ref_mut,
            is_method_call,
        )
    }
    /// Call a registered native Rust function inside the [evaluation context][`EvalContext`].
    ///
    /// This is often useful because Rust functions typically only want to cross-call other
    /// registered Rust functions and not have to worry about scripted functions hijacking the
    /// process unknowingly (or deliberately).
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.
    ///
    /// # Arguments
    ///
    /// All arguments may be _consumed_, meaning that they may be replaced by `()`. This is to avoid
    /// unnecessarily cloning the arguments.
    ///
    /// **DO NOT** reuse the arguments after this call. If they are needed afterwards, clone them
    /// _before_ calling this function.
    ///
    /// If `is_ref_mut` is [`true`], the first argument is assumed to be passed by reference and is
    /// not consumed.
    #[inline(always)]
    pub fn call_native_fn_raw(
        &mut self,
        fn_name: impl AsRef<str>,
        is_ref_mut: bool,
        args: &mut [&mut Dynamic],
    ) -> RhaiResult {
        _call_fn_raw(
            self.engine(),
            self.global,
            self.caches,
            self.scope,
            fn_name,
            args,
            true,
            is_ref_mut,
            false,
        )
    }

    /// Create a new isolated runtime frame guard, restoring field values upon `Drop`.
    ///
    /// The [frame guard][EvalContextFrameGuard] returned derefs to [`EvalContext`].
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // The following pushes a new, empty, caching layer to make sure that new function resolutions
    /// // do not persist. This is useful if the resolution will be volatile.
    /// let result: i64 = context.new_frame()
    ///                          .with_new_caching_layer()
    ///                          .call_fn("foo", (0_i64,))?;
    ///
    /// // In the above example, the resolution to function 'foo' will not be cached outside the frame.
    /// // The next call to 'foo' will be resolved again instead of using the cached resolution,
    /// // even if 'foo' is redefined.
    ///
    /// // The following modifies the [`EvalContext`] before using, restoring its state afterwards.
    /// {
    ///     // Modify the context before using...
    ///     let context = context.new_frame()
    ///                          .rewind_scope(true)            // rewinds the scope...
    ///                          .with_source("new source")     // new source...
    ///                          .with_namespace(new_module)    // add new namespace module...
    ///                          .up_call_level();
    ///
    ///     // Call a function with the modified context...
    ///     let result: i64 = context.call_fn("foo", (0_i64,))?;
    ///
    ///     // ... at end of block, context automatically restored to previous state.
    /// }
    /// ```
    ///
    /// ## `Drop` behavior
    ///
    /// Upon `Drop`, the following fields will be automatically restored to the previous values:
    ///
    /// * the stack of imported [modules][crate::Module] will be rewound to the original depth if more have been added via [`EvalContextFrameGuard::with_import`].
    /// * the stack of scripted function [modules][crate::Module] will be rewound to the original depth if more have been added via [`EvalContextFrameGuard::with_namespace`].
    /// * the original functions resolution cache will be restored if a new caching layer was created via [`EvalContextFrameGuard::with_new_caching_layer`].
    /// * the original [scope][EvalContext::scope] will be rewound if [`EvalContextFrameGuard::rewind_scope`] was set to `true`.
    /// * the [source][GlobalRuntimeState::source] will be restored if a new source was set via [`EvalContextFrameGuard::with_source`] or cleared via [`EvalContextFrameGuard::clear_source`].
    /// * the current [nesting level][GlobalRuntimeState::level] of function calls will be restored if modified via [`EvalContextFrameGuard::up_call_level`].
    /// * the current [`tag`][GlobalRuntimeState::tag] will be restored if modified via [`EvalContextFrameGuard::with_tag`] or [`EvalContextFrameGuard::clear_tag`].
    pub fn new_frame<'f>(&'f mut self) -> EvalContextFrameGuard<'f, 'a, 's, 'ps, 'g, 'c, 't> {
        EvalContextFrameGuard {
            #[cfg(feature = "internals")]
            #[cfg(not(feature = "no_module"))]
            imports_len: None,
            #[cfg(feature = "internals")]
            #[cfg(not(feature = "no_function"))]
            lib_len: None,
            caches_len: None,
            scope_len: None,
            source: None,
            #[cfg(feature = "internals")]
            level: None,
            tag: None,

            context: self,
        }
    }
}

/// Call a function (native Rust or scripted) inside the [evaluation context][`EvalContext`].
fn _call_fn_raw(
    engine: &Engine,
    global: &mut GlobalRuntimeState,
    caches: &mut Caches,
    scope: &mut Scope,
    fn_name: impl AsRef<str>,
    args: &mut [&mut Dynamic],
    native_only: bool,
    is_ref_mut: bool,
    is_method_call: bool,
) -> RhaiResult {
    defer! { let orig_level = global.level; global.level += 1 }

    let fn_name = fn_name.as_ref();
    let op_token = Token::lookup_symbol_from_syntax(fn_name);
    let op_token = op_token.as_ref();
    let args_len = args.len();

    if native_only {
        // The functions rhai answers by name are all reserved names, and a
        // reserved name is what makes a call native-only — so without this the
        // branch routes straight past their only implementation. See
        // `Engine::exec_syntactic_fn_call`.
        if let Some(result) = engine.exec_syntactic_fn_call(fn_name, args, Position::NONE) {
            return result;
        }

        let hash = calc_fn_hash(None, fn_name, args_len);

        return engine
            .exec_native_fn_call(
                global,
                caches,
                fn_name,
                op_token,
                hash,
                args,
                is_ref_mut,
                false,
                Position::NONE,
            )
            .map(|(r, ..)| r);
    }

    // Native or script

    let hash = match is_method_call {
        #[cfg(not(feature = "no_function"))]
        true => FnCallHashes::from_script_and_native(
            calc_fn_hash(None, fn_name, args_len - 1),
            calc_fn_hash(None, fn_name, args_len),
        ),
        #[cfg(feature = "no_function")]
        true => FnCallHashes::from_native_only(calc_fn_hash(None, fn_name, args_len)),
        _ => FnCallHashes::from_hash(calc_fn_hash(None, fn_name, args_len)),
    };

    engine
        .exec_fn_call(
            global,
            caches,
            Some(scope),
            fn_name,
            op_token,
            hash,
            args,
            is_ref_mut,
            is_method_call,
            Position::NONE,
        )
        .map(|(r, ..)| r)
}

/// A new frame guard for [`EvalContext`], restoring field values upon `Drop`.
pub struct EvalContextFrameGuard<'f, 'a, 's, 'ps, 'g, 'c, 't> {
    context: &'f mut EvalContext<'a, 's, 'ps, 'g, 'c, 't>,

    caches_len: Option<usize>,
    scope_len: Option<usize>,
    #[cfg(feature = "internals")]
    #[cfg(not(feature = "no_module"))]
    imports_len: Option<usize>,
    #[cfg(feature = "internals")]
    #[cfg(not(feature = "no_function"))]
    lib_len: Option<usize>,
    source: Option<Option<ImmutableString>>,
    #[cfg(feature = "internals")]
    level: Option<usize>,
    tag: Option<Dynamic>,
}

impl Drop for EvalContextFrameGuard<'_, '_, '_, '_, '_, '_, '_> {
    #[inline(always)]
    fn drop(&mut self) {
        if let Some(caches_len) = self.caches_len {
            self.context.caches.rewind_fn_resolution_caches(caches_len);
        }
        if let Some(scope_len) = self.scope_len {
            self.context.scope.rewind(scope_len);
        }
        #[cfg(feature = "internals")]
        #[cfg(not(feature = "no_module"))]
        if let Some(imports_len) = self.imports_len {
            self.context.global.truncate_imports(imports_len);
        }
        #[cfg(feature = "internals")]
        #[cfg(not(feature = "no_function"))]
        if let Some(lib_len) = self.lib_len {
            self.context.global.lib.truncate(lib_len);
        }
        if let Some(source) = self.source.take() {
            *self.context.source_mut() = source;
        }
        #[cfg(feature = "internals")]
        if let Some(level) = self.level {
            self.context.global.level = level;
        }
        if let Some(tag) = self.tag.take() {
            *self.context.tag_mut() = tag;
        }
    }
}

impl<'a, 's, 'ps, 'g, 'c, 't> Deref for EvalContextFrameGuard<'_, 'a, 's, 'ps, 'g, 'c, 't> {
    type Target = EvalContext<'a, 's, 'ps, 'g, 'c, 't>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.context
    }
}

impl<'a, 's, 'ps, 'g, 'c, 't> DerefMut for EvalContextFrameGuard<'_, 'a, 's, 'ps, 'g, 'c, 't> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.context
    }
}

impl<'t> EvalContextFrameGuard<'_, '_, '_, '_, '_, '_, 't> {
    /// Push a new caching layer for function resolution results.
    pub fn with_new_caching_layer(mut self) -> Self {
        self.caches_len = Some(self.context.caches.fn_resolution_caches_len());
        self.context.caches.push_fn_resolution_cache();
        self
    }
    /// Rewind the [scope][EvalContext::scope].
    #[inline(always)]
    pub fn rewind_scope(mut self, enable: bool) -> Self {
        self.scope_len = if enable {
            Some(self.context.scope().len())
        } else {
            None
        };
        self
    }
    /// Modify the current source.
    #[inline(always)]
    pub fn with_source(mut self, source: impl Into<ImmutableString>) -> Self {
        self.source = Some(Some(source.into()));
        mem::swap(self.context.source_mut(), self.source.as_mut().unwrap());
        self
    }
    /// Clear the current source.
    #[inline(always)]
    pub fn clear_source(mut self) -> Self {
        self.source = Some(None);
        mem::swap(self.context.source_mut(), self.source.as_mut().unwrap());
        self
    }
    /// Modify the custom state.
    #[inline(always)]
    pub fn with_tag(mut self, tag: Dynamic) -> Self {
        self.tag = Some(tag);
        mem::swap(self.context.tag_mut(), self.tag.as_mut().unwrap());
        self
    }
    /// Modify the custom state to [`Dynamic::UNIT`].
    #[inline(always)]
    pub fn clear_tag(mut self) -> Self {
        self.tag = Some(Dynamic::UNIT);
        mem::swap(self.context.tag_mut(), self.tag.as_mut().unwrap());
        self
    }
    /// _(internals)_ Add a new imported [module][crate::Module].
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_module`.
    #[cfg(feature = "internals")]
    #[cfg(not(feature = "no_module"))]
    pub fn with_import(
        mut self,
        name: impl Into<ImmutableString>,
        module: impl Into<crate::SharedModule>,
    ) -> Self {
        self.imports_len = Some(self.context.global.num_imports());
        self.context.global.push_import(name.into(), module.into());
        self
    }
    /// _(internals)_ Add a new [module][crate::Module] containing script-defined functions.
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_function`.
    #[cfg(feature = "internals")]
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    #[must_use]
    pub fn with_namespace(mut self, module: impl Into<crate::SharedModule>) -> Self {
        self.lib_len = Some(self.context.global.lib.len());
        self.context.global.lib.push(module.into());
        self
    }
    /// _(internals)_ Increment the current nesting level of function calls.
    /// Exported under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[inline(always)]
    pub fn up_call_level(mut self) -> Self {
        self.level = Some(self.context.global.level);
        self.context.global.level += 1;
        self
    }
}
