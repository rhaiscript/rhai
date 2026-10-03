//! Module defining the standard Rhai function type.

use super::native::{FnAny, FnIterator, FnPlugin, SendSync};
#[cfg(not(feature = "no_function"))]
use super::EncapsulatedEnviron;
use crate::plugin::PluginFunc;
use crate::types::token::{is_reserved_keyword_or_symbol, is_valid_identifier, Token};
use crate::Shared;
use std::fmt;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// A type representing the access mode of a function.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
#[non_exhaustive]
pub enum FnAccess {
    /// Private function.
    Private,
    /// Public function.
    Public,
}

impl FnAccess {
    /// Is this function private?
    #[inline(always)]
    #[must_use]
    pub const fn is_private(self) -> bool {
        match self {
            Self::Private => true,
            Self::Public => false,
        }
    }
    /// Is this function public?
    #[inline(always)]
    #[must_use]
    pub const fn is_public(self) -> bool {
        match self {
            Self::Private => false,
            Self::Public => true,
        }
    }
}

/// _(internals)_ Is a text string a valid script-defined function name?
/// Exported under the `internals` feature only.
#[inline(always)]
#[must_use]
pub fn is_valid_function_name(name: &str) -> bool {
    is_valid_identifier(name)
        && !is_reserved_keyword_or_symbol(name).0
        && Token::lookup_symbol_from_syntax(name).is_none()
}

/// Is this function an anonymous function?
#[cfg(not(feature = "no_function"))]
#[inline(always)]
#[must_use]
pub fn is_anonymous_fn(fn_name: &str) -> bool {
    fn_name.starts_with(crate::engine::FN_ANONYMOUS)
}

/// _(internals)_ A type encapsulating a function callable by Rhai.
/// Exported under the `internals` feature only.
#[derive(Clone)]
#[non_exhaustive]
pub enum RhaiFunc {
    /// A pure native Rust function with all arguments passed by value.
    Pure {
        /// Shared function pointer.
        func: Shared<FnAny>,
        /// Does the function take a [`NativeCallContext`][crate::NativeCallContext] parameter?
        has_context: bool,
        /// This is a dummy field and is not used.
        is_pure: bool,
        /// Is this function volatile?
        ///
        /// A volatile function does not guarantee the same result for the same input(s).
        is_volatile: bool,
    },
    /// A native Rust object method with the first argument passed by reference,
    /// and the rest passed by value.
    Method {
        /// Shared function pointer.
        func: Shared<FnAny>,
        /// Does the function take a [`NativeCallContext`][crate::NativeCallContext] parameter?
        has_context: bool,
        /// Allow operating on constants?
        is_pure: bool,
        /// Is this function volatile?
        ///
        /// A volatile function does not guarantee the same result for the same input(s).
        is_volatile: bool,
    },
    /// An iterator function.
    Iterator {
        /// Shared function pointer.
        func: Shared<FnIterator>,
    },
    /// A plugin function,
    Plugin {
        /// Shared function pointer.
        func: Shared<FnPlugin>,
    },
    /// A plugin function in static storage (e.g. from a [`ModuleManifest`][crate::plugin::ModuleManifest]).
    StaticPlugin {
        /// Static function reference.
        func: &'static FnPlugin,
    },
    /// A script-defined function.
    #[cfg(not(feature = "no_function"))]
    Script {
        /// Shared reference to the [`ScriptFuncDef`][crate::func::ScriptFuncDef] function definition.
        fn_def: Shared<crate::func::ScriptFuncDef>,
        /// Encapsulated environment, if any.
        env: Option<Shared<EncapsulatedEnviron>>,
    },
}

impl fmt::Debug for RhaiFunc {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pure { .. } => f.write_str("NativePureFunction"),
            Self::Method { .. } => f.write_str("NativeMethod"),
            Self::Iterator { .. } => f.write_str("NativeIterator"),
            Self::Plugin { .. } | Self::StaticPlugin { .. } => f.write_str("PluginFunction"),

            #[cfg(not(feature = "no_function"))]
            Self::Script { fn_def, .. } => fmt::Debug::fmt(fn_def, f),
        }
    }
}

impl fmt::Display for RhaiFunc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pure { .. } => f.write_str("NativePureFunction"),
            Self::Method { .. } => f.write_str("NativeMethod"),
            Self::Iterator { .. } => f.write_str("NativeIterator"),
            Self::Plugin { .. } | Self::StaticPlugin { .. } => f.write_str("PluginFunction"),

            #[cfg(not(feature = "no_function"))]
            Self::Script { fn_def, .. } => fmt::Display::fmt(fn_def, f),
        }
    }
}

impl RhaiFunc {
    /// Is this a pure native Rust function?
    #[inline]
    #[must_use]
    pub fn is_pure(&self) -> bool {
        match self {
            Self::Pure { .. } => true,
            Self::Method { is_pure, .. } => *is_pure,
            Self::Iterator { .. } => true,

            Self::Plugin { func, .. } => func.is_pure(),
            Self::StaticPlugin { func } => func.is_pure(),

            #[cfg(not(feature = "no_function"))]
            Self::Script { .. } => false,
        }
    }
    /// Is this a native Rust method function?
    #[inline]
    #[must_use]
    pub fn is_method(&self) -> bool {
        match self {
            Self::Method { .. } => true,
            Self::Pure { .. } | Self::Iterator { .. } => false,

            Self::Plugin { func, .. } => func.is_method_call(),
            Self::StaticPlugin { func } => func.is_method_call(),

            #[cfg(not(feature = "no_function"))]
            Self::Script { .. } => false,
        }
    }
    /// Is this an iterator function?
    #[inline]
    #[must_use]
    pub const fn is_iter(&self) -> bool {
        match self {
            Self::Iterator { .. } => true,
            Self::Pure { .. }
            | Self::Method { .. }
            | Self::Plugin { .. }
            | Self::StaticPlugin { .. } => false,

            #[cfg(not(feature = "no_function"))]
            Self::Script { .. } => false,
        }
    }
    /// Is this a script-defined function?
    #[inline]
    #[must_use]
    pub const fn is_script(&self) -> bool {
        #[cfg(feature = "no_function")]
        return false;

        #[cfg(not(feature = "no_function"))]
        match self {
            Self::Script { .. } => true,
            Self::Pure { .. }
            | Self::Method { .. }
            | Self::Iterator { .. }
            | Self::Plugin { .. }
            | Self::StaticPlugin { .. } => false,
        }
    }
    /// Is this a plugin function?
    #[inline]
    #[must_use]
    pub const fn is_plugin_fn(&self) -> bool {
        match self {
            Self::Plugin { .. } | Self::StaticPlugin { .. } => true,
            Self::Pure { .. } | Self::Method { .. } | Self::Iterator { .. } => false,

            #[cfg(not(feature = "no_function"))]
            Self::Script { .. } => false,
        }
    }
    /// Is this a native Rust function?
    #[inline]
    #[must_use]
    pub const fn is_native(&self) -> bool {
        #[cfg(feature = "no_function")]
        return true;

        #[cfg(not(feature = "no_function"))]
        match self {
            Self::Pure { .. }
            | Self::Method { .. }
            | Self::Plugin { .. }
            | Self::StaticPlugin { .. }
            | Self::Iterator { .. } => true,
            Self::Script { .. } => false,
        }
    }
    /// Is there a [`NativeCallContext`][crate::NativeCallContext] parameter?
    #[inline]
    #[must_use]
    pub fn has_context(&self) -> bool {
        match self {
            Self::Pure { has_context, .. } | Self::Method { has_context, .. } => *has_context,
            Self::Plugin { func, .. } => func.has_context(),
            Self::StaticPlugin { func } => func.has_context(),
            Self::Iterator { .. } => false,
            #[cfg(not(feature = "no_function"))]
            Self::Script { .. } => false,
        }
    }
    /// Is this function volatile?
    ///
    /// A volatile function does not guarantee the same result for the same input(s).
    #[inline]
    #[must_use]
    pub fn is_volatile(&self) -> bool {
        match self {
            Self::Pure { is_volatile, .. } => *is_volatile,
            Self::Method { is_volatile, .. } => *is_volatile,
            Self::Iterator { .. } => true,

            Self::Plugin { func, .. } => func.is_volatile(),
            Self::StaticPlugin { func } => func.is_volatile(),

            // Scripts are assumed to be volatile -- it can be calling volatile native functions.
            #[cfg(not(feature = "no_function"))]
            Self::Script { .. } => true,
        }
    }
    /// Get the access mode.
    #[inline]
    #[must_use]
    pub fn access(&self) -> FnAccess {
        #[cfg(feature = "no_function")]
        return FnAccess::Public;

        #[cfg(not(feature = "no_function"))]
        match self {
            Self::Plugin { .. }
            | Self::StaticPlugin { .. }
            | Self::Pure { .. }
            | Self::Method { .. }
            | Self::Iterator { .. } => FnAccess::Public,
            Self::Script { fn_def, .. } => fn_def.access,
        }
    }
    /// Get a shared reference to a native Rust function.
    #[inline]
    #[must_use]
    pub fn get_native_fn(&self) -> Option<&Shared<FnAny>> {
        match self {
            Self::Pure { func, .. } | Self::Method { func, .. } => Some(func),
            Self::Iterator { .. } | Self::Plugin { .. } | Self::StaticPlugin { .. } => None,

            #[cfg(not(feature = "no_function"))]
            Self::Script { .. } => None,
        }
    }
    /// Get a shared reference to a script-defined function definition.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    #[must_use]
    pub const fn get_script_fn_def(&self) -> Option<&Shared<crate::func::ScriptFuncDef>> {
        match self {
            Self::Pure { .. }
            | Self::Method { .. }
            | Self::Iterator { .. }
            | Self::Plugin { .. }
            | Self::StaticPlugin { .. } => None,
            Self::Script { fn_def, .. } => Some(fn_def),
        }
    }
    /// Get a reference to an iterator function.
    #[inline]
    #[must_use]
    pub fn get_iter_fn(&self) -> Option<&FnIterator> {
        match self {
            Self::Iterator { func, .. } => Some(&**func),
            Self::Pure { .. }
            | Self::Method { .. }
            | Self::Plugin { .. }
            | Self::StaticPlugin { .. } => None,

            #[cfg(not(feature = "no_function"))]
            Self::Script { .. } => None,
        }
    }
    /// Get a reference to a plugin function, whether shared or static.
    #[inline]
    #[must_use]
    pub fn get_plugin_fn_ref(&self) -> Option<&FnPlugin> {
        match self {
            Self::Plugin { func, .. } => Some(&**func),
            Self::StaticPlugin { func } => Some(*func),
            Self::Pure { .. } | Self::Method { .. } | Self::Iterator { .. } => None,

            #[cfg(not(feature = "no_function"))]
            Self::Script { .. } => None,
        }
    }
    /// Get a shared reference to a plugin function.
    ///
    /// Returns [`None`] for a [static][RhaiFunc::StaticPlugin] plugin function;
    /// use [`get_plugin_fn_ref`][RhaiFunc::get_plugin_fn_ref] instead.
    #[inline]
    #[must_use]
    pub fn get_plugin_fn(&self) -> Option<&Shared<FnPlugin>> {
        match self {
            Self::Plugin { func, .. } => Some(func),
            Self::Pure { .. }
            | Self::Method { .. }
            | Self::Iterator { .. }
            | Self::StaticPlugin { .. } => None,

            #[cfg(not(feature = "no_function"))]
            Self::Script { .. } => None,
        }
    }
}

#[cfg(not(feature = "no_function"))]
impl From<crate::func::ScriptFuncDef> for RhaiFunc {
    #[inline(always)]
    fn from(fn_def: crate::func::ScriptFuncDef) -> Self {
        Self::Script {
            fn_def: fn_def.into(),
            env: None,
        }
    }
}

#[cfg(not(feature = "no_function"))]
impl From<Shared<crate::func::ScriptFuncDef>> for RhaiFunc {
    #[inline(always)]
    fn from(fn_def: Shared<crate::func::ScriptFuncDef>) -> Self {
        Self::Script { fn_def, env: None }
    }
}

impl<T: PluginFunc + 'static + SendSync> From<T> for RhaiFunc {
    #[inline(always)]
    fn from(func: T) -> Self {
        Self::Plugin {
            func: Shared::new(func),
        }
    }
}

impl From<Shared<FnPlugin>> for RhaiFunc {
    #[inline(always)]
    fn from(func: Shared<FnPlugin>) -> Self {
        Self::Plugin { func }
    }
}
