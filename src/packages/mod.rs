//! Module containing all built-in _packages_ available to Rhai, plus facilities to define custom packages.

use crate::{Engine, Module, Shared};

pub(crate) mod arithmetic;
pub(crate) mod array_basic;
pub(crate) mod bit_field;
pub(crate) mod blob_basic;
pub(crate) mod debugging;
pub(crate) mod fn_basic;
pub(crate) mod iter_basic;
pub(crate) mod lang_core;
pub(crate) mod logic;
pub(crate) mod map_basic;
pub(crate) mod math_basic;
pub(crate) mod pkg_core;
pub(crate) mod pkg_std;
pub(crate) mod string_basic;
pub(crate) mod string_more;
pub(crate) mod time_basic;

pub use arithmetic::ArithmeticPackage;
#[cfg(not(feature = "no_index"))]
pub use array_basic::BasicArrayPackage;
pub use bit_field::BitFieldPackage;
#[cfg(not(feature = "no_index"))]
pub use blob_basic::BasicBlobPackage;
#[cfg(feature = "debugging")]
pub use debugging::DebuggingPackage;
pub use fn_basic::BasicFnPackage;
pub use iter_basic::BasicIteratorPackage;
pub use lang_core::LanguageCorePackage;
pub use logic::LogicPackage;
#[cfg(not(feature = "no_object"))]
pub use map_basic::BasicMapPackage;
pub use math_basic::BasicMathPackage;
pub use pkg_core::CorePackage;
pub use pkg_std::StandardPackage;
pub use string_basic::BasicStringPackage;
pub use string_more::MoreStringPackage;
#[cfg(not(feature = "no_time"))]
pub use time_basic::BasicTimePackage;

/// Trait that all packages must implement.
pub trait Package {
    /// Initialize the package.
    /// Functions should be registered into `module` here.
    #[cold]
    fn init(module: &mut Module);

    /// Initialize the package with an [`Engine`].
    ///
    /// Perform tasks such as registering custom operators/syntax.
    #[cold]
    #[inline]
    #[allow(unused_variables)]
    fn init_engine(engine: &mut Engine) {}

    /// Register the package with an [`Engine`].
    ///
    /// # Example
    ///
    /// ```rust
    /// # use rhai::Engine;
    /// # use rhai::packages::{Package, CorePackage};
    /// let mut engine = Engine::new_raw();
    /// let package = CorePackage::new();
    ///
    /// package.register_into_engine(&mut engine);
    /// ```
    #[cold]
    #[inline]
    fn register_into_engine(&self, engine: &mut Engine) -> &Self {
        Self::init_engine(engine);
        engine.register_global_module(self.as_shared_module());
        self
    }

    /// Register the package with an [`Engine`] under a static namespace.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use rhai::Engine;
    /// # use rhai::packages::{Package, CorePackage};
    /// let mut engine = Engine::new_raw();
    /// let package = CorePackage::new();
    ///
    /// package.register_into_engine_as(&mut engine, "core");
    /// ```
    #[cfg(not(feature = "no_module"))]
    #[cold]
    #[inline]
    fn register_into_engine_as(&self, engine: &mut Engine, name: &str) -> &Self {
        Self::init_engine(engine);
        engine.register_static_module(name, self.as_shared_module());
        self
    }

    /// Get a reference to a shared module from this package.
    #[must_use]
    fn as_shared_module(&self) -> Shared<Module>;
}

/// Macro that makes it easy to define a _package_ (which is basically a shared [module][Module])
/// and register functions into it.
///
/// Functions can be added to the package using [`Module::set_native_fn`].
///
/// # Example
///
/// Define a package named `MyPackage` with a single function named `my_add`:
///
/// ```
/// use rhai::{Dynamic, EvalAltResult};
/// use rhai::def_package;
///
/// fn add(x: i64, y: i64) -> Result<i64, Box<EvalAltResult>> { Ok(x + y) }
///
/// def_package! {
///     /// My super-duper package.
///     pub MyPackage(module) {
///         // Load a native Rust function.
///         module.set_native_fn("my_add", add);
///     }
/// }
/// ```
#[macro_export]
macro_rules! def_package {
    ($($(#[$outer:meta])* $mod:vis $package:ident($lib:ident)
                $( : $($(#[$base_meta:meta])* $base_pkg:ty),+ )?
                $block:block
                $( |> | $engine:ident | $init_engine:block )?
    )+) => { $(
        $(#[$outer])*
        $mod struct $package($crate::Shared<$crate::Module>);

        impl $crate::packages::Package for $package {
            #[inline(always)]
            fn as_shared_module(&self) -> $crate::Shared<$crate::Module> {
                self.0.clone()
            }
            fn init($lib: &mut $crate::Module) {
                $($(
                    $(#[$base_meta])* { <$base_pkg>::init($lib); }
                )*)*

                $block
            }
            fn init_engine(_engine: &mut $crate::Engine) {
                $($(
                    $(#[$base_meta])* { <$base_pkg>::init_engine(_engine); }
                )*)*

                $(
                    let $engine = _engine;
                    $init_engine
                )*
            }
        }

        impl Default for $package {
            #[inline(always)]
            #[must_use]
            fn default() -> Self {
                Self::new()
            }
        }

        impl $package {
            #[doc=concat!("Create a new `", stringify!($package), "`")]
            #[inline]
            #[must_use]
            pub fn new() -> Self {
                let mut module = $crate::Module::new();
                <Self as $crate::packages::Package>::init(&mut module);
                module.build_index();
                Self(module.into())
            }
        }
    )* };
    ($($(#[$outer:meta])* $root:ident :: $package:ident => | $lib:ident | $block:block)+) => { $(
        $(#[$outer])*
        /// # Deprecated
        ///
        /// This old syntax of `def_package!` is deprecated. Use the new syntax instead.
        ///
        /// This syntax will be removed in the next major version.
        #[deprecated(since = "1.5.0", note = "this is an old syntax of `def_package!` and is deprecated; use the new syntax of `def_package!` instead")]
        pub struct $package($root::Shared<$root::Module>);

        impl $root::packages::Package for $package {
            fn as_shared_module(&self) -> $root::Shared<$root::Module> {
                self.0.clone()
            }
            fn init($lib: &mut $root::Module) {
                $block
            }
        }

        impl Default for $package {
            #[inline(always)]
            #[must_use]
            fn default() -> Self {
                Self::new()
            }
        }

        impl $package {
            #[inline]
            #[must_use]
            pub fn new() -> Self {
                let mut module = $root::Module::new();
                <Self as $root::packages::Package>::init(&mut module);
                module.build_index();
                Self(module.into())
            }
        }
    )* };
    ($root:ident : $package:ident : $comment:expr , $lib:ident , $block:stmt) => {
        #[doc=$comment]
        ///
        /// # Deprecated
        ///
        /// This old syntax of `def_package!` is deprecated. Use the new syntax instead.
        ///
        /// This syntax will be removed in the next major version.
        #[deprecated(since = "1.4.0", note = "this is an old syntax of `def_package!` and is deprecated; use the new syntax of `def_package!` instead")]
        pub struct $package($root::Shared<$root::Module>);

        impl $root::packages::Package for $package {
            fn as_shared_module(&self) -> $root::Shared<$root::Module> {
                #[allow(deprecated)]
                self.0.clone()
            }
            fn init($lib: &mut $root::Module) {
                $block
            }
        }

        impl Default for $package {
            #[inline(always)]
            #[must_use]
            fn default() -> Self {
                Self::new()
            }
        }

        impl $package {
            #[inline]
            #[must_use]
            pub fn new() -> Self {
                let mut module = $root::Module::new();
                <Self as $root::packages::Package>::init(&mut module);
                module.build_index();
                Self(module.into())
            }
        }
    };
}
