//! Module defining interfaces to native-Rust functions.

use super::call::FnCallArgs;
use crate::ast::FnCallHashes;
use crate::eval::{Caches, GlobalRuntimeState};
use crate::plugin::PluginFunction;
use crate::tokenizer::{Token, TokenizeState};
use crate::types::dynamic::Variant;
use crate::{
    calc_fn_hash, Dynamic, Engine, EvalContext, FuncArgs, Module, Position, RhaiResult,
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
    global: Option<&'a GlobalRuntimeState<'a>>,
    /// The current stack of loaded [modules][Module].
    lib: &'a [&'a Module],
    /// [Position] of the function call.
    pos: Position,
    /// The current nesting level of function calls.
    level: usize,
}

impl<'a, M: AsRef<[&'a Module]> + ?Sized, S: AsRef<str> + 'a + ?Sized>
    From<(
        &'a Engine,
        &'a S,
        Option<&'a S>,
        &'a GlobalRuntimeState<'a>,
        &'a M,
        Position,
        usize,
    )> for NativeCallContext<'a>
{
    #[inline(always)]
    fn from(
        value: (
            &'a Engine,
            &'a S,
            Option<&'a S>,
            &'a GlobalRuntimeState,
            &'a M,
            Position,
            usize,
        ),
    ) -> Self {
        Self {
            engine: value.0,
            fn_name: value.1.as_ref(),
            source: value.2.map(<_>::as_ref),
            global: Some(value.3),
            lib: value.4.as_ref(),
            pos: value.5,
            level: value.6,
        }
    }
}

impl<'a, M: AsRef<[&'a Module]> + ?Sized, S: AsRef<str> + 'a + ?Sized>
    From<(&'a Engine, &'a S, &'a M)> for NativeCallContext<'a>
{
    #[inline(always)]
    fn from(value: (&'a Engine, &'a S, &'a M)) -> Self {
        Self {
            engine: value.0,
            fn_name: value.1.as_ref(),
            source: None,
            global: None,
            lib: value.2.as_ref(),
            pos: Position::NONE,
            level: 0,
        }
    }
}

impl<'a> NativeCallContext<'a> {
    /// _(internals)_ Create a new [`NativeCallContext`].
    /// Exported under the `metadata` feature only.
    #[deprecated(
        since = "1.3.0",
        note = "`NativeCallContext::new` will be moved under `internals`. Use `FnPtr::call` to call a function pointer directly."
    )]
    #[inline(always)]
    #[must_use]
    pub fn new(
        engine: &'a Engine,
        fn_name: &'a (impl AsRef<str> + 'a + ?Sized),
        lib: &'a [&Module],
    ) -> Self {
        Self {
            engine,
            fn_name: fn_name.as_ref(),
            source: None,
            global: None,
            lib,
            pos: Position::NONE,
            level: 0,
        }
    }
    /// _(internals)_ Create a new [`NativeCallContext`].
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_module`.
    #[cfg(feature = "internals")]
    #[cfg(not(feature = "no_module"))]
    #[inline(always)]
    #[must_use]
    pub fn new_with_all_fields(
        engine: &'a Engine,
        fn_name: &'a (impl AsRef<str> + 'a + ?Sized),
        source: Option<&'a (impl AsRef<str> + 'a + ?Sized)>,
        global: &'a GlobalRuntimeState,
        lib: &'a [&Module],
        pos: Position,
        level: usize,
    ) -> Self {
        Self {
            engine,
            fn_name: fn_name.as_ref(),
            source: source.map(<_>::as_ref),
            global: Some(global),
            lib,
            pos,
            level,
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
        self.level
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
    pub fn tag(&self) -> Option<&Dynamic> {
        self.global.as_ref().map(|g| &g.tag)
    }
    /// Get an iterator over the current set of modules imported via `import` statements
    /// in reverse order.
    ///
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    #[inline]
    pub fn iter_imports(&self) -> impl Iterator<Item = (&str, &Module)> {
        self.global.iter().flat_map(|&g| g.iter_imports())
    }
    /// Get an iterator over the current set of modules imported via `import` statements in reverse order.
    #[cfg(not(feature = "no_module"))]
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn iter_imports_raw(
        &self,
    ) -> impl Iterator<Item = (&crate::ImmutableString, &Shared<Module>)> {
        self.global.iter().flat_map(|&g| g.iter_imports_raw())
    }
    /// _(internals)_ The current [`GlobalRuntimeState`], if any.
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_module`.
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub const fn global_runtime_state(&self) -> Option<&GlobalRuntimeState> {
        self.global
    }
    /// Get an iterator over the namespaces containing definitions of all script-defined functions
    /// in reverse order (i.e. parent namespaces are iterated after child namespaces).
    #[inline]
    pub fn iter_namespaces(&self) -> impl Iterator<Item = &Module> {
        self.lib.iter().copied()
    }
    /// _(internals)_ The current stack of namespaces containing definitions of all script-defined functions.
    /// Exported under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub const fn namespaces(&self) -> &[&Module] {
        self.lib
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

        let mut args: StaticVec<_> = arg_values.iter_mut().collect();

        let result = self.call_fn_raw(fn_name, false, false, &mut args)?;

        let typ = self.engine().map_type_name(result.type_name());

        result.try_cast().ok_or_else(|| {
            let t = self.engine().map_type_name(type_name::<T>()).into();
            ERR::ErrorMismatchOutputType(t, typ.into(), Position::NONE).into()
        })
    }
    /// Call a function inside the call context.
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
    pub fn call_fn_raw(
        &self,
        fn_name: impl AsRef<str>,
        is_ref_mut: bool,
        is_method_call: bool,
        args: &mut [&mut Dynamic],
    ) -> RhaiResult {
        let mut global = self
            .global
            .cloned()
            .unwrap_or_else(|| GlobalRuntimeState::new(self.engine()));
        let mut caches = Caches::new();

        let fn_name = fn_name.as_ref();
        let args_len = args.len();

        let hash = if is_method_call {
            FnCallHashes::from_all(
                #[cfg(not(feature = "no_function"))]
                calc_fn_hash(fn_name, args_len - 1),
                calc_fn_hash(fn_name, args_len),
            )
        } else {
            calc_fn_hash(fn_name, args_len).into()
        };

        self.engine()
            .exec_fn_call(
                None,
                &mut global,
                &mut caches,
                self.lib,
                fn_name,
                hash,
                args,
                is_ref_mut,
                is_method_call,
                Position::NONE,
                self.level + 1,
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
    shared_try_take(value).ok().expect("not shared")
}

/// Lock a [`Locked`] resource for mutable access.
#[inline(always)]
#[must_use]
#[allow(dead_code)]
pub fn locked_read<T>(value: &Locked<T>) -> LockGuard<T> {
    #[cfg(not(feature = "sync"))]
    return value.borrow();

    #[cfg(feature = "sync")]
    return value.read().unwrap();
}

/// Lock a [`Locked`] resource for mutable access.
#[inline(always)]
#[must_use]
#[allow(dead_code)]
pub fn locked_write<T>(value: &Locked<T>) -> LockGuardMut<T> {
    #[cfg(not(feature = "sync"))]
    return value.borrow_mut();

    #[cfg(feature = "sync")]
    return value.write().unwrap();
}

/// General function trail object.
#[cfg(not(feature = "sync"))]
pub type FnAny = dyn Fn(NativeCallContext, &mut FnCallArgs) -> RhaiResult;
/// General function trail object.
#[cfg(feature = "sync")]
pub type FnAny = dyn Fn(NativeCallContext, &mut FnCallArgs) -> RhaiResult + Send + Sync;

/// Trail object for built-in functions.
pub type FnBuiltin = fn(NativeCallContext, &mut FnCallArgs) -> RhaiResult;

/// Function that gets an iterator from a type.
#[cfg(not(feature = "sync"))]
pub type IteratorFn = dyn Fn(Dynamic) -> Box<dyn Iterator<Item = RhaiResultOf<Dynamic>>>;
/// Function that gets an iterator from a type.
#[cfg(feature = "sync")]
pub type IteratorFn =
    dyn Fn(Dynamic) -> Box<dyn Iterator<Item = RhaiResultOf<Dynamic>>> + Send + Sync;

#[cfg(not(feature = "sync"))]
pub type FnPlugin = dyn PluginFunction;
#[cfg(feature = "sync")]
pub type FnPlugin = dyn PluginFunction + Send + Sync;

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
