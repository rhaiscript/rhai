//! Module defining interfaces to native-Rust functions.

use crate::ast::{FnAccess, ScriptFnDef};
use crate::dynamic::Dynamic;
use crate::engine::{Engine, EvalContext, Imports};
use crate::module::Module;
use crate::plugin::PluginFunction;
use crate::result::EvalAltResult;
use crate::token::{is_valid_identifier, NO_POS};
use crate::utils::ImmutableString;
use crate::{calc_script_fn_hash, StaticVec};

#[cfg(not(feature = "no_function"))]
use crate::engine::FN_ANONYMOUS;

use crate::stdlib::{boxed::Box, convert::TryFrom, fmt, iter::empty, mem, string::String};

#[cfg(feature = "sync")]
use crate::stdlib::sync::{Arc, RwLock};
#[cfg(not(feature = "sync"))]
use crate::stdlib::{cell::RefCell, rc::Rc};

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
pub type Shared<T> = Rc<T>;
/// Immutable reference-counted container.
#[cfg(feature = "sync")]
pub type Shared<T> = Arc<T>;

/// Synchronized shared object.
#[cfg(not(feature = "sync"))]
pub type Locked<T> = RefCell<T>;
/// Synchronized shared object.
#[cfg(feature = "sync")]
pub type Locked<T> = RwLock<T>;

/// Context of native Rust function call.
#[derive(Debug, Copy, Clone)]
pub struct NativeCallContext<'e, 'a, 'm, 'pm: 'm> {
    engine: &'e Engine,
    mods: Option<&'a Imports>,
    lib: &'m [&'pm Module],
}

impl<'e, 'a, 'm, 'pm: 'm, M: AsRef<[&'pm Module]> + ?Sized>
    From<(&'e Engine, &'a mut Imports, &'m M)> for NativeCallContext<'e, 'a, 'm, 'pm>
{
    fn from(value: (&'e Engine, &'a mut Imports, &'m M)) -> Self {
        Self {
            engine: value.0,
            mods: Some(value.1),
            lib: value.2.as_ref(),
        }
    }
}

impl<'e, 'm, 'pm: 'm, M: AsRef<[&'pm Module]> + ?Sized> From<(&'e Engine, &'m M)>
    for NativeCallContext<'e, '_, 'm, 'pm>
{
    fn from(value: (&'e Engine, &'m M)) -> Self {
        Self {
            engine: value.0,
            mods: None,
            lib: value.1.as_ref(),
        }
    }
}

impl<'e, 'a, 'm, 'pm> NativeCallContext<'e, 'a, 'm, 'pm> {
    /// The current `Engine`.
    #[inline(always)]
    pub fn engine(&self) -> &'e Engine {
        self.engine
    }
    /// _[INTERNALS]_ The current set of modules imported via `import` statements.
    /// Available under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[cfg(not(feature = "no_module"))]
    #[inline(always)]
    pub fn imports(&self) -> Option<&Imports> {
        self.mods
    }
    /// Get an iterator over the namespaces containing definition of all script-defined functions.
    #[inline(always)]
    pub fn iter_namespaces(&self) -> impl Iterator<Item = &'pm Module> + 'm {
        self.lib.iter().cloned()
    }
}

/// Consume a `Shared` resource and return a mutable reference to the wrapped value.
/// If the resource is shared (i.e. has other outstanding references), a cloned copy is used.
pub fn shared_make_mut<T: Clone>(value: &mut Shared<T>) -> &mut T {
    #[cfg(not(feature = "sync"))]
    return Rc::make_mut(value);
    #[cfg(feature = "sync")]
    return Arc::make_mut(value);
}

/// Consume a `Shared` resource if is unique (i.e. not shared).
pub fn shared_try_take<T>(value: Shared<T>) -> Result<T, Shared<T>> {
    #[cfg(not(feature = "sync"))]
    return Rc::try_unwrap(value);
    #[cfg(feature = "sync")]
    return Arc::try_unwrap(value);
}

/// Consume a `Shared` resource, assuming that it is unique (i.e. not shared).
///
/// # Panics
///
/// Panics if the resource is shared (i.e. has other outstanding references).
pub fn shared_take<T>(value: Shared<T>) -> T {
    shared_try_take(value).map_err(|_| ()).unwrap()
}

pub type FnCallArgs<'a> = [&'a mut Dynamic];

/// A general function pointer, which may carry additional (i.e. curried) argument values
/// to be passed onto a function during a call.
#[derive(Debug, Clone, Default)]
pub struct FnPtr(ImmutableString, StaticVec<Dynamic>);

impl FnPtr {
    /// Create a new function pointer.
    #[inline(always)]
    pub(crate) fn new_unchecked(
        name: impl Into<ImmutableString>,
        curry: StaticVec<Dynamic>,
    ) -> Self {
        Self(name.into(), curry)
    }
    /// Get the name of the function.
    #[inline(always)]
    pub fn fn_name(&self) -> &str {
        self.get_fn_name().as_ref()
    }
    /// Get the name of the function.
    #[inline(always)]
    pub(crate) fn get_fn_name(&self) -> &ImmutableString {
        &self.0
    }
    /// Get the underlying data of the function pointer.
    #[inline(always)]
    pub(crate) fn take_data(self) -> (ImmutableString, StaticVec<Dynamic>) {
        (self.0, self.1)
    }
    /// Get the curried arguments.
    #[inline(always)]
    pub fn curry(&self) -> &[Dynamic] {
        self.1.as_ref()
    }
    /// Does this function pointer refer to an anonymous function?
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub fn is_anonymous(&self) -> bool {
        self.0.starts_with(FN_ANONYMOUS)
    }
    /// Call the function pointer with curried arguments (if any).
    ///
    /// If this function is a script-defined function, it must not be marked private.
    ///
    /// ## WARNING
    ///
    /// All the arguments are _consumed_, meaning that they're replaced by `()`.
    /// This is to avoid unnecessarily cloning the arguments.
    /// Do not use the arguments after this call. If they are needed afterwards,
    /// clone them _before_ calling this function.
    pub fn call_dynamic(
        &self,
        ctx: NativeCallContext,
        this_ptr: Option<&mut Dynamic>,
        mut arg_values: impl AsMut<[Dynamic]>,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let arg_values = arg_values.as_mut();
        let fn_name = self.fn_name();

        let mut args_data = self
            .curry()
            .iter()
            .cloned()
            .chain(arg_values.iter_mut().map(mem::take))
            .collect::<StaticVec<_>>();

        let has_this = this_ptr.is_some();
        let mut args = args_data.iter_mut().collect::<StaticVec<_>>();
        let hash_script = calc_script_fn_hash(empty(), fn_name, args.len());

        if let Some(obj) = this_ptr {
            args.insert(0, obj);
        }

        let mut mods = ctx.mods.cloned().unwrap_or_default();

        ctx.engine()
            .exec_fn_call(
                &mut mods,
                &mut Default::default(),
                ctx.lib,
                fn_name,
                hash_script,
                args.as_mut(),
                has_this,
                has_this,
                true,
                None,
                None,
                0,
            )
            .map(|(v, _)| v)
    }
}

impl fmt::Display for FnPtr {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fn({})", self.0)
    }
}

impl TryFrom<ImmutableString> for FnPtr {
    type Error = Box<EvalAltResult>;

    #[inline(always)]
    fn try_from(value: ImmutableString) -> Result<Self, Self::Error> {
        if is_valid_identifier(value.chars()) {
            Ok(Self(value, Default::default()))
        } else {
            EvalAltResult::ErrorFunctionNotFound(value.into(), NO_POS).into()
        }
    }
}

impl TryFrom<String> for FnPtr {
    type Error = Box<EvalAltResult>;

    #[inline(always)]
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let s: ImmutableString = value.into();
        Self::try_from(s)
    }
}

impl TryFrom<&str> for FnPtr {
    type Error = Box<EvalAltResult>;

    #[inline(always)]
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let s: ImmutableString = value.into();
        Self::try_from(s)
    }
}

/// A general function trail object.
#[cfg(not(feature = "sync"))]
pub type FnAny = dyn Fn(NativeCallContext, &mut FnCallArgs) -> Result<Dynamic, Box<EvalAltResult>>;
/// A general function trail object.
#[cfg(feature = "sync")]
pub type FnAny =
    dyn Fn(NativeCallContext, &mut FnCallArgs) -> Result<Dynamic, Box<EvalAltResult>> + Send + Sync;

/// A standard function that gets an iterator from a type.
pub type IteratorFn = fn(Dynamic) -> Box<dyn Iterator<Item = Dynamic>>;

#[cfg(not(feature = "sync"))]
pub type FnPlugin = dyn PluginFunction;
#[cfg(feature = "sync")]
pub type FnPlugin = dyn PluginFunction + Send + Sync;

/// A standard callback function.
#[cfg(not(feature = "sync"))]
pub type Callback<T, R> = Box<dyn Fn(&T) -> R + 'static>;
/// A standard callback function.
#[cfg(feature = "sync")]
pub type Callback<T, R> = Box<dyn Fn(&T) -> R + Send + Sync + 'static>;

/// A standard callback function.
#[cfg(not(feature = "sync"))]
pub type OnVarCallback =
    Box<dyn Fn(&str, usize, &EvalContext) -> Result<Option<Dynamic>, Box<EvalAltResult>> + 'static>;
/// A standard callback function.
#[cfg(feature = "sync")]
pub type OnVarCallback = Box<
    dyn Fn(&str, usize, &EvalContext) -> Result<Option<Dynamic>, Box<EvalAltResult>>
        + Send
        + Sync
        + 'static,
>;

/// A type encapsulating a function callable by Rhai.
#[derive(Clone)]
pub enum CallableFunction {
    /// A pure native Rust function with all arguments passed by value.
    Pure(Shared<FnAny>),
    /// A native Rust object method with the first argument passed by reference,
    /// and the rest passed by value.
    Method(Shared<FnAny>),
    /// An iterator function.
    Iterator(IteratorFn),
    /// A plugin function,
    Plugin(Shared<FnPlugin>),
    /// A script-defined function.
    #[cfg(not(feature = "no_function"))]
    Script(Shared<ScriptFnDef>),
}

impl fmt::Debug for CallableFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pure(_) => write!(f, "NativePureFunction"),
            Self::Method(_) => write!(f, "NativeMethod"),
            Self::Iterator(_) => write!(f, "NativeIterator"),
            Self::Plugin(_) => write!(f, "PluginFunction"),

            #[cfg(not(feature = "no_function"))]
            Self::Script(fn_def) => fmt::Debug::fmt(fn_def, f),
        }
    }
}

impl fmt::Display for CallableFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pure(_) => write!(f, "NativePureFunction"),
            Self::Method(_) => write!(f, "NativeMethod"),
            Self::Iterator(_) => write!(f, "NativeIterator"),
            Self::Plugin(_) => write!(f, "PluginFunction"),

            #[cfg(not(feature = "no_function"))]
            CallableFunction::Script(s) => fmt::Display::fmt(s, f),
        }
    }
}

impl CallableFunction {
    /// Is this a pure native Rust function?
    pub fn is_pure(&self) -> bool {
        match self {
            Self::Pure(_) => true,
            Self::Method(_) | Self::Iterator(_) => false,

            Self::Plugin(p) => !p.is_method_call(),

            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => false,
        }
    }
    /// Is this a native Rust method function?
    pub fn is_method(&self) -> bool {
        match self {
            Self::Method(_) => true,
            Self::Pure(_) | Self::Iterator(_) => false,

            Self::Plugin(p) => p.is_method_call(),

            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => false,
        }
    }
    /// Is this an iterator function?
    pub fn is_iter(&self) -> bool {
        match self {
            Self::Iterator(_) => true,
            Self::Pure(_) | Self::Method(_) | Self::Plugin(_) => false,

            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => false,
        }
    }
    /// Is this a Rhai-scripted function?
    pub fn is_script(&self) -> bool {
        match self {
            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => true,

            Self::Pure(_) | Self::Method(_) | Self::Iterator(_) | Self::Plugin(_) => false,
        }
    }
    /// Is this a plugin function?
    pub fn is_plugin_fn(&self) -> bool {
        match self {
            Self::Plugin(_) => true,
            Self::Pure(_) | Self::Method(_) | Self::Iterator(_) => false,

            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => false,
        }
    }
    /// Is this a native Rust function?
    pub fn is_native(&self) -> bool {
        match self {
            Self::Pure(_) | Self::Method(_) => true,
            Self::Plugin(_) => true,
            Self::Iterator(_) => true,

            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => false,
        }
    }
    /// Get the access mode.
    pub fn access(&self) -> FnAccess {
        match self {
            Self::Plugin(_) => FnAccess::Public,
            Self::Pure(_) | Self::Method(_) | Self::Iterator(_) => FnAccess::Public,

            #[cfg(not(feature = "no_function"))]
            Self::Script(f) => f.access,
        }
    }
    /// Get a shared reference to a native Rust function.
    ///
    /// # Panics
    ///
    /// Panics if the `CallableFunction` is not `Pure` or `Method`.
    pub fn get_native_fn(&self) -> &Shared<FnAny> {
        match self {
            Self::Pure(f) | Self::Method(f) => f,
            Self::Iterator(_) | Self::Plugin(_) => unreachable!(),

            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => unreachable!(),
        }
    }
    /// Get a shared reference to a script-defined function definition.
    ///
    /// # Panics
    ///
    /// Panics if the `CallableFunction` is not `Script`.
    #[cfg(not(feature = "no_function"))]
    pub fn get_fn_def(&self) -> &Shared<ScriptFnDef> {
        match self {
            Self::Pure(_) | Self::Method(_) | Self::Iterator(_) | Self::Plugin(_) => unreachable!(),
            Self::Script(f) => f,
        }
    }
    /// Get a reference to an iterator function.
    ///
    /// # Panics
    ///
    /// Panics if the `CallableFunction` is not `Iterator`.
    pub fn get_iter_fn(&self) -> IteratorFn {
        match self {
            Self::Iterator(f) => *f,
            Self::Pure(_) | Self::Method(_) | Self::Plugin(_) => unreachable!(),

            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => unreachable!(),
        }
    }
    /// Get a shared reference to a plugin function.
    ///
    /// # Panics
    ///
    /// Panics if the `CallableFunction` is not `Plugin`.
    pub fn get_plugin_fn<'s>(&'s self) -> &Shared<FnPlugin> {
        match self {
            Self::Plugin(f) => f,
            Self::Pure(_) | Self::Method(_) | Self::Iterator(_) => unreachable!(),

            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => unreachable!(),
        }
    }
    /// Create a new `CallableFunction::Pure`.
    #[inline(always)]
    pub fn from_pure(func: Box<FnAny>) -> Self {
        Self::Pure(func.into())
    }
    /// Create a new `CallableFunction::Method`.
    #[inline(always)]
    pub fn from_method(func: Box<FnAny>) -> Self {
        Self::Method(func.into())
    }
    /// Create a new `CallableFunction::Plugin`.
    #[inline(always)]
    pub fn from_plugin(func: impl PluginFunction + 'static + SendSync) -> Self {
        Self::Plugin((Box::new(func) as Box<FnPlugin>).into())
    }
}

impl From<IteratorFn> for CallableFunction {
    #[inline(always)]
    fn from(func: IteratorFn) -> Self {
        Self::Iterator(func)
    }
}

impl From<ScriptFnDef> for CallableFunction {
    #[inline(always)]
    fn from(_func: ScriptFnDef) -> Self {
        #[cfg(feature = "no_function")]
        unreachable!();

        #[cfg(not(feature = "no_function"))]
        Self::Script(_func.into())
    }
}

impl From<Shared<ScriptFnDef>> for CallableFunction {
    #[inline(always)]
    fn from(_func: Shared<ScriptFnDef>) -> Self {
        #[cfg(feature = "no_function")]
        unreachable!();

        #[cfg(not(feature = "no_function"))]
        Self::Script(_func)
    }
}

impl<T: PluginFunction + 'static + SendSync> From<T> for CallableFunction {
    #[inline(always)]
    fn from(func: T) -> Self {
        Self::from_plugin(func)
    }
}

impl From<Shared<FnPlugin>> for CallableFunction {
    #[inline(always)]
    fn from(func: Shared<FnPlugin>) -> Self {
        Self::Plugin(func.into())
    }
}
