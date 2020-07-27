//! Module defining interfaces to native-Rust functions.

use crate::any::{Dynamic, Variant};
use crate::engine::Engine;
use crate::module::Module;
use crate::result::EvalAltResult;
use crate::token::{is_valid_identifier, Position};
use crate::utils::ImmutableString;

#[cfg(not(feature = "no_function"))]
use crate::{module::FuncReturn, parser::ScriptFnDef, scope::Scope, utils::StaticVec};

use crate::stdlib::{boxed::Box, convert::TryFrom, fmt, string::String, vec::Vec};

#[cfg(not(feature = "no_function"))]
use crate::stdlib::mem;

#[cfg(not(feature = "sync"))]
use crate::stdlib::{rc::Rc, cell::{RefCell, Ref, RefMut}};
#[cfg(feature = "sync")]
use crate::stdlib::sync::{Arc, RwLock};

/// Trait that maps to `Send + Sync` only under the `sync` feature.
#[cfg(feature = "sync")]
pub trait SendSync: Send + Sync {}
#[cfg(feature = "sync")]
impl<T: Send + Sync> SendSync for T {}

/// Trait that maps to `Send + Sync` only under the `sync` feature.
#[cfg(not(feature = "sync"))]
pub trait SendSync {}
#[cfg(not(feature = "sync"))]
impl<T> SendSync for T {}

/// Immutable reference counting container
#[cfg(not(feature = "sync"))]
pub type Shared<T> = Rc<T>;
#[cfg(feature = "sync")]
pub type Shared<T> = Arc<T>;

/// Mutable reference counting container(read-write lock)
#[cfg(not(feature = "sync"))]
pub type SharedMut<T> = Rc<RefCell<T>>;
#[cfg(feature = "sync")]
pub type SharedMut<T> = Arc<RwLock<T>>;

/// Consume a `Shared` resource and return a mutable reference to the wrapped value.
/// If the resource is shared (i.e. has other outstanding references), a cloned copy is used.
pub fn shared_make_mut<T: Clone>(value: &mut Shared<T>) -> &mut T {
    #[cfg(not(feature = "sync"))]
    return Rc::make_mut(value);
    #[cfg(feature = "sync")]
    return Arc::make_mut(value);
}

/// Consume a `Shared` resource, assuming that it is unique (i.e. not shared).
///
/// # Panics
///
/// Panics if the resource is shared (i.e. has other outstanding references).
pub fn shared_take<T: Clone>(value: Shared<T>) -> T {
    #[cfg(not(feature = "sync"))]
    return Rc::try_unwrap(value).map_err(|_| ()).unwrap();
    #[cfg(feature = "sync")]
    return Arc::try_unwrap(value).map_err(|_| ()).unwrap();
}

pub type FnCallArgs<'a> = [&'a mut Dynamic];

/// A general function pointer, which may carry additional (i.e. curried) argument values
/// to be passed onto a function during a call.
#[derive(Debug, Clone, Default)]
pub struct FnPtr(ImmutableString, Vec<Dynamic>);

impl FnPtr {
    /// Create a new function pointer.
    pub(crate) fn new_unchecked<S: Into<ImmutableString>>(name: S, curry: Vec<Dynamic>) -> Self {
        Self(name.into(), curry)
    }
    /// Get the name of the function.
    pub fn fn_name(&self) -> &str {
        self.get_fn_name().as_ref()
    }
    /// Get the name of the function.
    pub(crate) fn get_fn_name(&self) -> &ImmutableString {
        &self.0
    }
    /// Get the underlying data of the function pointer.
    pub(crate) fn take_data(self) -> (ImmutableString, Vec<Dynamic>) {
        (self.0, self.1)
    }
    /// Get the curried arguments.
    pub fn curry(&self) -> &[Dynamic] {
        &self.1
    }

    /// Call the function pointer with curried arguments (if any).
    ///
    /// The function must be a script-defined function.  It cannot be a Rust function.
    ///
    /// To call a Rust function, just call it directly in Rust!
    ///
    /// ## WARNING
    ///
    /// All the arguments are _consumed_, meaning that they're replaced by `()`.
    /// This is to avoid unnecessarily cloning the arguments.
    /// Do not use the arguments after this call. If they are needed afterwards,
    /// clone them _before_ calling this function.
    #[cfg(not(feature = "no_function"))]
    pub fn call_dynamic(
        &self,
        engine: &Engine,
        lib: impl AsRef<Module>,
        this_ptr: Option<&mut Dynamic>,
        mut arg_values: impl AsMut<[Dynamic]>,
    ) -> FuncReturn<Dynamic> {
        let args = self
            .1
            .iter()
            .cloned()
            .chain(arg_values.as_mut().iter_mut().map(|v| mem::take(v)))
            .collect::<StaticVec<_>>();

        engine.call_fn_dynamic(&mut Scope::new(), lib, self.0.as_str(), this_ptr, args)
    }
}

impl fmt::Display for FnPtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fn({})", self.0)
    }
}

impl TryFrom<ImmutableString> for FnPtr {
    type Error = Box<EvalAltResult>;

    fn try_from(value: ImmutableString) -> Result<Self, Self::Error> {
        if is_valid_identifier(value.chars()) {
            Ok(Self(value, Default::default()))
        } else {
            Err(Box::new(EvalAltResult::ErrorFunctionNotFound(
                value.into(),
                Position::none(),
            )))
        }
    }
}

impl TryFrom<String> for FnPtr {
    type Error = Box<EvalAltResult>;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let s: ImmutableString = value.into();
        Self::try_from(s)
    }
}

impl TryFrom<&str> for FnPtr {
    type Error = Box<EvalAltResult>;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let s: ImmutableString = value.into();
        Self::try_from(s)
    }
}

/// A general function trail object.
#[cfg(not(feature = "sync"))]
pub type FnAny = dyn Fn(&Engine, &Module, &mut FnCallArgs) -> Result<Dynamic, Box<EvalAltResult>>;
/// A general function trail object.
#[cfg(feature = "sync")]
pub type FnAny =
    dyn Fn(&Engine, &Module, &mut FnCallArgs) -> Result<Dynamic, Box<EvalAltResult>> + Send + Sync;

/// A standard function that gets an iterator from a type.
pub type IteratorFn = fn(Dynamic) -> Box<dyn Iterator<Item = Dynamic>>;

/// A standard callback function.
#[cfg(not(feature = "sync"))]
pub type Callback<T, R> = Box<dyn Fn(&T) -> R + 'static>;
/// A standard callback function.
#[cfg(feature = "sync")]
pub type Callback<T, R> = Box<dyn Fn(&T) -> R + Send + Sync + 'static>;

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

            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => false,
        }
    }
    /// Is this a native Rust method function?
    pub fn is_method(&self) -> bool {
        match self {
            Self::Method(_) => true,
            Self::Pure(_) | Self::Iterator(_) => false,

            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => false,
        }
    }
    /// Is this an iterator function?
    pub fn is_iter(&self) -> bool {
        match self {
            Self::Iterator(_) => true,
            Self::Pure(_) | Self::Method(_) => false,

            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => false,
        }
    }
    /// Is this a Rhai-scripted function?
    #[cfg(not(feature = "no_function"))]
    pub fn is_script(&self) -> bool {
        match self {
            Self::Script(_) => true,
            Self::Pure(_) | Self::Method(_) | Self::Iterator(_) => false,
        }
    }
    /// Get a reference to a native Rust function.
    ///
    /// # Panics
    ///
    /// Panics if the `CallableFunction` is not `Pure` or `Method`.
    pub fn get_native_fn(&self) -> &FnAny {
        match self {
            Self::Pure(f) | Self::Method(f) => f.as_ref(),
            Self::Iterator(_) => unreachable!(),

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
    pub fn get_shared_fn_def(&self) -> Shared<ScriptFnDef> {
        match self {
            Self::Pure(_) | Self::Method(_) | Self::Iterator(_) => unreachable!(),
            Self::Script(f) => f.clone(),
        }
    }
    /// Get a reference to a script-defined function definition.
    ///
    /// # Panics
    ///
    /// Panics if the `CallableFunction` is not `Script`.
    #[cfg(not(feature = "no_function"))]
    pub fn get_fn_def(&self) -> &ScriptFnDef {
        match self {
            Self::Pure(_) | Self::Method(_) | Self::Iterator(_) => unreachable!(),
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
            Self::Pure(_) | Self::Method(_) => unreachable!(),

            #[cfg(not(feature = "no_function"))]
            Self::Script(_) => unreachable!(),
        }
    }
    /// Create a new `CallableFunction::Pure`.
    pub fn from_pure(func: Box<FnAny>) -> Self {
        Self::Pure(func.into())
    }
    /// Create a new `CallableFunction::Method`.
    pub fn from_method(func: Box<FnAny>) -> Self {
        Self::Method(func.into())
    }
}

impl From<IteratorFn> for CallableFunction {
    fn from(func: IteratorFn) -> Self {
        Self::Iterator(func)
    }
}

#[cfg(not(feature = "no_function"))]
impl From<ScriptFnDef> for CallableFunction {
    fn from(func: ScriptFnDef) -> Self {
        Self::Script(func.into())
    }
}

#[cfg(not(feature = "no_function"))]
impl From<Shared<ScriptFnDef>> for CallableFunction {
    fn from(func: Shared<ScriptFnDef>) -> Self {
        Self::Script(func)
    }
}
