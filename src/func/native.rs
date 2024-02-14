//! Module defining interfaces to native-Rust functions.

use super::call::FnCallArgs;
use crate::ast::FnCallHashes;
use crate::eval::{Caches, GlobalRuntimeState};
use crate::plugin::PluginFunc;
use crate::tokenizer::{is_valid_function_name, Token, TokenizeState};
use crate::types::dynamic::Variant;
use crate::{
    calc_fn_hash, Dynamic, Engine, EvalContext, FnArgsVec, FuncArgs, Position, RhaiResult,
    RhaiResultOf, StaticVec, VarDefInfo, ERR,
};
use std::any::type_name;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Trait that maps to `Send + Sync` only under the `sync` feature.
#[cfg(feature = "sync")]
pub trait SendSync: Send + Sync {}
/// Trait that maps to `Send + Sync` only under the `sync` feature.
#[cfg(feature = "sync")]
impl<T: Send + Sync> SendSync for T {}

/// Trait that maps to `Send + Sync` only under the `sync` feature.
#[cfg(not(feature = "sync"))]
pub trait SendSync {}
/// Trait that maps to `Send + Sync` only under the `sync` feature.
#[cfg(not(feature = "sync"))]
impl<T> SendSync for T {}

/// Immutable reference-counted container.
#[cfg(not(feature = "sync"))]
pub use std::rc::Rc as Shared;
/// Immutable reference-counted container.
#[cfg(feature = "sync")]
pub use std::sync::Arc as Shared;

/// Synchronized shared object.
#[cfg(not(feature = "sync"))]
pub use std::cell::RefCell as Locked;

/// Read-only lock guard for synchronized shared object.
#[cfg(not(feature = "sync"))]
pub type LockGuard<'a, T> = std::cell::Ref<'a, T>;

/// Mutable lock guard for synchronized shared object.
#[cfg(not(feature = "sync"))]
pub type LockGuardMut<'a, T> = std::cell::RefMut<'a, T>;

/// Synchronized shared object.
#[cfg(feature = "sync")]
#[allow(dead_code)]
pub use std::sync::RwLock as Locked;

/// Read-only lock guard for synchronized shared object.
#[cfg(feature = "sync")]
#[allow(dead_code)]
pub type LockGuard<'a, T> = std::sync::RwLockReadGuard<'a, T>;

/// Mutable lock guard for synchronized shared object.
#[cfg(feature = "sync")]
#[allow(dead_code)]
pub type LockGuardMut<'a, T> = std::sync::RwLockWriteGuard<'a, T>;

/// Context of a native Rust function call.
#[derive(Debug)]
pub struct NativeCallContext<'a> {
    /// The current [`Engine`].
    engine: &'a Engine,
    /// Name of function called.
    fn_name: &'a str,
    /// Function source, if any.
    source: Option<&'a str>,
    /// The current [`GlobalRuntimeState`], if any.
    global: &'a GlobalRuntimeState,
    /// [Position] of the function call.
    pos: Position,
}

/// _(internals)_ Context of a native Rust function call, intended for persistence.
/// Exported under the `internals` feature only.
///
/// # WARNING - Volatile Type
///
/// This type is volatile and may change in the future.
#[deprecated = "This type is NOT deprecated, but it is considered volatile and may change in the future."]
#[cfg(feature = "internals")]
#[derive(Debug, Clone)]
pub struct NativeCallContextStore {
    /// Name of function called.
    pub fn_name: String,
    /// Function source, if any.
    pub source: Option<String>,
    /// The current [`GlobalRuntimeState`], if any.
    pub global: GlobalRuntimeState,
    /// [Position] of the function call.
    pub pos: Position,
}

#[cfg(feature = "internals")]
#[allow(deprecated)]
impl NativeCallContextStore {
    /// Create a [`NativeCallContext`] from a [`NativeCallContextStore`].
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    #[deprecated = "This API is NOT deprecated, but it is considered volatile and may change in the future."]
    #[inline(always)]
    #[must_use]
    pub fn create_context<'a>(&'a self, engine: &'a Engine) -> NativeCallContext<'a> {
        NativeCallContext::from_stored_data(engine, self)
    }
}

impl<'a>
    From<(
        &'a Engine,
        &'a str,
        Option<&'a str>,
        &'a GlobalRuntimeState,
        Position,
    )> for NativeCallContext<'a>
{
    #[inline(always)]
    fn from(
        value: (
            &'a Engine,
            &'a str,
            Option<&'a str>,
            &'a GlobalRuntimeState,
            Position,
        ),
    ) -> Self {
        Self {
            engine: value.0,
            fn_name: value.1,
            source: value.2,
            global: value.3,
            pos: value.4,
        }
    }
}

impl<'a> NativeCallContext<'a> {
    /// _(internals)_ Create a new [`NativeCallContext`].
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_module`.
    #[cfg(feature = "internals")]
    #[cfg(not(feature = "no_module"))]
    #[inline(always)]
    #[must_use]
    pub const fn new_with_all_fields(
        engine: &'a Engine,
        fn_name: &'a str,
        source: Option<&'a str>,
        global: &'a GlobalRuntimeState,
        pos: Position,
    ) -> Self {
        Self {
            engine,
            fn_name,
            source,
            global,
            pos,
        }
    }

    /// _(internals)_ Create a [`NativeCallContext`] from a [`NativeCallContextStore`].
    /// Exported under the `internals` feature only.
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    #[deprecated = "This API is NOT deprecated, but it is considered volatile and may change in the future."]
    #[cfg(feature = "internals")]
    #[inline]
    #[must_use]
    #[allow(deprecated)]
    pub fn from_stored_data(engine: &'a Engine, context: &'a NativeCallContextStore) -> Self {
        Self {
            engine,
            fn_name: &context.fn_name,
            source: context.source.as_deref(),
            global: &context.global,
            pos: context.pos,
        }
    }
    /// _(internals)_ Store this [`NativeCallContext`] into a [`NativeCallContextStore`].
    /// Exported under the `internals` feature only.
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    #[deprecated = "This API is NOT deprecated, but it is considered volatile and may change in the future."]
    #[cfg(feature = "internals")]
    #[inline]
    #[must_use]
    #[allow(deprecated)]
    pub fn store_data(&self) -> NativeCallContextStore {
        NativeCallContextStore {
            fn_name: self.fn_name.to_string(),
            source: self.source.map(ToString::to_string),
            global: self.global.clone(),
            pos: self.pos,
        }
    }

    /// The current [`Engine`].
    #[inline(always)]
    #[must_use]
    pub const fn engine(&self) -> &Engine {
        self.engine
    }
    /// Name of the function called.
    #[inline(always)]
    #[must_use]
    pub const fn fn_name(&self) -> &str {
        self.fn_name
    }
    /// [Position] of the function call.
    #[inline(always)]
    #[must_use]
    pub const fn position(&self) -> Position {
        self.pos
    }
    /// Current nesting level of function calls.
    #[inline(always)]
    #[must_use]
    pub const fn call_level(&self) -> usize {
        self.global.level
    }
    /// The current source.
    #[inline(always)]
    #[must_use]
    pub const fn source(&self) -> Option<&str> {
        self.source
    }
    /// Custom state kept in a [`Dynamic`].
    #[inline(always)]
    #[must_use]
    pub const fn tag(&self) -> Option<&Dynamic> {
        Some(&self.global.tag)
    }
    /// Get an iterator over the current set of modules imported via `import` statements
    /// in reverse order.
    ///
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    #[inline]
    pub fn iter_imports(&self) -> impl Iterator<Item = (&str, &crate::Module)> {
        self.global.iter_imports()
    }
    /// _(internals)_ The current [`GlobalRuntimeState`], if any.
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_module`.
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub const fn global_runtime_state(&self) -> &GlobalRuntimeState {
        self.global
    }
    /// _(internals)_ The current [`GlobalRuntimeState`], if any.
    #[cfg(not(feature = "internals"))]
    #[inline(always)]
    #[must_use]
    #[allow(dead_code)]
    pub(crate) const fn global_runtime_state(&self) -> &GlobalRuntimeState {
        self.global
    }
    /// Get an iterator over the namespaces containing definitions of all script-defined functions
    /// in reverse order (i.e. parent namespaces are iterated after child namespaces).
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub fn iter_namespaces(&self) -> impl Iterator<Item = &crate::Module> {
        self.global.lib.iter().map(<_>::as_ref)
    }
    /// _(internals)_ The current stack of namespaces containing definitions of all script-defined functions.
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
    /// Call a function inside the call context with the provided arguments.
    #[inline]
    pub fn call_fn<T: Variant + Clone>(
        &self,
        fn_name: impl AsRef<str>,
        args: impl FuncArgs,
    ) -> RhaiResultOf<T> {
        let mut arg_values = StaticVec::new_const();
        args.parse(&mut arg_values);

        let args = &mut arg_values.iter_mut().collect::<FnArgsVec<_>>();

        self._call_fn_raw(fn_name, args, false, false, false)
            .and_then(|result| {
                result.try_cast_raw().map_err(|r| {
                    let result_type = self.engine().map_type_name(r.type_name());
                    let cast_type = match type_name::<T>() {
                        typ if typ.contains("::") => self.engine.map_type_name(typ),
                        typ => typ,
                    };
                    ERR::ErrorMismatchOutputType(
                        cast_type.into(),
                        result_type.into(),
                        Position::NONE,
                    )
                    .into()
                })
            })
    }
    /// Call a registered native Rust function inside the call context with the provided arguments.
    ///
    /// This is often useful because Rust functions typically only want to cross-call other
    /// registered Rust functions and not have to worry about scripted functions hijacking the
    /// process unknowingly (or deliberately).
    #[inline]
    pub fn call_native_fn<T: Variant + Clone>(
        &self,
        fn_name: impl AsRef<str>,
        args: impl FuncArgs,
    ) -> RhaiResultOf<T> {
        let mut arg_values = StaticVec::new_const();
        args.parse(&mut arg_values);

        let args = &mut arg_values.iter_mut().collect::<FnArgsVec<_>>();

        self._call_fn_raw(fn_name, args, true, false, false)
            .and_then(|result| {
                result.try_cast_raw().map_err(|r| {
                    let result_type = self.engine().map_type_name(r.type_name());
                    let cast_type = match type_name::<T>() {
                        typ if typ.contains("::") => self.engine.map_type_name(typ),
                        typ => typ,
                    };
                    ERR::ErrorMismatchOutputType(
                        cast_type.into(),
                        result_type.into(),
                        Position::NONE,
                    )
                    .into()
                })
            })
    }
    /// Call a function (native Rust or scripted) inside the call context.
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
        &self,
        fn_name: impl AsRef<str>,
        is_ref_mut: bool,
        is_method_call: bool,
        args: &mut [&mut Dynamic],
    ) -> RhaiResult {
        let name = fn_name.as_ref();
        let native_only = !is_valid_function_name(name);
        #[cfg(not(feature = "no_function"))]
        let native_only = native_only && !crate::parser::is_anonymous_fn(name);

        self._call_fn_raw(fn_name, args, native_only, is_ref_mut, is_method_call)
    }
    /// Call a registered native Rust function inside the call context.
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
        &self,
        fn_name: impl AsRef<str>,
        is_ref_mut: bool,
        args: &mut [&mut Dynamic],
    ) -> RhaiResult {
        self._call_fn_raw(fn_name, args, true, is_ref_mut, false)
    }

    /// Call a function (native Rust or scripted) inside the call context.
    fn _call_fn_raw(
        &self,
        fn_name: impl AsRef<str>,
        args: &mut [&mut Dynamic],
        native_only: bool,
        is_ref_mut: bool,
        is_method_call: bool,
    ) -> RhaiResult {
        let global = &mut self.global.clone();
        global.level += 1;

        let caches = &mut Caches::new();

        let fn_name = fn_name.as_ref();
        let op_token = Token::lookup_symbol_from_syntax(fn_name);
        let args_len = args.len();

        if native_only {
            return self
                .engine()
                .exec_native_fn_call(
                    global,
                    caches,
                    fn_name,
                    op_token.as_ref(),
                    calc_fn_hash(None, fn_name, args_len),
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

        self.engine()
            .exec_fn_call(
                global,
                caches,
                None,
                fn_name,
                op_token.as_ref(),
                hash,
                args,
                is_ref_mut,
                is_method_call,
                Position::NONE,
            )
            .map(|(r, ..)| r)
    }
}

/// Return a mutable reference to the wrapped value of a [`Shared`] resource.
/// If the resource is shared (i.e. has other outstanding references), a cloned copy is used.
#[inline(always)]
#[must_use]
#[allow(dead_code)]
pub fn shared_make_mut<T: Clone>(value: &mut Shared<T>) -> &mut T {
    Shared::make_mut(value)
}

/// Return a mutable reference to the wrapped value of a [`Shared`] resource.
#[inline(always)]
#[must_use]
#[allow(dead_code)]
pub fn shared_get_mut<T: Clone>(value: &mut Shared<T>) -> Option<&mut T> {
    Shared::get_mut(value)
}

/// Consume a [`Shared`] resource if is unique (i.e. not shared), or clone it otherwise.
#[inline]
#[must_use]
#[allow(dead_code)]
pub fn shared_take_or_clone<T: Clone>(value: Shared<T>) -> T {
    shared_try_take(value).unwrap_or_else(|v| v.as_ref().clone())
}

/// Consume a [`Shared`] resource if is unique (i.e. not shared).
#[inline(always)]
#[allow(dead_code)]
pub fn shared_try_take<T>(value: Shared<T>) -> Result<T, Shared<T>> {
    Shared::try_unwrap(value)
}

/// Consume a [`Shared`] resource, assuming that it is unique (i.e. not shared).
///
/// # Panics
///
/// Panics if the resource is shared (i.e. has other outstanding references).
#[inline]
#[must_use]
#[allow(dead_code)]
pub fn shared_take<T>(value: Shared<T>) -> T {
    shared_try_take(value)
        .ok()
        .unwrap_or_else(|| panic!("`value` is shared (i.e. has outstanding references)"))
}

/// _(internals)_ Lock a [`Locked`] resource for immutable access.
/// Exported under the `internals` feature only.
#[inline(always)]
#[must_use]
#[allow(dead_code)]
pub fn locked_read<T>(value: &Locked<T>) -> LockGuard<T> {
    #[cfg(not(feature = "sync"))]
    return value.borrow();

    #[cfg(feature = "sync")]
    return value.read().unwrap();
}

/// _(internals)_ Lock a [`Locked`] resource for mutable access.
/// Exported under the `internals` feature only.
#[inline(always)]
#[must_use]
#[allow(dead_code)]
pub fn locked_write<T>(value: &Locked<T>) -> LockGuardMut<T> {
    #[cfg(not(feature = "sync"))]
    return value.borrow_mut();

    #[cfg(feature = "sync")]
    return value.write().unwrap();
}

/// General Rust function trail object.
#[cfg(not(feature = "sync"))]
pub type FnAny = dyn Fn(Option<NativeCallContext>, &mut FnCallArgs) -> RhaiResult;
/// General Rust function trail object.
#[cfg(feature = "sync")]
pub type FnAny = dyn Fn(Option<NativeCallContext>, &mut FnCallArgs) -> RhaiResult + Send + Sync;

/// Built-in function trait object.
pub type FnBuiltin = (
    fn(Option<NativeCallContext>, &mut FnCallArgs) -> RhaiResult,
    bool,
);

/// Function that gets an iterator from a type.
#[cfg(not(feature = "sync"))]
pub type FnIterator = dyn Fn(Dynamic) -> Box<dyn Iterator<Item = RhaiResultOf<Dynamic>>>;
/// Function that gets an iterator from a type.
#[cfg(feature = "sync")]
pub type FnIterator =
    dyn Fn(Dynamic) -> Box<dyn Iterator<Item = RhaiResultOf<Dynamic>>> + Send + Sync;

/// Plugin function trait object.
#[cfg(not(feature = "sync"))]
pub type FnPlugin = dyn PluginFunc;
/// Plugin function trait object.
#[cfg(feature = "sync")]
pub type FnPlugin = dyn PluginFunc + Send + Sync;

/// Callback function for progress reporting.
#[cfg(not(feature = "unchecked"))]
#[cfg(not(feature = "sync"))]
pub type OnProgressCallback = dyn Fn(u64) -> Option<Dynamic>;
/// Callback function for progress reporting.
#[cfg(not(feature = "unchecked"))]
#[cfg(feature = "sync")]
pub type OnProgressCallback = dyn Fn(u64) -> Option<Dynamic> + Send + Sync;

/// Callback function for printing.
#[cfg(not(feature = "sync"))]
pub type OnPrintCallback = dyn Fn(&str);
/// Callback function for printing.
#[cfg(feature = "sync")]
pub type OnPrintCallback = dyn Fn(&str) + Send + Sync;

/// Callback function for debugging.
#[cfg(not(feature = "sync"))]
pub type OnDebugCallback = dyn Fn(&str, Option<&str>, Position);
/// Callback function for debugging.
#[cfg(feature = "sync")]
pub type OnDebugCallback = dyn Fn(&str, Option<&str>, Position) + Send + Sync;

/// Callback function for mapping tokens during parsing.
#[cfg(not(feature = "sync"))]
pub type OnParseTokenCallback = dyn Fn(Token, Position, &TokenizeState) -> Token;
/// Callback function for mapping tokens during parsing.
#[cfg(feature = "sync")]
pub type OnParseTokenCallback = dyn Fn(Token, Position, &TokenizeState) -> Token + Send + Sync;

/// Callback function for variable access.
#[cfg(not(feature = "sync"))]
pub type OnVarCallback = dyn Fn(&str, usize, EvalContext) -> RhaiResultOf<Option<Dynamic>>;
/// Callback function for variable access.
#[cfg(feature = "sync")]
pub type OnVarCallback =
    dyn Fn(&str, usize, EvalContext) -> RhaiResultOf<Option<Dynamic>> + Send + Sync;

/// Callback function for variable definition.
#[cfg(not(feature = "sync"))]
pub type OnDefVarCallback = dyn Fn(bool, VarDefInfo, EvalContext) -> RhaiResultOf<bool>;
/// Callback function for variable definition.
#[cfg(feature = "sync")]
pub type OnDefVarCallback =
    dyn Fn(bool, VarDefInfo, EvalContext) -> RhaiResultOf<bool> + Send + Sync;
