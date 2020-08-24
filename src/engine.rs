//! Main module defining the script evaluation `Engine`.

use crate::any::{map_std_type_name, Dynamic, Union};
use crate::calc_fn_hash;
use crate::fn_call::run_builtin_op_assignment;
use crate::fn_native::{CallableFunction, Callback, FnPtr};
use crate::module::{Module, ModuleRef};
use crate::optimize::OptimizationLevel;
use crate::packages::{Package, PackagesCollection, StandardPackage};
use crate::parser::{Expr, ReturnType, Stmt};
use crate::r#unsafe::unsafe_cast_var_name_to_lifetime;
use crate::result::EvalAltResult;
use crate::scope::{EntryType as ScopeEntryType, Scope};
use crate::syntax::{CustomSyntax, EvalContext};
use crate::token::Position;
use crate::utils::StaticVec;

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
use crate::any::Variant;

#[cfg(not(feature = "no_function"))]
use crate::parser::ScriptFnDef;

#[cfg(not(feature = "no_module"))]
use crate::module::ModuleResolver;

#[cfg(not(feature = "no_std"))]
#[cfg(not(feature = "no_module"))]
use crate::module::resolvers;

#[cfg(any(not(feature = "no_object"), not(feature = "no_module")))]
use crate::utils::ImmutableString;

#[cfg(not(feature = "no_closure"))]
#[cfg(not(feature = "no_object"))]
use crate::any::DynamicWriteLock;

use crate::stdlib::{
    borrow::Cow,
    boxed::Box,
    collections::{HashMap, HashSet},
    fmt, format,
    iter::{empty, once},
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

/// [INTERNALS] A stack of imported modules.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
pub type Imports<'a> = Vec<(Cow<'a, str>, Module)>;

#[cfg(not(feature = "unchecked"))]
#[cfg(debug_assertions)]
pub const MAX_CALL_STACK_DEPTH: usize = 16;
#[cfg(not(feature = "unchecked"))]
#[cfg(debug_assertions)]
pub const MAX_EXPR_DEPTH: usize = 32;
#[cfg(not(feature = "unchecked"))]
#[cfg(debug_assertions)]
pub const MAX_FUNCTION_EXPR_DEPTH: usize = 16;

#[cfg(not(feature = "unchecked"))]
#[cfg(not(debug_assertions))]
pub const MAX_CALL_STACK_DEPTH: usize = 128;
#[cfg(not(feature = "unchecked"))]
#[cfg(not(debug_assertions))]
pub const MAX_EXPR_DEPTH: usize = 128;
#[cfg(not(feature = "unchecked"))]
#[cfg(not(debug_assertions))]
pub const MAX_FUNCTION_EXPR_DEPTH: usize = 32;

pub const KEYWORD_PRINT: &str = "print";
pub const KEYWORD_DEBUG: &str = "debug";
pub const KEYWORD_TYPE_OF: &str = "type_of";
pub const KEYWORD_EVAL: &str = "eval";
pub const KEYWORD_FN_PTR: &str = "Fn";
pub const KEYWORD_FN_PTR_CALL: &str = "call";
pub const KEYWORD_FN_PTR_CURRY: &str = "curry";
pub const KEYWORD_IS_SHARED: &str = "is_shared";
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

/// A type that encapsulates a mutation target for an expression with side effects.
#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
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

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
impl Target<'_> {
    /// Is the `Target` a reference pointing to other data?
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
    pub fn clone_into_dynamic(self) -> Dynamic {
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
    /// Update the value of the `Target`.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    pub fn set_value(&mut self, new_val: Dynamic) -> Result<(), Box<EvalAltResult>> {
        match self {
            Self::Ref(r) => **r = new_val,
            #[cfg(not(feature = "no_closure"))]
            #[cfg(not(feature = "no_object"))]
            Self::LockGuard((r, _)) => **r = new_val,
            Self::Value(_) => {
                return EvalAltResult::ErrorAssignmentToUnknownLHS(Position::none()).into();
            }
            #[cfg(not(feature = "no_index"))]
            Self::StringChar(string, index, _) if string.is::<ImmutableString>() => {
                let mut s = string.write_lock::<ImmutableString>().unwrap();

                // Replace the character at the specified index position
                let new_ch = new_val
                    .as_char()
                    .map_err(|_| EvalAltResult::ErrorCharMismatch(Position::none()))?;

                let mut chars = s.chars().collect::<StaticVec<_>>();
                let ch = chars[*index];

                // See if changed - if so, update the String
                if ch != new_ch {
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

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
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

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
impl<T: Into<Dynamic>> From<T> for Target<'_> {
    #[inline(always)]
    fn from(value: T) -> Self {
        Self::Value(value.into())
    }
}

/// [INTERNALS] A type that holds all the current states of the Engine.
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
    /// Create a new `State`.
    #[inline(always)]
    pub fn new() -> Self {
        Default::default()
    }
}

/// Get a script-defined function definition from a module.
#[cfg(not(feature = "no_function"))]
pub fn get_script_function_by_signature<'a>(
    module: &'a Module,
    name: &str,
    params: usize,
    pub_only: bool,
) -> Option<&'a ScriptFnDef> {
    // Qualifiers (none) + function name + number of arguments.
    let hash_script = calc_fn_hash(empty(), name, params, empty());
    let func = module.get_fn(hash_script, pub_only)?;
    if func.is_script() {
        Some(func.get_fn_def())
    } else {
        None
    }
}

/// [INTERNALS] A type containing all the limits imposed by the `Engine`.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[cfg(not(feature = "unchecked"))]
pub struct Limits {
    /// Maximum levels of call-stack to prevent infinite recursion.
    ///
    /// Defaults to 16 for debug builds and 128 for non-debug builds.
    pub max_call_stack_depth: usize,
    /// Maximum depth of statements/expressions at global level.
    pub max_expr_depth: usize,
    /// Maximum depth of statements/expressions in functions.
    pub max_function_expr_depth: usize,
    /// Maximum number of operations allowed to run.
    pub max_operations: u64,
    /// Maximum number of modules allowed to load.
    pub max_modules: usize,
    /// Maximum length of a string.
    pub max_string_size: usize,
    /// Maximum length of an array.
    pub max_array_size: usize,
    /// Maximum number of properties in a map.
    pub max_map_size: usize,
}

/// Rhai main scripting engine.
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
///
/// Currently, `Engine` is neither `Send` nor `Sync`. Use the `sync` feature to make it `Send + Sync`.
pub struct Engine {
    /// A unique ID identifying this scripting `Engine`.
    pub id: Option<String>,

    /// A module containing all functions directly loaded into the Engine.
    pub(crate) global_module: Module,
    /// A collection of all library packages loaded into the Engine.
    pub(crate) packages: PackagesCollection,

    /// A module resolution service.
    #[cfg(not(feature = "no_module"))]
    pub(crate) module_resolver: Option<Box<dyn ModuleResolver>>,

    /// A hashmap mapping type names to pretty-print names.
    pub(crate) type_names: Option<HashMap<String, String>>,

    /// A hashset containing symbols to disable.
    pub(crate) disabled_symbols: Option<HashSet<String>>,
    /// A hashset containing custom keywords and precedence to recognize.
    pub(crate) custom_keywords: Option<HashMap<String, u8>>,
    /// Custom syntax.
    pub(crate) custom_syntax: Option<HashMap<String, CustomSyntax>>,

    /// Callback closure for implementing the `print` command.
    pub(crate) print: Callback<str, ()>,
    /// Callback closure for implementing the `debug` command.
    pub(crate) debug: Callback<str, ()>,
    /// Callback closure for progress reporting.
    pub(crate) progress: Option<Callback<u64, bool>>,

    /// Optimize the AST after compilation.
    pub(crate) optimization_level: OptimizationLevel,

    /// Max limits.
    #[cfg(not(feature = "unchecked"))]
    pub(crate) limits: Limits,
}

impl fmt::Debug for Engine {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.id.as_ref() {
            Some(id) => write!(f, "Engine({})", id),
            None => f.write_str("Engine"),
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        // Create the new scripting Engine
        let mut engine = Self {
            id: None,

            packages: Default::default(),
            global_module: Default::default(),

            #[cfg(not(feature = "no_module"))]
            #[cfg(not(feature = "no_std"))]
            #[cfg(not(target_arch = "wasm32"))]
            module_resolver: Some(Box::new(resolvers::FileModuleResolver::new())),
            #[cfg(not(feature = "no_module"))]
            #[cfg(any(feature = "no_std", target_arch = "wasm32",))]
            module_resolver: None,

            type_names: None,
            disabled_symbols: None,
            custom_keywords: None,
            custom_syntax: None,

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
            limits: Limits {
                max_call_stack_depth: MAX_CALL_STACK_DEPTH,
                max_expr_depth: MAX_EXPR_DEPTH,
                max_function_expr_depth: MAX_FUNCTION_EXPR_DEPTH,
                max_operations: 0,
                max_modules: usize::MAX,
                max_string_size: 0,
                max_array_size: 0,
                max_map_size: 0,
            },
        };

        engine.load_package(StandardPackage::new().get());

        engine
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

/// Print/debug to stdout
fn default_print(_s: &str) {
    #[cfg(not(feature = "no_std"))]
    #[cfg(not(target_arch = "wasm32"))]
    println!("{}", _s);
}

/// Search for a module within an imports stack.
/// Position in `EvalAltResult` is `None` and must be set afterwards.
pub fn search_imports<'s>(
    mods: &'s Imports,
    state: &mut State,
    modules: &Box<ModuleRef>,
) -> Result<&'s Module, Box<EvalAltResult>> {
    let (root, root_pos) = &modules[0];

    // Qualified - check if the root module is directly indexed
    let index = if state.always_search {
        None
    } else {
        modules.index()
    };

    Ok(if let Some(index) = index {
        let offset = mods.len() - index.get();
        &mods.get(offset).unwrap().1
    } else {
        mods.iter()
            .rev()
            .find(|(n, _)| n == root)
            .map(|(_, m)| m)
            .ok_or_else(|| EvalAltResult::ErrorModuleNotFound(root.to_string(), *root_pos))?
    })
}

/// Search for a module within an imports stack.
/// Position in `EvalAltResult` is `None` and must be set afterwards.
pub fn search_imports_mut<'s>(
    mods: &'s mut Imports,
    state: &mut State,
    modules: &Box<ModuleRef>,
) -> Result<&'s mut Module, Box<EvalAltResult>> {
    let (root, root_pos) = &modules[0];

    // Qualified - check if the root module is directly indexed
    let index = if state.always_search {
        None
    } else {
        modules.index()
    };

    Ok(if let Some(index) = index {
        let offset = mods.len() - index.get();
        &mut mods.get_mut(offset).unwrap().1
    } else {
        mods.iter_mut()
            .rev()
            .find(|(n, _)| n == root)
            .map(|(_, m)| m)
            .ok_or_else(|| EvalAltResult::ErrorModuleNotFound(root.to_string(), *root_pos))?
    })
}

/// Search for a variable within the scope or within imports,
/// depending on whether the variable name is qualified.
pub fn search_namespace<'s, 'a>(
    scope: &'s mut Scope,
    mods: &'s mut Imports,
    state: &mut State,
    this_ptr: &'s mut Option<&mut Dynamic>,
    expr: &'a Expr,
) -> Result<(&'s mut Dynamic, &'a str, ScopeEntryType, Position), Box<EvalAltResult>> {
    match expr {
        Expr::Variable(v) => match v.as_ref() {
            // Qualified variable
            ((name, pos), Some(modules), hash_var, _) => {
                let module = search_imports_mut(mods, state, modules)?;
                let target = module
                    .get_qualified_var_mut(*hash_var)
                    .map_err(|err| match *err {
                        EvalAltResult::ErrorVariableNotFound(_, _) => {
                            EvalAltResult::ErrorVariableNotFound(
                                format!("{}{}", modules, name),
                                *pos,
                            )
                            .into()
                        }
                        _ => err.new_position(*pos),
                    })?;

                // Module variables are constant
                Ok((target, name, ScopeEntryType::Constant, *pos))
            }
            // Normal variable access
            _ => search_scope_only(scope, state, this_ptr, expr),
        },
        _ => unreachable!(),
    }
}

/// Search for a variable within the scope
pub fn search_scope_only<'s, 'a>(
    scope: &'s mut Scope,
    state: &mut State,
    this_ptr: &'s mut Option<&mut Dynamic>,
    expr: &'a Expr,
) -> Result<(&'s mut Dynamic, &'a str, ScopeEntryType, Position), Box<EvalAltResult>> {
    let ((name, pos), _, _, index) = match expr {
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
    let index = if state.always_search { None } else { *index };

    let index = if let Some(index) = index {
        scope.len() - index.get()
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

    Ok((val, name, typ, *pos))
}

impl Engine {
    /// Create a new `Engine`
    pub fn new() -> Self {
        Default::default()
    }

    /// Create a new `Engine` with minimal built-in functions.
    /// Use the `load_package` method to load additional packages of functions.
    pub fn new_raw() -> Self {
        Self {
            id: None,

            packages: Default::default(),
            global_module: Default::default(),

            #[cfg(not(feature = "no_module"))]
            module_resolver: None,

            type_names: None,
            disabled_symbols: None,
            custom_keywords: None,
            custom_syntax: None,

            print: Box::new(|_| {}),
            debug: Box::new(|_| {}),
            progress: None,

            optimization_level: if cfg!(feature = "no_optimize") {
                OptimizationLevel::None
            } else {
                OptimizationLevel::Simple
            },

            #[cfg(not(feature = "unchecked"))]
            limits: Limits {
                max_call_stack_depth: MAX_CALL_STACK_DEPTH,
                max_expr_depth: MAX_EXPR_DEPTH,
                max_function_expr_depth: MAX_FUNCTION_EXPR_DEPTH,
                max_operations: 0,
                max_modules: usize::MAX,
                max_string_size: 0,
                max_array_size: 0,
                max_map_size: 0,
            },
        }
    }

    /// Chain-evaluate a dot/index chain.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    fn eval_dot_index_chain_helper(
        &self,
        state: &mut State,
        lib: &Module,
        this_ptr: &mut Option<&mut Dynamic>,
        target: &mut Target,
        rhs: &Expr,
        idx_values: &mut StaticVec<Dynamic>,
        chain_type: ChainType,
        level: usize,
        new_val: Option<Dynamic>,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        if chain_type == ChainType::None {
            panic!();
        }

        let is_ref = target.is_ref();

        let next_chain = match rhs {
            Expr::Index(_) => ChainType::Index,
            Expr::Dot(_) => ChainType::Dot,
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
                    Expr::Dot(x) | Expr::Index(x) => {
                        let (idx, expr, pos) = x.as_ref();
                        let idx_pos = idx.position();
                        let obj_ptr = &mut self.get_indexed_mut(
                            state, lib, target, idx_val, idx_pos, false, true, level,
                        )?;

                        self.eval_dot_index_chain_helper(
                            state, lib, this_ptr, obj_ptr, expr, idx_values, next_chain, level,
                            new_val,
                        )
                        .map_err(|err| err.new_position(*pos))
                    }
                    // xxx[rhs] = new_val
                    _ if new_val.is_some() => {
                        let mut idx_val2 = idx_val.clone();

                        // `call_setter` is introduced to bypass double mutable borrowing of target
                        let _call_setter = match self
                            .get_indexed_mut(state, lib, target, idx_val, pos, true, false, level)
                        {
                            // Indexed value is a reference - update directly
                            Ok(ref mut obj_ptr) => {
                                obj_ptr
                                    .set_value(new_val.unwrap())
                                    .map_err(|err| err.new_position(rhs.position()))?;

                                None
                            }
                            Err(err) => match *err {
                                // No index getter - try to call an index setter
                                #[cfg(not(feature = "no_index"))]
                                EvalAltResult::ErrorIndexingType(_, _) => {
                                    // Raise error if there is no index getter nor setter
                                    Some(new_val.unwrap())
                                }
                                // Any other error - return
                                err => return Err(Box::new(err)),
                            },
                        };

                        #[cfg(not(feature = "no_index"))]
                        if let Some(mut new_val) = _call_setter {
                            let val = target.as_mut();
                            let val_type_name = val.type_name();
                            let args = &mut [val, &mut idx_val2, &mut new_val];

                            self.exec_fn_call(
                                state, lib, FN_IDX_SET, 0, args, is_ref, true, false, None, None,
                                level,
                            )
                            .map_err(|err| match *err {
                                EvalAltResult::ErrorFunctionNotFound(_, _) => {
                                    EvalAltResult::ErrorIndexingType(
                                        self.map_type_name(val_type_name).into(),
                                        Position::none(),
                                    )
                                }
                                err => err,
                            })?;
                        }

                        Ok(Default::default())
                    }
                    // xxx[rhs]
                    _ => self
                        .get_indexed_mut(state, lib, target, idx_val, pos, false, true, level)
                        .map(|v| (v.clone_into_dynamic(), false)),
                }
            }

            #[cfg(not(feature = "no_object"))]
            ChainType::Dot => {
                match rhs {
                    // xxx.fn_name(arg_expr_list)
                    Expr::FnCall(x) if x.1.is_none() => {
                        let ((name, native, _, pos), _, hash, _, def_val) = x.as_ref();
                        self.make_method_call(
                            state, lib, name, *hash, target, idx_val, *def_val, *native, false,
                            level,
                        )
                        .map_err(|err| err.new_position(*pos))
                    }
                    // xxx.module::fn_name(...) - syntax error
                    Expr::FnCall(_) => unreachable!(),
                    // {xxx:map}.id = ???
                    Expr::Property(x) if target.is::<Map>() && new_val.is_some() => {
                        let ((prop, _, _), pos) = x.as_ref();
                        let index = prop.clone().into();
                        let mut val = self
                            .get_indexed_mut(state, lib, target, index, *pos, true, false, level)?;

                        val.set_value(new_val.unwrap())
                            .map_err(|err| err.new_position(rhs.position()))?;
                        Ok((Default::default(), true))
                    }
                    // {xxx:map}.id
                    Expr::Property(x) if target.is::<Map>() => {
                        let ((prop, _, _), pos) = x.as_ref();
                        let index = prop.clone().into();
                        let val = self.get_indexed_mut(
                            state, lib, target, index, *pos, false, false, level,
                        )?;

                        Ok((val.clone_into_dynamic(), false))
                    }
                    // xxx.id = ???
                    Expr::Property(x) if new_val.is_some() => {
                        let ((_, _, setter), pos) = x.as_ref();
                        let mut new_val = new_val;
                        let mut args = [target.as_mut(), new_val.as_mut().unwrap()];
                        self.exec_fn_call(
                            state, lib, setter, 0, &mut args, is_ref, true, false, None, None,
                            level,
                        )
                        .map(|(v, _)| (v, true))
                        .map_err(|err| err.new_position(*pos))
                    }
                    // xxx.id
                    Expr::Property(x) => {
                        let ((_, getter, _), pos) = x.as_ref();
                        let mut args = [target.as_mut()];
                        self.exec_fn_call(
                            state, lib, getter, 0, &mut args, is_ref, true, false, None, None,
                            level,
                        )
                        .map(|(v, _)| (v, false))
                        .map_err(|err| err.new_position(*pos))
                    }
                    // {xxx:map}.sub_lhs[expr] | {xxx:map}.sub_lhs.expr
                    Expr::Index(x) | Expr::Dot(x) if target.is::<Map>() => {
                        let (sub_lhs, expr, pos) = x.as_ref();

                        let mut val = match sub_lhs {
                            Expr::Property(p) => {
                                let ((prop, _, _), pos) = p.as_ref();
                                let index = prop.clone().into();
                                self.get_indexed_mut(
                                    state, lib, target, index, *pos, false, true, level,
                                )?
                            }
                            // {xxx:map}.fn_name(arg_expr_list)[expr] | {xxx:map}.fn_name(arg_expr_list).expr
                            Expr::FnCall(x) if x.1.is_none() => {
                                let ((name, native, _, pos), _, hash, _, def_val) = x.as_ref();
                                let (val, _) = self
                                    .make_method_call(
                                        state, lib, name, *hash, target, idx_val, *def_val,
                                        *native, false, level,
                                    )
                                    .map_err(|err| err.new_position(*pos))?;
                                val.into()
                            }
                            // {xxx:map}.module::fn_name(...) - syntax error
                            Expr::FnCall(_) => unreachable!(),
                            // Others - syntax error
                            _ => unreachable!(),
                        };

                        self.eval_dot_index_chain_helper(
                            state, lib, this_ptr, &mut val, expr, idx_values, next_chain, level,
                            new_val,
                        )
                        .map_err(|err| err.new_position(*pos))
                    }
                    // xxx.sub_lhs[expr] | xxx.sub_lhs.expr
                    Expr::Index(x) | Expr::Dot(x) => {
                        let (sub_lhs, expr, _) = x.as_ref();

                        match sub_lhs {
                            // xxx.prop[expr] | xxx.prop.expr
                            Expr::Property(p) => {
                                let ((_, getter, setter), pos) = p.as_ref();
                                let arg_values = &mut [target.as_mut(), &mut Default::default()];
                                let args = &mut arg_values[..1];
                                let (mut val, updated) = self
                                    .exec_fn_call(
                                        state, lib, getter, 0, args, is_ref, true, false, None,
                                        None, level,
                                    )
                                    .map_err(|err| err.new_position(*pos))?;

                                let val = &mut val;

                                let (result, may_be_changed) = self
                                    .eval_dot_index_chain_helper(
                                        state,
                                        lib,
                                        this_ptr,
                                        &mut val.into(),
                                        expr,
                                        idx_values,
                                        next_chain,
                                        level,
                                        new_val,
                                    )
                                    .map_err(|err| err.new_position(*pos))?;

                                // Feed the value back via a setter just in case it has been updated
                                if updated || may_be_changed {
                                    // Re-use args because the first &mut parameter will not be consumed
                                    arg_values[1] = val;
                                    self.exec_fn_call(
                                        state, lib, setter, 0, arg_values, is_ref, true, false,
                                        None, None, level,
                                    )
                                    .or_else(
                                        |err| match *err {
                                            // If there is no setter, no need to feed it back because the property is read-only
                                            EvalAltResult::ErrorDotExpr(_, _) => {
                                                Ok(Default::default())
                                            }
                                            _ => Err(err.new_position(*pos)),
                                        },
                                    )?;
                                }

                                Ok((result, may_be_changed))
                            }
                            // xxx.fn_name(arg_expr_list)[expr] | xxx.fn_name(arg_expr_list).expr
                            Expr::FnCall(x) if x.1.is_none() => {
                                let ((name, native, _, pos), _, hash, _, def_val) = x.as_ref();
                                let (mut val, _) = self
                                    .make_method_call(
                                        state, lib, name, *hash, target, idx_val, *def_val,
                                        *native, false, level,
                                    )
                                    .map_err(|err| err.new_position(*pos))?;
                                let val = &mut val;
                                let target = &mut val.into();

                                self.eval_dot_index_chain_helper(
                                    state, lib, this_ptr, target, expr, idx_values, next_chain,
                                    level, new_val,
                                )
                                .map_err(|err| err.new_position(*pos))
                            }
                            // xxx.module::fn_name(...) - syntax error
                            Expr::FnCall(_) => unreachable!(),
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
        lib: &Module,
        this_ptr: &mut Option<&mut Dynamic>,
        expr: &Expr,
        level: usize,
        new_val: Option<Dynamic>,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let ((dot_lhs, dot_rhs, op_pos), chain_type) = match expr {
            Expr::Index(x) => (x.as_ref(), ChainType::Index),
            Expr::Dot(x) => (x.as_ref(), ChainType::Dot),
            _ => unreachable!(),
        };

        let idx_values = &mut StaticVec::new();

        self.eval_indexed_chain(
            scope, mods, state, lib, this_ptr, dot_rhs, chain_type, idx_values, 0, level,
        )?;

        match dot_lhs {
            // id.??? or id[???]
            Expr::Variable(x) => {
                let (var_name, var_pos) = &x.0;

                self.inc_operations(state)
                    .map_err(|err| err.new_position(*var_pos))?;

                let (target, _, typ, pos) =
                    search_namespace(scope, mods, state, this_ptr, dot_lhs)?;

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
                    state, lib, &mut None, obj_ptr, dot_rhs, idx_values, chain_type, level, new_val,
                )
                .map(|(v, _)| v)
                .map_err(|err| err.new_position(*op_pos))
            }
            // {expr}.??? = ??? or {expr}[???] = ???
            expr if new_val.is_some() => {
                return EvalAltResult::ErrorAssignmentToUnknownLHS(expr.position()).into();
            }
            // {expr}.??? or {expr}[???]
            expr => {
                let val = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;
                let obj_ptr = &mut val.into();
                self.eval_dot_index_chain_helper(
                    state, lib, this_ptr, obj_ptr, dot_rhs, idx_values, chain_type, level, new_val,
                )
                .map(|(v, _)| v)
                .map_err(|err| err.new_position(*op_pos))
            }
        }
    }

    /// Evaluate a chain of indexes and store the results in a list.
    /// The first few results are stored in the array `list` which is of fixed length.
    /// Any spill-overs are stored in `more`, which is dynamic.
    /// The fixed length array is used to avoid an allocation in the overwhelming cases of just a few levels of indexing.
    /// The total number of values is returned.
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    fn eval_indexed_chain(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &Module,
        this_ptr: &mut Option<&mut Dynamic>,
        expr: &Expr,
        chain_type: ChainType,
        idx_values: &mut StaticVec<Dynamic>,
        size: usize,
        level: usize,
    ) -> Result<(), Box<EvalAltResult>> {
        self.inc_operations(state)
            .map_err(|err| err.new_position(expr.position()))?;

        match expr {
            Expr::FnCall(x) if x.1.is_none() => {
                let arg_values =
                    x.3.iter()
                        .map(|arg_expr| {
                            self.eval_expr(scope, mods, state, lib, this_ptr, arg_expr, level)
                        })
                        .collect::<Result<StaticVec<Dynamic>, _>>()?;

                idx_values.push(Dynamic::from(arg_values));
            }
            Expr::FnCall(_) => unreachable!(),
            Expr::Property(_) => idx_values.push(().into()), // Store a placeholder - no need to copy the property name
            Expr::Index(x) | Expr::Dot(x) => {
                let (lhs, rhs, _) = x.as_ref();

                // Evaluate in left-to-right order
                let lhs_val = match lhs {
                    Expr::Property(_) => Default::default(), // Store a placeholder in case of a property
                    Expr::FnCall(x) if chain_type == ChainType::Dot && x.1.is_none() => {
                        let arg_values = x
                            .3
                            .iter()
                            .map(|arg_expr| {
                                self.eval_expr(scope, mods, state, lib, this_ptr, arg_expr, level)
                            })
                            .collect::<Result<StaticVec<Dynamic>, _>>()?;

                        Dynamic::from(arg_values)
                    }
                    Expr::FnCall(_) => unreachable!(),
                    _ => self.eval_expr(scope, mods, state, lib, this_ptr, lhs, level)?,
                };

                // Push in reverse order
                let chain_type = match expr {
                    Expr::Index(_) => ChainType::Index,
                    Expr::Dot(_) => ChainType::Dot,
                    _ => unreachable!(),
                };
                self.eval_indexed_chain(
                    scope, mods, state, lib, this_ptr, rhs, chain_type, idx_values, size, level,
                )?;

                idx_values.push(lhs_val);
            }
            _ => idx_values.push(self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?),
        }

        Ok(())
    }

    /// Get the value at the indexed position of a base type
    /// Position in `EvalAltResult` may be None and should be set afterwards.
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    fn get_indexed_mut<'a>(
        &self,
        state: &mut State,
        _lib: &Module,
        target: &'a mut Target,
        idx: Dynamic,
        idx_pos: Position,
        _create: bool,
        _indexers: bool,
        _level: usize,
    ) -> Result<Target<'a>, Box<EvalAltResult>> {
        self.inc_operations(state)?;

        #[cfg(not(feature = "no_index"))]
        #[cfg(not(feature = "no_object"))]
        let is_ref = target.is_ref();

        let val = target.as_mut();

        match val {
            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Array(arr)) => {
                // val_array[idx]
                let index = idx
                    .as_int()
                    .map_err(|_| EvalAltResult::ErrorNumericIndexExpr(idx_pos))?;

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
                    let index = idx
                        .take_immutable_string()
                        .map_err(|_| EvalAltResult::ErrorStringIndexExpr(idx_pos))?;

                    map.entry(index).or_insert(Default::default()).into()
                } else {
                    let index = idx
                        .read_lock::<ImmutableString>()
                        .ok_or_else(|| EvalAltResult::ErrorStringIndexExpr(idx_pos))?;

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
                    .map_err(|_| EvalAltResult::ErrorNumericIndexExpr(idx_pos))?;

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

            #[cfg(not(feature = "no_object"))]
            #[cfg(not(feature = "no_index"))]
            _ if _indexers => {
                let type_name = val.type_name();
                let mut idx = idx;
                let args = &mut [val, &mut idx];
                self.exec_fn_call(
                    state, _lib, FN_IDX_GET, 0, args, is_ref, true, false, None, None, _level,
                )
                .map(|(v, _)| v.into())
                .map_err(|err| match *err {
                    EvalAltResult::ErrorFunctionNotFound(_, _) => Box::new(
                        EvalAltResult::ErrorIndexingType(type_name.into(), Position::none()),
                    ),
                    _ => err,
                })
            }

            _ => EvalAltResult::ErrorIndexingType(
                self.map_type_name(val.type_name()).into(),
                Position::none(),
            )
            .into(),
        }
    }

    // Evaluate an 'in' expression
    fn eval_in_expr(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        state: &mut State,
        lib: &Module,
        this_ptr: &mut Option<&mut Dynamic>,
        lhs: &Expr,
        rhs: &Expr,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.inc_operations(state)
            .map_err(|err| err.new_position(rhs.position()))?;

        let lhs_value = self.eval_expr(scope, mods, state, lib, this_ptr, lhs, level)?;
        let rhs_value = self.eval_expr(scope, mods, state, lib, this_ptr, rhs, level)?;

        match rhs_value {
            #[cfg(not(feature = "no_index"))]
            Dynamic(Union::Array(mut rhs_value)) => {
                let op = "==";

                // Call the `==` operator to compare each value
                for value in rhs_value.iter_mut() {
                    let def_value = Some(false);
                    let args = &mut [&mut lhs_value.clone(), value];

                    // Qualifiers (none) + function name + number of arguments + argument `TypeId`'s.
                    let hash =
                        calc_fn_hash(empty(), op, args.len(), args.iter().map(|a| a.type_id()));

                    if self
                        .call_native_fn(state, lib, op, hash, args, false, false, def_value)
                        .map_err(|err| err.new_position(rhs.position()))?
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
                // Only allows String or char
                Dynamic(Union::Str(s)) => Ok(rhs_value.contains_key(&s).into()),
                Dynamic(Union::Char(c)) => Ok(rhs_value.contains_key(&c.to_string()).into()),
                _ => EvalAltResult::ErrorInExpr(lhs.position()).into(),
            },
            Dynamic(Union::Str(rhs_value)) => match lhs_value {
                // Only allows String or char
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
        lib: &Module,
        this_ptr: &mut Option<&mut Dynamic>,
        expr: &Expr,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.inc_operations(state)
            .map_err(|err| err.new_position(expr.position()))?;

        let result = match expr {
            Expr::Expr(x) => self.eval_expr(scope, mods, state, lib, this_ptr, x.as_ref(), level),

            Expr::IntegerConstant(x) => Ok(x.0.into()),
            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(x) => Ok(x.0.into()),
            Expr::DecimalConstant(x) => Ok(x.0.into()),
            Expr::StringConstant(x) => Ok(x.0.to_string().into()),
            Expr::CharConstant(x) => Ok(x.0.into()),
            Expr::FnPointer(x) => Ok(FnPtr::new_unchecked(x.0.clone(), Default::default()).into()),
            Expr::Variable(x) if (x.0).0 == KEYWORD_THIS => {
                if let Some(val) = this_ptr {
                    Ok(val.clone())
                } else {
                    EvalAltResult::ErrorUnboundThis((x.0).1).into()
                }
            }
            Expr::Variable(_) => {
                let (val, _, _, _) = search_namespace(scope, mods, state, this_ptr, expr)?;
                Ok(val.clone())
            }
            Expr::Property(_) => unreachable!(),

            // Statement block
            Expr::Stmt(x) => self.eval_stmt(scope, mods, state, lib, this_ptr, &x.0, level),

            // var op= rhs
            Expr::Assignment(x) if matches!(x.0, Expr::Variable(_)) => {
                let (lhs_expr, op, rhs_expr, op_pos) = x.as_ref();
                let mut rhs_val =
                    self.eval_expr(scope, mods, state, lib, this_ptr, rhs_expr, level)?;
                let (lhs_ptr, name, typ, pos) =
                    search_namespace(scope, mods, state, this_ptr, lhs_expr)?;
                self.inc_operations(state)
                    .map_err(|err| err.new_position(pos))?;

                match typ {
                    // Assignment to constant variable
                    ScopeEntryType::Constant => Err(Box::new(
                        EvalAltResult::ErrorAssignmentToConstant(name.to_string(), pos),
                    )),
                    // Normal assignment
                    ScopeEntryType::Normal if op.is_empty() => {
                        let value = rhs_val.flatten();
                        if cfg!(not(feature = "no_closure")) && lhs_ptr.is_shared() {
                            *lhs_ptr.write_lock::<Dynamic>().unwrap() = value;
                        } else {
                            *lhs_ptr = value;
                        }
                        Ok(Default::default())
                    }
                    // Op-assignment - in order of precedence:
                    ScopeEntryType::Normal => {
                        // 1) Native registered overriding function
                        // 2) Built-in implementation
                        // 3) Map to `var = var op rhs`

                        // Qualifiers (none) + function name + number of arguments + argument `TypeId`'s.
                        let arg_types = once(lhs_ptr.type_id()).chain(once(rhs_val.type_id()));
                        let hash_fn = calc_fn_hash(empty(), op, 2, arg_types);

                        if let Some(CallableFunction::Method(func)) = self
                            .global_module
                            .get_fn(hash_fn, false)
                            .or_else(|| self.packages.get_fn(hash_fn, false))
                        {
                            if cfg!(not(feature = "no_closure")) && lhs_ptr.is_shared() {
                                let mut lock_guard = lhs_ptr.write_lock::<Dynamic>().unwrap();
                                let lhs_ptr_inner = lock_guard.deref_mut();

                                // Overriding exact implementation
                                func(self, lib, &mut [lhs_ptr_inner, &mut rhs_val])?;
                            } else {
                                // Overriding exact implementation
                                func(self, lib, &mut [lhs_ptr, &mut rhs_val])?;
                            }
                        } else if run_builtin_op_assignment(op, lhs_ptr, &rhs_val)?.is_none() {
                            // Not built in, map to `var = var op rhs`
                            let op = &op[..op.len() - 1]; // extract operator without =

                            // Clone the LHS value
                            let args = &mut [&mut lhs_ptr.clone(), &mut rhs_val];

                            // Run function
                            let (value, _) = self
                                .exec_fn_call(
                                    state, lib, op, 0, args, false, false, false, None, None, level,
                                )
                                .map_err(|err| err.new_position(*op_pos))?;

                            let value = value.flatten();
                            if cfg!(not(feature = "no_closure")) && lhs_ptr.is_shared() {
                                *lhs_ptr.write_lock::<Dynamic>().unwrap() = value;
                            } else {
                                *lhs_ptr = value;
                            }
                        }
                        Ok(Default::default())
                    }
                }
            }

            // lhs op= rhs
            Expr::Assignment(x) => {
                let (lhs_expr, op, rhs_expr, op_pos) = x.as_ref();
                let mut rhs_val =
                    self.eval_expr(scope, mods, state, lib, this_ptr, rhs_expr, level)?;

                let _new_val = Some(if op.is_empty() {
                    // Normal assignment
                    rhs_val
                } else {
                    // Op-assignment - always map to `lhs = lhs op rhs`
                    let op = &op[..op.len() - 1]; // extract operator without =
                    let args = &mut [
                        &mut self.eval_expr(scope, mods, state, lib, this_ptr, lhs_expr, level)?,
                        &mut rhs_val,
                    ];
                    self.exec_fn_call(
                        state, lib, op, 0, args, false, false, false, None, None, level,
                    )
                    .map(|(v, _)| v)
                    .map_err(|err| err.new_position(*op_pos))?
                });

                match lhs_expr {
                    // name op= rhs
                    Expr::Variable(_) => unreachable!(),
                    // idx_lhs[idx_expr] op= rhs
                    #[cfg(not(feature = "no_index"))]
                    Expr::Index(_) => {
                        self.eval_dot_index_chain(
                            scope, mods, state, lib, this_ptr, lhs_expr, level, _new_val,
                        )?;
                        Ok(Default::default())
                    }
                    // dot_lhs.dot_rhs op= rhs
                    #[cfg(not(feature = "no_object"))]
                    Expr::Dot(_) => {
                        self.eval_dot_index_chain(
                            scope, mods, state, lib, this_ptr, lhs_expr, level, _new_val,
                        )?;
                        Ok(Default::default())
                    }
                    // Error assignment to constant
                    expr if expr.is_constant() => EvalAltResult::ErrorAssignmentToConstant(
                        expr.get_constant_str(),
                        expr.position(),
                    )
                    .into(),
                    // Syntax error
                    expr => EvalAltResult::ErrorAssignmentToUnknownLHS(expr.position()).into(),
                }
            }

            // lhs[idx_expr]
            #[cfg(not(feature = "no_index"))]
            Expr::Index(_) => {
                self.eval_dot_index_chain(scope, mods, state, lib, this_ptr, expr, level, None)
            }

            // lhs.dot_rhs
            #[cfg(not(feature = "no_object"))]
            Expr::Dot(_) => {
                self.eval_dot_index_chain(scope, mods, state, lib, this_ptr, expr, level, None)
            }

            #[cfg(not(feature = "no_index"))]
            Expr::Array(x) => Ok(Dynamic(Union::Array(Box::new(
                x.0.iter()
                    .map(|item| self.eval_expr(scope, mods, state, lib, this_ptr, item, level))
                    .collect::<Result<Vec<_>, _>>()?,
            )))),

            #[cfg(not(feature = "no_object"))]
            Expr::Map(x) => Ok(Dynamic(Union::Map(Box::new(
                x.0.iter()
                    .map(|((key, _), expr)| {
                        self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)
                            .map(|val| (key.clone(), val))
                    })
                    .collect::<Result<HashMap<_, _>, _>>()?,
            )))),

            // Normal function call
            Expr::FnCall(x) if x.1.is_none() => {
                let ((name, native, capture, pos), _, hash, args_expr, def_val) = x.as_ref();
                self.make_function_call(
                    scope, mods, state, lib, this_ptr, name, args_expr, *def_val, *hash, *native,
                    false, *capture, level,
                )
                .map_err(|err| err.new_position(*pos))
            }

            // Module-qualified function call
            Expr::FnCall(x) if x.1.is_some() => {
                let ((name, _, capture, pos), modules, hash, args_expr, def_val) = x.as_ref();
                self.make_qualified_function_call(
                    scope, mods, state, lib, this_ptr, modules, name, args_expr, *def_val, *hash,
                    *capture, level,
                )
                .map_err(|err| err.new_position(*pos))
            }

            Expr::In(x) => self.eval_in_expr(scope, mods, state, lib, this_ptr, &x.0, &x.1, level),

            Expr::And(x) => {
                let (lhs, rhs, _) = x.as_ref();
                Ok((self
                    .eval_expr(scope, mods, state, lib, this_ptr, lhs, level)?
                    .as_bool()
                    .map_err(|_| {
                        EvalAltResult::ErrorBooleanArgMismatch("AND".into(), lhs.position())
                    })?
                    && // Short-circuit using &&
                self
                    .eval_expr(scope, mods, state, lib, this_ptr, rhs, level)?
                    .as_bool()
                    .map_err(|_| {
                        EvalAltResult::ErrorBooleanArgMismatch("AND".into(), rhs.position())
                    })?)
                .into())
            }

            Expr::Or(x) => {
                let (lhs, rhs, _) = x.as_ref();
                Ok((self
                    .eval_expr(scope, mods, state, lib, this_ptr, lhs, level)?
                    .as_bool()
                    .map_err(|_| {
                        EvalAltResult::ErrorBooleanArgMismatch("OR".into(), lhs.position())
                    })?
                    || // Short-circuit using ||
                self
                    .eval_expr(scope, mods, state, lib, this_ptr, rhs, level)?
                    .as_bool()
                    .map_err(|_| {
                        EvalAltResult::ErrorBooleanArgMismatch("OR".into(), rhs.position())
                    })?)
                .into())
            }

            Expr::True(_) => Ok(true.into()),
            Expr::False(_) => Ok(false.into()),
            Expr::Unit(_) => Ok(().into()),

            Expr::Custom(x) => {
                let func = (x.0).1.as_ref();
                let ep = (x.0).0.iter().map(|e| e.into()).collect::<StaticVec<_>>();
                let mut context = EvalContext {
                    mods,
                    state,
                    lib,
                    this_ptr,
                    level,
                };
                func(self, &mut context, scope, ep.as_ref())
            }

            _ => unreachable!(),
        };

        self.check_data_size(result)
            .map_err(|err| err.new_position(expr.position()))
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
        lib: &Module,
        this_ptr: &mut Option<&mut Dynamic>,
        stmt: &Stmt,
        level: usize,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        self.inc_operations(state)
            .map_err(|err| err.new_position(stmt.position()))?;

        let result = match stmt {
            // No-op
            Stmt::Noop(_) => Ok(Default::default()),

            // Expression as statement
            Stmt::Expr(expr) => self.eval_expr(scope, mods, state, lib, this_ptr, expr, level),

            // Block scope
            Stmt::Block(x) => {
                let prev_scope_len = scope.len();
                let prev_mods_len = mods.len();
                state.scope_level += 1;

                let result = x.0.iter().try_fold(Default::default(), |_, stmt| {
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

            // If-else statement
            Stmt::IfThenElse(x) => {
                let (expr, if_block, else_block, _) = x.as_ref();

                self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?
                    .as_bool()
                    .map_err(|_| EvalAltResult::ErrorLogicGuard(expr.position()).into())
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
            Stmt::While(x) => loop {
                let (expr, body, _) = x.as_ref();

                match self
                    .eval_expr(scope, mods, state, lib, this_ptr, expr, level)?
                    .as_bool()
                {
                    Ok(true) => {
                        match self.eval_stmt(scope, mods, state, lib, this_ptr, body, level) {
                            Ok(_) => (),
                            Err(err) => match *err {
                                EvalAltResult::ErrorLoopBreak(false, _) => (),
                                EvalAltResult::ErrorLoopBreak(true, _) => {
                                    return Ok(Default::default())
                                }
                                _ => return Err(err),
                            },
                        }
                    }
                    Ok(false) => return Ok(Default::default()),
                    Err(_) => return EvalAltResult::ErrorLogicGuard(expr.position()).into(),
                }
            },

            // Loop statement
            Stmt::Loop(x) => loop {
                match self.eval_stmt(scope, mods, state, lib, this_ptr, &x.0, level) {
                    Ok(_) => (),
                    Err(err) => match *err {
                        EvalAltResult::ErrorLoopBreak(false, _) => (),
                        EvalAltResult::ErrorLoopBreak(true, _) => return Ok(Default::default()),
                        _ => return Err(err),
                    },
                }
            },

            // For loop
            Stmt::For(x) => {
                let (name, expr, stmt, _) = x.as_ref();
                let iter_type = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;
                let tid = iter_type.type_id();

                if let Some(func) = self
                    .global_module
                    .get_iter(tid)
                    .or_else(|| self.packages.get_iter(tid))
                {
                    // Add the loop variable
                    let var_name = unsafe_cast_var_name_to_lifetime(name, &state);
                    scope.push(var_name, ());
                    let index = scope.len() - 1;
                    state.scope_level += 1;

                    for iter_value in func(iter_type) {
                        let (loop_var, _) = scope.get_mut(index);

                        let value = iter_value.flatten();
                        if cfg!(not(feature = "no_closure")) && loop_var.is_shared() {
                            *loop_var.write_lock().unwrap() = value;
                        } else {
                            *loop_var = value;
                        }

                        self.inc_operations(state)
                            .map_err(|err| err.new_position(stmt.position()))?;

                        match self.eval_stmt(scope, mods, state, lib, this_ptr, stmt, level) {
                            Ok(_) => (),
                            Err(err) => match *err {
                                EvalAltResult::ErrorLoopBreak(false, _) => (),
                                EvalAltResult::ErrorLoopBreak(true, _) => break,
                                _ => return Err(err),
                            },
                        }
                    }

                    scope.rewind(scope.len() - 1);
                    state.scope_level -= 1;
                    Ok(Default::default())
                } else {
                    EvalAltResult::ErrorFor(x.1.position()).into()
                }
            }

            // Continue statement
            Stmt::Continue(pos) => EvalAltResult::ErrorLoopBreak(false, *pos).into(),

            // Break statement
            Stmt::Break(pos) => EvalAltResult::ErrorLoopBreak(true, *pos).into(),

            // Return value
            Stmt::ReturnWithVal(x) if x.1.is_some() && (x.0).0 == ReturnType::Return => {
                let expr = x.1.as_ref().unwrap();
                EvalAltResult::Return(
                    self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?,
                    (x.0).1,
                )
                .into()
            }

            // Empty return
            Stmt::ReturnWithVal(x) if (x.0).0 == ReturnType::Return => {
                EvalAltResult::Return(Default::default(), (x.0).1).into()
            }

            // Throw value
            Stmt::ReturnWithVal(x) if x.1.is_some() && (x.0).0 == ReturnType::Exception => {
                let expr = x.1.as_ref().unwrap();
                let val = self.eval_expr(scope, mods, state, lib, this_ptr, expr, level)?;
                EvalAltResult::ErrorRuntime(
                    val.take_string().unwrap_or_else(|_| "".into()),
                    (x.0).1,
                )
                .into()
            }

            // Empty throw
            Stmt::ReturnWithVal(x) if (x.0).0 == ReturnType::Exception => {
                EvalAltResult::ErrorRuntime("".into(), (x.0).1).into()
            }

            Stmt::ReturnWithVal(_) => unreachable!(),

            // Let statement
            Stmt::Let(x) if x.1.is_some() => {
                let ((var_name, _), expr, _) = x.as_ref();
                let expr = expr.as_ref().unwrap();
                let val = self
                    .eval_expr(scope, mods, state, lib, this_ptr, expr, level)?
                    .flatten();
                let var_name = unsafe_cast_var_name_to_lifetime(var_name, &state);
                scope.push_dynamic_value(var_name, ScopeEntryType::Normal, val, false);
                Ok(Default::default())
            }

            Stmt::Let(x) => {
                let ((var_name, _), _, _) = x.as_ref();
                let var_name = unsafe_cast_var_name_to_lifetime(var_name, &state);
                scope.push(var_name, ());
                Ok(Default::default())
            }

            // Const statement
            Stmt::Const(x) if x.1.is_constant() => {
                let ((var_name, _), expr, _) = x.as_ref();
                let val = self
                    .eval_expr(scope, mods, state, lib, this_ptr, &expr, level)?
                    .flatten();
                let var_name = unsafe_cast_var_name_to_lifetime(var_name, &state);
                scope.push_dynamic_value(var_name, ScopeEntryType::Constant, val, true);
                Ok(Default::default())
            }

            // Const expression not constant
            Stmt::Const(_) => unreachable!(),

            // Import statement
            #[cfg(not(feature = "no_module"))]
            Stmt::Import(x) => {
                let (expr, alias, _pos) = x.as_ref();

                // Guard against too many modules
                #[cfg(not(feature = "unchecked"))]
                if state.modules >= self.limits.max_modules {
                    return EvalAltResult::ErrorTooManyModules(*_pos).into();
                }

                if let Some(path) = self
                    .eval_expr(scope, mods, state, lib, this_ptr, &expr, level)?
                    .try_cast::<ImmutableString>()
                {
                    if let Some(resolver) = &self.module_resolver {
                        let mut module = resolver.resolve(self, &path, expr.position())?;

                        if let Some((name, _)) = alias {
                            module.index_all_sub_modules();
                            mods.push((name.clone().into(), module));
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
                    EvalAltResult::ErrorImportExpr(expr.position()).into()
                }
            }

            // Export statement
            #[cfg(not(feature = "no_module"))]
            Stmt::Export(x) => {
                for ((id, id_pos), rename) in x.0.iter() {
                    // Mark scope variables as public
                    if let Some(index) = scope.get_index(id).map(|(i, _)| i) {
                        let alias = rename.as_ref().map(|(n, _)| n).unwrap_or_else(|| id);
                        scope.set_entry_alias(index, alias.clone());
                    } else {
                        return EvalAltResult::ErrorVariableNotFound(id.into(), *id_pos).into();
                    }
                }
                Ok(Default::default())
            }

            // Share statement
            #[cfg(not(feature = "no_closure"))]
            Stmt::Share(x) => {
                let (var_name, _) = x.as_ref();

                match scope.get_index(var_name) {
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
            .map_err(|err| err.new_position(stmt.position()))
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
        if self.limits.max_string_size + self.limits.max_array_size + self.limits.max_map_size == 0
        {
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
            Ok(Dynamic(Union::Str(_))) if self.limits.max_string_size > 0 => (),
            // Array with limit
            #[cfg(not(feature = "no_index"))]
            Ok(Dynamic(Union::Array(_))) if self.limits.max_array_size > 0 => (),
            // Map with limit
            #[cfg(not(feature = "no_object"))]
            Ok(Dynamic(Union::Map(_))) if self.limits.max_map_size > 0 => (),
            // Everything else is simply returned
            Ok(_) => return result,
        };

        let (arr, map, s) = calc_size(result.as_ref().unwrap());

        if s > self.limits.max_string_size {
            EvalAltResult::ErrorDataTooLarge(
                "Length of string".to_string(),
                self.limits.max_string_size,
                s,
                Position::none(),
            )
            .into()
        } else if arr > self.limits.max_array_size {
            EvalAltResult::ErrorDataTooLarge(
                "Size of array".to_string(),
                self.limits.max_array_size,
                arr,
                Position::none(),
            )
            .into()
        } else if map > self.limits.max_map_size {
            EvalAltResult::ErrorDataTooLarge(
                "Number of properties in object map".to_string(),
                self.limits.max_map_size,
                map,
                Position::none(),
            )
            .into()
        } else {
            result
        }
    }

    /// Check if the number of operations stay within limit.
    /// Position in `EvalAltResult` is `None` and must be set afterwards.
    pub(crate) fn inc_operations(&self, state: &mut State) -> Result<(), Box<EvalAltResult>> {
        state.operations += 1;

        #[cfg(not(feature = "unchecked"))]
        // Guard against too many operations
        if self.limits.max_operations > 0 && state.operations > self.limits.max_operations {
            return EvalAltResult::ErrorTooManyOperations(Position::none()).into();
        }

        // Report progress - only in steps
        if let Some(progress) = &self.progress {
            if !progress(&state.operations) {
                // Terminate script if progress returns false
                return EvalAltResult::ErrorTerminated(Position::none()).into();
            }
        }

        Ok(())
    }

    /// Map a type_name into a pretty-print name
    #[inline(always)]
    pub(crate) fn map_type_name<'a>(&'a self, name: &'a str) -> &'a str {
        self.type_names
            .as_ref()
            .and_then(|t| t.get(name).map(String::as_str))
            .unwrap_or(map_std_type_name(name))
    }
}
