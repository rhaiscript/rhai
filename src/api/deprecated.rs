//! Module containing all deprecated API that will be removed in the next major version.

use crate::func::{RhaiFunc, SendSync};
use crate::types::dynamic::Variant;
use crate::{
    Dynamic, Engine, EvalAltResult, FnAccess, FnNamespace, FnPtr, FuncRegistration, Identifier,
    ImmutableString, Module, NativeCallContext, Position, RhaiNativeFunc, RhaiResult, RhaiResultOf,
    Scope, SharedModule, TypeBuilder, AST,
};
use std::any::TypeId;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
use crate::func::register::Mut;

#[cfg(not(feature = "no_std"))]
#[cfg(any(not(target_family = "wasm"), not(target_os = "unknown")))]
use std::path::PathBuf;

impl Engine {
    /// Evaluate a file, but throw away the result and only return error (if any).
    /// Useful for when you don't need the result, but still need to keep track of possible errors.
    ///
    /// Not available under `no_std` or `WASM`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`run_file`][Engine::run_file] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.1.0", note = "use `run_file` instead")]
    #[cfg(not(feature = "no_std"))]
    #[cfg(any(not(target_family = "wasm"), not(target_os = "unknown")))]
    #[inline(always)]
    pub fn consume_file(&self, path: PathBuf) -> RhaiResultOf<()> {
        self.run_file(path)
    }

    /// Evaluate a file with own scope, but throw away the result and only return error (if any).
    /// Useful for when you don't need the result, but still need to keep track of possible errors.
    ///
    /// Not available under `no_std` or `WASM`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`run_file_with_scope`][Engine::run_file_with_scope] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.1.0", note = "use `run_file_with_scope` instead")]
    #[cfg(not(feature = "no_std"))]
    #[cfg(any(not(target_family = "wasm"), not(target_os = "unknown")))]
    #[inline(always)]
    pub fn consume_file_with_scope(&self, scope: &mut Scope, path: PathBuf) -> RhaiResultOf<()> {
        self.run_file_with_scope(scope, path)
    }

    /// Evaluate a string, but throw away the result and only return error (if any).
    /// Useful for when you don't need the result, but still need to keep track of possible errors.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`run`][Engine::run] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.1.0", note = "use `run` instead")]
    #[inline(always)]
    pub fn consume(&self, script: &str) -> RhaiResultOf<()> {
        self.run(script)
    }

    /// Evaluate a string with own scope, but throw away the result and only return error (if any).
    /// Useful for when you don't need the result, but still need to keep track of possible errors.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`run_with_scope`][Engine::run_with_scope] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.1.0", note = "use `run_with_scope` instead")]
    #[inline(always)]
    pub fn consume_with_scope(&self, scope: &mut Scope, script: &str) -> RhaiResultOf<()> {
        self.run_with_scope(scope, script)
    }

    /// Evaluate an [`AST`], but throw away the result and only return error (if any).
    /// Useful for when you don't need the result, but still need to keep track of possible errors.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`run_ast`][Engine::run_ast] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.1.0", note = "use `run_ast` instead")]
    #[inline(always)]
    pub fn consume_ast(&self, ast: &AST) -> RhaiResultOf<()> {
        self.run_ast(ast)
    }

    /// Evaluate an [`AST`] with own scope, but throw away the result and only return error (if any).
    /// Useful for when you don't need the result, but still need to keep track of possible errors.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`run_ast_with_scope`][Engine::run_ast_with_scope] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.1.0", note = "use `run_ast_with_scope` instead")]
    #[inline(always)]
    pub fn consume_ast_with_scope(&self, scope: &mut Scope, ast: &AST) -> RhaiResultOf<()> {
        self.run_ast_with_scope(scope, ast)
    }
    /// Call a script function defined in an [`AST`] with multiple [`Dynamic`] arguments
    /// and optionally a value for binding to the `this` pointer.
    ///
    /// Not available under `no_function`.
    ///
    /// There is an option to evaluate the [`AST`] to load necessary modules before calling the function.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`call_fn_with_options`][Engine::call_fn_with_options] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.1.0", note = "use `call_fn_with_options` instead")]
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub fn call_fn_dynamic(
        &self,
        scope: &mut Scope,
        ast: &AST,
        eval_ast: bool,
        name: impl AsRef<str>,
        this_ptr: Option<&mut Dynamic>,
        arg_values: impl AsMut<[Dynamic]>,
    ) -> RhaiResult {
        #[allow(deprecated)]
        self.call_fn_raw(scope, ast, eval_ast, true, name, this_ptr, arg_values)
    }
    /// Call a script function defined in an [`AST`] with multiple [`Dynamic`] arguments.
    ///
    /// Not available under `no_function`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`call_fn_with_options`][Engine::call_fn_with_options] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.12.0", note = "use `call_fn_with_options` instead")]
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub fn call_fn_raw(
        &self,
        scope: &mut Scope,
        ast: &AST,
        eval_ast: bool,
        rewind_scope: bool,
        name: impl AsRef<str>,
        this_ptr: Option<&mut Dynamic>,
        arg_values: impl AsMut<[Dynamic]>,
    ) -> RhaiResult {
        let mut arg_values = arg_values;

        let options = crate::CallFnOptions {
            this_ptr,
            eval_ast,
            rewind_scope,
            ..<_>::default()
        };

        self._call_fn(
            scope,
            &mut crate::eval::GlobalRuntimeState::new(self),
            &mut crate::eval::Caches::new(),
            ast,
            name.as_ref(),
            arg_values.as_mut(),
            options,
        )
    }
    /// Register a custom fallible function with the [`Engine`].
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`register_fn`][Engine::register_fn] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.9.1", note = "use `register_fn` instead")]
    #[inline(always)]
    pub fn register_result_fn<A: 'static, const N: usize, const X: bool, R: Variant + Clone>(
        &mut self,
        name: impl AsRef<str> + Into<Identifier>,
        func: impl RhaiNativeFunc<A, N, X, R, true> + SendSync + 'static,
    ) -> &mut Self {
        self.register_fn(name, func)
    }
    /// Register a getter function for a member of a registered type with the [`Engine`].
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under `no_object`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`register_get`][Engine::register_get] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.9.1", note = "use `register_get` instead")]
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn register_get_result<T: Variant + Clone, const X: bool, R: Variant + Clone>(
        &mut self,
        name: impl AsRef<str>,
        get_fn: impl RhaiNativeFunc<(Mut<T>,), 1, X, R, true> + SendSync + 'static,
    ) -> &mut Self {
        self.register_get(name, get_fn)
    }
    /// Register a setter function for a member of a registered type with the [`Engine`].
    ///
    /// Not available under `no_object`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`register_set`][Engine::register_set] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.9.1", note = "use `register_set` instead")]
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn register_set_result<T: Variant + Clone, V: Variant + Clone, const X: bool>(
        &mut self,
        name: impl AsRef<str>,
        set_fn: impl RhaiNativeFunc<(Mut<T>, V), 2, X, (), true> + SendSync + 'static,
    ) -> &mut Self {
        self.register_set(name, set_fn)
    }
    /// Register an index getter for a custom type with the [`Engine`].
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`register_indexer_get`][Engine::register_indexer_get] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.9.1", note = "use `register_indexer_get` instead")]
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline(always)]
    pub fn register_indexer_get_result<
        T: Variant + Clone,
        IDX: Variant + Clone,
        R: Variant + Clone,
        const X: bool,
    >(
        &mut self,
        get_fn: impl RhaiNativeFunc<(Mut<T>, IDX), 2, X, R, true> + SendSync + 'static,
    ) -> &mut Self {
        self.register_indexer_get(get_fn)
    }
    /// Register an index setter for a custom type with the [`Engine`].
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`register_indexer_set`][Engine::register_indexer_set] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.9.1", note = "use `register_indexer_set` instead")]
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline(always)]
    pub fn register_indexer_set_result<
        T: Variant + Clone,
        IDX: Variant + Clone,
        R: Variant + Clone,
        const X: bool,
    >(
        &mut self,
        set_fn: impl RhaiNativeFunc<(Mut<T>, IDX, R), 3, X, (), true> + SendSync + 'static,
    ) -> &mut Self {
        self.register_indexer_set(set_fn)
    }
    /// Register a custom syntax with the [`Engine`].
    ///
    /// Not available under `no_custom_syntax`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`register_custom_syntax_with_state_raw`][Engine::register_custom_syntax_with_state_raw] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(
        since = "1.11.0",
        note = "use `register_custom_syntax_with_state_raw` instead"
    )]
    #[inline(always)]
    #[cfg(not(feature = "no_custom_syntax"))]
    pub fn register_custom_syntax_raw(
        &mut self,
        key: impl Into<Identifier>,
        parse: impl Fn(&[ImmutableString], &str) -> crate::parser::ParseResult<Option<ImmutableString>>
            + SendSync
            + 'static,
        scope_may_be_changed: bool,
        func: impl Fn(&mut crate::EvalContext, &[crate::Expression]) -> RhaiResult + SendSync + 'static,
    ) -> &mut Self {
        self.register_custom_syntax_with_state_raw(
            key,
            move |keywords, look_ahead, _| parse(keywords, look_ahead),
            scope_may_be_changed,
            move |context, expressions, _| func(context, expressions),
        )
    }
    /// _(internals)_ Evaluate a list of statements with no `this` pointer.
    /// Exported under the `internals` feature only.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated. It will be removed in the next major version.
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[deprecated(since = "1.12.0")]
    pub fn eval_statements_raw(
        &self,
        global: &mut crate::eval::GlobalRuntimeState,
        caches: &mut crate::eval::Caches,
        scope: &mut Scope,
        statements: &[crate::ast::Stmt],
    ) -> RhaiResult {
        self.eval_global_statements(global, caches, scope, statements, true)
    }
}

impl Dynamic {
    /// Convert the [`Dynamic`] into a [`String`] and return it.
    /// If there are other references to the same string, a cloned copy is returned.
    /// Returns the name of the actual type if the cast fails.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`into_string`][Dynamic::into_string] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.1.0", note = "use `into_string` instead")]
    #[inline(always)]
    pub fn as_string(self) -> Result<String, &'static str> {
        self.into_string()
    }

    /// Convert the [`Dynamic`] into an [`ImmutableString`] and return it.
    /// Returns the name of the actual type if the cast fails.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`into_immutable_string`][Dynamic::into_immutable_string] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.1.0", note = "use `into_immutable_string` instead")]
    #[inline(always)]
    pub fn as_immutable_string(self) -> Result<ImmutableString, &'static str> {
        self.into_immutable_string()
    }
}

impl AST {
    /// _(internals)_ Get the internal [`Module`][crate::Module] containing all script-defined functions.
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_function`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated. Use [`shared_lib`][AST::shared_lib] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.3.0", note = "use `shared_lib` instead")]
    #[cfg(feature = "internals")]
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    #[must_use]
    pub fn lib(&self) -> &crate::Module {
        self.shared_lib()
    }
    /// Clear the documentation.
    /// Exported under the `metadata` feature only.
    ///
    /// # Deprecated
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.17.0")]
    #[cfg(feature = "metadata")]
    #[inline(always)]
    pub fn clear_doc(&mut self) -> &mut Self {
        self.doc.clear();
        self
    }
}

impl NativeCallContext<'_> {
    /// Create a new [`NativeCallContext`].
    ///
    /// # Unimplemented
    ///
    /// This method is deprecated. It is no longer implemented and always panics.
    ///
    /// Use [`FnPtr::call`] to call a function pointer directly.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(
        since = "1.3.0",
        note = "use `FnPtr::call` to call a function pointer directly."
    )]
    #[inline(always)]
    #[must_use]
    #[allow(unused_variables)]
    pub fn new(engine: &Engine, fn_name: &str, lib: &[SharedModule]) -> Self {
        unimplemented!("`NativeCallContext::new` is deprecated");
    }

    /// Call a function inside the call context.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`call_fn_raw`][NativeCallContext::call_fn_raw] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.2.0", note = "use `call_fn_raw` instead")]
    #[inline(always)]
    pub fn call_fn_dynamic_raw(
        &self,
        fn_name: impl AsRef<str>,
        is_method_call: bool,
        args: &mut [&mut Dynamic],
    ) -> RhaiResult {
        self.call_fn_raw(fn_name.as_ref(), is_method_call, is_method_call, args)
    }
}

#[allow(useless_deprecated)]
#[deprecated(since = "1.2.0", note = "explicitly wrap `EvalAltResult` in `Err`")]
impl<T> From<EvalAltResult> for RhaiResultOf<T> {
    #[inline(always)]
    fn from(err: EvalAltResult) -> Self {
        Err(err.into())
    }
}

impl FnPtr {
    /// Get the number of curried arguments.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`curry().len()`][`FnPtr::curry`] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.8.0", note = "use `curry().len()` instead")]
    #[inline(always)]
    #[must_use]
    pub fn num_curried(&self) -> usize {
        self.curry().len()
    }
    /// Call the function pointer with curried arguments (if any).
    /// The function may be script-defined (not available under `no_function`) or native Rust.
    ///
    /// This method is intended for calling a function pointer that is passed into a native Rust
    /// function as an argument.  Therefore, the [`AST`] is _NOT_ evaluated before calling the
    /// function.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`call_within_context`][FnPtr::call_within_context] or [`call_raw`][FnPtr::call_raw] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(
        since = "1.3.0",
        note = "use `call_within_context` or `call_raw` instead"
    )]
    #[inline(always)]
    pub fn call_dynamic(
        &self,
        context: &NativeCallContext,
        this_ptr: Option<&mut Dynamic>,
        arg_values: impl AsMut<[Dynamic]>,
    ) -> RhaiResult {
        self.call_raw(context, this_ptr, arg_values)
    }
}

#[cfg(not(feature = "no_custom_syntax"))]
impl crate::Expression<'_> {
    /// If this expression is a variable name, return it.  Otherwise [`None`].
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`get_string_value`][crate::Expression::get_string_value] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.4.0", note = "use `get_string_value` instead")]
    #[inline(always)]
    #[must_use]
    pub fn get_variable_name(&self) -> Option<&str> {
        self.get_string_value()
    }
}

impl Position {
    /// Create a new [`Position`].
    ///
    /// If `line` is zero, then [`None`] is returned.
    ///
    /// If `position` is zero, then it is at the beginning of a line.
    ///
    /// # Deprecated
    ///
    /// This function is deprecated.
    /// Use [`new`][Position::new] (which panics when `line` is zero) instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.6.0", note = "use `new` instead")]
    #[inline(always)]
    #[must_use]
    pub const fn new_const(line: u16, position: u16) -> Option<Self> {
        if line == 0 {
            None
        } else {
            Some(Self::new(line, position))
        }
    }
}

#[allow(deprecated)]
impl<T: Variant + Clone> TypeBuilder<'_, T> {
    /// Register a custom fallible function.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`with_fn`][`TypeBuilder::with_fn`] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.9.1", note = "use `with_fn` instead")]
    #[inline(always)]
    pub fn with_result_fn<S, A: 'static, const N: usize, const X: bool, R, FUNC>(
        &mut self,
        name: S,
        method: FUNC,
    ) -> &mut Self
    where
        S: AsRef<str> + Into<Identifier>,
        R: Variant + Clone,
        FUNC: RhaiNativeFunc<A, N, X, R, true> + SendSync + 'static,
    {
        self.with_fn(name, method)
    }

    /// Register a fallible getter function.
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under `no_object`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`with_get`][`TypeBuilder::with_get`] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.9.1", note = "use `with_get` instead")]
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn with_get_result<const X: bool, R: Variant + Clone>(
        &mut self,
        name: impl AsRef<str>,
        get_fn: impl RhaiNativeFunc<(Mut<T>,), 1, X, R, true> + SendSync + 'static,
    ) -> &mut Self {
        self.with_get(name, get_fn)
    }

    /// Register a fallible setter function.
    ///
    /// Not available under `no_object`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`with_set`][`TypeBuilder::with_set`] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.9.1", note = "use `with_set` instead")]
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn with_set_result<const X: bool, R: Variant + Clone>(
        &mut self,
        name: impl AsRef<str>,
        set_fn: impl RhaiNativeFunc<(Mut<T>, R), 2, X, (), true> + SendSync + 'static,
    ) -> &mut Self {
        self.with_set(name, set_fn)
    }

    /// Register an fallible index getter.
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`with_indexer_get`][`TypeBuilder::with_indexer_get`] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.9.1", note = "use `with_indexer_get` instead")]
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline(always)]
    pub fn with_indexer_get_result<IDX: Variant + Clone, R: Variant + Clone, const X: bool>(
        &mut self,
        get_fn: impl RhaiNativeFunc<(Mut<T>, IDX), 2, X, R, true> + SendSync + 'static,
    ) -> &mut Self {
        self.with_indexer_get(get_fn)
    }

    /// Register an fallible index setter.
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`with_indexer_set`][`TypeBuilder::with_indexer_set`] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.9.1", note = "use `with_indexer_set` instead")]
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline(always)]
    pub fn with_indexer_set_result<IDX: Variant + Clone, R: Variant + Clone, const X: bool>(
        &mut self,
        set_fn: impl RhaiNativeFunc<(Mut<T>, IDX, R), 3, X, (), true> + SendSync + 'static,
    ) -> &mut Self {
        self.with_indexer_set(set_fn)
    }
}

impl Module {
    /// Create a new [`Module`] with a pre-sized capacity for functions.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`new`][`Module::new`] instead.
    ///
    /// This method will be removed in the next major version.
    #[inline(always)]
    #[must_use]
    #[deprecated(since = "1.12.0", note = "use `new` instead")]
    pub const fn with_capacity(_capacity: usize) -> Self {
        Self::new()
    }

    /// Get the display name of a registered custom type.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`get_custom_type_display_by_name`][`Module::get_custom_type_display_by_name`] instead.
    ///
    /// This method will be removed in the next major version.
    #[inline(always)]
    #[must_use]
    #[deprecated(
        since = "1.16.0",
        note = "use `get_custom_type_display_by_name` instead"
    )]
    pub fn get_custom_type(&self, type_name: &str) -> Option<&str> {
        self.get_custom_type_display_by_name(type_name)
    }

    /// Set a native Rust function into the [`Module`], returning a [`u64`] hash key.
    ///
    /// If there is an existing Rust function of the same hash, it is replaced.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use the [`FuncRegistration`] API instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.17.0", note = "use the `FuncRegistration` API instead")]
    #[inline(always)]
    pub fn set_fn(
        &mut self,
        name: impl Into<Identifier>,
        namespace: FnNamespace,
        _access: FnAccess,
        arg_names: Option<&[&str]>,
        arg_types: impl AsRef<[TypeId]>,
        func: RhaiFunc,
    ) -> u64 {
        let _arg_names = arg_names;

        let fx = FuncRegistration::new(name).with_namespace(namespace);

        #[cfg(feature = "metadata")]
        let fx = if let Some(arg_names) = _arg_names {
            fx.with_params_info(arg_names)
        } else {
            fx
        };

        fx.set_into_module_raw(self, arg_types, func).hash
    }

    /// _(metadata)_ Set a native Rust function into the [`Module`], returning a [`u64`] hash key.
    /// Exported under the `metadata` feature only.
    ///
    /// If there is an existing Rust function of the same hash, it is replaced.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use the [`FuncRegistration`] API instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.17.0", note = "use the `FuncRegistration` API instead")]
    #[cfg(feature = "metadata")]
    #[inline(always)]
    pub fn set_fn_with_comments<C: AsRef<str>>(
        &mut self,
        name: impl Into<Identifier>,
        namespace: FnNamespace,
        _access: FnAccess,
        arg_names: Option<&[&str]>,
        arg_types: impl AsRef<[TypeId]>,
        comments: impl IntoIterator<Item = C>,
        func: RhaiFunc,
    ) -> u64 {
        FuncRegistration::new(name)
            .with_namespace(namespace)
            .with_params_info(arg_names.unwrap_or(&[]))
            .with_comments(comments)
            .set_into_module_raw(self, arg_types, func)
            .hash
    }

    /// _(metadata)_ Update the metadata (parameter names/types and return type) of a registered function.
    /// Exported under the `metadata` feature only.
    ///
    /// The [`u64`] hash is returned by the [`set_native_fn`][Module::set_native_fn] call.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`update_fn_metadata_with_comments`][`Module::update_fn_metadata_with_comments`] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(
        since = "1.17.0",
        note = "use `update_fn_metadata_with_comments` instead"
    )]
    #[cfg(feature = "metadata")]
    #[inline(always)]
    #[allow(deprecated)]
    pub fn update_fn_metadata<S: Into<Identifier>>(
        &mut self,
        hash_fn: u64,
        arg_names: impl IntoIterator<Item = S>,
    ) -> &mut Self {
        self.update_fn_metadata_with_comments(hash_fn, arg_names, [""; 0])
    }

    /// _(metadata)_ Generate signatures for all the non-private functions in the [`Module`].
    /// Exported under the `metadata` feature only.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use [`gen_fn_signatures_with_mapper`][`Module::gen_fn_signatures_with_mapper`] instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.17.0", note = "use `gen_fn_signatures_with_mapper` instead")]
    #[cfg(feature = "metadata")]
    #[inline(always)]
    pub fn gen_fn_signatures(&self) -> impl Iterator<Item = String> + '_ {
        self.gen_fn_signatures_with_mapper(|s| s.into())
    }
}

#[cfg(not(feature = "no_index"))]
use crate::plugin::*;

#[cfg(not(feature = "no_index"))]
#[export_module]
pub mod deprecated_array_functions {
    use crate::packages::array_basic::array_functions::*;
    use crate::{Array, INT};

    /// Iterate through all the elements in the array, applying a function named by `mapper` to each
    /// element in turn, and return the results as a new array.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.map(Fn("fn_name"))` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `mapper` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn square(x) { x * x }
    ///
    /// fn multiply(x, i) { x * i }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.map("square");
    ///
    /// print(y);       // prints "[1, 4, 9, 16, 25]"
    ///
    /// let y = x.map("multiply");
    ///
    /// print(y);       // prints "[0, 2, 6, 12, 20]"
    /// ```
    #[rhai_fn(name = "map", return_raw)]
    pub fn map_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        mapper: &str,
    ) -> RhaiResultOf<Array> {
        map(ctx, array, FnPtr::new(mapper)?)
    }
    /// Iterate through all the elements in the array, applying a function named by `filter` to each
    /// element in turn, and return a copy of all elements (in order) that return `true` as a new array.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.filter(Fn("fn_name"))` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn screen(x, i) { x * i >= 10 }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.filter("is_odd");
    ///
    /// print(y);       // prints "[1, 3, 5]"
    ///
    /// let y = x.filter("screen");
    ///
    /// print(y);       // prints "[12, 20]"
    /// ```
    #[rhai_fn(name = "filter", return_raw)]
    pub fn filter_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        filter_func: &str,
    ) -> RhaiResultOf<Array> {
        filter(ctx, array, FnPtr::new(filter_func)?)
    }
    /// Iterate through all the elements in the array, applying a function named by `filter` to each
    /// element in turn, and return the index of the first element that returns `true`.
    /// If no element returns `true`, `-1` is returned.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.index_of(Fn("fn_name"))` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn is_special(x) { x > 3 }
    ///
    /// fn is_dumb(x) { x > 8 }
    ///
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.index_of("is_special"));    // prints 3
    ///
    /// print(x.index_of("is_dumb"));       // prints -1
    /// ```
    #[rhai_fn(name = "index_of", return_raw, pure)]
    pub fn index_of_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: &str,
    ) -> RhaiResultOf<INT> {
        index_of_filter(ctx, array, FnPtr::new(filter)?)
    }
    /// Iterate through all the elements in the array, starting from a particular `start` position,
    /// applying a function named by `filter` to each element in turn, and return the index of the
    /// first element that returns `true`. If no element returns `true`, `-1` is returned.
    ///
    /// * If `start` < 0, position counts from the end of the array (`-1` is the last element).
    /// * If `start` < -length of array, position counts from the beginning of the array.
    /// * If `start` ≥ length of array, `-1` is returned.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.index_of(Fn("fn_name"), start)` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn plural(x) { x > 1 }
    ///
    /// fn singular(x) { x < 2 }
    ///
    /// fn screen(x, i) { x * i > 20 }
    ///
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.index_of("plural", 3));     // prints 5: 2 > 1
    ///
    /// print(x.index_of("singular", 9));   // prints -1: nothing < 2 past index 9
    ///
    /// print(x.index_of("plural", 15));    // prints -1: nothing found past end of array
    ///
    /// print(x.index_of("plural", -5));    // prints 9: -5 = start from index 8
    ///
    /// print(x.index_of("plural", -99));   // prints 1: -99 = start from beginning
    ///
    /// print(x.index_of("screen", 8));     // prints 10: 3 * 10 > 20
    /// ```
    #[rhai_fn(name = "index_of", return_raw, pure)]
    pub fn index_of_by_fn_name_starting_from(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: &str,
        start: INT,
    ) -> RhaiResultOf<INT> {
        index_of_filter_starting_from(ctx, array, FnPtr::new(filter)?, start)
    }
    /// Return `true` if any element in the array that returns `true` when applied a function named
    /// by `filter`.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.some(Fn("fn_name"))` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn large(x) { x > 3 }
    ///
    /// fn huge(x) { x > 10 }
    ///
    /// fn screen(x, i) { i > x }
    ///
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.some("large"));     // prints true
    ///
    /// print(x.some("huge"));      // prints false
    ///
    /// print(x.some("screen"));    // prints true
    /// ```
    #[rhai_fn(name = "some", return_raw, pure)]
    pub fn some_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: &str,
    ) -> RhaiResultOf<bool> {
        some(ctx, array, FnPtr::new(filter)?)
    }
    /// Return `true` if all elements in the array return `true` when applied a function named by `filter`.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.all(Fn("fn_name"))` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];
    ///
    /// print(x.all(|v| v > 3));        // prints false
    ///
    /// print(x.all(|v| v > 1));        // prints true
    ///
    /// print(x.all(|v, i| i > v));     // prints false
    /// ```
    #[rhai_fn(name = "all", return_raw, pure)]
    pub fn all_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: &str,
    ) -> RhaiResultOf<bool> {
        all(ctx, array, FnPtr::new(filter)?)
    }
    /// Remove duplicated _consecutive_ elements from the array that return `true` when applied a
    /// function named by `comparer`.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.dedup(Fn("fn_name"))` instead.
    ///
    /// No element is removed if the correct `comparer` function does not exist.
    ///
    /// # Function Parameters
    ///
    /// * `element1`: copy of the current array element to compare
    /// * `element2`: copy of the next array element to compare
    ///
    /// ## Return Value
    ///
    /// `true` if `element1 == element2`, otherwise `false`.
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn declining(a, b) { a >= b }
    ///
    /// let x = [1, 2, 2, 2, 3, 1, 2, 3, 4, 3, 3, 2, 1];
    ///
    /// x.dedup("declining");
    ///
    /// print(x);       // prints "[1, 2, 3, 4]"
    /// ```
    #[rhai_fn(name = "dedup", return_raw)]
    pub fn dedup_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        comparer: &str,
    ) -> RhaiResultOf<()> {
        dedup_by_comparer(ctx, array, FnPtr::new(comparer)?);
        Ok(())
    }
    /// Reduce an array by iterating through all elements while applying a function named by `reducer`.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.reduce(Fn("fn_name"))` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `reducer` must exist taking these parameters:
    ///
    /// * `result`: accumulated result, initially `()`
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn process(r, x) {
    ///     x + (r ?? 0)
    /// }
    /// fn process_extra(r, x, i) {
    ///     x + i + (r ?? 0)
    /// }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.reduce("process");
    ///
    /// print(y);       // prints 15
    ///
    /// let y = x.reduce("process_extra");
    ///
    /// print(y);       // prints 25
    /// ```
    #[rhai_fn(name = "reduce", return_raw, pure)]
    pub fn reduce_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: &str,
    ) -> RhaiResult {
        reduce(ctx, array, FnPtr::new(reducer)?)
    }
    /// Reduce an array by iterating through all elements while applying a function named by `reducer`.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.reduce(Fn("fn_name"), initial)` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `reducer` must exist taking these parameters:
    ///
    /// * `result`: accumulated result, starting with the value of `initial`
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn process(r, x) { x + r }
    ///
    /// fn process_extra(r, x, i) { x + i + r }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.reduce("process", 5);
    ///
    /// print(y);       // prints 20
    ///
    /// let y = x.reduce("process_extra", 5);
    ///
    /// print(y);       // prints 30
    /// ```
    #[rhai_fn(name = "reduce", return_raw, pure)]
    pub fn reduce_by_fn_name_with_initial(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: &str,
        initial: Dynamic,
    ) -> RhaiResult {
        reduce_with_initial(ctx, array, FnPtr::new(reducer)?, initial)
    }
    /// Reduce an array by iterating through all elements, in _reverse_ order,
    /// while applying a function named by `reducer`.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.reduce_rev(Fn("fn_name"))` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `reducer` must exist taking these parameters:
    ///
    /// * `result`: accumulated result, initially `()`
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn process(r, x) {
    ///     x + (r ?? 0)
    /// }
    /// fn process_extra(r, x, i) {
    ///     x + i + (r ?? 0)
    /// }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.reduce_rev("process");
    ///
    /// print(y);       // prints 15
    ///
    /// let y = x.reduce_rev("process_extra");
    ///
    /// print(y);       // prints 25
    /// ```
    #[rhai_fn(name = "reduce_rev", return_raw, pure)]
    pub fn reduce_rev_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: &str,
    ) -> RhaiResult {
        reduce_rev(ctx, array, FnPtr::new(reducer)?)
    }
    /// Reduce an array by iterating through all elements, in _reverse_ order,
    /// while applying a function named by `reducer`.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.reduce_rev(Fn("fn_name"), initial)` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `reducer` must exist taking these parameters:
    ///
    /// * `result`: accumulated result, starting with the value of `initial`
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn process(r, x) { x + r }
    ///
    /// fn process_extra(r, x, i) { x + i + r }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.reduce_rev("process", 5);
    ///
    /// print(y);       // prints 20
    ///
    /// let y = x.reduce_rev("process_extra", 5);
    ///
    /// print(y);       // prints 30
    /// ```
    #[rhai_fn(name = "reduce_rev", return_raw, pure)]
    pub fn reduce_rev_by_fn_name_with_initial(
        ctx: NativeCallContext,
        array: &mut Array,
        reducer: &str,
        initial: Dynamic,
    ) -> RhaiResult {
        reduce_rev_with_initial(ctx, array, FnPtr::new(reducer)?, initial)
    }
    /// Sort the array based on applying a function named by `comparer`.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.sort(Fn("fn_name"))` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `comparer` must exist taking these parameters:
    ///
    /// * `element1`: copy of the current array element to compare
    /// * `element2`: copy of the next array element to compare
    ///
    /// ## Return Value
    ///
    /// * Any integer > 0 if `element1 > element2`
    /// * Zero if `element1 == element2`
    /// * Any integer < 0 if `element1 < element2`
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn reverse(a, b) {
    ///     if a > b {
    ///         -1
    ///     } else if a < b {
    ///         1
    ///     } else {
    ///         0
    ///     }
    /// }
    /// let x = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];
    ///
    /// x.sort("reverse");
    ///
    /// print(x);       // prints "[10, 9, 8, 7, 6, 5, 4, 3, 2, 1]"
    /// ```
    #[rhai_fn(name = "sort", return_raw)]
    pub fn sort_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        comparer: &str,
    ) -> RhaiResultOf<()> {
        sort(ctx, array, FnPtr::new(comparer)?);
        Ok(())
    }
    /// Remove all elements in the array that returns `true` when applied a function named by `filter`
    /// and return them as a new array.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.drain(Fn("fn_name"))` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn small(x) { x < 3 }
    ///
    /// fn screen(x, i) { x + i > 5 }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.drain("small");
    ///
    /// print(x);       // prints "[3, 4, 5]"
    ///
    /// print(y);       // prints "[1, 2]"
    ///
    /// let z = x.drain("screen");
    ///
    /// print(x);       // prints "[3, 4]"
    ///
    /// print(z);       // prints "[5]"
    /// ```
    #[rhai_fn(name = "drain", return_raw)]
    pub fn drain_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: &str,
    ) -> RhaiResultOf<Array> {
        drain(ctx, array, FnPtr::new(filter)?)
    }
    /// Remove all elements in the array that do not return `true` when applied a function named by
    /// `filter` and return them as a new array.
    ///
    /// # Deprecated API
    ///
    /// This method is deprecated and will be removed from the next major version.
    /// Use `array.retain(Fn("fn_name"))` instead.
    ///
    /// # Function Parameters
    ///
    /// A function with the same name as the value of `filter` must exist taking these parameters:
    ///
    /// * `element`: copy of array element
    /// * `index` _(optional)_: current index in the array
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn large(x) { x >= 3 }
    ///
    /// fn screen(x, i) { x + i <= 5 }
    ///
    /// let x = [1, 2, 3, 4, 5];
    ///
    /// let y = x.retain("large");
    ///
    /// print(x);       // prints "[3, 4, 5]"
    ///
    /// print(y);       // prints "[1, 2]"
    ///
    /// let z = x.retain("screen");
    ///
    /// print(x);       // prints "[3, 4]"
    ///
    /// print(z);       // prints "[5]"
    /// ```
    #[rhai_fn(name = "retain", return_raw)]
    pub fn retain_by_fn_name(
        ctx: NativeCallContext,
        array: &mut Array,
        filter: &str,
    ) -> RhaiResultOf<Array> {
        retain(ctx, array, FnPtr::new(filter)?)
    }
}

pub mod config {
    pub mod hashing {
        /// Set the hashing seed. This is used to hash functions etc.
        ///
        /// # Deprecated
        ///
        /// This method is deprecated.
        /// Use [`set_hashing_seed`][crate::config::hashing::set_hashing_seed] instead.
        ///
        /// This method will be removed in the next major version.
        #[deprecated(since = "1.17.0", note = "use `set_hashing_seed` instead")]
        #[inline(always)]
        pub fn set_ahash_seed(new_seed: Option<[u64; 4]>) -> Result<(), Option<[u64; 4]>> {
            crate::config::hashing::set_hashing_seed(new_seed)
        }

        /// Get the current hashing Seed.
        ///
        /// # Deprecated
        ///
        /// This method is deprecated.
        /// Use [`get_hashing_seed`][crate::config::hashing::get_hashing_seed] instead.
        ///
        /// This method will be removed in the next major version.
        #[deprecated(since = "1.17.0", note = "use `get_hashing_seed` instead")]
        #[inline]
        #[must_use]
        pub fn get_ahash_seed() -> &'static Option<[u64; 4]> {
            crate::config::hashing::get_hashing_seed()
        }
    }
}
