//! Main module defining the script evaluation [`Engine`].

use crate::api::options::LangOptions;
use crate::func::native::{
    locked_write, OnDebugCallback, OnDefVarCallback, OnParseTokenCallback, OnPrintCallback,
    OnVarCallback,
};
use crate::packages::{Package, StandardPackage};
use crate::tokenizer::Token;
use crate::types::StringsInterner;
use crate::{Dynamic, Identifier, ImmutableString, Locked, OptimizationLevel, SharedModule};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{collections::BTreeSet, fmt, num::NonZeroU8};

pub type Precedence = NonZeroU8;

pub const KEYWORD_PRINT: &str = "print";
pub const KEYWORD_DEBUG: &str = "debug";
pub const KEYWORD_TYPE_OF: &str = "type_of";
pub const KEYWORD_EVAL: &str = "eval";
pub const KEYWORD_FN_PTR: &str = "Fn";
pub const KEYWORD_FN_PTR_CALL: &str = "call";
pub const KEYWORD_FN_PTR_CURRY: &str = "curry";
#[cfg(not(feature = "no_closure"))]
pub const KEYWORD_IS_SHARED: &str = "is_shared";
pub const KEYWORD_IS_DEF_VAR: &str = "is_def_var";
#[cfg(not(feature = "no_function"))]
pub const KEYWORD_IS_DEF_FN: &str = "is_def_fn";
#[cfg(not(feature = "no_function"))]
pub const KEYWORD_THIS: &str = "this";
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_module"))]
pub const KEYWORD_GLOBAL: &str = "global";
#[cfg(not(feature = "no_object"))]
pub const FN_GET: &str = "get$";
#[cfg(not(feature = "no_object"))]
pub const FN_SET: &str = "set$";
#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
pub const FN_IDX_GET: &str = "index$get$";
#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
pub const FN_IDX_SET: &str = "index$set$";
#[cfg(not(feature = "no_function"))]
pub const FN_ANONYMOUS: &str = "anon$";

/// Standard equality comparison operator.
///
/// Some standard functions (e.g. searching an [`Array`][crate::Array]) implicitly call this
/// function to compare two [`Dynamic`] values.
pub const OP_EQUALS: &str = Token::EqualsTo.literal_syntax();

/// Standard containment testing function.
///
/// The `in` operator is implemented as a call to this function.
pub const OP_CONTAINS: &str = "contains";

/// Standard not operator.
pub const OP_NOT: &str = Token::Bang.literal_syntax();

/// Separator for namespaces.
#[cfg(not(feature = "no_module"))]
pub const NAMESPACE_SEPARATOR: &str = Token::DoubleColon.literal_syntax();

/// Rhai main scripting engine.
///
/// # Thread Safety
///
/// [`Engine`] is re-entrant.
///
/// Currently, [`Engine`] is neither [`Send`] nor [`Sync`].
/// Use the `sync` feature to make it [`Send`] `+` [`Sync`].
///
/// # Example
///
/// ```
/// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
/// use rhai::Engine;
///
/// let engine = Engine::new();
///
/// let result = engine.eval::<i64>("40 + 2")?;
///
/// println!("Answer: {result}");   // prints 42
/// # Ok(())
/// # }
/// ```
pub struct Engine {
    /// A collection of all modules loaded into the global namespace of the Engine.
    pub(crate) global_modules: Vec<SharedModule>,
    /// A collection of all sub-modules directly loaded into the Engine.
    #[cfg(not(feature = "no_module"))]
    pub(crate) global_sub_modules: std::collections::BTreeMap<Identifier, SharedModule>,

    /// A module resolution service.
    #[cfg(not(feature = "no_module"))]
    pub(crate) module_resolver: Option<Box<dyn crate::ModuleResolver>>,

    /// Strings interner.
    pub(crate) interned_strings: Option<Box<Locked<StringsInterner>>>,

    /// A set of symbols to disable.
    pub(crate) disabled_symbols: BTreeSet<Identifier>,
    /// A map containing custom keywords and precedence to recognize.
    #[cfg(not(feature = "no_custom_syntax"))]
    pub(crate) custom_keywords: std::collections::BTreeMap<Identifier, Option<Precedence>>,
    /// Custom syntax.
    #[cfg(not(feature = "no_custom_syntax"))]
    pub(crate) custom_syntax:
        std::collections::BTreeMap<Identifier, Box<crate::api::custom_syntax::CustomSyntax>>,

    /// Callback closure for filtering variable definition.
    pub(crate) def_var_filter: Option<Box<OnDefVarCallback>>,
    /// Callback closure for resolving variable access.
    pub(crate) resolve_var: Option<Box<OnVarCallback>>,
    /// Callback closure to remap tokens during parsing.
    pub(crate) token_mapper: Option<Box<OnParseTokenCallback>>,

    /// Callback closure for implementing the `print` command.
    pub(crate) print: Option<Box<OnPrintCallback>>,
    /// Callback closure for implementing the `debug` command.
    pub(crate) debug: Option<Box<OnDebugCallback>>,
    /// Callback closure for progress reporting.
    #[cfg(not(feature = "unchecked"))]
    pub(crate) progress: Option<Box<crate::func::native::OnProgressCallback>>,

    /// Language options.
    pub(crate) options: LangOptions,

    /// Default value for the custom state.
    pub(crate) def_tag: Dynamic,

    /// Script optimization level.
    pub(crate) optimization_level: OptimizationLevel,

    /// Max limits.
    #[cfg(not(feature = "unchecked"))]
    pub(crate) limits: crate::api::limits::Limits,

    /// Callback closure for debugging.
    #[cfg(feature = "debugging")]
    pub(crate) debugger_interface: Option<(
        Box<crate::eval::OnDebuggingInit>,
        Box<crate::eval::OnDebuggerCallback>,
    )>,
}

impl fmt::Debug for Engine {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("Engine");

        f.field("global_modules", &self.global_modules);

        #[cfg(not(feature = "no_module"))]
        f.field("global_sub_modules", &self.global_sub_modules);

        f.field("disabled_symbols", &self.disabled_symbols);

        #[cfg(not(feature = "no_custom_syntax"))]
        f.field("custom_keywords", &self.custom_keywords).field(
            "custom_syntax",
            &self
                .custom_syntax
                .keys()
                .map(crate::SmartString::as_str)
                .collect::<String>(),
        );

        f.field("def_var_filter", &self.def_var_filter.is_some())
            .field("resolve_var", &self.resolve_var.is_some())
            .field("token_mapper", &self.token_mapper.is_some());

        #[cfg(not(feature = "unchecked"))]
        f.field("progress", &self.progress.is_some());

        f.field("options", &self.options)
            .field("default_tag", &self.def_tag);

        #[cfg(not(feature = "no_optimize"))]
        f.field("optimization_level", &self.optimization_level);

        #[cfg(not(feature = "unchecked"))]
        f.field("limits", &self.limits);

        #[cfg(feature = "debugging")]
        f.field("debugger_interface", &self.debugger_interface.is_some());

        f.finish()
    }
}

impl Default for Engine {
    #[inline(always)]
    #[must_use]
    fn default() -> Self {
        Self::new()
    }
}

/// Make getter function
#[cfg(not(feature = "no_object"))]
#[inline(always)]
#[must_use]
pub fn make_getter(id: &str) -> Identifier {
    let mut buf = Identifier::new_const();
    buf.push_str(FN_GET);
    buf.push_str(id);
    buf
}

/// Make setter function
#[cfg(not(feature = "no_object"))]
#[inline(always)]
#[must_use]
pub fn make_setter(id: &str) -> Identifier {
    let mut buf = Identifier::new_const();
    buf.push_str(FN_SET);
    buf.push_str(id);
    buf
}

impl Engine {
    /// An empty raw [`Engine`].
    pub const RAW: Self = Self {
        global_modules: Vec::new(),

        #[cfg(not(feature = "no_module"))]
        global_sub_modules: std::collections::BTreeMap::new(),

        #[cfg(not(feature = "no_module"))]
        module_resolver: None,

        interned_strings: None,
        disabled_symbols: BTreeSet::new(),
        #[cfg(not(feature = "no_custom_syntax"))]
        custom_keywords: std::collections::BTreeMap::new(),
        #[cfg(not(feature = "no_custom_syntax"))]
        custom_syntax: std::collections::BTreeMap::new(),

        def_var_filter: None,
        resolve_var: None,
        token_mapper: None,

        print: None,
        debug: None,

        #[cfg(not(feature = "unchecked"))]
        progress: None,

        options: LangOptions::new(),

        def_tag: Dynamic::UNIT,

        #[cfg(not(feature = "no_optimize"))]
        optimization_level: OptimizationLevel::Simple,
        #[cfg(feature = "no_optimize")]
        optimization_level: (),

        #[cfg(not(feature = "unchecked"))]
        limits: crate::api::limits::Limits::new(),

        #[cfg(feature = "debugging")]
        debugger_interface: None,
    };

    /// Create a new [`Engine`].
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        // Create the new scripting Engine
        let mut engine = Self::new_raw();

        #[cfg(not(feature = "no_module"))]
        #[cfg(not(feature = "no_std"))]
        #[cfg(any(not(target_family = "wasm"), not(target_os = "unknown")))]
        {
            engine.module_resolver =
                Some(Box::new(crate::module::resolvers::FileModuleResolver::new()));
        }

        engine.interned_strings = Some(Locked::new(StringsInterner::new()).into());

        // default print/debug implementations
        #[cfg(not(feature = "no_std"))]
        #[cfg(any(not(target_family = "wasm"), not(target_os = "unknown")))]
        {
            engine.print = Some(Box::new(|s| println!("{s}")));
            engine.debug = Some(Box::new(|s, source, pos| match (source, pos) {
                (Some(source), crate::Position::NONE) => println!("{source} | {s}"),
                #[cfg(not(feature = "no_position"))]
                (Some(source), pos) => println!("{source} @ {pos:?} | {s}"),
                (None, crate::Position::NONE) => println!("{s}"),
                #[cfg(not(feature = "no_position"))]
                (None, pos) => println!("{pos:?} | {s}"),
            }));
        }

        engine.register_global_module(StandardPackage::new().as_shared_module());

        engine
    }

    /// Create a new [`Engine`] with minimal built-in functions.
    /// It returns a copy of [`Engine::RAW`].
    ///
    /// This is useful for creating a custom scripting engine with only the functions you need.
    ///
    /// Use [`register_global_module`][Engine::register_global_module] to add packages of functions.
    #[inline]
    #[must_use]
    pub const fn new_raw() -> Self {
        Self::RAW
    }

    /// Get an interned [string][ImmutableString].
    ///
    /// [`Engine`] keeps a cache of [`ImmutableString`] instances and tries to avoid new allocations
    /// and save memory when an existing instance is found.
    ///
    /// It is usually a good idea to intern strings if they are used frequently.
    #[inline]
    #[must_use]
    pub fn get_interned_string(
        &self,
        string: impl AsRef<str> + Into<ImmutableString>,
    ) -> ImmutableString {
        match self.interned_strings {
            Some(ref interner) => locked_write(interner).get(string),
            None => string.into(),
        }
    }

    /// Get an empty [`ImmutableString`] which refers to a shared instance.
    #[inline(always)]
    #[must_use]
    pub fn const_empty_string(&self) -> ImmutableString {
        self.get_interned_string("")
    }

    /// Is there a debugger interface registered with this [`Engine`]?
    #[cfg(feature = "debugging")]
    #[inline(always)]
    #[must_use]
    pub(crate) const fn is_debugger_registered(&self) -> bool {
        self.debugger_interface.is_some()
    }
}
