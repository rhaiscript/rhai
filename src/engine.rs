//! Main module defining the script evaluation `Engine`.

use crate::ast::{BinaryExpr, Expr, FnCallInfo, Ident, IdentX, ReturnType, Stmt};
use crate::dynamic::{map_std_type_name, Dynamic, Union, Variant};
use crate::fn_call::run_builtin_op_assignment;
use crate::fn_native::{shared_try_take, Callback, FnPtr, OnVarCallback, Shared};
use crate::module::{Module, ModuleRef};
use crate::optimize::OptimizationLevel;
use crate::packages::{Package, PackagesCollection, StandardPackage};
use crate::r#unsafe::unsafe_cast_var_name_to_lifetime;
use crate::result::EvalAltResult;
use crate::scope::{EntryType as ScopeEntryType, Scope};
use crate::syntax::CustomSyntax;
use crate::token::{Position, NO_POS};
use crate::{calc_native_fn_hash, StaticVec};

#[cfg(not(feature = "no_index"))]
use crate::INT;

#[cfg(not(feature = "no_module"))]
use crate::module::ModuleResolver;

#[cfg(not(feature = "no_std"))]
#[cfg(not(feature = "no_module"))]
use crate::module::resolvers;

#[cfg(any(not(feature = "no_object"), not(feature = "no_module")))]
use crate::utils::ImmutableString;

#[cfg(not(feature = "no_closure"))]
#[cfg(not(feature = "no_object"))]
use crate::dynamic::DynamicWriteLock;

use crate::stdlib::{
    any::type_name,
    borrow::Cow,
    boxed::Box,
    collections::{HashMap, HashSet},
    fmt, format,
    iter::{empty, once},
    num::NonZeroUsize,
    ops::DerefMut,
    string::{String, ToString},
    vec::Vec,
};

#[cfg(not(feature = "no_index"))]
use crate::stdlib::any::TypeId;

#[cfg(not(feature = "no_closure"))]
use crate::stdlib::mem;

/// Variable-sized array of `Dynamic` values.
///
/// Not available under the `no_index` feature.
#[cfg(not(feature = "no_index"))]
pub type Array = Vec<Dynamic>;

/// Hash map of `Dynamic` values with `ImmutableString` keys.
///
/// Not available under the `no_object` feature.
#[cfg(not(feature = "no_object"))]
pub type Map = HashMap<ImmutableString, Dynamic>;

/// _[INTERNALS]_ A stack of imported modules.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
//
// # Implementation Notes
//
// We cannot use &str or Cow<str> here because `eval` may load a module and the module name will live beyond
// the AST of the eval script text. The best we can do is a shared reference.
#[derive(Debug, Clone, Default)]
pub struct Imports(StaticVec<(Shared<Module>, ImmutableString)>);

impl Imports {
    /// Get the length of this stack of imported modules.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Is this stack of imported modules empty?
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Get the imported module at a particular index.
    pub fn get(&self, index: usize) -> Option<Shared<Module>> {
        self.0.get(index).map(|(m, _)| m).cloned()
    }
    /// Get the index of an imported module by name.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.0
            .iter()
            .enumerate()
            .rev()
            .find(|(_, (_, key))| key.as_str() == name)
            .map(|(index, _)| index)
    }
    /// Push an imported module onto the stack.
    pub fn push(&mut self, name: impl Into<ImmutableString>, module: impl Into<Shared<Module>>) {
        self.0.push((module.into(), name.into()));
    }
    /// Truncate the stack of imported modules to a particular length.
    pub fn truncate(&mut self, size: usize) {
        self.0.truncate(size);
    }
    /// Get an iterator to this stack of imported modules.
    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = (&str, Shared<Module>)> {
        self.0
            .iter()
            .map(|(module, name)| (name.as_str(), module.clone()))
    }
    /// Get a consuming iterator to this stack of imported modules.
    pub fn into_iter(self) -> impl Iterator<Item = (ImmutableString, Shared<Module>)> {
        self.0.into_iter().map(|(module, name)| (name, module))
    }
}

#[cfg(not(feature = "unchecked"))]
#[cfg(debug_assertions)]
pub const MAX_CALL_STACK_DEPTH: usize = 8;
#[cfg(not(feature = "unchecked"))]
#[cfg(debug_assertions)]
pub const MAX_EXPR_DEPTH: usize = 32;
#[cfg(not(feature = "unchecked"))]
#[cfg(not(feature = "no_function"))]
#[cfg(debug_assertions)]
pub const MAX_FUNCTION_EXPR_DEPTH: usize = 16;

#[cfg(not(feature = "unchecked"))]
#[cfg(not(debug_assertions))]
pub const MAX_CALL_STACK_DEPTH: usize = 128;
#[cfg(not(feature = "unchecked"))]
#[cfg(not(debug_assertions))]
pub const MAX_EXPR_DEPTH: usize = 128;
#[cfg(not(feature = "unchecked"))]
#[cfg(not(feature = "no_function"))]
#[cfg(not(debug_assertions))]
pub const MAX_FUNCTION_EXPR_DEPTH: usize = 32;

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
pub const KEYWORD_IS_DEF_FN: &str = "is_def_fn";
pub const KEYWORD_THIS: &str = "this";
pub const FN_TO_STRING: &str = "to_string";
#[cfg(not(feature = "no_object"))]
pub const FN_GET: &str = "get$";
#[cfg(not(feature = "no_object"))]
pub const FN_SET: &str = "set$";
#[cfg(not(feature = "no_index"))]
pub const FN_IDX_GET: &str = "index$get$";
#[cfg(not(feature = "no_index"))]
pub const FN_IDX_SET: &str = "index$set$";
#[cfg(not(feature = "no_function"))]
pub const FN_ANONYMOUS: &str = "anon$";
pub const MARKER_EXPR: &str = "$expr$";
pub const MARKER_BLOCK: &str = "$block$";
pub const MARKER_IDENT: &str = "$ident$";

/// A type specifying the method of chaining.
#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ChainType {
    None,
    Index,
    Dot,
}

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
#[derive(Debug, Clone)]
pub enum IndexChainValue {
    None,
    FnCallArgs(StaticVec<Dynamic>),
    Value(Dynamic),
}

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
impl IndexChainValue {
    /// Return the `Dynamic` value.
    ///
    /// # Panics
    ///
    /// Panics if not `IndexChainValue::Value`.
    #[cfg(not(feature = "no_index"))]
    pub fn as_value(self) -> Dynamic {
        match self {
            Self::None | Self::FnCallArgs(_) => panic!("expecting IndexChainValue::Value"),
            Self::Value(value) => value,
        }
    }
    /// Return the `StaticVec<Dynamic>` value.
    ///
    /// # Panics
    ///
    /// Panics if not `IndexChainValue::FnCallArgs`.
    #[cfg(not(feature = "no_object"))]
    pub fn as_fn_call_args(self) -> StaticVec<Dynamic> {
        match self {
            Self::None | Self::Value(_) => panic!("expecting IndexChainValue::FnCallArgs"),
            Self::FnCallArgs(value) => value,
        }
    }
}

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
impl From<StaticVec<Dynamic>> for IndexChainValue {
    fn from(value: StaticVec<Dynamic>) -> Self {
        Self::FnCallArgs(value)
    }
}

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
impl From<Dynamic> for IndexChainValue {
    fn from(value: Dynamic) -> Self {
        Self::Value(value)
    }
}

/// A type that encapsulates a mutation target for an expression with side effects.
#[derive(Debug)]
pub enum Target<'a> {
    /// The target is a mutable reference to a `Dynamic` value somewhere.
    Ref(&'a mut Dynamic),
    /// The target is a mutable reference to a Shared `Dynamic` value.
    /// It holds both the access guard and the original shared value.
    #[cfg(not(feature = "no_closure"))]
    #[cfg(not(feature = "no_object"))]
    LockGuard((DynamicWriteLock<'a, Dynamic>, Dynamic)),
    /// The target is a temporary `Dynamic` value (i.e. the mutation can cause no side effects).
    Value(Dynamic),
    /// The target is a character inside a String.
    /// This is necessary because directly pointing to a char inside a String is impossible.
    #[cfg(not(feature = "no_index"))]
    StringChar(&'a mut Dynamic, usize, Dynamic),
}

impl<'a> Target<'a> {
    /// Is the `Target` a reference pointing to other data?
    #[allow(dead_code)]
    #[inline(always)]
    pub fn is_ref(&self) -> bool {
        match self {
            Self::Ref(_) => true,
            #[cfg(not(feature = "no_closure"))]
            #[cfg(not(feature = "no_object"))]
            Self::LockGuard(_) => true,
            Self::Value(_) => false,
            #[cfg(not(feature = "no_index"))]
            Self::StringChar(_, _, _) => false,
        }
    }
    /// Is the `Target` an owned value?
    #[allow(dead_code)]
    #[inline(always)]
    pub fn is_value(&self) -> bool {
        match self {
            Self::Ref(_) => false,
            #[cfg(not(feature = "no_closure"))]
            #[cfg(not(feature = "no_object"))]
            Self::LockGuard(_) => false,
            Self::Value(_) => true,
            #[cfg(not(feature = "no_index"))]
            Self::StringChar(_, _, _) => false,
        }
    }
    /// Is the `Target` a shared value?
    #[allow(dead_code)]
    #[inline(always)]
    pub fn is_shared(&self) -> bool {
        match self {
            Self::Ref(r) => r.is_shared(),
            #[cfg(not(feature = "no_closure"))]
            #[cfg(not(feature = "no_object"))]
            Self::LockGuard(_) => true,
            Self::Value(r) => r.is_shared(),
            #[cfg(not(feature = "no_index"))]
            Self::StringChar(_, _, _) => false,
        }
    }
    /// Is the `Target` a specific type?
    #[allow(dead_code)]
    #[inline(always)]
    pub fn is<T: Variant + Clone>(&self) -> bool {
        match self {
            Target::Ref(r) => r.is::<T>(),
            #[cfg(not(feature = "no_closure"))]
            #[cfg(not(feature = "no_object"))]
            Target::LockGuard((r, _)) => r.is::<T>(),
            Target::Value(r) => r.is::<T>(),
            #[cfg(not(feature = "no_index"))]
            Target::StringChar(_, _, _) => TypeId::of::<T>() == TypeId::of::<char>(),
        }
    }
    /// Get the value of the `Target` as a `Dynamic`, cloning a referenced value if necessary.
    #[inline(always)]
    pub fn take_or_clone(self) -> Dynamic {
        match self {
            Self::Ref(r) => r.clone(), // Referenced value is cloned
            #[cfg(not(feature = "no_closure"))]
            #[cfg(not(feature = "no_object"))]
            Self::LockGuard((_, orig)) => orig, // Original value is simply taken
            Self::Value(v) => v,       // Owned value is simply taken
            #[cfg(not(feature = "no_index"))]
            Self::StringChar(_, _, ch) => ch, // Character is taken
        }
    }
    /// Take a `&mut Dynamic` reference from the `Target`.
    #[inline(always)]
    pub fn take_ref(self) -> Option<&'a mut Dynamic> {
        match self {
            Self::Ref(r) => Some(r),
            _ => None,
        }
    }
    /// Get a mutable reference from the `Target`.
    #[inline(always)]
    pub fn as_mut(&mut self) -> &mut Dynamic {
        match self {
            Self::Ref(r) => *r,
            #[cfg(not(feature = "no_closure"))]
            #[cfg(not(feature = "no_object"))]
            Self::LockGuard((r, _)) => r.deref_mut(),
            Self::Value(ref mut r) => r,
            #[cfg(not(feature = "no_index"))]
            Self::StringChar(_, _, ref mut r) => r,
        }
    }
    /// Propagate a changed value back to the original source.
    /// This has no effect except for string indexing.
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn propagate_changed_value(&mut self) {
        match self {
            Self::Ref(_) | Self::Value(_) => (),
            #[cfg(not(feature = "no_closure"))]
            Self::LockGuard(_) => (),
            #[cfg(not(feature = "no_index"))]
            Self::StringChar(_, _, ch) => {
                let char_value = ch.clone();
                self.set_value((char_value, NO_POS)).unwrap();
            }
        }
    }
    /// Update the value of the `Target`.
    #[cfg(any(not(feature = "no_object"), not(feature = "no_index")))]
    pub fn set_value(&mut self, new_val: (Dynamic, Position)) -> Result<(), Box<EvalAltResult>> {
        match self {
            Self::Ref(r) => **r = new_val.0,
            #[cfg(not(feature = "no_closure"))]
            #[cfg(not(feature = "no_object"))]
            Self::LockGuard((r, _)) => **r = new_val.0,
            Self::Value(_) => unreachable!(),
            #[cfg(not(feature = "no_index"))]
            Self::StringChar(string, index, _) if string.is::<ImmutableString>() => {
                let mut s = string.write_lock::<ImmutableString>().unwrap();

                // Replace the character at the specified index position
                let new_ch = new_val.0.as_char().map_err(|err| {
                    Box::new(EvalAltResult::ErrorMismatchDataType(
                        err.to_string(),
                        "char".to_string(),
                        new_val.1,
                    ))
                })?;

                let mut chars = s.chars().collect::<StaticVec<_>>();

                // See if changed - if so, update the String
                if chars[*index] != new_ch {
                    chars[*index] = new_ch;
                    *s = chars.iter().collect::<String>().into();
                }
            }
            #[cfg(not(feature = "no_index"))]
            Self::StringChar(_, _, _) => unreachable!(),
        }

        Ok(())
    }
}

impl<'a> From<&'a mut Dynamic> for Target<'a> {
    #[inline(always)]
    fn from(value: &'a mut Dynamic) -> Self {
        #[cfg(not(feature = "no_closure"))]
        #[cfg(not(feature = "no_object"))]
        if value.is_shared() {
            // Cloning is cheap for a shared value
            let container = value.clone();
            return Self::LockGuard((value.write_lock::<Dynamic>().unwrap(), container));
        }

        Self::Ref(value)
    }
}

impl<T: Into<Dynamic>> From<T> for Target<'_> {
    #[inline(always)]
    fn from(value: T) -> Self {
        Self::Value(value.into())
    }
}

/// _[INTERNALS]_ A type that holds all the current states of the Engine.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Default)]
pub struct State {
    /// Normally, access to variables are parsed with a relative offset into the scope to avoid a lookup.
    /// In some situation, e.g. after running an `eval` statement, subsequent offsets become mis-aligned.
    /// When that happens, this flag is turned on to force a scope lookup by name.
    pub always_search: bool,
    /// Level of the current scope.  The global (root) level is zero, a new block (or function call)
    /// is one level higher, and so on.
    pub scope_level: usize,
    /// Number of operations performed.
    pub operations: u64,
    /// Number of modules loaded.
    pub modules: usize,
}

impl State {
    /// Is the state currently at global (root) level?
    #[inline(always)]
    pub fn is_global(&self) -> bool {
        self.scope_level == 0
    }
}

/// _[INTERNALS]_ A type containing all the limits imposed by the `Engine`.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[cfg(not(feature = "unchecked"))]
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Limits {
    /// Maximum levels of call-stack to prevent infinite recursion.
    ///
    /// Defaults to 16 for debug builds and 128 for non-debug builds.
    pub max_call_stack_depth: usize,
    /// Maximum depth of statements/expressions at global level (0 = unlimited).
    pub max_expr_depth: usize,
    /// Maximum depth of statements/expressions in functions (0 = unlimited).
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    pub max_function_expr_depth: usize,
    /// Maximum number of operations allowed to run (0 = unlimited).
    pub max_operations: u64,
    /// Maximum number of modules allowed to load.
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    pub max_modules: usize,
    /// Maximum length of a string (0 = unlimited).
    pub max_string_size: usize,
    /// Maximum length of an array (0 = unlimited).
    /// Not available under `no_index`.
    #[cfg(not(feature = "no_index"))]
    pub max_array_size: usize,
    /// Maximum number of properties in a map (0 = unlimited).
    /// Not available under `no_object`.
    #[cfg(not(feature = "no_object"))]
    pub max_map_size: usize,
}

/// Context of a script evaluation process.
#[derive(Debug)]
pub struct EvalContext<'e, 'x, 'px: 'x, 'a, 's, 'm, 'pm: 'm, 't, 'pt: 't> {
    pub(crate) engine: &'e Engine,
    pub scope: &'x mut Scope<'px>,
    pub(crate) mods: &'a mut Imports,
    pub(crate) state: &'s mut State,
    pub(crate) lib: &'m [&'pm Module],
    pub(crate) this_ptr: &'t mut Option<&'pt mut Dynamic>,
    pub(crate) level: usize,
}

impl<'e, 'x, 'px, 'a, 's, 'm, 'pm, 't, 'pt> EvalContext<'e, 'x, 'px, 'a, 's, 'm, 'pm, 't, 'pt> {
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
    pub fn imports(&'a self) -> &'a Imports {
        self.mods
    }
    /// Get an iterator over the namespaces containing definition of all script-defined functions.
    #[inline(always)]
    pub fn iter_namespaces(&self) -> impl Iterator<Item = &'pm Module> + 'm {
        self.lib.iter().cloned()
    }
    /// The current bound `this` pointer, if any.
    #[inline(always)]
    pub fn this_ptr(&self) -> Option<&Dynamic> {
        self.this_ptr.as_ref().map(|v| &**v)
    }
    /// The current nesting level of function calls.
    #[inline(always)]
    pub fn call_level(&self) -> usize {
        self.level
    }
}

/// Rhai main scripting engine.
///
/// # Thread Safety
///
/// `Engine` is re-entrant.
///
/// Currently, `Engine` is neither `Send` nor `Sync`. Use the `sync` feature to make it `Send + Sync`.
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
/// println!("Answer: {}", result);  // prints 42
/// # Ok(())
/// # }
/// ```
pub struct Engine {
    /// A unique ID identifying this scripting `Engine`.
    pub id: String,

    /// A module containing all functions directly loaded into the Engine.
    pub(crate) global_module: Module,
    /// A collection of all library packages loaded into the Engine.
    pub(crate) packages: PackagesCollection,

    /// A module resolution service.
    #[cfg(not(feature = "no_module"))]
    pub(crate) module_resolver: Option<Box<dyn ModuleResolver>>,

    /// A hashmap mapping type names to pretty-print names.
    pub(crate) type_names: HashMap<String, String>,

    /// A hashset containing symbols to disable.
    pub(crate) disabled_symbols: HashSet<String>,
    /// A hashset containing custom keywords and precedence to recognize.
    pub(crate) custom_keywords: HashMap<String, Option<u8>>,
    /// Custom syntax.
    pub(crate) custom_syntax: HashMap<ImmutableString, CustomSyntax>,
    /// Callback closure for resolving variable access.
    pub(crate) resolve_var: Option<OnVarCallback>,

    /// Callback closure for implementing the `print` command.
    pub(crate) print: Callback<str, ()>,
    /// Callback closure for implementing the `debug` command.
    pub(crate) debug: Callback<str, ()>,
    /// Callback closure for progress reporting.
    pub(crate) progress: Option<Callback<u64, Option<Dynamic>>>,

    /// Optimize the AST after compilation.
    pub(crate) optimization_level: OptimizationLevel,

    /// Max limits.
    #[cfg(not(feature = "unchecked"))]
    pub(crate) limits_set: Limits,
}

impl fmt::Debug for Engine {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.id.is_empty() {
            write!(f, "Engine({})", self.id)
        } else {
            f.write_str("Engine")
        }
    }
}

impl Default for Engine {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

/// Make getter function
#[cfg(not(feature = "no_object"))]
#[inline(always)]
pub fn make_getter(id: &str) -> String {
    format!("{}{}", FN_GET, id)
}

/// Make setter function
#[cfg(not(feature = "no_object"))]
#[inline(always)]
pub fn make_setter(id: &str) -> String {
    format!("{}{}", FN_SET, id)
}

/// Is this function an anonymous function?
#[cfg(not(feature = "no_function"))]
#[inline(always)]
pub fn is_anonymous_fn(fn_name: &str) -> bool {
    fn_name.starts_with(FN_ANONYMOUS)
}

/// Print/debug to stdout
fn default_print(_s: &str) {
    #[cfg(not(feature = "no_std"))]
    #[cfg(not(target_arch = "wasm32"))]
    println!("{}", _s);
}

/// Search for a module within an imports stack.
/// Position in `EvalAltResult` is `None` and must be set afterwards.
pub fn search_imports(
    mods: &Imports,
    state: &mut State,
    modules: &ModuleRef,
) -> Result<Shared<Module>, Box<EvalAltResult>> {
    let Ident { name: root, pos } = &modules[0];

    // Qualified - check if the root module is directly indexed
    let index = if state.always_search {
        0
    } else {
        modules.index().map_or(0, NonZeroUsize::get)
    };

    Ok(if index > 0 {
        let offset = mods.len() - index;
        mods.get(offset).expect("invalid index in Imports")
    } else {
        mods.find(root)
            .map(|n| mods.get(n).expect("invalid index in Imports"))
            .ok_or_else(|| EvalAltResult::ErrorModuleNotFound(root.to_string(), *pos))?
    })
}

impl Engine {
    /// Create a new `Engine`
    #[inline(always)]
    pub fn new() -> Self {
        // Create the new scripting Engine
        let mut engine = Self {
            id: Default::default(),

            packages: Default::default(),
            global_module: Default::default(),

            #[cfg(not(feature = "no_module"))]
            #[cfg(not(feature = "no_std"))]
            #[cfg(not(target_arch = "wasm32"))]
            module_resolver: Some(Box::new(resolvers::FileModuleResolver::new())),
            #[cfg(not(feature = "no_module"))]
            #[cfg(any(feature = "no_std", target_arch = "wasm32",))]
            module_resolver: None,

            type_names: Default::default(),
            disabled_symbols: Default::default(),
            custom_keywords: Default::default(),
            custom_syntax: Default::default(),

            // variable resolver
            resolve_var: None,

            // default print/debug implementations
            print: Box::new(default_print),
            debug: Box::new(default_print),

            // progress callback
            progress: None,

            // optimization level
            optimization_level: if cfg!(feature = "no_optimize") {
                OptimizationLevel::None
            } else {
                OptimizationLevel::Simple
            },

            #[cfg(not(feature = "unchecked"))]
            limits_set: Limits {
                max_call_stack_depth: MAX_CALL_STACK_DEPTH,
                max_expr_depth: MAX_EXPR_DEPTH,
                #[cfg(not(feature = "no_function"))]
                max_function_expr_depth: MAX_FUNCTION_EXPR_DEPTH,
                max_operations: 0,
                #[cfg(not(feature = "no_module"))]
                max_modules: usize::MAX,
                max_string_size: 0,
                #[cfg(not(feature = "no_index"))]
                max_array_size: 0,
                #[cfg(not(feature = "no_object"))]
                max_map_size: 0,
            },
        };

        engine.load_package(StandardPackage::new().get());

        engine
    }

    /// Create a new `Engine` with minimal built-in functions.
    /// Use the `load_package` method to load additional packages of functions.
    #[inline(always)]
    pub fn new_raw() -> Self {
        Self {
            id: Default::default(),

            packages: Default::default(),
            global_module: Default::default(),

            #[cfg(not(feature = "no_module"))]
            module_resolver: None,

            type_names: Default::default(),
            disabled_symbols: Default::default(),
            custom_keywords: Default::default(),
            custom_syntax: Default::default(),

            resolve_var: None,

            print: Box::new(|_| {}),
            debug: Box::new(|_| {}),
            progress: None,

            optimization_level: if cfg!(feature = "no_optimize") {
                OptimizationLevel::None
            } else {
                OptimizationLevel::Simple
            },

            #[cfg(not(feature = "unchecked"))]
            limits_set: Limits {
                max_call_stack_depth: MAX_CALL_STACK_DEPTH,
                max_expr_depth: MAX_EXPR_DEPTH,
                #[cfg(not(feature = "no_function"))]
                max_function_expr_depth: MAX_FUNCTION_EXPR_DEPTH,
                max_operations: 0,
                #[cfg(not(feature = "no_module"))]
                max_modules: usize::MAX,
                max_string_size: 0,
                #[cfg(not(feature = "no_index"))]
                max_array_size: 0,
                #[cfg(not(feature = "no_object"))]
                max_map_size: 0,
            },
        }
    }

    /// Search for a variable within the scope or within imports,
    /// depending on whether the variable name is qualified.
    pub(crate) fn search_namespace<'s, 'a>(
        &self,
        scope: &'s mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        this_ptr: &'s mut Option<&mut Dynamic>,
        expr: &'a Expr,
    ) -> Result<(Target<'s>, &'a str, ScopeEntryType, Position), Box<EvalAltResult>> {
        match expr {
            Expr::Variable(v) => match v.as_ref() {
                // Qualified variable
                (_, Some(modules), hash_var, Ident { name, pos }) => {
                    let module = search_imports(mods, state, modules)?;
                    let target = module.get_qualified_var(*hash_var).map_err(|mut err| {
                        match *err {
                            EvalAltResult::ErrorVariableNotFound(ref mut err_name, _) => {
                                *err_name = format!("{}{}", modules, name);
                            }
                            _ => (),
                        }
                        err.fill_position(*pos)
                    })?;

                    // Module variables are constant
                    Ok((target.clone().into(), name, ScopeEntryType::Constant, *pos))
                }
                // Normal variable access
                _ => self.search_scope_only(scope, mods, state, lib, this_ptr, expr),
            },
            _ => unreachable!(),
        }
    }

    /// Search for a variable within the scope
    pub(crate) fn search_scope_only<'s, 'a>(
        &self,
        scope: &'s mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        this_ptr: &'s mut Option<&mut Dynamic>,
        expr: &'a Expr,
    ) -> Result<(Target<'s>, &'a str, ScopeEntryType, Position), Box<EvalAltResult>> {
        let (index, _, _, Ident { name, pos }) = match expr {
            Expr::Variable(v) => v.as_ref(),
            _ => unreachable!(),
        };

        // Check if the variable is `this`
        if name == KEYWORD_THIS {
            if let Some(val) = this_ptr {
                return Ok(((*val).into(), KEYWORD_THIS, ScopeEntryType::Normal, *pos));
            } else {
                return EvalAltResult::ErrorUnboundThis(*pos).into();
            }
        }

        // Check if it is directly indexed
        let index = if state.always_search {
            0
        } else {
            index.map_or(0, NonZeroUsize::get)
        };

        // Check the variable resolver, if any
        if let Some(ref resolve_var) = self.resolve_var {
            let context = EvalContext {
                engine: self,
                scope,
                mods,
                state,
                lib,
                this_ptr,
                level: 0,
            };
            if let Some(result) =
                resolve_var(name, index, &context).map_err(|err| err.fill_position(*pos))?
            {
                return Ok((result.into(), name, ScopeEntryType::Constant, *pos));
            }
        }

        let index = if index > 0 {
            scope.len() - index
        } else {
            // Find the variable in the scope
            scope
                .get_index(name)
                .ok_or_else(|| EvalAltResult::ErrorVariableNotFound(name.into(), *pos))?
                .0
        };

        let (val, typ) = scope.get_mut(index);

        // Check for data race - probably not necessary because the only place it should conflict is in a method call
        //                       when the object variable is also used as a parameter.
        // if cfg!(not(feature = "no_closure")) && val.is_locked() {
        //     return EvalAltResult::ErrorDataRace(name.into(), *pos).into();
        // }

        Ok((val.into(), name, typ, *pos))
    }

    /// Chain-evaluate a dot/index chain.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    fn eval_dot_index_chain_helper(
        &self,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        target: &mut Target,
        rhs: &Expr,
        mut idx_values: StaticVec<IndexChainValue>,
        chain_type: ChainType,
        level: usize,
        new_val: Option<(Dynamic, Position)>,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        if chain_type == ChainType::None {
            panic!();
        }

        let is_ref = target.is_ref();

        let next_chain = match rhs {
            Expr::Index(_, _) => ChainType::Index,
            Expr::Dot(_, _) => ChainType::Dot,
            _ => ChainType::None,
        };

        // Pop the last index value
        let idx_val = idx_values.pop().unwrap();

        match chain_type {
            #[cfg(not(feature = "no_index"))]
            ChainType::Index => {
                let pos = rhs.position();

                match rhs {
                    // xxx[idx].expr... | xxx[idx][expr]...
                    Expr::Dot(x, x_pos) | Expr::Index(x, x_pos) => {
                        let idx_pos = x.lhs.position();
                        let idx_val = idx_val.as_value();
                        let obj_ptr = &mut self.get_indexed_mut(
                            mods, state, lib, target, idx_val, idx_pos, false, true, level,
                        )?;

                        self.eval_dot_index_chain_helper(
                            mods, state, lib, this_ptr, obj_ptr, &x.rhs, idx_values, next_chain,
                            level, new_val,
                        )
                        .map_err(|err| err.fill_position(*x_pos))
                    }
                    // xxx[rhs] = new_val
                    _ if new_val.is_some() => {
                        let idx_val = idx_val.as_value();
                        let mut idx_val2 = idx_val.clone();

                        // `call_setter` is introduced to bypass double mutable borrowing of target
                        let _call_setter = match self.get_indexed_mut(
                            mods, state, lib, target, idx_val, pos, true, false, level,
                        ) {
                            // Indexed value is a reference - update directly
                            Ok(ref mut obj_ptr) => {
                                obj_ptr.set_value(new_val.unwrap())?;
                                None
                            }
                            Err(err) => match *err {
                                // No index getter - try to call an index setter
                                #[cfg(not(feature = "no_index"))]
                                EvalAltResult::ErrorIndexingType(_, _) => Some(new_val.unwrap()),
                                // Any other error - return
                                err => return Err(Box::new(err)),
                            },
                        };

                        #[cfg(not(feature = "no_index"))]
                        if let Some(mut new_val) = _call_setter {
                            let val = target.as_mut();
                            let val_type_name = val.type_name();
                            let args = &mut [val, &mut idx_val2, &mut new_val.0];

                            self.exec_fn_call(
                                mods, state, lib, FN_IDX_SET, 0, args, is_ref, true, false, None,
                                None, level,
                            )
                            .map_err(|err| match *err {
                                EvalAltResult::ErrorFunctionNotFound(fn_sig, _)
                                    if fn_sig.ends_with("]=") =>
                                {
                                    EvalAltResult::ErrorIndexingType(
                                        self.map_type_name(val_type_name).into(),
                                        NO_POS,
                                    )
                                }
                                err => err,
                            })?;
                        }

                        Ok(Default::default())
                    }
                    // xxx[rhs]
                    _ => {
                        let idx_val = idx_val.as_value();
                        self.get_indexed_mut(
                            mods, state, lib, target, idx_val, pos, false, true, level,
                        )
                        .map(|v| (v.take_or_clone(), false))
                    }
                }
            }

            #[cfg(not(feature = "no_object"))]
            ChainType::Dot => {
                match rhs {
                    // xxx.fn_name(arg_expr_list)
                    Expr::FnCall(x, pos) if x.namespace.is_none() => {
                        let FnCallInfo {
                            name,
                            native_only: native,
                            hash,
                            def_value,
                            ..
                        } = x.as_ref();
                        let def_value = def_value.map(Into::<Dynamic>::into);
                        let args = idx_val.as_fn_call_args();
                        self.make_method_call(
                            mods, state, lib, name, *hash, target, args, def_value, *native, false,
                            level,
                        )
                        .map_err(|err| err.fill_position(*pos))
                    }
                    // xxx.module::fn_name(...) - syntax error
                    Expr::FnCall(_, _) => unreachable!(),
                    // {xxx:map}.id = ???
                    Expr::Property(x) if target.is::<Map>() && new_val.is_some() => {
                        let IdentX { name, pos } = &x.1;
                        let index = name.clone().into();
                        let mut val = self.get_indexed_mut(
                            mods, state, lib, target, index, *pos, true, false, level,
                        )?;

                        val.set_value(new_val.unwrap())?;
                        Ok((Default::default(), true))
                    }
                    // {xxx:map}.id
                    Expr::Property(x) if target.is::<Map>() => {
                        let IdentX { name, pos } = &x.1;
                        let index = name.clone().into();
                        let val = self.get_indexed_mut(
                            mods, state, lib, target, index, *pos, false, false, level,
                        )?;

                        Ok((val.take_or_clone(), false))
                    }
                    // xxx.id = ???
                    Expr::Property(x) if new_val.is_some() => {
                        let ((_, setter), IdentX { pos, .. }) = x.as_ref();
                        let mut new_val = new_val;
                        let mut args = [target.as_mut(), &mut new_val.as_mut().unwrap().0];
                        self.exec_fn_call(
                            mods, state, lib, setter, 0, &mut args, is_ref, true, false, None,
                            None, level,
                        )
                        .map(|(v, _)| (v, true))
                        .map_err(|err| err.fill_position(*pos))
                    }
                    // xxx.id
                    Expr::Property(x) => {
                        let ((getter, _), IdentX { pos, .. }) = x.as_ref();
                        let mut args = [target.as_mut()];
                        self.exec_fn_call(
                            mods, state, lib, getter, 0, &mut args, is_ref, true, false, None,
                            None, level,
                        )
                        .map(|(v, _)| (v, false))
                        .map_err(|err| err.fill_position(*pos))
                    }
                    // {xxx:map}.sub_lhs[expr] | {xxx:map}.sub_lhs.expr
                    Expr::Index(x, x_pos) | Expr::Dot(x, x_pos) if target.is::<Map>() => {
                        let mut val = match &x.lhs {
                            Expr::Property(p) => {
                                let IdentX { name, pos } = &p.1;
                                let index = name.clone().into();
                                self.get_indexed_mut(
                                    mods, state, lib, target, index, *pos, false, true, level,
                                )?
                            }
                            // {xxx:map}.fn_name(arg_expr_list)[expr] | {xxx:map}.fn_name(arg_expr_list).expr
                            Expr::FnCall(x, pos) if x.namespace.is_none() => {
                                let FnCallInfo {
                                    name,
                                    native_only: native,
                                    hash,
                                    def_value,
                                    ..
                                } = x.as_ref();
                                let def_value = def_value.map(Into::<Dynamic>::into);
                                let args = idx_val.as_fn_call_args();
                                let (val, _) = self
                                    .make_method_call(
                                        mods, state, lib, name, *hash, target, args, def_value,
                                        *native, false, level,
                                    )
                                    .map_err(|err| err.fill_position(*pos))?;
                                val.into()
                            }
                            // {xxx:map}.module::fn_name(...) - syntax error
                            Expr::FnCall(_, _) => unreachable!(),
                            // Others - syntax error
                            _ => unreachable!(),
                        };

                        self.eval_dot_index_chain_helper(
                            mods, state, lib, this_ptr, &mut val, &x.rhs, idx_values, next_chain,
                            level, new_val,
                        )
                        .map_err(|err| err.fill_position(*x_pos))
                    }
                    // xxx.sub_lhs[expr] | xxx.sub_lhs.expr
                    Expr::Index(x, x_pos) | Expr::Dot(x, x_pos) => {
                        match &x.lhs {
                            // xxx.prop[expr] | xxx.prop.expr
                            Expr::Property(p) => {
                                let ((getter, setter), IdentX { pos, .. }) = p.as_ref();
                                let arg_values = &mut [target.as_mut(), &mut Default::default()];
                                let args = &mut arg_values[..1];
                                let (mut val, updated) = self
                                    .exec_fn_call(
                                        mods, state, lib, getter, 0, args, is_ref, true, false,
                                        None, None, level,
                                    )
                                    .map_err(|err| err.fill_position(*pos))?;

                                let val = &mut val;

                                let (result, may_be_changed) = self
                                    .eval_dot_index_chain_helper(
                                        mods,
                                        state,
                                        lib,
                                        this_ptr,
                                        &mut val.into(),
                                        &x.rhs,
                                        idx_values,
                                        next_chain,
                                        level,
                                        new_val,
                                    )
                                    .map_err(|err| err.fill_position(*x_pos))?;

                                // Feed the value back via a setter just in case it has been updated
                                if updated || may_be_changed {
                                    // Re-use args because the first &mut parameter will not be consumed
                                    arg_values[1] = val;
                                    self.exec_fn_call(
                                        mods, state, lib, setter, 0, arg_values, is_ref, true,
                                        false, None, None, level,
                                    )
                                    .or_else(
                                        |err| match *err {
                                            // If there is no setter, no need to feed it back because the property is read-only
                                            EvalAltResult::ErrorDotExpr(_, _) => {
                                                Ok(Default::default())
                                            }
                                            _ => Err(err.fill_position(*x_pos)),
                                        },
                                    )?;
                                }

                                Ok((result, may_be_changed))
                            }
                            // xxx.fn_name(arg_expr_list)[expr] | xxx.fn_name(arg_expr_list).expr
                            Expr::FnCall(f, pos) if f.namespace.is_none() => {
                                let FnCallInfo {
                                    name,
                                    native_only: native,
                                    hash,
                                    def_value,
                                    ..
                                } = f.as_ref();
                                let def_value = def_value.map(Into::<Dynamic>::into);
                                let args = idx_val.as_fn_call_args();
                                let (mut val, _) = self
                                    .make_method_call(
                                        mods, state, lib, name, *hash, target, args, def_value,
                                        *native, false, level,
                                    )
                                    .map_err(|err| err.fill_position(*pos))?;
                                let val = &mut val;
                                let target = &mut val.into();

                                self.eval_dot_index_chain_helper(
                                    mods, state, lib, this_ptr, target, &x.rhs, idx_values,
                                    next_chain, level, new_val,
                                )
                                .map_err(|err| err.fill_position(*pos))
                            }
                            // xxx.module::fn_name(...) - syntax error
                            Expr::FnCall(_, _) => unreachable!(),
                            // Others - syntax error
                            _ => unreachable!(),
                        }
                    }
                    // Syntax error
                    _ => EvalAltResult::ErrorDotExpr("".into(), rhs.position()).into(),
                }
            }

            _ => unreachable!(),
        }
    }

    /// Evaluate a dot/index chain.
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    fn eval_dot_index_chain(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        expr: &Expr,
        level: usize,
        new_val: Option<(Dynamic, Position)>,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let (
            BinaryExpr {
                lhs: dot_lhs,
                rhs: dot_rhs,
            },
            chain_type,
            op_pos,
        ) = match expr {
            Expr::Index(x, pos) => (x.as_ref(), ChainType::Index, *pos),
            Expr::Dot(x, pos) => (x.as_ref(), ChainType::Dot, *pos),
            _ => unreachable!(),
        };

        let mut idx_values = StaticVec::new();

        self.eval_indexed_chain(
            scope,
            mods,
            state,
            lib,
            this_ptr,
            dot_rhs,
            chain_type,
            &mut idx_values,
            0,
            level,
        )?;

        match dot_lhs {
            // id.??? or id[???]
            Expr::Variable(x) => {
                let Ident {
                    name: var_name,
                    pos: var_pos,
                } = &x.3;

                self.inc_operations(state)
                    .map_err(|err| err.fill_position(*var_pos))?;

                let (target, _, typ, pos) =
                    self.search_namespace(scope, mods, state, lib, this_ptr, dot_lhs)?;

                // Constants cannot be modified
                match typ {
                    ScopeEntryType::Constant if new_val.is_some() => {
                        return EvalAltResult::ErrorAssignmentToConstant(var_name.to_string(), pos)
                            .into();
                    }
                    ScopeEntryType::Constant | ScopeEntryType::Normal => (),
                }

                let obj_ptr = &mut target.into();
                self.eval_dot_index_chain_helper(
                    mods, state, lib, &mut None, obj_ptr, dot_rhs, idx_values, chain_type, level,
                    new_val,
                )
                .map(|(v, _)| v)
                .map_err(|err| err.fill_position(op_pos))
            }
            // {expr}.??? = ??? or {expr}[???] = ???
            _ if new_val.is_some() => unreachable!(),
            // {expr}.??? or {expr}[???]
            expr => {
                let val = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;
                let obj_ptr = &mut val.into();
                self.eval_dot_index_chain_helper(
                    mods, state, lib, this_ptr, obj_ptr, dot_rhs, idx_values, chain_type, level,
                    new_val,
                )
                .map(|(v, _)| v)
                .map_err(|err| err.fill_position(op_pos))
            }
        }
    }

    /// Evaluate a chain of indexes and store the results in a StaticVec.
    /// StaticVec is used to avoid an allocation in the overwhelming cases of just a few levels of indexing.
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    fn eval_indexed_chain(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        expr: &Expr,
        chain_type: ChainType,
        idx_values: &mut StaticVec<IndexChainValue>,
        size: usize,
        level: usize,
    ) -> Result<(), Box<EvalAltResult>> {
        self.inc_operations(state)
            .map_err(|err| err.fill_position(expr.position()))?;

        match expr {
            Expr::FnCall(x, _) if x.namespace.is_none() => {
                let arg_values = x
                    .args
                    .iter()
                    .map(|arg_expr| {
                        self.eval_expr(scope, mods, state, lib, this_ptr, arg_expr, level)
                    })
                    .collect::<Result<StaticVec<_>, _>>()?;

                idx_values.push(arg_values.into());
            }
            Expr::FnCall(_, _) => unreachable!(),
            Expr::Property(_) => idx_values.push(IndexChainValue::None),
            Expr::Index(x, _) | Expr::Dot(x, _) => {
                let BinaryExpr { lhs, rhs, .. } = x.as_ref();

                // Evaluate in left-to-right order
                let lhs_val = match lhs {
                    Expr::Property(_) => IndexChainValue::None,
                    Expr::FnCall(x, _) if chain_type == ChainType::Dot && x.namespace.is_none() => {
                        x.args
                            .iter()
                            .map(|arg_expr| {
                                self.eval_expr(scope, mods, state, lib, this_ptr, arg_expr, level)
                            })
                            .collect::<Result<StaticVec<Dynamic>, _>>()?
                            .into()
                    }
                    Expr::FnCall(_, _) => unreachable!(),
                    _ => self
                        .eval_expr(scope, mods, state, lib, this_ptr, lhs, level)?
                        .into(),
                };

                // Push in reverse order
                let chain_type = match expr {
                    Expr::Index(_, _) => ChainType::Index,
                    Expr::Dot(_, _) => ChainType::Dot,
                    _ => unreachable!(),
                };
                self.eval_indexed_chain(
                    scope, mods, state, lib, this_ptr, rhs, chain_type, idx_values, size, level,
                )?;

                idx_values.push(lhs_val);
            }
            _ => idx_values.push(
                self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?
                    .into(),
            ),
        }

        Ok(())
    }

    /// Get the value at the indexed position of a base type
    /// Position in `EvalAltResult` may be None and should be set afterwards.
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    fn get_indexed_mut<'a>(
        &self,
        _mods: &mut Imports,
        state: &mut State,
        _lib: &[&Module],
        target: &'a mut Target,
        idx: Dynamic,
        idx_pos: Position,
        _create: bool,
        _indexers: bool,
        _level: usize,
    ) -> Result<Target<'a>, Box<EvalAltResult>> {
        self.inc_operations(state)?;

        #[cfg(not(feature = "no_index"))]
        let is_ref = target.is_ref();

        let val = target.as_mut();

        match val {
            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Array(arr)) => {
                // val_array[idx]
                let index = idx
                    .as_int()
                    .map_err(|err| self.make_type_mismatch_err::<INT>(err, idx_pos))?;

                let arr_len = arr.len();

                if index >= 0 {
                    arr.get_mut(index as usize)
                        .map(Target::from)
                        .ok_or_else(|| {
                            EvalAltResult::ErrorArrayBounds(arr_len, index, idx_pos).into()
                        })
                } else {
                    EvalAltResult::ErrorArrayBounds(arr_len, index, idx_pos).into()
                }
            }

            #[cfg(not(feature = "no_object"))]
            Dynamic(Union::Map(map)) => {
                // val_map[idx]
                Ok(if _create {
                    let index = idx.take_immutable_string().map_err(|err| {
                        self.make_type_mismatch_err::<ImmutableString>(err, idx_pos)
                    })?;

                    map.entry(index).or_insert_with(Default::default).into()
                } else {
                    let index = idx.read_lock::<ImmutableString>().ok_or_else(|| {
                        self.make_type_mismatch_err::<ImmutableString>("", idx_pos)
                    })?;

                    map.get_mut(&*index)
                        .map(Target::from)
                        .unwrap_or_else(|| Target::from(()))
                })
            }

            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Str(s)) => {
                // val_string[idx]
                let chars_len = s.chars().count();
                let index = idx
                    .as_int()
                    .map_err(|err| self.make_type_mismatch_err::<INT>(err, idx_pos))?;

                if index >= 0 {
                    let offset = index as usize;
                    let ch = s.chars().nth(offset).ok_or_else(|| {
                        EvalAltResult::ErrorStringBounds(chars_len, index, idx_pos)
                    })?;
                    Ok(Target::StringChar(val, offset, ch.into()))
                } else {
                    EvalAltResult::ErrorStringBounds(chars_len, index, idx_pos).into()
                }
            }

            #[cfg(not(feature = "no_index"))]
            _ if _indexers => {
                let type_name = val.type_name();
                let mut idx = idx;
                let args = &mut [val, &mut idx];
                self.exec_fn_call(
                    _mods, state, _lib, FN_IDX_GET, 0, args, is_ref, true, false, None, None,
                    _level,
                )
                .map(|(v, _)| v.into())
                .map_err(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(fn_sig, _) if fn_sig.ends_with(']') => {
                        Box::new(EvalAltResult::ErrorIndexingType(type_name.into(), NO_POS))
                    }
                    _ => err,
                })
            }

            _ => {
                EvalAltResult::ErrorIndexingType(self.map_type_name(val.type_name()).into(), NO_POS)
                    .into()
            }
        }
    }

    // Evaluate an 'in' expression
    fn eval_in_expr(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        lhs: &Expr,
        rhs: &Expr,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.inc_operations(state)
            .map_err(|err| err.fill_position(rhs.position()))?;

        let lhs_value = self.eval_expr(scope, mods, state, lib, this_ptr, lhs, level)?;
        let rhs_value = self.eval_expr(scope, mods, state, lib, this_ptr, rhs, level)?;

        match rhs_value {
            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Array(mut rhs_value)) => {
                const OP_FUNC: &str = "==";

                // Call the `==` operator to compare each value
                let def_value = Some(false.into());

                for value in rhs_value.iter_mut() {
                    let args = &mut [&mut lhs_value.clone(), value];
                    let def_value = def_value.clone();

                    // Qualifiers (none) + function name + number of arguments + argument `TypeId`'s.
                    let hash =
                        calc_native_fn_hash(empty(), OP_FUNC, args.iter().map(|a| a.type_id()));

                    if self
                        .call_native_fn(
                            mods, state, lib, OP_FUNC, hash, args, false, false, def_value,
                        )
                        .map_err(|err| err.fill_position(rhs.position()))?
                        .0
                        .as_bool()
                        .unwrap_or(false)
                    {
                        return Ok(true.into());
                    }
                }

                Ok(false.into())
            }
            #[cfg(not(feature = "no_object"))]
            Dynamic(Union::Map(rhs_value)) => match lhs_value {
                // Only allows string or char
                Dynamic(Union::Str(s)) => Ok(rhs_value.contains_key(&s).into()),
                Dynamic(Union::Char(c)) => Ok(rhs_value.contains_key(&c.to_string()).into()),
                _ => EvalAltResult::ErrorInExpr(lhs.position()).into(),
            },
            Dynamic(Union::Str(rhs_value)) => match lhs_value {
                // Only allows string or char
                Dynamic(Union::Str(s)) => Ok(rhs_value.contains(s.as_str()).into()),
                Dynamic(Union::Char(c)) => Ok(rhs_value.contains(c).into()),
                _ => EvalAltResult::ErrorInExpr(lhs.position()).into(),
            },
            _ => EvalAltResult::ErrorInExpr(rhs.position()).into(),
        }
    }

    /// Evaluate an expression
    pub(crate) fn eval_expr(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        expr: &Expr,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.inc_operations(state)
            .map_err(|err| err.fill_position(expr.position()))?;

        let result = match expr {
            Expr::Expr(x) => self.eval_expr(scope, mods, state, lib, this_ptr, x, level),

            Expr::IntegerConstant(x, _) => Ok((*x).into()),
            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(x, _) => Ok(x.0.into()),
            Expr::StringConstant(x) => Ok(x.name.clone().into()),
            Expr::CharConstant(x, _) => Ok((*x).into()),
            Expr::FnPointer(x) => {
                Ok(FnPtr::new_unchecked(x.name.clone(), Default::default()).into())
            }
            Expr::Variable(x) if (x.3).name == KEYWORD_THIS => {
                if let Some(val) = this_ptr {
                    Ok(val.clone())
                } else {
                    EvalAltResult::ErrorUnboundThis((x.3).pos).into()
                }
            }
            Expr::Variable(_) => {
                let (val, _, _, _) =
                    self.search_namespace(scope, mods, state, lib, this_ptr, expr)?;
                Ok(val.take_or_clone())
            }
            Expr::Property(_) => unreachable!(),

            // Statement block
            Expr::Stmt(x, _) => {
                self.eval_statements(scope, mods, state, lib, this_ptr, x.as_ref(), level)
            }

            // lhs[idx_expr]
            #[cfg(not(feature = "no_index"))]
            Expr::Index(_, _) => {
                self.eval_dot_index_chain(scope, mods, state, lib, this_ptr, expr, level, None)
            }

            // lhs.dot_rhs
            #[cfg(not(feature = "no_object"))]
            Expr::Dot(_, _) => {
                self.eval_dot_index_chain(scope, mods, state, lib, this_ptr, expr, level, None)
            }

            #[cfg(not(feature = "no_index"))]
            Expr::Array(x, _) => Ok(Dynamic(Union::Array(Box::new(
                x.iter()
                    .map(|item| self.eval_expr(scope, mods, state, lib, this_ptr, item, level))
                    .collect::<Result<Vec<_>, _>>()?,
            )))),

            #[cfg(not(feature = "no_object"))]
            Expr::Map(x, _) => Ok(Dynamic(Union::Map(Box::new(
                x.iter()
                    .map(|(key, expr)| {
                        self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)
                            .map(|val| (key.name.clone(), val))
                    })
                    .collect::<Result<HashMap<_, _>, _>>()?,
            )))),

            // Normal function call
            Expr::FnCall(x, pos) if x.namespace.is_none() => {
                let FnCallInfo {
                    name,
                    native_only: native,
                    capture: cap_scope,
                    hash,
                    args,
                    def_value,
                    ..
                } = x.as_ref();
                let def_value = def_value.map(Into::<Dynamic>::into);
                self.make_function_call(
                    scope, mods, state, lib, this_ptr, name, args, def_value, *hash, *native,
                    false, *cap_scope, level,
                )
                .map_err(|err| err.fill_position(*pos))
            }

            // Module-qualified function call
            Expr::FnCall(x, pos) if x.namespace.is_some() => {
                let FnCallInfo {
                    name,
                    namespace,
                    hash,
                    args,
                    def_value,
                    ..
                } = x.as_ref();
                let modules = namespace.as_ref().map(|v| v.as_ref());
                self.make_qualified_function_call(
                    scope, mods, state, lib, this_ptr, modules, name, args, *def_value, *hash,
                    level,
                )
                .map_err(|err| err.fill_position(*pos))
            }

            Expr::In(x, _) => {
                self.eval_in_expr(scope, mods, state, lib, this_ptr, &x.lhs, &x.rhs, level)
            }

            Expr::And(x, _) => {
                Ok((self
                    .eval_expr(scope, mods, state, lib, this_ptr, &x.lhs, level)?
                    .as_bool()
                    .map_err(|err| self.make_type_mismatch_err::<bool>(err, x.lhs.position()))?
                    && // Short-circuit using &&
                self
                    .eval_expr(scope, mods, state, lib, this_ptr, &x.rhs, level)?
                    .as_bool()
                    .map_err(|err| self.make_type_mismatch_err::<bool>(err, x.rhs.position()))?)
                .into())
            }

            Expr::Or(x, _) => {
                Ok((self
                    .eval_expr(scope, mods, state, lib, this_ptr, &x.lhs, level)?
                    .as_bool()
                    .map_err(|err| self.make_type_mismatch_err::<bool>(err, x.lhs.position()))?
                    || // Short-circuit using ||
                self
                    .eval_expr(scope, mods, state, lib, this_ptr, &x.rhs, level)?
                    .as_bool()
                    .map_err(|err| self.make_type_mismatch_err::<bool>(err, x.rhs.position()))?)
                .into())
            }

            Expr::True(_) => Ok(true.into()),
            Expr::False(_) => Ok(false.into()),
            Expr::Unit(_) => Ok(().into()),

            Expr::Custom(custom, _) => {
                let expressions = custom
                    .keywords()
                    .iter()
                    .map(Into::into)
                    .collect::<StaticVec<_>>();
                let mut context = EvalContext {
                    engine: self,
                    scope,
                    mods,
                    state,
                    lib,
                    this_ptr,
                    level,
                };
                (custom.func)(&mut context, &expressions)
            }

            _ => unreachable!(),
        };

        self.check_data_size(result)
            .map_err(|err| err.fill_position(expr.position()))
    }

    pub(crate) fn eval_statements<'a>(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        statements: impl IntoIterator<Item = &'a Stmt>,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let prev_scope_len = scope.len();
        let prev_mods_len = mods.len();
        state.scope_level += 1;

        let result = statements
            .into_iter()
            .try_fold(Default::default(), |_, stmt| {
                self.eval_stmt(scope, mods, state, lib, this_ptr, stmt, level)
            });

        scope.rewind(prev_scope_len);
        mods.truncate(prev_mods_len);
        state.scope_level -= 1;

        // The impact of an eval statement goes away at the end of a block
        // because any new variables introduced will go out of scope
        state.always_search = false;

        result
    }

    /// Evaluate a statement
    ///
    ///
    /// # Safety
    ///
    /// This method uses some unsafe code, mainly for avoiding cloning of local variable names via
    /// direct lifetime casting.
    pub(crate) fn eval_stmt(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &[&Module],
        this_ptr: &mut Option<&mut Dynamic>,
        stmt: &Stmt,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.inc_operations(state)
            .map_err(|err| err.fill_position(stmt.position()))?;

        let result = match stmt {
            // No-op
            Stmt::Noop(_) => Ok(Default::default()),

            // Expression as statement
            Stmt::Expr(expr) => self.eval_expr(scope, mods, state, lib, this_ptr, expr, level),

            // var op= rhs
            Stmt::Assignment(x, op_pos) if x.0.get_variable_access(false).is_some() => {
                let (lhs_expr, op, rhs_expr) = x.as_ref();
                let mut rhs_val = self
                    .eval_expr(scope, mods, state, lib, this_ptr, rhs_expr, level)?
                    .flatten();
                let (mut lhs_ptr, name, typ, pos) =
                    self.search_namespace(scope, mods, state, lib, this_ptr, lhs_expr)?;

                if !lhs_ptr.is_ref() {
                    return EvalAltResult::ErrorAssignmentToConstant(name.to_string(), pos).into();
                }

                self.inc_operations(state)
                    .map_err(|err| err.fill_position(pos))?;

                match typ {
                    // Assignment to constant variable
                    ScopeEntryType::Constant => Err(Box::new(
                        EvalAltResult::ErrorAssignmentToConstant(name.to_string(), pos),
                    )),
                    // Normal assignment
                    ScopeEntryType::Normal if op.is_empty() => {
                        if cfg!(not(feature = "no_closure")) && lhs_ptr.is_shared() {
                            *lhs_ptr.as_mut().write_lock::<Dynamic>().unwrap() = rhs_val;
                        } else {
                            *lhs_ptr.as_mut() = rhs_val;
                        }
                        Ok(Default::default())
                    }
                    // Op-assignment - in order of precedence:
                    ScopeEntryType::Normal => {
                        // 1) Native registered overriding function
                        // 2) Built-in implementation
                        // 3) Map to `var = var op rhs`

                        // Qualifiers (none) + function name + number of arguments + argument `TypeId`'s.
                        let arg_types =
                            once(lhs_ptr.as_mut().type_id()).chain(once(rhs_val.type_id()));
                        let hash_fn = calc_native_fn_hash(empty(), op, arg_types);

                        match self
                            .global_module
                            .get_fn(hash_fn, false)
                            .or_else(|| self.packages.get_fn(hash_fn, false))
                        {
                            // op= function registered as method
                            Some(func) if func.is_method() => {
                                let mut lock_guard;
                                let lhs_ptr_inner;

                                if cfg!(not(feature = "no_closure")) && lhs_ptr.is_shared() {
                                    lock_guard = lhs_ptr.as_mut().write_lock::<Dynamic>().unwrap();
                                    lhs_ptr_inner = lock_guard.deref_mut();
                                } else {
                                    lhs_ptr_inner = lhs_ptr.as_mut();
                                }

                                let args = &mut [lhs_ptr_inner, &mut rhs_val];

                                // Overriding exact implementation
                                if func.is_plugin_fn() {
                                    func.get_plugin_fn().call((self, mods, lib).into(), args)?;
                                } else {
                                    func.get_native_fn()((self, mods, lib).into(), args)?;
                                }
                            }
                            // Built-in op-assignment function
                            _ if run_builtin_op_assignment(op, lhs_ptr.as_mut(), &rhs_val)?
                                .is_some() => {}
                            // Not built-in: expand to `var = var op rhs`
                            _ => {
                                let op = &op[..op.len() - 1]; // extract operator without =

                                // Clone the LHS value
                                let args = &mut [&mut lhs_ptr.as_mut().clone(), &mut rhs_val];

                                // Run function
                                let (value, _) = self
                                    .exec_fn_call(
                                        mods, state, lib, op, 0, args, false, false, false, None,
                                        None, level,
                                    )
                                    .map_err(|err| err.fill_position(*op_pos))?;

                                let value = value.flatten();

                                if cfg!(not(feature = "no_closure")) && lhs_ptr.is_shared() {
                                    *lhs_ptr.as_mut().write_lock::<Dynamic>().unwrap() = value;
                                } else {
                                    *lhs_ptr.as_mut() = value;
                                }
                            }
                        }
                        Ok(Default::default())
                    }
                }
            }

            // lhs op= rhs
            Stmt::Assignment(x, op_pos) => {
                let (lhs_expr, op, rhs_expr) = x.as_ref();
                let mut rhs_val =
                    self.eval_expr(scope, mods, state, lib, this_ptr, rhs_expr, level)?;

                let _new_val = if op.is_empty() {
                    // Normal assignment
                    Some((rhs_val, rhs_expr.position()))
                } else {
                    // Op-assignment - always map to `lhs = lhs op rhs`
                    let op = &op[..op.len() - 1]; // extract operator without =
                    let args = &mut [
                        &mut self.eval_expr(scope, mods, state, lib, this_ptr, lhs_expr, level)?,
                        &mut rhs_val,
                    ];

                    let result = self
                        .exec_fn_call(
                            mods, state, lib, op, 0, args, false, false, false, None, None, level,
                        )
                        .map(|(v, _)| v)
                        .map_err(|err| err.fill_position(*op_pos))?;

                    Some((result, rhs_expr.position()))
                };

                // Must be either `var[index] op= val` or `var.prop op= val`
                match lhs_expr {
                    // name op= rhs (handled above)
                    Expr::Variable(_) => unreachable!(),
                    // idx_lhs[idx_expr] op= rhs
                    #[cfg(not(feature = "no_index"))]
                    Expr::Index(_, _) => {
                        self.eval_dot_index_chain(
                            scope, mods, state, lib, this_ptr, lhs_expr, level, _new_val,
                        )?;
                        Ok(Default::default())
                    }
                    // dot_lhs.dot_rhs op= rhs
                    #[cfg(not(feature = "no_object"))]
                    Expr::Dot(_, _) => {
                        self.eval_dot_index_chain(
                            scope, mods, state, lib, this_ptr, lhs_expr, level, _new_val,
                        )?;
                        Ok(Default::default())
                    }
                    // Non-lvalue expression (should be caught during parsing)
                    _ => unreachable!(),
                }
            }

            // Block scope
            Stmt::Block(statements, _) => {
                self.eval_statements(scope, mods, state, lib, this_ptr, statements, level)
            }

            // If-else statement
            Stmt::IfThenElse(expr, x, _) => {
                let (if_block, else_block) = x.as_ref();
                self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?
                    .as_bool()
                    .map_err(|err| self.make_type_mismatch_err::<bool>(err, expr.position()))
                    .and_then(|guard_val| {
                        if guard_val {
                            self.eval_stmt(scope, mods, state, lib, this_ptr, if_block, level)
                        } else if let Some(stmt) = else_block {
                            self.eval_stmt(scope, mods, state, lib, this_ptr, stmt, level)
                        } else {
                            Ok(Default::default())
                        }
                    })
            }

            // While loop
            Stmt::While(expr, body, _) => loop {
                match self
                    .eval_expr(scope, mods, state, lib, this_ptr, expr, level)?
                    .as_bool()
                {
                    Ok(true) => {
                        match self.eval_stmt(scope, mods, state, lib, this_ptr, body, level) {
                            Ok(_) => (),
                            Err(err) => match *err {
                                EvalAltResult::LoopBreak(false, _) => (),
                                EvalAltResult::LoopBreak(true, _) => return Ok(Default::default()),
                                _ => return Err(err),
                            },
                        }
                    }
                    Ok(false) => return Ok(Default::default()),
                    Err(err) => {
                        return Err(self.make_type_mismatch_err::<bool>(err, expr.position()))
                    }
                }
            },

            // Loop statement
            Stmt::Loop(block, _) => loop {
                match self.eval_stmt(scope, mods, state, lib, this_ptr, block, level) {
                    Ok(_) => (),
                    Err(err) => match *err {
                        EvalAltResult::LoopBreak(false, _) => (),
                        EvalAltResult::LoopBreak(true, _) => return Ok(Default::default()),
                        _ => return Err(err),
                    },
                }
            },

            // For loop
            Stmt::For(expr, x, _) => {
                let (name, stmt) = x.as_ref();
                let iter_obj = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;
                let iter_type = iter_obj.type_id();

                let func = self
                    .global_module
                    .get_iter(iter_type)
                    .or_else(|| self.packages.get_iter(iter_type));

                if let Some(func) = func {
                    // Add the loop variable
                    let var_name: Cow<'_, str> = if state.is_global() {
                        name.clone().into()
                    } else {
                        unsafe_cast_var_name_to_lifetime(name).into()
                    };
                    scope.push(var_name, ());
                    let index = scope.len() - 1;
                    state.scope_level += 1;

                    for iter_value in func(iter_obj) {
                        let (loop_var, _) = scope.get_mut(index);

                        let value = iter_value.flatten();
                        if cfg!(not(feature = "no_closure")) && loop_var.is_shared() {
                            *loop_var.write_lock().unwrap() = value;
                        } else {
                            *loop_var = value;
                        }

                        self.inc_operations(state)
                            .map_err(|err| err.fill_position(stmt.position()))?;

                        match self.eval_stmt(scope, mods, state, lib, this_ptr, stmt, level) {
                            Ok(_) => (),
                            Err(err) => match *err {
                                EvalAltResult::LoopBreak(false, _) => (),
                                EvalAltResult::LoopBreak(true, _) => break,
                                _ => return Err(err),
                            },
                        }
                    }

                    state.scope_level -= 1;
                    scope.rewind(scope.len() - 1);
                    Ok(Default::default())
                } else {
                    EvalAltResult::ErrorFor(expr.position()).into()
                }
            }

            // Continue statement
            Stmt::Continue(pos) => EvalAltResult::LoopBreak(false, *pos).into(),

            // Break statement
            Stmt::Break(pos) => EvalAltResult::LoopBreak(true, *pos).into(),

            // Try/Catch statement
            Stmt::TryCatch(x, _, _) => {
                let (try_body, var_def, catch_body) = x.as_ref();

                let result = self
                    .eval_stmt(scope, mods, state, lib, this_ptr, try_body, level)
                    .map(|_| ().into());

                match result {
                    Ok(_) => result,
                    Err(err) => match *err {
                        mut err @ EvalAltResult::ErrorRuntime(_, _) | mut err
                            if err.is_catchable() =>
                        {
                            let value = if let EvalAltResult::ErrorRuntime(ref x, _) = err {
                                x.clone()
                            } else {
                                err.set_position(NO_POS);
                                err.to_string().into()
                            };

                            let orig_scope_len = scope.len();
                            state.scope_level += 1;

                            if let Some(Ident { name, .. }) = var_def {
                                let var_name: Cow<'_, str> = if state.is_global() {
                                    name.clone().into()
                                } else {
                                    unsafe_cast_var_name_to_lifetime(name).into()
                                };
                                scope.push(var_name, value);
                            }

                            let mut result = self
                                .eval_stmt(scope, mods, state, lib, this_ptr, catch_body, level)
                                .map(|_| ().into());

                            if let Some(result_err) = result.as_ref().err() {
                                if let EvalAltResult::ErrorRuntime(Dynamic(Union::Unit(_)), pos) =
                                    result_err.as_ref()
                                {
                                    err.set_position(*pos);
                                    result = Err(Box::new(err));
                                }
                            }

                            state.scope_level -= 1;
                            scope.rewind(orig_scope_len);

                            result
                        }
                        _ => Err(err),
                    },
                }
            }

            // Return value
            Stmt::ReturnWithVal((ReturnType::Return, pos), Some(expr), _) => EvalAltResult::Return(
                self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?,
                *pos,
            )
            .into(),

            // Empty return
            Stmt::ReturnWithVal((ReturnType::Return, pos), None, _) => {
                EvalAltResult::Return(Default::default(), *pos).into()
            }

            // Throw value
            Stmt::ReturnWithVal((ReturnType::Exception, pos), Some(expr), _) => {
                let val = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;
                EvalAltResult::ErrorRuntime(val, *pos).into()
            }

            // Empty throw
            Stmt::ReturnWithVal((ReturnType::Exception, pos), None, _) => {
                EvalAltResult::ErrorRuntime(().into(), *pos).into()
            }

            // Let/const statement
            Stmt::Let(var_def, expr, _) | Stmt::Const(var_def, expr, _) => {
                let entry_type = match stmt {
                    Stmt::Let(_, _, _) => ScopeEntryType::Normal,
                    Stmt::Const(_, _, _) => ScopeEntryType::Constant,
                    _ => unreachable!(),
                };

                let val = if let Some(expr) = expr {
                    self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?
                        .flatten()
                } else {
                    ().into()
                };
                let var_name: Cow<'_, str> = if state.is_global() {
                    var_def.name.clone().into()
                } else {
                    unsafe_cast_var_name_to_lifetime(&var_def.name).into()
                };
                scope.push_dynamic_value(var_name, entry_type, val);
                Ok(Default::default())
            }

            // Import statement
            #[cfg(not(feature = "no_module"))]
            Stmt::Import(expr, alias, _pos) => {
                // Guard against too many modules
                #[cfg(not(feature = "unchecked"))]
                if state.modules >= self.max_modules() {
                    return EvalAltResult::ErrorTooManyModules(*_pos).into();
                }

                if let Some(path) = self
                    .eval_expr(scope, mods, state, lib, this_ptr, &expr, level)?
                    .try_cast::<ImmutableString>()
                {
                    if let Some(resolver) = &self.module_resolver {
                        let module = resolver.resolve(self, &path, expr.position())?;

                        if let Some(name_def) = alias {
                            if !module.is_indexed() {
                                // Index the module (making a clone copy if necessary) if it is not indexed
                                let mut module =
                                    shared_try_take(module).unwrap_or_else(|m| m.as_ref().clone());
                                module.build_index();
                                mods.push(name_def.name.clone(), module);
                            } else {
                                mods.push(name_def.name.clone(), module);
                            }
                        }

                        state.modules += 1;

                        Ok(Default::default())
                    } else {
                        Err(
                            EvalAltResult::ErrorModuleNotFound(path.to_string(), expr.position())
                                .into(),
                        )
                    }
                } else {
                    Err(self.make_type_mismatch_err::<ImmutableString>("", expr.position()))
                }
            }

            // Export statement
            #[cfg(not(feature = "no_module"))]
            Stmt::Export(list, _) => {
                for (Ident { name, pos: id_pos }, rename) in list.iter() {
                    // Mark scope variables as public
                    if let Some(index) = scope.get_index(name).map(|(i, _)| i) {
                        let alias = rename.as_ref().map(|x| &x.name).unwrap_or_else(|| name);
                        scope.set_entry_alias(index, alias.clone());
                    } else {
                        return EvalAltResult::ErrorVariableNotFound(name.into(), *id_pos).into();
                    }
                }
                Ok(Default::default())
            }

            // Share statement
            #[cfg(not(feature = "no_closure"))]
            Stmt::Share(x) => {
                match scope.get_index(&x.name) {
                    Some((index, ScopeEntryType::Normal)) => {
                        let (val, _) = scope.get_mut(index);

                        if !val.is_shared() {
                            // Replace the variable with a shared value.
                            *val = mem::take(val).into_shared();
                        }
                    }
                    _ => (),
                }
                Ok(Default::default())
            }
        };

        self.check_data_size(result)
            .map_err(|err| err.fill_position(stmt.position()))
    }

    /// Check a result to ensure that the data size is within allowable limit.
    /// Position in `EvalAltResult` may be None and should be set afterwards.
    #[cfg(feature = "unchecked")]
    #[inline(always)]
    fn check_data_size(
        &self,
        result: Result<Dynamic, Box<EvalAltResult>>,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        result
    }

    /// Check a result to ensure that the data size is within allowable limit.
    /// Position in `EvalAltResult` may be None and should be set afterwards.
    #[cfg(not(feature = "unchecked"))]
    fn check_data_size(
        &self,
        result: Result<Dynamic, Box<EvalAltResult>>,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        // If no data size limits, just return
        let mut total = 0;

        total += self.max_string_size();
        #[cfg(not(feature = "no_index"))]
        {
            total += self.max_array_size();
        }
        #[cfg(not(feature = "no_object"))]
        {
            total += self.max_map_size();
        }

        if total == 0 {
            return result;
        }

        // Recursively calculate the size of a value (especially `Array` and `Map`)
        fn calc_size(value: &Dynamic) -> (usize, usize, usize) {
            match value {
                #[cfg(not(feature = "no_index"))]
                Dynamic(Union::Array(arr)) => {
                    let mut arrays = 0;
                    let mut maps = 0;

                    arr.iter().for_each(|value| match value {
                        Dynamic(Union::Array(_)) => {
                            let (a, m, _) = calc_size(value);
                            arrays += a;
                            maps += m;
                        }
                        #[cfg(not(feature = "no_object"))]
                        Dynamic(Union::Map(_)) => {
                            let (a, m, _) = calc_size(value);
                            arrays += a;
                            maps += m;
                        }
                        _ => arrays += 1,
                    });

                    (arrays, maps, 0)
                }
                #[cfg(not(feature = "no_object"))]
                Dynamic(Union::Map(map)) => {
                    let mut arrays = 0;
                    let mut maps = 0;

                    map.values().for_each(|value| match value {
                        #[cfg(not(feature = "no_index"))]
                        Dynamic(Union::Array(_)) => {
                            let (a, m, _) = calc_size(value);
                            arrays += a;
                            maps += m;
                        }
                        Dynamic(Union::Map(_)) => {
                            let (a, m, _) = calc_size(value);
                            arrays += a;
                            maps += m;
                        }
                        _ => maps += 1,
                    });

                    (arrays, maps, 0)
                }
                Dynamic(Union::Str(s)) => (0, 0, s.len()),
                _ => (0, 0, 0),
            }
        }

        match result {
            // Simply return all errors
            Err(_) => return result,
            // String with limit
            Ok(Dynamic(Union::Str(_))) if self.max_string_size() > 0 => (),
            // Array with limit
            #[cfg(not(feature = "no_index"))]
            Ok(Dynamic(Union::Array(_))) if self.max_array_size() > 0 => (),
            // Map with limit
            #[cfg(not(feature = "no_object"))]
            Ok(Dynamic(Union::Map(_))) if self.max_map_size() > 0 => (),
            // Everything else is simply returned
            Ok(_) => return result,
        };

        let (_arr, _map, s) = calc_size(result.as_ref().unwrap());

        if s > self.max_string_size() {
            return EvalAltResult::ErrorDataTooLarge("Length of string".to_string(), NO_POS).into();
        }

        #[cfg(not(feature = "no_index"))]
        if _arr > self.max_array_size() {
            return EvalAltResult::ErrorDataTooLarge("Size of array".to_string(), NO_POS).into();
        }

        #[cfg(not(feature = "no_object"))]
        if _map > self.max_map_size() {
            return EvalAltResult::ErrorDataTooLarge("Size of object map".to_string(), NO_POS)
                .into();
        }

        result
    }

    /// Check if the number of operations stay within limit.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    pub(crate) fn inc_operations(&self, state: &mut State) -> Result<(), Box<EvalAltResult>> {
        state.operations += 1;

        #[cfg(not(feature = "unchecked"))]
        // Guard against too many operations
        if self.max_operations() > 0 && state.operations > self.max_operations() {
            return EvalAltResult::ErrorTooManyOperations(NO_POS).into();
        }

        // Report progress - only in steps
        if let Some(progress) = &self.progress {
            if let Some(token) = progress(&state.operations) {
                // Terminate script if progress returns a termination token
                return EvalAltResult::ErrorTerminated(token, NO_POS).into();
            }
        }

        Ok(())
    }

    /// Map a type_name into a pretty-print name
    #[inline(always)]
    pub(crate) fn map_type_name<'a>(&'a self, name: &'a str) -> &'a str {
        self.type_names
            .get(name)
            .map(String::as_str)
            .unwrap_or_else(|| map_std_type_name(name))
    }

    /// Make a Box<EvalAltResult<ErrorMismatchDataType>>.
    #[inline(always)]
    pub(crate) fn make_type_mismatch_err<T>(&self, typ: &str, pos: Position) -> Box<EvalAltResult> {
        EvalAltResult::ErrorMismatchDataType(
            typ.into(),
            self.map_type_name(type_name::<T>()).into(),
            pos,
        )
        .into()
    }
}
