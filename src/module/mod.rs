//! Module defining external-loaded modules for Rhai.

use crate::ast::{FnAccess, Ident};
use crate::dynamic::Variant;
use crate::fn_call::FnCallArgs;
use crate::fn_native::{shared_take_or_clone, CallableFunction, IteratorFn, SendSync};
use crate::fn_register::RegisterNativeFunction;
use crate::parse::IdentifierBuilder;
use crate::token::Token;
use crate::{
    calc_fn_params_hash, calc_qualified_fn_hash, combine_hashes, Dynamic, EvalAltResult,
    Identifier, ImmutableString, NativeCallContext, Position, Shared, StaticVec,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::TypeId,
    collections::{BTreeMap, BTreeSet},
    fmt,
    iter::{empty, once},
    num::NonZeroUsize,
    ops::{Add, AddAssign, Deref, DerefMut},
};

#[cfg(not(feature = "no_index"))]
use crate::Array;

#[cfg(not(feature = "no_object"))]
use crate::Map;

/// A type representing the namespace of a function.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum FnNamespace {
    /// Expose to global namespace.
    Global,
    /// Module namespace only.
    Internal,
}

impl Default for FnNamespace {
    #[inline(always)]
    fn default() -> Self {
        Self::Internal
    }
}

/// Data structure containing a single registered function.
#[derive(Debug, Clone)]
pub struct FuncInfo {
    /// Function instance.
    pub func: Shared<CallableFunction>,
    /// Function namespace.
    pub namespace: FnNamespace,
    /// Function access mode.
    pub access: FnAccess,
    /// Function name.
    pub name: Identifier,
    /// Number of parameters.
    pub params: usize,
    /// Parameter types (if applicable).
    pub param_types: StaticVec<TypeId>,
    /// Parameter names (if available).
    #[cfg(feature = "metadata")]
    pub param_names: StaticVec<Identifier>,
}

impl FuncInfo {
    /// Generate a signature of the function.
    /// Exported under the `metadata` feature only.
    #[cfg(feature = "metadata")]
    #[must_use]
    pub fn gen_signature(&self) -> String {
        let mut sig = format!("{}(", self.name);

        if !self.param_names.is_empty() {
            let mut params: StaticVec<String> =
                self.param_names.iter().map(|s| s.as_str().into()).collect();
            let return_type = params.pop().unwrap_or_else(|| "()".into());
            sig.push_str(&params.join(", "));
            if return_type != "()" {
                sig.push_str(") -> ");
                sig.push_str(&return_type);
            } else {
                sig.push(')');
            }
        } else {
            for x in 0..self.params {
                sig.push('_');
                if x < self.params - 1 {
                    sig.push_str(", ");
                }
            }

            if self.func.is_script() {
                sig.push(')');
            } else {
                sig.push_str(") -> ?");
            }
        }

        sig
    }
}

/// _(internals)_ Calculate a [`u64`] hash key from a namespace-qualified function name and
/// parameter types.
/// Exported under the `internals` feature only.
///
/// Module names are passed in via `&str` references from an iterator.
/// Parameter types are passed in via [`TypeId`] values from an iterator.
///
/// # Note
///
/// The first module name is skipped.  Hashing starts from the _second_ module in the chain.
#[inline]
fn calc_native_fn_hash<'a>(
    modules: impl Iterator<Item = &'a str>,
    fn_name: &str,
    params: &[TypeId],
) -> u64 {
    let hash_script = calc_qualified_fn_hash(modules, fn_name, params.len());
    let hash_params = calc_fn_params_hash(params.iter().cloned());
    combine_hashes(hash_script, hash_params)
}

/// A module which may contain variables, sub-modules, external Rust functions,
/// and/or script-defined functions.
#[derive(Clone)]
pub struct Module {
    /// ID identifying the module.
    id: Option<Identifier>,
    /// Is this module internal?
    pub(crate) internal: bool,
    /// Is this module part of a standard library?
    pub(crate) standard: bool,
    /// Sub-modules.
    modules: BTreeMap<Identifier, Shared<Module>>,
    /// [`Module`] variables.
    variables: BTreeMap<Identifier, Dynamic>,
    /// Flattened collection of all [`Module`] variables, including those in sub-modules.
    all_variables: BTreeMap<u64, Dynamic>,
    /// External Rust functions.
    functions: BTreeMap<u64, Box<FuncInfo>>,
    /// Flattened collection of all external Rust functions, native or scripted.
    /// including those in sub-modules.
    all_functions: BTreeMap<u64, Shared<CallableFunction>>,
    /// Iterator functions, keyed by the type producing the iterator.
    type_iterators: BTreeMap<TypeId, IteratorFn>,
    /// Flattened collection of iterator functions, including those in sub-modules.
    all_type_iterators: BTreeMap<TypeId, IteratorFn>,
    /// Is the [`Module`] indexed?
    indexed: bool,
    /// Does the [`Module`] contain indexed functions that have been exposed to the global namespace?
    contains_indexed_global_functions: bool,
    /// Interned strings
    identifiers: IdentifierBuilder,
}

impl Default for Module {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Module");

        self.id.as_ref().map(|id| d.field("id", id));

        if !self.modules.is_empty() {
            d.field(
                "modules",
                &self
                    .modules
                    .keys()
                    .map(|m| m.as_str())
                    .collect::<BTreeSet<_>>(),
            );
        }
        if !self.variables.is_empty() {
            d.field("vars", &self.variables);
        }
        if !self.functions.is_empty() {
            d.field(
                "functions",
                &self
                    .functions
                    .values()
                    .map(|f| f.func.to_string())
                    .collect::<BTreeSet<_>>(),
            );
        }
        d.finish()
    }
}

impl AsRef<Module> for Module {
    #[inline(always)]
    fn as_ref(&self) -> &Module {
        self
    }
}

impl<M: AsRef<Module>> Add<M> for &Module {
    type Output = Module;

    #[inline(always)]
    fn add(self, rhs: M) -> Self::Output {
        let mut module = self.clone();
        module.merge(rhs.as_ref());
        module
    }
}

impl<M: AsRef<Module>> Add<M> for Module {
    type Output = Self;

    #[inline(always)]
    fn add(mut self, rhs: M) -> Self::Output {
        self.merge(rhs.as_ref());
        self
    }
}

impl<M: Into<Module>> AddAssign<M> for Module {
    #[inline(always)]
    fn add_assign(&mut self, rhs: M) {
        self.combine(rhs.into());
    }
}

impl Module {
    /// Create a new [`Module`].
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// module.set_var("answer", 42_i64);
    /// assert_eq!(module.get_var_value::<i64>("answer").expect("answer should exist"), 42);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            internal: false,
            standard: false,
            modules: Default::default(),
            variables: Default::default(),
            all_variables: Default::default(),
            functions: Default::default(),
            all_functions: Default::default(),
            type_iterators: Default::default(),
            all_type_iterators: Default::default(),
            indexed: true,
            contains_indexed_global_functions: false,
            identifiers: Default::default(),
        }
    }

    /// Get the ID of the [`Module`], if any.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// module.set_id("hello");
    /// assert_eq!(module.id(), Some("hello"));
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.id_raw().map(|s| s.as_str())
    }

    /// Get the ID of the [`Module`] as an [`Identifier`], if any.
    #[inline(always)]
    #[must_use]
    pub(crate) const fn id_raw(&self) -> Option<&Identifier> {
        self.id.as_ref()
    }

    /// Set the ID of the [`Module`].
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// module.set_id("hello");
    /// assert_eq!(module.id(), Some("hello"));
    /// ```
    #[inline(always)]
    pub fn set_id(&mut self, id: impl Into<Identifier>) -> &mut Self {
        self.id = Some(id.into());
        self
    }
    /// Clear the ID of the [`Module`].
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// module.set_id("hello");
    /// assert_eq!(module.id(), Some("hello"));
    /// module.clear_id();
    /// assert_eq!(module.id(), None);
    /// ```
    #[inline(always)]
    pub fn clear_id(&mut self) -> &mut Self {
        self.id = None;
        self
    }

    /// Is the [`Module`] empty?
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let module = Module::new();
    /// assert!(module.is_empty());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
            && self.all_functions.is_empty()
            && self.variables.is_empty()
            && self.all_variables.is_empty()
            && self.modules.is_empty()
            && self.type_iterators.is_empty()
            && self.all_type_iterators.is_empty()
    }

    /// Is the [`Module`] indexed?
    ///
    /// A module must be indexed before it can be used in an `import` statement.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// assert!(module.is_indexed());
    ///
    /// module.set_native_fn("foo", |x: &mut i64, y: i64| { *x = y; Ok(()) });
    /// assert!(!module.is_indexed());
    ///
    /// # #[cfg(not(feature = "no_module"))]
    /// # {
    /// module.build_index();
    /// assert!(module.is_indexed());
    /// # }
    /// ```
    #[inline(always)]
    #[must_use]
    pub const fn is_indexed(&self) -> bool {
        self.indexed
    }

    /// Generate signatures for all the non-private functions in the [`Module`].
    /// Exported under the `metadata` feature only.
    #[cfg(feature = "metadata")]
    #[inline(always)]
    pub fn gen_fn_signatures(&self) -> impl Iterator<Item = String> + '_ {
        self.functions
            .values()
            .filter(|f| match f.access {
                FnAccess::Public => true,
                FnAccess::Private => false,
            })
            .map(|f| f.gen_signature())
    }

    /// Does a variable exist in the [`Module`]?
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// module.set_var("answer", 42_i64);
    /// assert!(module.contains_var("answer"));
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn contains_var(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    /// Get the value of a [`Module`] variable.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// module.set_var("answer", 42_i64);
    /// assert_eq!(module.get_var_value::<i64>("answer").expect("answer should exist"), 42);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn get_var_value<T: Variant + Clone>(&self, name: &str) -> Option<T> {
        self.get_var(name).and_then(Dynamic::try_cast::<T>)
    }

    /// Get a [`Module`] variable as a [`Dynamic`].
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// module.set_var("answer", 42_i64);
    /// assert_eq!(module.get_var("answer").expect("answer should exist").cast::<i64>(), 42);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn get_var(&self, name: &str) -> Option<Dynamic> {
        self.variables.get(name).cloned()
    }

    /// Set a variable into the [`Module`].
    ///
    /// If there is an existing variable of the same name, it is replaced.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// module.set_var("answer", 42_i64);
    /// assert_eq!(module.get_var_value::<i64>("answer").expect("answer should exist"), 42);
    /// ```
    #[inline]
    pub fn set_var(
        &mut self,
        name: impl Into<Identifier>,
        value: impl Variant + Clone,
    ) -> &mut Self {
        let ident = name.into();
        let value = Dynamic::from(value);

        if self.indexed {
            let hash_var = crate::calc_qualified_var_hash(once(""), &ident);
            self.all_variables.insert(hash_var, value.clone());
        }
        self.variables.insert(ident, value);
        self
    }

    /// Get a reference to a namespace-qualified variable.
    /// Name and Position in [`EvalAltResult`] are [`None`] and [`NONE`][Position::NONE] and must be set afterwards.
    #[inline]
    pub(crate) fn get_qualified_var(&self, hash_var: u64) -> Result<&Dynamic, Box<EvalAltResult>> {
        self.all_variables.get(&hash_var).ok_or_else(|| {
            EvalAltResult::ErrorVariableNotFound(String::new(), Position::NONE).into()
        })
    }

    /// Set a script-defined function into the [`Module`].
    ///
    /// If there is an existing function of the same name and number of arguments, it is replaced.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub fn set_script_fn(&mut self, fn_def: impl Into<Shared<crate::ast::ScriptFnDef>>) -> u64 {
        let fn_def = fn_def.into();

        // None + function name + number of arguments.
        let num_params = fn_def.params.len();
        let hash_script = crate::calc_fn_hash(&fn_def.name, num_params);
        let mut param_names = fn_def.params.clone();
        param_names.push("Dynamic".into());
        self.functions.insert(
            hash_script,
            FuncInfo {
                name: fn_def.name.clone(),
                namespace: FnNamespace::Internal,
                access: fn_def.access,
                params: num_params,
                param_types: Default::default(),
                #[cfg(feature = "metadata")]
                param_names,
                func: Into::<CallableFunction>::into(fn_def).into(),
            }
            .into(),
        );
        self.indexed = false;
        self.contains_indexed_global_functions = false;
        hash_script
    }

    /// Get a shared reference to the script-defined function in the [`Module`] based on name
    /// and number of parameters.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    #[must_use]
    pub fn get_script_fn(
        &self,
        name: &str,
        num_params: usize,
    ) -> Option<&Shared<crate::ast::ScriptFnDef>> {
        self.functions
            .values()
            .find(|f| f.params == num_params && f.name == name)
            .and_then(|f| f.func.get_script_fn_def())
    }

    /// Get a mutable reference to the underlying [`BTreeMap`] of sub-modules.
    ///
    /// # WARNING
    ///
    /// By taking a mutable reference, it is assumed that some sub-modules will be modified.
    /// Thus the [`Module`] is automatically set to be non-indexed.
    #[cfg(not(feature = "no_module"))]
    #[inline]
    #[must_use]
    pub(crate) fn sub_modules_mut(&mut self) -> &mut BTreeMap<Identifier, Shared<Module>> {
        // We must assume that the user has changed the sub-modules
        // (otherwise why take a mutable reference?)
        self.all_functions.clear();
        self.all_variables.clear();
        self.all_type_iterators.clear();
        self.indexed = false;
        self.contains_indexed_global_functions = false;

        &mut self.modules
    }

    /// Does a sub-module exist in the [`Module`]?
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// let sub_module = Module::new();
    /// module.set_sub_module("question", sub_module);
    /// assert!(module.contains_sub_module("question"));
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn contains_sub_module(&self, name: &str) -> bool {
        self.modules.contains_key(name)
    }

    /// Get a sub-module in the [`Module`].
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// let sub_module = Module::new();
    /// module.set_sub_module("question", sub_module);
    /// assert!(module.get_sub_module("question").is_some());
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn get_sub_module(&self, name: &str) -> Option<&Module> {
        self.modules.get(name).map(|m| m.as_ref())
    }

    /// Set a sub-module into the [`Module`].
    ///
    /// If there is an existing sub-module of the same name, it is replaced.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// let sub_module = Module::new();
    /// module.set_sub_module("question", sub_module);
    /// assert!(module.get_sub_module("question").is_some());
    /// ```
    #[inline]
    pub fn set_sub_module(
        &mut self,
        name: impl Into<Identifier>,
        sub_module: impl Into<Shared<Module>>,
    ) -> &mut Self {
        self.modules.insert(name.into(), sub_module.into());
        self.indexed = false;
        self.contains_indexed_global_functions = false;
        self
    }

    /// Does the particular Rust function exist in the [`Module`]?
    ///
    /// The [`u64`] hash is returned by the [`set_native_fn`][Module::set_native_fn] call.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// let hash = module.set_native_fn("calc", || Ok(42_i64));
    /// assert!(module.contains_fn(hash));
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn contains_fn(&self, hash_fn: u64) -> bool {
        self.functions.contains_key(&hash_fn)
    }

    /// Update the metadata (parameter names/types and return type) of a registered function.
    /// Exported under the `metadata` feature only.
    ///
    /// The [`u64`] hash is returned by the [`set_native_fn`][Module::set_native_fn] call.
    ///
    /// ## Parameter Names and Types
    ///
    /// Each parameter name/type pair should be a single string of the format: `var_name: type`.
    ///
    /// ## Return Type
    ///
    /// The _last entry_ in the list should be the _return type_ of the function.
    /// In other words, the number of entries should be one larger than the number of parameters.
    #[cfg(feature = "metadata")]
    #[inline]
    pub fn update_fn_metadata(&mut self, hash_fn: u64, arg_names: &[&str]) -> &mut Self {
        let param_names = arg_names
            .iter()
            .map(|&name| self.identifiers.get(name))
            .collect();

        if let Some(f) = self.functions.get_mut(&hash_fn) {
            f.param_names = param_names;
        }

        self
    }

    /// Update the namespace of a registered function.
    ///
    /// The [`u64`] hash is returned by the [`set_native_fn`][Module::set_native_fn] call.
    #[inline]
    pub fn update_fn_namespace(&mut self, hash_fn: u64, namespace: FnNamespace) -> &mut Self {
        if let Some(f) = self.functions.get_mut(&hash_fn) {
            f.namespace = namespace;
            self.indexed = false;
            self.contains_indexed_global_functions = false;
        }
        self
    }

    /// Remap type ID.
    #[inline]
    #[must_use]
    fn map_type(map: bool, type_id: TypeId) -> TypeId {
        if !map {
            return type_id;
        }
        if type_id == TypeId::of::<&str>() {
            // Map &str to ImmutableString
            return TypeId::of::<ImmutableString>();
        }
        if type_id == TypeId::of::<String>() {
            // Map String to ImmutableString
            return TypeId::of::<ImmutableString>();
        }

        type_id
    }

    /// Set a Rust function into the [`Module`], returning a hash key.
    ///
    /// If there is an existing Rust function of the same hash, it is replaced.
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.
    #[inline]
    pub fn set_fn(
        &mut self,
        name: impl AsRef<str> + Into<Identifier>,
        namespace: FnNamespace,
        access: FnAccess,
        _arg_names: Option<&[&str]>,
        arg_types: &[TypeId],
        func: CallableFunction,
    ) -> u64 {
        let is_method = func.is_method();

        let mut param_types: StaticVec<_> = arg_types
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, type_id)| Self::map_type(!is_method || i > 0, type_id))
            .collect();
        param_types.shrink_to_fit();

        #[cfg(feature = "metadata")]
        let mut param_names: StaticVec<_> = _arg_names
            .iter()
            .flat_map(|p| p.iter())
            .map(|&arg| self.identifiers.get(arg))
            .collect();
        #[cfg(feature = "metadata")]
        param_names.shrink_to_fit();

        let hash_fn = calc_native_fn_hash(empty(), name.as_ref(), &param_types);

        self.functions.insert(
            hash_fn,
            FuncInfo {
                name: self.identifiers.get(name),
                namespace,
                access,
                params: param_types.len(),
                param_types,
                #[cfg(feature = "metadata")]
                param_names,
                func: func.into(),
            }
            .into(),
        );

        self.indexed = false;
        self.contains_indexed_global_functions = false;

        hash_fn
    }

    /// Set a Rust function taking a reference to the scripting [`Engine`][crate::Engine],
    /// the current set of functions, plus a list of mutable [`Dynamic`] references
    /// into the [`Module`], returning a hash key.
    ///
    /// Use this to register a built-in function which must reference settings on the scripting
    /// [`Engine`][crate::Engine] (e.g. to prevent growing an array beyond the allowed maximum size),
    /// or to call a script-defined function in the current evaluation context.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.
    ///
    /// # Arguments
    ///
    /// A list of [`TypeId`]'s is taken as the argument types.
    ///
    /// Arguments are simply passed in as a mutable array of [`&mut Dynamic`][Dynamic],
    /// which is guaranteed to contain enough arguments of the correct types.
    ///
    /// The function is assumed to be a _method_, meaning that the first argument should not be consumed.
    /// All other arguments can be consumed.
    ///
    /// To access a primary argument value (i.e. cloning is cheap), use: `args[n].as_xxx().unwrap()`
    ///
    /// To access an argument value and avoid cloning, use `std::mem::take(args[n]).cast::<T>()`.
    /// Notice that this will _consume_ the argument, replacing it with `()`.
    ///
    /// To access the first mutable argument, use `args.get_mut(0).unwrap()`
    ///
    /// # Function Metadata
    ///
    /// No metadata for the function is registered. Use `update_fn_metadata` to add metadata.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Module, FnNamespace, FnAccess};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_raw_fn("double_or_not", FnNamespace::Internal, FnAccess::Public,
    ///                 // Pass parameter types via a slice with TypeId's
    ///                 &[std::any::TypeId::of::<i64>(), std::any::TypeId::of::<bool>()],
    ///                 // Fixed closure signature
    ///                 |context, args| {
    ///                     // 'args' is guaranteed to be the right length and of the correct types
    ///
    ///                     // Get the second parameter by 'consuming' it
    ///                     let double = std::mem::take(args[1]).cast::<bool>();
    ///                     // Since it is a primary type, it can also be cheaply copied
    ///                     let double = args[1].clone_cast::<bool>();
    ///                     // Get a mutable reference to the first argument.
    ///                     let mut x = args[0].write_lock::<i64>().unwrap();
    ///
    ///                     let orig = *x;
    ///
    ///                     if double {
    ///                         *x *= 2;            // the first argument can be mutated
    ///                     }
    ///
    ///                     Ok(orig)                // return Result<T, Box<EvalAltResult>>
    ///                 });
    ///
    /// assert!(module.contains_fn(hash));
    /// ```
    #[inline(always)]
    pub fn set_raw_fn<N, T, F>(
        &mut self,
        name: N,
        namespace: FnNamespace,
        access: FnAccess,
        arg_types: &[TypeId],
        func: F,
    ) -> u64
    where
        N: AsRef<str> + Into<Identifier>,
        T: Variant + Clone,
        F: Fn(NativeCallContext, &mut FnCallArgs) -> Result<T, Box<EvalAltResult>>
            + SendSync
            + 'static,
    {
        let f =
            move |ctx: NativeCallContext, args: &mut FnCallArgs| func(ctx, args).map(Dynamic::from);

        self.set_fn(
            name,
            namespace,
            access,
            None,
            arg_types,
            CallableFunction::from_method(Box::new(f)),
        )
    }

    /// Set a Rust function into the [`Module`], returning a hash key.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Function Namespace
    ///
    /// The default function namespace is [`FnNamespace::Internal`].
    /// Use [`update_fn_namespace`][Module::update_fn_namespace] to change it.
    ///
    /// # Function Metadata
    ///
    /// No metadata for the function is registered.
    /// Use [`update_fn_metadata`][Module::update_fn_metadata] to add metadata.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// let hash = module.set_native_fn("calc", || Ok(42_i64));
    /// assert!(module.contains_fn(hash));
    /// ```
    #[inline(always)]
    pub fn set_native_fn<ARGS, N, T, F>(&mut self, name: N, func: F) -> u64
    where
        N: AsRef<str> + Into<Identifier>,
        T: Variant + Clone,
        F: RegisterNativeFunction<ARGS, Result<T, Box<EvalAltResult>>>,
    {
        self.set_fn(
            name,
            FnNamespace::Internal,
            FnAccess::Public,
            None,
            &F::param_types(),
            func.into_callable_function(),
        )
    }

    /// Set a Rust getter function taking one mutable parameter, returning a hash key.
    /// This function is automatically exposed to the global namespace.
    ///
    /// If there is a similar existing Rust getter function, it is replaced.
    ///
    /// # Function Metadata
    ///
    /// No metadata for the function is registered. Use `update_fn_metadata` to add metadata.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// let hash = module.set_getter_fn("value", |x: &mut i64| { Ok(*x) });
    /// assert!(module.contains_fn(hash));
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn set_getter_fn<ARGS, A, T, F>(&mut self, name: &str, func: F) -> u64
    where
        A: Variant + Clone,
        T: Variant + Clone,
        F: RegisterNativeFunction<ARGS, Result<T, Box<EvalAltResult>>>,
        F: Fn(&mut A) -> Result<T, Box<EvalAltResult>> + SendSync + 'static,
    {
        self.set_fn(
            &crate::engine::make_getter(name),
            FnNamespace::Global,
            FnAccess::Public,
            None,
            &F::param_types(),
            func.into_callable_function(),
        )
    }

    /// Set a Rust setter function taking two parameters (the first one mutable) into the [`Module`],
    /// returning a hash key.
    /// This function is automatically exposed to the global namespace.
    ///
    /// If there is a similar existing setter Rust function, it is replaced.
    ///
    /// # Function Metadata
    ///
    /// No metadata for the function is registered. Use `update_fn_metadata` to add metadata.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_setter_fn("value", |x: &mut i64, y: ImmutableString| {
    ///     *x = y.len() as i64;
    ///     Ok(())
    /// });
    /// assert!(module.contains_fn(hash));
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn set_setter_fn<ARGS, A, B, F>(&mut self, name: &str, func: F) -> u64
    where
        A: Variant + Clone,
        B: Variant + Clone,
        F: RegisterNativeFunction<ARGS, Result<(), Box<EvalAltResult>>>,
        F: Fn(&mut A, B) -> Result<(), Box<EvalAltResult>> + SendSync + 'static,
    {
        self.set_fn(
            &crate::engine::make_setter(name),
            FnNamespace::Global,
            FnAccess::Public,
            None,
            &F::param_types(),
            func.into_callable_function(),
        )
    }

    /// Set a Rust index getter taking two parameters (the first one mutable) into the [`Module`],
    /// returning a hash key.
    /// This function is automatically exposed to the global namespace.
    ///
    /// If there is a similar existing setter Rust function, it is replaced.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`] or [`Map`].
    /// Indexers for arrays, object maps and strings cannot be registered.
    ///
    /// # Function Metadata
    ///
    /// No metadata for the function is registered. Use `update_fn_metadata` to add metadata.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_indexer_get_fn(|x: &mut i64, y: ImmutableString| {
    ///     Ok(*x + y.len() as i64)
    /// });
    /// assert!(module.contains_fn(hash));
    /// ```
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline]
    pub fn set_indexer_get_fn<ARGS, A, B, T, F>(&mut self, func: F) -> u64
    where
        A: Variant + Clone,
        B: Variant + Clone,
        T: Variant + Clone,
        F: RegisterNativeFunction<ARGS, Result<T, Box<EvalAltResult>>>,
        F: Fn(&mut A, B) -> Result<T, Box<EvalAltResult>> + SendSync + 'static,
    {
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<A>() == TypeId::of::<Array>() {
            panic!("Cannot register indexer for arrays.");
        }
        #[cfg(not(feature = "no_object"))]
        if TypeId::of::<A>() == TypeId::of::<Map>() {
            panic!("Cannot register indexer for object maps.");
        }
        if TypeId::of::<A>() == TypeId::of::<String>()
            || TypeId::of::<A>() == TypeId::of::<&str>()
            || TypeId::of::<A>() == TypeId::of::<ImmutableString>()
        {
            panic!("Cannot register indexer for strings.");
        }

        self.set_fn(
            crate::engine::FN_IDX_GET,
            FnNamespace::Global,
            FnAccess::Public,
            None,
            &F::param_types(),
            func.into_callable_function(),
        )
    }

    /// Set a Rust index setter taking three parameters (the first one mutable) into the [`Module`],
    /// returning a hash key.
    /// This function is automatically exposed to the global namespace.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`] or [`Map`].
    /// Indexers for arrays, object maps and strings cannot be registered.
    ///
    /// # Function Metadata
    ///
    /// No metadata for the function is registered. Use `update_fn_metadata` to add metadata.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_indexer_set_fn(|x: &mut i64, y: ImmutableString, value: i64| {
    ///     *x = y.len() as i64 + value; Ok(())
    /// });
    /// assert!(module.contains_fn(hash));
    /// ```
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline]
    pub fn set_indexer_set_fn<ARGS, A, B, C, F>(&mut self, func: F) -> u64
    where
        A: Variant + Clone,
        B: Variant + Clone,
        C: Variant + Clone,
        F: RegisterNativeFunction<ARGS, Result<(), Box<EvalAltResult>>>,
        F: Fn(&mut A, B, C) -> Result<(), Box<EvalAltResult>> + SendSync + 'static,
    {
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<A>() == TypeId::of::<Array>() {
            panic!("Cannot register indexer for arrays.");
        }
        #[cfg(not(feature = "no_object"))]
        if TypeId::of::<A>() == TypeId::of::<Map>() {
            panic!("Cannot register indexer for object maps.");
        }
        if TypeId::of::<A>() == TypeId::of::<String>()
            || TypeId::of::<A>() == TypeId::of::<&str>()
            || TypeId::of::<A>() == TypeId::of::<ImmutableString>()
        {
            panic!("Cannot register indexer for strings.");
        }

        self.set_fn(
            crate::engine::FN_IDX_SET,
            FnNamespace::Global,
            FnAccess::Public,
            None,
            &F::param_types(),
            func.into_callable_function(),
        )
    }

    /// Set a pair of Rust index getter and setter functions, returning both hash keys.
    /// This is a short-hand for [`set_indexer_get_fn`][Module::set_indexer_get_fn] and
    /// [`set_indexer_set_fn`][Module::set_indexer_set_fn].
    ///
    /// If there are similar existing Rust functions, they are replaced.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`] or [`Map`].
    /// Indexers for arrays, object maps and strings cannot be registered.
    ///
    /// # Function Metadata
    ///
    /// No metadata for the function is registered. Use `update_fn_metadata` to add metadata.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let (hash_get, hash_set) = module.set_indexer_get_set_fn(
    ///     |x: &mut i64, y: ImmutableString| {
    ///         Ok(*x + y.len() as i64)
    ///     },
    ///     |x: &mut i64, y: ImmutableString, value: i64| {
    ///         *x = y.len() as i64 + value; Ok(())
    ///     }
    /// );
    /// assert!(module.contains_fn(hash_get));
    /// assert!(module.contains_fn(hash_set));
    /// ```
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline(always)]
    pub fn set_indexer_get_set_fn<A, B, T>(
        &mut self,
        get_fn: impl Fn(&mut A, B) -> Result<T, Box<EvalAltResult>> + SendSync + 'static,
        set_fn: impl Fn(&mut A, B, T) -> Result<(), Box<EvalAltResult>> + SendSync + 'static,
    ) -> (u64, u64)
    where
        A: Variant + Clone,
        B: Variant + Clone,
        T: Variant + Clone,
    {
        (
            self.set_indexer_get_fn(get_fn),
            self.set_indexer_set_fn(set_fn),
        )
    }

    /// Get a Rust function.
    ///
    /// The [`u64`] hash is returned by the [`set_native_fn`][Module::set_native_fn] call.
    #[inline(always)]
    #[must_use]
    pub(crate) fn get_fn(&self, hash_fn: u64) -> Option<&CallableFunction> {
        self.functions.get(&hash_fn).map(|f| f.func.as_ref())
    }

    /// Does the particular namespace-qualified function exist in the [`Module`]?
    ///
    /// The [`u64`] hash is calculated by [`build_index`][Module::build_index].
    #[inline(always)]
    #[must_use]
    pub fn contains_qualified_fn(&self, hash_fn: u64) -> bool {
        self.all_functions.contains_key(&hash_fn)
    }

    /// Get a namespace-qualified function.
    ///
    /// The [`u64`] hash is calculated by [`build_index`][Module::build_index].
    #[inline(always)]
    #[must_use]
    pub(crate) fn get_qualified_fn(&self, hash_qualified_fn: u64) -> Option<&CallableFunction> {
        self.all_functions
            .get(&hash_qualified_fn)
            .map(|f| f.as_ref())
    }

    /// Combine another [`Module`] into this [`Module`].
    /// The other [`Module`] is _consumed_ to merge into this [`Module`].
    #[inline]
    pub fn combine(&mut self, other: Self) -> &mut Self {
        self.modules.extend(other.modules.into_iter());
        self.variables.extend(other.variables.into_iter());
        self.functions.extend(other.functions.into_iter());
        self.type_iterators.extend(other.type_iterators.into_iter());
        self.all_functions.clear();
        self.all_variables.clear();
        self.all_type_iterators.clear();
        self.indexed = false;
        self.contains_indexed_global_functions = false;
        self
    }

    /// Combine another [`Module`] into this [`Module`].
    /// The other [`Module`] is _consumed_ to merge into this [`Module`].
    /// Sub-modules are flattened onto the root [`Module`], with higher level overriding lower level.
    #[inline]
    pub fn combine_flatten(&mut self, other: Self) -> &mut Self {
        other.modules.into_iter().for_each(|(_, m)| {
            self.combine_flatten(shared_take_or_clone(m));
        });
        self.variables.extend(other.variables.into_iter());
        self.functions.extend(other.functions.into_iter());
        self.type_iterators.extend(other.type_iterators.into_iter());
        self.all_functions.clear();
        self.all_variables.clear();
        self.all_type_iterators.clear();
        self.indexed = false;
        self.contains_indexed_global_functions = false;
        self
    }

    /// Polyfill this [`Module`] with another [`Module`].
    /// Only items not existing in this [`Module`] are added.
    #[inline]
    pub fn fill_with(&mut self, other: &Self) -> &mut Self {
        other.modules.iter().for_each(|(k, v)| {
            if !self.modules.contains_key(k) {
                self.modules.insert(k.clone(), v.clone());
            }
        });
        other.variables.iter().for_each(|(k, v)| {
            if !self.variables.contains_key(k) {
                self.variables.insert(k.clone(), v.clone());
            }
        });
        other.functions.iter().for_each(|(&k, v)| {
            self.functions.entry(k).or_insert_with(|| v.clone());
        });
        other.type_iterators.iter().for_each(|(&k, &v)| {
            self.type_iterators.entry(k).or_insert(v);
        });
        self.all_functions.clear();
        self.all_variables.clear();
        self.all_type_iterators.clear();
        self.indexed = false;
        self.contains_indexed_global_functions = false;
        self
    }

    /// Merge another [`Module`] into this [`Module`].
    #[inline(always)]
    pub fn merge(&mut self, other: &Self) -> &mut Self {
        self.merge_filtered(other, &|_, _, _, _, _| true)
    }

    /// Merge another [`Module`] into this [`Module`] based on a filter predicate.
    pub(crate) fn merge_filtered(
        &mut self,
        other: &Self,
        _filter: &impl Fn(FnNamespace, FnAccess, bool, &str, usize) -> bool,
    ) -> &mut Self {
        #[cfg(not(feature = "no_function"))]
        other.modules.iter().for_each(|(k, v)| {
            let mut m = Self::new();
            m.merge_filtered(v, _filter);
            self.set_sub_module(k.clone(), m);
        });
        #[cfg(feature = "no_function")]
        self.modules
            .extend(other.modules.iter().map(|(k, v)| (k.clone(), v.clone())));

        self.variables
            .extend(other.variables.iter().map(|(k, v)| (k.clone(), v.clone())));
        self.functions.extend(
            other
                .functions
                .iter()
                .filter(|(_, f)| {
                    _filter(
                        f.namespace,
                        f.access,
                        f.func.is_script(),
                        f.name.as_str(),
                        f.params,
                    )
                })
                .map(|(&k, v)| (k, v.clone())),
        );

        self.type_iterators.extend(other.type_iterators.iter());
        self.all_functions.clear();
        self.all_variables.clear();
        self.all_type_iterators.clear();
        self.indexed = false;
        self.contains_indexed_global_functions = false;
        self
    }

    /// Filter out the functions, retaining only some script-defined functions based on a filter predicate.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub(crate) fn retain_script_functions(
        &mut self,
        filter: impl Fn(FnNamespace, FnAccess, &str, usize) -> bool,
    ) -> &mut Self {
        self.functions = std::mem::take(&mut self.functions)
            .into_iter()
            .filter(|(_, f)| {
                if f.func.is_script() {
                    filter(f.namespace, f.access, f.name.as_str(), f.params)
                } else {
                    false
                }
            })
            .collect();

        self.all_functions.clear();
        self.all_variables.clear();
        self.all_type_iterators.clear();
        self.indexed = false;
        self.contains_indexed_global_functions = false;
        self
    }

    /// Get the number of variables, functions and type iterators in the [`Module`].
    #[inline(always)]
    #[must_use]
    pub fn count(&self) -> (usize, usize, usize) {
        (
            self.variables.len(),
            self.functions.len(),
            self.type_iterators.len(),
        )
    }

    /// Get an iterator to the sub-modules in the [`Module`].
    #[inline(always)]
    pub fn iter_sub_modules(&self) -> impl Iterator<Item = (&str, Shared<Module>)> {
        self.modules.iter().map(|(k, m)| (k.as_str(), m.clone()))
    }

    /// Get an iterator to the variables in the [`Module`].
    #[inline(always)]
    pub fn iter_var(&self) -> impl Iterator<Item = (&str, &Dynamic)> {
        self.variables.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Get an iterator to the functions in the [`Module`].
    #[inline(always)]
    #[allow(dead_code)]
    pub(crate) fn iter_fn(&self) -> impl Iterator<Item = &FuncInfo> {
        self.functions.values().map(Box::as_ref)
    }

    /// Get an iterator over all script-defined functions in the [`Module`].
    ///
    /// Function metadata includes:
    /// 1) Namespace ([`FnNamespace::Global`] or [`FnNamespace::Internal`]).
    /// 2) Access mode ([`FnAccess::Public`] or [`FnAccess::Private`]).
    /// 3) Function name (as string slice).
    /// 4) Number of parameters.
    /// 5) Shared reference to function definition [`ScriptFnDef`][crate::ast::ScriptFnDef].
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub(crate) fn iter_script_fn(
        &self,
    ) -> impl Iterator<
        Item = (
            FnNamespace,
            FnAccess,
            &str,
            usize,
            &Shared<crate::ast::ScriptFnDef>,
        ),
    > + '_ {
        self.functions
            .values()
            .filter(|f| f.func.is_script())
            .map(|f| {
                (
                    f.namespace,
                    f.access,
                    f.name.as_str(),
                    f.params,
                    f.func.get_script_fn_def().expect("scripted function"),
                )
            })
    }

    /// Get an iterator over all script-defined functions in the [`Module`].
    ///
    /// Function metadata includes:
    /// 1) Namespace ([`FnNamespace::Global`] or [`FnNamespace::Internal`]).
    /// 2) Access mode ([`FnAccess::Public`] or [`FnAccess::Private`]).
    /// 3) Function name (as string slice).
    /// 4) Number of parameters.
    #[cfg(not(feature = "no_function"))]
    #[cfg(not(feature = "internals"))]
    #[inline(always)]
    pub fn iter_script_fn_info(
        &self,
    ) -> impl Iterator<Item = (FnNamespace, FnAccess, &str, usize)> {
        self.functions
            .values()
            .filter(|f| f.func.is_script())
            .map(|f| (f.namespace, f.access, f.name.as_str(), f.params))
    }

    /// _(internals)_ Get an iterator over all script-defined functions in the [`Module`].
    /// Exported under the `internals` feature only.
    ///
    /// Function metadata includes:
    /// 1) Namespace ([`FnNamespace::Global`] or [`FnNamespace::Internal`]).
    /// 2) Access mode ([`FnAccess::Public`] or [`FnAccess::Private`]).
    /// 3) Function name (as string slice).
    /// 4) Number of parameters.
    /// 5) _(internals)_ Shared reference to function definition [`ScriptFnDef`][crate::ast::ScriptFnDef].
    #[cfg(not(feature = "no_function"))]
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn iter_script_fn_info(
        &self,
    ) -> impl Iterator<
        Item = (
            FnNamespace,
            FnAccess,
            &str,
            usize,
            &Shared<crate::ast::ScriptFnDef>,
        ),
    > {
        self.iter_script_fn()
    }

    /// Create a new [`Module`] by evaluating an [`AST`][crate::AST].
    ///
    /// The entire [`AST`][crate::AST] is encapsulated into each function, allowing functions
    /// to cross-call each other.  Functions in the global namespace, plus all functions
    /// defined in the [`Module`], are _merged_ into a _unified_ namespace before each call.
    /// Therefore, all functions will be found.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Module, Scope};
    ///
    /// let engine = Engine::new();
    /// let ast = engine.compile("let answer = 42; export answer;")?;
    /// let module = Module::eval_ast_as_new(Scope::new(), &ast, &engine)?;
    /// assert!(module.contains_var("answer"));
    /// assert_eq!(module.get_var_value::<i64>("answer").expect("answer should exist"), 42);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_module"))]
    pub fn eval_ast_as_new(
        mut scope: crate::Scope,
        ast: &crate::AST,
        engine: &crate::Engine,
    ) -> Result<Self, Box<EvalAltResult>> {
        let mut mods = crate::engine::Imports::new();
        let orig_mods_len = mods.len();

        // Run the script
        engine.eval_ast_with_scope_raw(&mut scope, &mut mods, &ast, 0)?;

        // Create new module
        let mut module = Module::new();

        scope.into_iter().for_each(|(_, value, mut aliases)| {
            // Variables with an alias left in the scope become module variables
            match aliases.len() {
                0 => (),
                1 => {
                    let alias = aliases.pop().expect("list has one item");
                    module.set_var(alias, value);
                }
                _ => aliases.into_iter().for_each(|alias| {
                    module.set_var(alias, value.clone());
                }),
            }
        });

        // Extra modules left in the scope become sub-modules
        let mut func_mods = crate::engine::Imports::new();

        mods.into_iter().skip(orig_mods_len).for_each(|(alias, m)| {
            func_mods.push(alias.clone(), m.clone());
            module.set_sub_module(alias, m);
        });

        // Non-private functions defined become module functions
        #[cfg(not(feature = "no_function"))]
        ast.lib()
            .functions
            .values()
            .filter(|f| match f.access {
                FnAccess::Public => true,
                FnAccess::Private => false,
            })
            .filter(|f| f.func.is_script())
            .for_each(|f| {
                // Encapsulate AST environment
                let mut func = f
                    .func
                    .get_script_fn_def()
                    .expect("scripted function")
                    .as_ref()
                    .clone();
                func.lib = Some(ast.shared_lib());
                func.mods = func_mods.clone();
                module.set_script_fn(func);
            });

        if let Some(s) = ast.source_raw() {
            module.set_id(s.clone());
        } else {
            module.clear_id();
        }

        module.build_index();

        Ok(module)
    }

    /// Does the [`Module`] contain indexed functions that have been exposed to the global namespace?
    ///
    /// # Panics
    ///
    /// Panics if the [`Module`] is not yet indexed via [`build_index`][Module::build_index].
    #[inline(always)]
    #[must_use]
    pub fn contains_indexed_global_functions(&self) -> bool {
        self.contains_indexed_global_functions
    }

    /// Scan through all the sub-modules in the [`Module`] and build a hash index of all
    /// variables and functions as one flattened namespace.
    ///
    /// If the [`Module`] is already indexed, this method has no effect.
    pub fn build_index(&mut self) -> &mut Self {
        // Collect a particular module.
        fn index_module<'a>(
            module: &'a Module,
            path: &mut Vec<&'a str>,
            variables: &mut BTreeMap<u64, Dynamic>,
            functions: &mut BTreeMap<u64, Shared<CallableFunction>>,
            type_iterators: &mut BTreeMap<TypeId, IteratorFn>,
        ) -> bool {
            let mut contains_indexed_global_functions = false;

            module.modules.iter().for_each(|(name, m)| {
                // Index all the sub-modules first.
                path.push(name);
                if index_module(m, path, variables, functions, type_iterators) {
                    contains_indexed_global_functions = true;
                }
                path.pop();
            });

            // Index all variables
            module.variables.iter().for_each(|(var_name, value)| {
                let hash_var = crate::calc_qualified_var_hash(path.iter().copied(), var_name);
                variables.insert(hash_var, value.clone());
            });

            // Index type iterators
            module.type_iterators.iter().for_each(|(&type_id, func)| {
                type_iterators.insert(type_id, *func);
                contains_indexed_global_functions = true;
            });

            // Index all Rust functions
            module.functions.iter().for_each(|(&hash, f)| {
                match f.namespace {
                    FnNamespace::Global => {
                        // Flatten all functions with global namespace
                        functions.insert(hash, f.func.clone());
                        contains_indexed_global_functions = true;
                    }
                    FnNamespace::Internal => (),
                }
                match f.access {
                    FnAccess::Public => (),
                    FnAccess::Private => return, // Do not index private functions
                }

                if !f.func.is_script() {
                    let hash_qualified_fn =
                        calc_native_fn_hash(path.iter().cloned(), f.name.as_str(), &f.param_types);
                    functions.insert(hash_qualified_fn, f.func.clone());
                } else if cfg!(not(feature = "no_function")) {
                    let hash_qualified_script = crate::calc_qualified_fn_hash(
                        path.iter().cloned(),
                        f.name.as_str(),
                        f.params,
                    );
                    functions.insert(hash_qualified_script, f.func.clone());
                }
            });

            contains_indexed_global_functions
        }

        if !self.indexed {
            let mut path = Vec::with_capacity(4);
            let mut variables = Default::default();
            let mut functions = Default::default();
            let mut type_iterators = Default::default();

            path.push("");

            self.contains_indexed_global_functions = index_module(
                self,
                &mut path,
                &mut variables,
                &mut functions,
                &mut type_iterators,
            );

            self.all_variables = variables;
            self.all_functions = functions;
            self.all_type_iterators = type_iterators;
            self.indexed = true;
        }

        self
    }

    /// Does a type iterator exist in the entire module tree?
    #[inline(always)]
    #[must_use]
    pub fn contains_qualified_iter(&self, id: TypeId) -> bool {
        self.all_type_iterators.contains_key(&id)
    }

    /// Does a type iterator exist in the module?
    #[inline(always)]
    #[must_use]
    pub fn contains_iter(&self, id: TypeId) -> bool {
        self.type_iterators.contains_key(&id)
    }

    /// Set a type iterator into the [`Module`].
    #[inline]
    pub fn set_iter(&mut self, type_id: TypeId, func: IteratorFn) -> &mut Self {
        if self.indexed {
            self.all_type_iterators.insert(type_id, func);
            self.contains_indexed_global_functions = true;
        }
        self.type_iterators.insert(type_id, func);
        self
    }

    /// Set a type iterator into the [`Module`].
    #[inline(always)]
    pub fn set_iterable<T>(&mut self) -> &mut Self
    where
        T: Variant + Clone + IntoIterator,
        <T as IntoIterator>::Item: Variant + Clone,
    {
        self.set_iter(TypeId::of::<T>(), |obj: Dynamic| {
            Box::new(obj.cast::<T>().into_iter().map(Dynamic::from))
        })
    }

    /// Set an iterator type into the [`Module`] as a type iterator.
    #[inline(always)]
    pub fn set_iterator<T>(&mut self) -> &mut Self
    where
        T: Variant + Clone + Iterator,
        <T as Iterator>::Item: Variant + Clone,
    {
        self.set_iter(TypeId::of::<T>(), |obj: Dynamic| {
            Box::new(obj.cast::<T>().map(Dynamic::from))
        })
    }

    /// Get the specified type iterator.
    #[inline(always)]
    #[must_use]
    pub(crate) fn get_qualified_iter(&self, id: TypeId) -> Option<IteratorFn> {
        self.all_type_iterators.get(&id).cloned()
    }

    /// Get the specified type iterator.
    #[inline(always)]
    #[must_use]
    pub(crate) fn get_iter(&self, id: TypeId) -> Option<IteratorFn> {
        self.type_iterators.get(&id).cloned()
    }
}

/// _(internals)_ A chain of [module][Module] names to namespace-qualify a variable or function call.
/// Exported under the `internals` feature only.
///
/// A [`u64`] offset to the current [`Scope`][crate::Scope] is cached for quick search purposes.
///
/// A [`StaticVec`] is used because most namespace-qualified access contains only one level,
/// and it is wasteful to always allocate a [`Vec`] with one element.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Clone, Eq, PartialEq, Default, Hash)]
pub struct NamespaceRef {
    index: Option<NonZeroUsize>,
    path: StaticVec<Ident>,
}

impl fmt::Debug for NamespaceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(index) = self.index {
            write!(f, "{} -> ", index)?;
        }

        f.write_str(
            &self
                .path
                .iter()
                .map(|Ident { name, .. }| name.as_str())
                .collect::<StaticVec<_>>()
                .join("::"),
        )
    }
}

impl fmt::Display for NamespaceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for Ident { name, .. } in self.path.iter() {
            write!(f, "{}{}", name, Token::DoubleColon.syntax())?;
        }
        Ok(())
    }
}

impl Deref for NamespaceRef {
    type Target = StaticVec<Ident>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl DerefMut for NamespaceRef {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.path
    }
}

impl From<StaticVec<Ident>> for NamespaceRef {
    #[inline(always)]
    fn from(path: StaticVec<Ident>) -> Self {
        let mut path = path;
        path.shrink_to_fit();
        Self { index: None, path }
    }
}

impl NamespaceRef {
    /// Create a new [`NamespaceRef`].
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            index: None,
            path: StaticVec::new(),
        }
    }
    /// Get the [`Scope`][crate::Scope] index offset.
    #[inline(always)]
    #[must_use]
    pub(crate) const fn index(&self) -> Option<NonZeroUsize> {
        self.index
    }
    /// Set the [`Scope`][crate::Scope] index offset.
    #[cfg(not(feature = "no_module"))]
    #[inline(always)]
    pub(crate) fn set_index(&mut self, index: Option<NonZeroUsize>) {
        self.index = index
    }
}

#[cfg(not(feature = "no_module"))]
pub use resolvers::ModuleResolver;

/// Module containing all built-in [module resolvers][ModuleResolver].
#[cfg(not(feature = "no_module"))]
pub mod resolvers;
