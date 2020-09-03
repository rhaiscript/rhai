//! Module defining external-loaded modules for Rhai.

use crate::any::{Dynamic, Variant};
use crate::calc_fn_hash;
use crate::engine::Engine;
use crate::fn_native::{CallableFunction as Func, FnCallArgs, IteratorFn, SendSync};
use crate::parser::{FnAccess, FnAccess::Public, ScriptFnDef};
use crate::result::EvalAltResult;
use crate::token::{Position, Token};
use crate::utils::{StaticVec, StraightHasherBuilder};

#[cfg(not(feature = "no_function"))]
use crate::fn_native::Shared;

#[cfg(not(feature = "no_module"))]
use crate::{
    engine::Imports,
    parser::AST,
    scope::{Entry as ScopeEntry, Scope},
};

#[cfg(not(feature = "no_index"))]
#[cfg(not(feature = "no_object"))]
use crate::engine::{FN_IDX_GET, FN_IDX_SET};

#[cfg(not(feature = "no_object"))]
use crate::engine::{make_getter, make_setter};

use crate::stdlib::{
    any::TypeId,
    boxed::Box,
    collections::HashMap,
    fmt, format,
    iter::empty,
    mem,
    num::NonZeroUsize,
    ops::{Deref, DerefMut},
    string::{String, ToString},
    vec::Vec,
};

#[cfg(not(feature = "no_std"))]
#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "sync"))]
use crate::stdlib::cell::RefCell;

#[cfg(not(feature = "no_std"))]
#[cfg(not(feature = "no_module"))]
#[cfg(feature = "sync")]
use crate::stdlib::sync::RwLock;

/// Return type of module-level Rust function.
pub type FuncReturn<T> = Result<T, Box<EvalAltResult>>;

/// An imported module, which may contain variables, sub-modules,
/// external Rust functions, and script-defined functions.
///
/// Not available under the `no_module` feature.
#[derive(Default)]
pub struct Module {
    /// Sub-modules.
    modules: HashMap<String, Module>,

    /// Module variables.
    variables: HashMap<String, Dynamic>,

    /// Flattened collection of all module variables, including those in sub-modules.
    all_variables: HashMap<u64, Dynamic, StraightHasherBuilder>,

    /// External Rust functions.
    functions: HashMap<u64, (String, FnAccess, StaticVec<TypeId>, Func), StraightHasherBuilder>,

    /// Iterator functions, keyed by the type producing the iterator.
    type_iterators: HashMap<TypeId, IteratorFn>,

    /// Flattened collection of all external Rust functions, native or scripted,
    /// including those in sub-modules.
    all_functions: HashMap<u64, Func, StraightHasherBuilder>,

    /// Is the module indexed?
    indexed: bool,
}

impl fmt::Debug for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Module(\n    modules: {}\n    vars: {}\n    functions: {}\n)",
            self.modules
                .keys()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            self.variables
                .iter()
                .map(|(k, v)| format!("{}={:?}", k, v))
                .collect::<Vec<_>>()
                .join(", "),
            self.functions
                .values()
                .map(|(_, _, _, f)| f.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

impl Clone for Module {
    fn clone(&self) -> Self {
        // Only clone the index at the top level
        Self {
            all_variables: self.all_variables.clone(),
            all_functions: self.all_functions.clone(),
            indexed: self.indexed,
            ..self.do_clone(false)
        }
    }
}

impl AsRef<Module> for Module {
    fn as_ref(&self) -> &Module {
        self
    }
}

impl Module {
    /// Create a new module.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// module.set_var("answer", 42_i64);
    /// assert_eq!(module.get_var_value::<i64>("answer").unwrap(), 42);
    /// ```
    pub fn new() -> Self {
        Default::default()
    }

    /// Create a new module with a specified capacity for native Rust functions.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// module.set_var("answer", 42_i64);
    /// assert_eq!(module.get_var_value::<i64>("answer").unwrap(), 42);
    /// ```
    pub fn new_with_capacity(capacity: usize) -> Self {
        Self {
            functions: HashMap::with_capacity_and_hasher(capacity, StraightHasherBuilder),
            ..Default::default()
        }
    }

    /// Clone the module, optionally skipping the index.
    fn do_clone(&self, clone_index: bool) -> Self {
        Self {
            modules: if clone_index {
                self.modules.clone()
            } else {
                self.modules
                    .iter()
                    .map(|(k, m)| (k.clone(), m.do_clone(clone_index)))
                    .collect()
            },
            variables: self.variables.clone(),
            functions: self.functions.clone(),
            type_iterators: self.type_iterators.clone(),
            ..Default::default()
        }
    }

    /// Does a variable exist in the module?
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// module.set_var("answer", 42_i64);
    /// assert!(module.contains_var("answer"));
    /// ```
    pub fn contains_var(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    /// Get the value of a module variable.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// module.set_var("answer", 42_i64);
    /// assert_eq!(module.get_var_value::<i64>("answer").unwrap(), 42);
    /// ```
    pub fn get_var_value<T: Variant + Clone>(&self, name: &str) -> Option<T> {
        self.get_var(name).and_then(Dynamic::try_cast::<T>)
    }

    /// Get a module variable as a `Dynamic`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// module.set_var("answer", 42_i64);
    /// assert_eq!(module.get_var("answer").unwrap().cast::<i64>(), 42);
    /// ```
    pub fn get_var(&self, name: &str) -> Option<Dynamic> {
        self.variables.get(name).cloned()
    }

    /// Set a variable into the module.
    ///
    /// If there is an existing variable of the same name, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// module.set_var("answer", 42_i64);
    /// assert_eq!(module.get_var_value::<i64>("answer").unwrap(), 42);
    /// ```
    pub fn set_var(&mut self, name: impl Into<String>, value: impl Variant + Clone) -> &mut Self {
        self.variables.insert(name.into(), Dynamic::from(value));
        self.indexed = false;
        self
    }

    /// Get a mutable reference to a modules-qualified variable.
    /// Name and Position in `EvalAltResult` are None and must be set afterwards.
    ///
    /// The `u64` hash is calculated by the function `crate::calc_fn_hash`.
    pub(crate) fn get_qualified_var_mut(
        &mut self,
        hash_var: u64,
    ) -> Result<&mut Dynamic, Box<EvalAltResult>> {
        self.all_variables.get_mut(&hash_var).ok_or_else(|| {
            EvalAltResult::ErrorVariableNotFound(String::new(), Position::none()).into()
        })
    }

    /// Set a script-defined function into the module.
    ///
    /// If there is an existing function of the same name and number of arguments, it is replaced.
    pub(crate) fn set_script_fn(&mut self, fn_def: ScriptFnDef) -> &mut Self {
        // None + function name + number of arguments.
        let hash_script = calc_fn_hash(empty(), &fn_def.name, fn_def.params.len(), empty());
        self.functions.insert(
            hash_script,
            (
                fn_def.name.to_string(),
                fn_def.access,
                Default::default(),
                fn_def.into(),
            ),
        );
        self.indexed = false;
        self
    }

    /// Does a sub-module exist in the module?
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// let sub_module = Module::new();
    /// module.set_sub_module("question", sub_module);
    /// assert!(module.contains_sub_module("question"));
    /// ```
    pub fn contains_sub_module(&self, name: &str) -> bool {
        self.modules.contains_key(name)
    }

    /// Get a sub-module.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// let sub_module = Module::new();
    /// module.set_sub_module("question", sub_module);
    /// assert!(module.get_sub_module("question").is_some());
    /// ```
    pub fn get_sub_module(&self, name: &str) -> Option<&Module> {
        self.modules.get(name)
    }

    /// Get a mutable reference to a sub-module.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// let sub_module = Module::new();
    /// module.set_sub_module("question", sub_module);
    /// assert!(module.get_sub_module_mut("question").is_some());
    /// ```
    pub fn get_sub_module_mut(&mut self, name: &str) -> Option<&mut Module> {
        self.modules.get_mut(name)
    }

    /// Set a sub-module into the module.
    ///
    /// If there is an existing sub-module of the same name, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// let sub_module = Module::new();
    /// module.set_sub_module("question", sub_module);
    /// assert!(module.get_sub_module("question").is_some());
    /// ```
    pub fn set_sub_module(&mut self, name: impl Into<String>, sub_module: Module) -> &mut Self {
        self.modules.insert(name.into(), sub_module.into());
        self.indexed = false;
        self
    }

    /// Does the particular Rust function exist in the module?
    ///
    /// The `u64` hash is calculated by the function `crate::calc_fn_hash`.
    /// It is also returned by the `set_fn_XXX` calls.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_fn_0("calc", || Ok(42_i64));
    /// assert!(module.contains_fn(hash, true));
    /// ```
    pub fn contains_fn(&self, hash_fn: u64, public_only: bool) -> bool {
        if public_only {
            self.functions
                .get(&hash_fn)
                .map(|(_, access, _, _)| match access {
                    FnAccess::Public => true,
                    FnAccess::Private => false,
                })
                .unwrap_or(false)
        } else {
            self.functions.contains_key(&hash_fn)
        }
    }

    /// Set a Rust function into the module, returning a hash key.
    ///
    /// If there is an existing Rust function of the same hash, it is replaced.
    ///
    /// ## WARNING - Low Level API
    ///
    /// This function is very low level.
    pub fn set_fn(
        &mut self,
        name: impl Into<String>,
        access: FnAccess,
        arg_types: &[TypeId],
        func: Func,
    ) -> u64 {
        let name = name.into();

        let hash_fn = calc_fn_hash(empty(), &name, arg_types.len(), arg_types.iter().cloned());

        let params = arg_types.into_iter().cloned().collect();

        self.functions
            .insert(hash_fn, (name, access, params, func.into()));

        self.indexed = false;

        hash_fn
    }

    /// Set a Rust function taking a reference to the scripting `Engine`, the current set of functions,
    /// plus a list of mutable `Dynamic` references into the module, returning a hash key.
    ///
    /// Use this to register a built-in function which must reference settings on the scripting
    /// `Engine` (e.g. to prevent growing an array beyond the allowed maximum size), or to call a
    /// script-defined function in the current evaluation context.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// ## WARNING - Low Level API
    ///
    /// This function is very low level.
    ///
    /// A list of `TypeId`'s is taken as the argument types.
    ///
    /// Arguments are simply passed in as a mutable array of `&mut Dynamic`,
    /// which is guaranteed to contain enough arguments of the correct types.
    ///
    /// To access a primary parameter value (i.e. cloning is cheap), use: `args[n].clone().cast::<T>()`
    ///
    /// To access a parameter value and avoid cloning, use `std::mem::take(args[n]).cast::<T>()`.
    /// Notice that this will _consume_ the argument, replacing it with `()`.
    ///
    /// To access the first mutable parameter, use `args.get_mut(0).unwrap()`
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_raw_fn("double_or_not",
    ///                 // Pass parameter types via a slice with TypeId's
    ///                 &[std::any::TypeId::of::<i64>(), std::any::TypeId::of::<bool>()],
    ///                 // Fixed closure signature
    ///                 |engine, lib, args| {
    ///                     // 'args' is guaranteed to be the right length and of the correct types
    ///
    ///                     // Get the second parameter by 'consuming' it
    ///                     let double = std::mem::take(args[1]).cast::<bool>();
    ///                     // Since it is a primary type, it can also be cheaply copied
    ///                     let double = args[1].clone().cast::<bool>();
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
    /// assert!(module.contains_fn(hash, true));
    /// ```
    pub fn set_raw_fn<T: Variant + Clone>(
        &mut self,
        name: impl Into<String>,
        arg_types: &[TypeId],
        func: impl Fn(&Engine, &Module, &mut [&mut Dynamic]) -> FuncReturn<T> + SendSync + 'static,
    ) -> u64 {
        let f = move |engine: &Engine, lib: &Module, args: &mut FnCallArgs| {
            func(engine, lib, args).map(Dynamic::from)
        };
        self.set_fn(name, Public, arg_types, Func::from_method(Box::new(f)))
    }

    /// Set a Rust function taking no parameters into the module, returning a hash key.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_fn_0("calc", || Ok(42_i64));
    /// assert!(module.contains_fn(hash, true));
    /// ```
    pub fn set_fn_0<T: Variant + Clone>(
        &mut self,
        name: impl Into<String>,
        func: impl Fn() -> FuncReturn<T> + SendSync + 'static,
    ) -> u64 {
        let f = move |_: &Engine, _: &Module, _: &mut FnCallArgs| func().map(Dynamic::from);
        let arg_types = [];
        self.set_fn(name, Public, &arg_types, Func::from_pure(Box::new(f)))
    }

    /// Set a Rust function taking one parameter into the module, returning a hash key.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_fn_1("calc", |x: i64| Ok(x + 1));
    /// assert!(module.contains_fn(hash, true));
    /// ```
    pub fn set_fn_1<A: Variant + Clone, T: Variant + Clone>(
        &mut self,
        name: impl Into<String>,
        func: impl Fn(A) -> FuncReturn<T> + SendSync + 'static,
    ) -> u64 {
        let f = move |_: &Engine, _: &Module, args: &mut FnCallArgs| {
            func(mem::take(args[0]).cast::<A>()).map(Dynamic::from)
        };
        let arg_types = [TypeId::of::<A>()];
        self.set_fn(name, Public, &arg_types, Func::from_pure(Box::new(f)))
    }

    /// Set a Rust function taking one mutable parameter into the module, returning a hash key.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_fn_1_mut("calc", |x: &mut i64| { *x += 1; Ok(*x) });
    /// assert!(module.contains_fn(hash, true));
    /// ```
    pub fn set_fn_1_mut<A: Variant + Clone, T: Variant + Clone>(
        &mut self,
        name: impl Into<String>,
        func: impl Fn(&mut A) -> FuncReturn<T> + SendSync + 'static,
    ) -> u64 {
        let f = move |_: &Engine, _: &Module, args: &mut FnCallArgs| {
            func(&mut args[0].write_lock::<A>().unwrap()).map(Dynamic::from)
        };
        let arg_types = [TypeId::of::<A>()];
        self.set_fn(name, Public, &arg_types, Func::from_method(Box::new(f)))
    }

    /// Set a Rust getter function taking one mutable parameter, returning a hash key.
    ///
    /// If there is a similar existing Rust getter function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Module;
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_getter_fn("value", |x: &mut i64| { Ok(*x) });
    /// assert!(module.contains_fn(hash, true));
    /// ```
    #[cfg(not(feature = "no_object"))]
    pub fn set_getter_fn<A: Variant + Clone, T: Variant + Clone>(
        &mut self,
        name: impl Into<String>,
        func: impl Fn(&mut A) -> FuncReturn<T> + SendSync + 'static,
    ) -> u64 {
        self.set_fn_1_mut(make_getter(&name.into()), func)
    }

    /// Set a Rust function taking two parameters into the module, returning a hash key.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_fn_2("calc", |x: i64, y: ImmutableString| {
    ///     Ok(x + y.len() as i64)
    /// });
    /// assert!(module.contains_fn(hash, true));
    /// ```
    pub fn set_fn_2<A: Variant + Clone, B: Variant + Clone, T: Variant + Clone>(
        &mut self,
        name: impl Into<String>,
        func: impl Fn(A, B) -> FuncReturn<T> + SendSync + 'static,
    ) -> u64 {
        let f = move |_: &Engine, _: &Module, args: &mut FnCallArgs| {
            let a = mem::take(args[0]).cast::<A>();
            let b = mem::take(args[1]).cast::<B>();

            func(a, b).map(Dynamic::from)
        };
        let arg_types = [TypeId::of::<A>(), TypeId::of::<B>()];
        self.set_fn(name, Public, &arg_types, Func::from_pure(Box::new(f)))
    }

    /// Set a Rust function taking two parameters (the first one mutable) into the module,
    /// returning a hash key.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_fn_2_mut("calc", |x: &mut i64, y: ImmutableString| {
    ///     *x += y.len() as i64; Ok(*x)
    /// });
    /// assert!(module.contains_fn(hash, true));
    /// ```
    pub fn set_fn_2_mut<A: Variant + Clone, B: Variant + Clone, T: Variant + Clone>(
        &mut self,
        name: impl Into<String>,
        func: impl Fn(&mut A, B) -> FuncReturn<T> + SendSync + 'static,
    ) -> u64 {
        let f = move |_: &Engine, _: &Module, args: &mut FnCallArgs| {
            let b = mem::take(args[1]).cast::<B>();
            let a = &mut args[0].write_lock::<A>().unwrap();

            func(a, b).map(Dynamic::from)
        };
        let arg_types = [TypeId::of::<A>(), TypeId::of::<B>()];
        self.set_fn(name, Public, &arg_types, Func::from_method(Box::new(f)))
    }

    /// Set a Rust setter function taking two parameters (the first one mutable) into the module,
    /// returning a hash key.
    ///
    /// If there is a similar existing setter Rust function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_setter_fn("value", |x: &mut i64, y: ImmutableString| {
    ///     *x = y.len() as i64;
    ///     Ok(())
    /// });
    /// assert!(module.contains_fn(hash, true));
    /// ```
    #[cfg(not(feature = "no_object"))]
    pub fn set_setter_fn<A: Variant + Clone, B: Variant + Clone>(
        &mut self,
        name: impl Into<String>,
        func: impl Fn(&mut A, B) -> FuncReturn<()> + SendSync + 'static,
    ) -> u64 {
        self.set_fn_2_mut(make_setter(&name.into()), func)
    }

    /// Set a Rust index getter taking two parameters (the first one mutable) into the module,
    /// returning a hash key.
    ///
    /// If there is a similar existing setter Rust function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_indexer_get_fn(|x: &mut i64, y: ImmutableString| {
    ///     Ok(*x + y.len() as i64)
    /// });
    /// assert!(module.contains_fn(hash, true));
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[cfg(not(feature = "no_index"))]
    pub fn set_indexer_get_fn<A: Variant + Clone, B: Variant + Clone, T: Variant + Clone>(
        &mut self,
        func: impl Fn(&mut A, B) -> FuncReturn<T> + SendSync + 'static,
    ) -> u64 {
        self.set_fn_2_mut(FN_IDX_GET, func)
    }

    /// Set a Rust function taking three parameters into the module, returning a hash key.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_fn_3("calc", |x: i64, y: ImmutableString, z: i64| {
    ///     Ok(x + y.len() as i64 + z)
    /// });
    /// assert!(module.contains_fn(hash, true));
    /// ```
    pub fn set_fn_3<
        A: Variant + Clone,
        B: Variant + Clone,
        C: Variant + Clone,
        T: Variant + Clone,
    >(
        &mut self,
        name: impl Into<String>,
        func: impl Fn(A, B, C) -> FuncReturn<T> + SendSync + 'static,
    ) -> u64 {
        let f = move |_: &Engine, _: &Module, args: &mut FnCallArgs| {
            let a = mem::take(args[0]).cast::<A>();
            let b = mem::take(args[1]).cast::<B>();
            let c = mem::take(args[2]).cast::<C>();

            func(a, b, c).map(Dynamic::from)
        };
        let arg_types = [TypeId::of::<A>(), TypeId::of::<B>(), TypeId::of::<C>()];
        self.set_fn(name, Public, &arg_types, Func::from_pure(Box::new(f)))
    }

    /// Set a Rust function taking three parameters (the first one mutable) into the module,
    /// returning a hash key.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_fn_3_mut("calc", |x: &mut i64, y: ImmutableString, z: i64| {
    ///     *x += y.len() as i64 + z; Ok(*x)
    /// });
    /// assert!(module.contains_fn(hash, true));
    /// ```
    pub fn set_fn_3_mut<
        A: Variant + Clone,
        B: Variant + Clone,
        C: Variant + Clone,
        T: Variant + Clone,
    >(
        &mut self,
        name: impl Into<String>,
        func: impl Fn(&mut A, B, C) -> FuncReturn<T> + SendSync + 'static,
    ) -> u64 {
        let f = move |_: &Engine, _: &Module, args: &mut FnCallArgs| {
            let b = mem::take(args[1]).cast::<B>();
            let c = mem::take(args[2]).cast::<C>();
            let a = &mut args[0].write_lock::<A>().unwrap();

            func(a, b, c).map(Dynamic::from)
        };
        let arg_types = [TypeId::of::<A>(), TypeId::of::<B>(), TypeId::of::<C>()];
        self.set_fn(name, Public, &arg_types, Func::from_method(Box::new(f)))
    }

    /// Set a Rust index setter taking three parameters (the first one mutable) into the module,
    /// returning a hash key.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_indexer_set_fn(|x: &mut i64, y: ImmutableString, value: i64| {
    ///     *x = y.len() as i64 + value;
    ///     Ok(())
    /// });
    /// assert!(module.contains_fn(hash, true));
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[cfg(not(feature = "no_index"))]
    pub fn set_indexer_set_fn<A: Variant + Clone, B: Variant + Clone, C: Variant + Clone>(
        &mut self,
        func: impl Fn(&mut A, B, C) -> FuncReturn<()> + SendSync + 'static,
    ) -> u64 {
        let f = move |_: &Engine, _: &Module, args: &mut FnCallArgs| {
            let b = mem::take(args[1]).cast::<B>();
            let c = mem::take(args[2]).cast::<C>();
            let a = &mut args[0].write_lock::<A>().unwrap();

            func(a, b, c).map(Dynamic::from)
        };
        let arg_types = [TypeId::of::<A>(), TypeId::of::<B>(), TypeId::of::<C>()];
        self.set_fn(
            FN_IDX_SET,
            Public,
            &arg_types,
            Func::from_method(Box::new(f)),
        )
    }

    /// Set a pair of Rust index getter and setter functions, returning both hash keys.
    /// This is a shorthand for `set_indexer_get_fn` and `set_indexer_set_fn`.
    ///
    /// If there are similar existing Rust functions, they are replaced.
    ///
    /// # Examples
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
    ///         *x = y.len() as i64 + value;
    ///         Ok(())
    ///     }
    /// );
    /// assert!(module.contains_fn(hash_get, true));
    /// assert!(module.contains_fn(hash_set, true));
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[cfg(not(feature = "no_index"))]
    pub fn set_indexer_get_set_fn<A: Variant + Clone, B: Variant + Clone, T: Variant + Clone>(
        &mut self,
        getter: impl Fn(&mut A, B) -> FuncReturn<T> + SendSync + 'static,
        setter: impl Fn(&mut A, B, T) -> FuncReturn<()> + SendSync + 'static,
    ) -> (u64, u64) {
        (
            self.set_indexer_get_fn(getter),
            self.set_indexer_set_fn(setter),
        )
    }

    /// Set a Rust function taking four parameters into the module, returning a hash key.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_fn_4("calc", |x: i64, y: ImmutableString, z: i64, _w: ()| {
    ///     Ok(x + y.len() as i64 + z)
    /// });
    /// assert!(module.contains_fn(hash, true));
    /// ```
    pub fn set_fn_4<
        A: Variant + Clone,
        B: Variant + Clone,
        C: Variant + Clone,
        D: Variant + Clone,
        T: Variant + Clone,
    >(
        &mut self,
        name: impl Into<String>,
        func: impl Fn(A, B, C, D) -> FuncReturn<T> + SendSync + 'static,
    ) -> u64 {
        let f = move |_: &Engine, _: &Module, args: &mut FnCallArgs| {
            let a = mem::take(args[0]).cast::<A>();
            let b = mem::take(args[1]).cast::<B>();
            let c = mem::take(args[2]).cast::<C>();
            let d = mem::take(args[3]).cast::<D>();

            func(a, b, c, d).map(Dynamic::from)
        };
        let arg_types = [
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            TypeId::of::<C>(),
            TypeId::of::<D>(),
        ];
        self.set_fn(name, Public, &arg_types, Func::from_pure(Box::new(f)))
    }

    /// Set a Rust function taking four parameters (the first one mutable) into the module,
    /// returning a hash key.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_fn_4_mut("calc", |x: &mut i64, y: ImmutableString, z: i64, _w: ()| {
    ///     *x += y.len() as i64 + z; Ok(*x)
    /// });
    /// assert!(module.contains_fn(hash, true));
    /// ```
    pub fn set_fn_4_mut<
        A: Variant + Clone,
        B: Variant + Clone,
        C: Variant + Clone,
        D: Variant + Clone,
        T: Variant + Clone,
    >(
        &mut self,
        name: impl Into<String>,
        func: impl Fn(&mut A, B, C, D) -> FuncReturn<T> + SendSync + 'static,
    ) -> u64 {
        let f = move |_: &Engine, _: &Module, args: &mut FnCallArgs| {
            let b = mem::take(args[1]).cast::<B>();
            let c = mem::take(args[2]).cast::<C>();
            let d = mem::take(args[3]).cast::<D>();
            let a = &mut args[0].write_lock::<A>().unwrap();

            func(a, b, c, d).map(Dynamic::from)
        };
        let arg_types = [
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            TypeId::of::<C>(),
            TypeId::of::<C>(),
        ];
        self.set_fn(name, Public, &arg_types, Func::from_method(Box::new(f)))
    }

    /// Get a Rust function.
    ///
    /// The `u64` hash is calculated by the function `crate::calc_fn_hash`.
    /// It is also returned by the `set_fn_XXX` calls.
    pub(crate) fn get_fn(&self, hash_fn: u64, public_only: bool) -> Option<&Func> {
        self.functions
            .get(&hash_fn)
            .and_then(|(_, access, _, f)| match access {
                _ if !public_only => Some(f),
                FnAccess::Public => Some(f),
                FnAccess::Private => None,
            })
    }

    /// Get a modules-qualified function.
    /// Name and Position in `EvalAltResult` are None and must be set afterwards.
    ///
    /// The `u64` hash is calculated by the function `crate::calc_fn_hash` and must match
    /// the hash calculated by `index_all_sub_modules`.
    pub(crate) fn get_qualified_fn(&self, hash_qualified_fn: u64) -> Option<&Func> {
        self.all_functions.get(&hash_qualified_fn)
    }

    /// Combine another module into this module.
    /// The other module is consumed to merge into this module.
    pub fn combine(&mut self, other: Self) -> &mut Self {
        self.modules.extend(other.modules.into_iter());
        self.variables.extend(other.variables.into_iter());
        self.functions.extend(other.functions.into_iter());
        self.type_iterators.extend(other.type_iterators.into_iter());
        self.all_functions.clear();
        self.all_variables.clear();
        self.indexed = false;
        self
    }

    /// Combine another module into this module.
    /// The other module is consumed to merge into this module.
    /// Sub-modules are flattened onto the root module, with higher level overriding lower level.
    pub fn combine_flatten(&mut self, other: Self) -> &mut Self {
        other.modules.into_iter().for_each(|(_, m)| {
            self.combine_flatten(m);
        });

        self.variables.extend(other.variables.into_iter());
        self.functions.extend(other.functions.into_iter());
        self.type_iterators.extend(other.type_iterators.into_iter());
        self.all_functions.clear();
        self.all_variables.clear();
        self.indexed = false;
        self
    }

    /// Merge another module into this module.
    pub fn merge(&mut self, other: &Self) -> &mut Self {
        self.merge_filtered(other, &|_, _, _| true)
    }

    /// Merge another module into this module, with only selected script-defined functions based on a filter predicate.
    pub(crate) fn merge_filtered(
        &mut self,
        other: &Self,
        _filter: &impl Fn(FnAccess, &str, usize) -> bool,
    ) -> &mut Self {
        #[cfg(not(feature = "no_function"))]
        for (k, v) in &other.modules {
            let mut m = Self::new();
            m.merge_filtered(v, _filter);
            self.modules.insert(k.clone(), m);
        }

        #[cfg(feature = "no_function")]
        self.modules
            .extend(other.modules.iter().map(|(k, v)| (k.clone(), v.clone())));

        self.variables
            .extend(other.variables.iter().map(|(k, v)| (k.clone(), v.clone())));

        self.functions.extend(
            other
                .functions
                .iter()
                .filter(|(_, (_, _, _, v))| match v {
                    #[cfg(not(feature = "no_function"))]
                    Func::Script(ref f) => _filter(f.access, f.name.as_str(), f.params.len()),
                    _ => true,
                })
                .map(|(&k, v)| (k, v.clone())),
        );

        self.type_iterators
            .extend(other.type_iterators.iter().map(|(&k, v)| (k, v.clone())));

        self.all_functions.clear();
        self.all_variables.clear();
        self.indexed = false;
        self
    }

    /// Filter out the functions, retaining only some based on a filter predicate.
    #[cfg(not(feature = "no_function"))]
    pub(crate) fn retain_functions(
        &mut self,
        filter: impl Fn(FnAccess, &str, usize) -> bool,
    ) -> &mut Self {
        self.functions.retain(|_, (_, _, _, v)| match v {
            Func::Script(ref f) => filter(f.access, f.name.as_str(), f.params.len()),
            _ => true,
        });

        self.all_functions.clear();
        self.all_variables.clear();
        self.indexed = false;
        self
    }

    /// Get the number of variables in the module.
    pub fn num_var(&self) -> usize {
        self.variables.len()
    }
    /// Get the number of functions in the module.
    pub fn num_fn(&self) -> usize {
        self.variables.len()
    }
    /// Get the number of type iterators in the module.
    pub fn num_iter(&self) -> usize {
        self.variables.len()
    }

    /// Get an iterator to the variables in the module.
    pub fn iter_var(&self) -> impl Iterator<Item = (&String, &Dynamic)> {
        self.variables.iter()
    }

    /// Get an iterator to the functions in the module.
    pub(crate) fn iter_fn(
        &self,
    ) -> impl Iterator<Item = &(String, FnAccess, StaticVec<TypeId>, Func)> {
        self.functions.values()
    }

    /// Get an iterator over all script-defined functions in the module.
    #[cfg(not(feature = "no_function"))]
    pub fn iter_script_fn<'a>(&'a self) -> impl Iterator<Item = Shared<ScriptFnDef>> + 'a {
        self.functions
            .values()
            .map(|(_, _, _, f)| f)
            .filter(|f| f.is_script())
            .map(|f| f.get_shared_fn_def())
    }

    /// Create a new `Module` by evaluating an `AST`.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Module, Scope};
    ///
    /// let engine = Engine::new();
    /// let ast = engine.compile("let answer = 42; export answer;")?;
    /// let module = Module::eval_ast_as_new(Scope::new(), &ast, &engine)?;
    /// assert!(module.contains_var("answer"));
    /// assert_eq!(module.get_var_value::<i64>("answer").unwrap(), 42);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_module"))]
    pub fn eval_ast_as_new(mut scope: Scope, ast: &AST, engine: &Engine) -> FuncReturn<Self> {
        let mut mods = Imports::new();

        // Run the script
        engine.eval_ast_with_scope_raw(&mut scope, &mut mods, &ast)?;

        // Create new module
        let mut module = Module::new();

        scope
            .into_iter()
            .for_each(|ScopeEntry { value, alias, .. }| {
                // Variables with an alias left in the scope become module variables
                if alias.is_some() {
                    module.variables.insert(*alias.unwrap(), value);
                }
            });

        // Modules left in the scope become sub-modules
        mods.into_iter().for_each(|(alias, m)| {
            module.modules.insert(alias.to_string(), m);
        });

        module.merge(ast.lib());

        Ok(module)
    }

    /// Scan through all the sub-modules in the module build an index of all
    /// variables and external Rust functions via hashing.
    #[cfg(not(feature = "no_module"))]
    pub(crate) fn index_all_sub_modules(&mut self) {
        // Collect a particular module.
        fn index_module<'a>(
            module: &'a Module,
            qualifiers: &mut Vec<&'a str>,
            variables: &mut Vec<(u64, Dynamic)>,
            functions: &mut Vec<(u64, Func)>,
        ) {
            for (name, m) in &module.modules {
                // Index all the sub-modules first.
                qualifiers.push(name);
                index_module(m, qualifiers, variables, functions);
                qualifiers.pop();
            }

            // Index all variables
            for (var_name, value) in &module.variables {
                // Qualifiers + variable name
                let hash_var = calc_fn_hash(qualifiers.iter().map(|&v| v), var_name, 0, empty());
                variables.push((hash_var, value.clone()));
            }
            // Index all Rust functions
            for (name, access, params, func) in module.functions.values() {
                match access {
                    // Private functions are not exported
                    FnAccess::Private => continue,
                    FnAccess::Public => (),
                }

                #[cfg(not(feature = "no_function"))]
                if func.is_script() {
                    let fn_def = func.get_shared_fn_def();
                    // Qualifiers + function name + number of arguments.
                    let hash_qualified_script = calc_fn_hash(
                        qualifiers.iter().map(|&v| v),
                        &fn_def.name,
                        fn_def.params.len(),
                        empty(),
                    );
                    functions.push((hash_qualified_script, fn_def.into()));
                    continue;
                }

                // Qualified Rust functions are indexed in two steps:
                // 1) Calculate a hash in a similar manner to script-defined functions,
                //    i.e. qualifiers + function name + number of arguments.
                let hash_qualified_script =
                    calc_fn_hash(qualifiers.iter().map(|&v| v), name, params.len(), empty());
                // 2) Calculate a second hash with no qualifiers, empty function name,
                //    zero number of arguments, and the actual list of argument `TypeId`'.s
                let hash_fn_args = calc_fn_hash(empty(), "", 0, params.iter().cloned());
                // 3) The final hash is the XOR of the two hashes.
                let hash_qualified_fn = hash_qualified_script ^ hash_fn_args;

                functions.push((hash_qualified_fn, func.clone()));
            }
        }

        if self.indexed {
            return;
        }

        let mut qualifiers: Vec<_> = Default::default();
        let mut variables: Vec<_> = Default::default();
        let mut functions: Vec<_> = Default::default();

        qualifiers.push("root");

        index_module(self, &mut qualifiers, &mut variables, &mut functions);

        self.all_variables = variables.into_iter().collect();
        self.all_functions = functions.into_iter().collect();
        self.indexed = true;
    }

    /// Does a type iterator exist in the module?
    pub fn contains_iter(&self, id: TypeId) -> bool {
        self.type_iterators.contains_key(&id)
    }

    /// Set a type iterator into the module.
    pub fn set_iter(&mut self, typ: TypeId, func: IteratorFn) -> &mut Self {
        self.type_iterators.insert(typ, func);
        self.indexed = false;
        self
    }

    /// Get the specified type iterator.
    pub(crate) fn get_iter(&self, id: TypeId) -> Option<IteratorFn> {
        self.type_iterators.get(&id).cloned()
    }
}

/// [INTERNALS] A chain of module names to qualify a variable or function call.
/// Exported under the `internals` feature only.
///
/// A `u64` hash key is cached for quick search purposes.
///
/// A `StaticVec` is used because most module-level access contains only one level,
/// and it is wasteful to always allocate a `Vec` with one element.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Clone, Eq, PartialEq, Default, Hash)]
pub struct ModuleRef(StaticVec<(String, Position)>, Option<NonZeroUsize>);

impl fmt::Debug for ModuleRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)?;

        if let Some(index) = self.1 {
            write!(f, " -> {}", index)
        } else {
            Ok(())
        }
    }
}

impl Deref for ModuleRef {
    type Target = StaticVec<(String, Position)>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ModuleRef {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Display for ModuleRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (m, _) in self.0.iter() {
            write!(f, "{}{}", m, Token::DoubleColon.syntax())?;
        }
        Ok(())
    }
}

impl From<StaticVec<(String, Position)>> for ModuleRef {
    fn from(modules: StaticVec<(String, Position)>) -> Self {
        Self(modules, None)
    }
}

impl ModuleRef {
    pub(crate) fn index(&self) -> Option<NonZeroUsize> {
        self.1
    }
    pub(crate) fn set_index(&mut self, index: Option<NonZeroUsize>) {
        self.1 = index
    }
}

/// Trait that encapsulates a module resolution service.
pub trait ModuleResolver: SendSync {
    /// Resolve a module based on a path string.
    fn resolve(&self, _: &Engine, path: &str, pos: Position) -> Result<Module, Box<EvalAltResult>>;
}

/// Re-export module resolvers.
#[cfg(not(feature = "no_module"))]
pub mod resolvers {
    pub use super::collection::ModuleResolversCollection;
    #[cfg(not(feature = "no_std"))]
    #[cfg(not(target_arch = "wasm32"))]
    pub use super::file::FileModuleResolver;
    pub use super::stat::StaticModuleResolver;
}
#[cfg(feature = "no_module")]
pub mod resolvers {}

/// Script file-based module resolver.
#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "no_std"))]
#[cfg(not(target_arch = "wasm32"))]
mod file {
    use super::*;
    use crate::stdlib::path::PathBuf;

    /// Module resolution service that loads module script files from the file system.
    ///
    /// Script files are cached so they are are not reloaded and recompiled in subsequent requests.
    ///
    /// The `new_with_path` and `new_with_path_and_extension` constructor functions
    /// allow specification of a base directory with module path used as a relative path offset
    /// to the base directory. The script file is then forced to be in a specified extension
    /// (default `.rhai`).
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Engine;
    /// use rhai::module_resolvers::FileModuleResolver;
    ///
    /// // Create a new 'FileModuleResolver' loading scripts from the 'scripts' subdirectory
    /// // with file extension '.x'.
    /// let resolver = FileModuleResolver::new_with_path_and_extension("./scripts", "x");
    ///
    /// let mut engine = Engine::new();
    /// engine.set_module_resolver(Some(resolver));
    /// ```
    #[derive(Debug)]
    pub struct FileModuleResolver {
        path: PathBuf,
        extension: String,

        #[cfg(not(feature = "sync"))]
        cache: RefCell<HashMap<PathBuf, AST>>,

        #[cfg(feature = "sync")]
        cache: RwLock<HashMap<PathBuf, AST>>,
    }

    impl Default for FileModuleResolver {
        fn default() -> Self {
            Self::new_with_path(PathBuf::default())
        }
    }

    impl FileModuleResolver {
        /// Create a new `FileModuleResolver` with a specific base path.
        ///
        /// # Examples
        ///
        /// ```
        /// use rhai::Engine;
        /// use rhai::module_resolvers::FileModuleResolver;
        ///
        /// // Create a new 'FileModuleResolver' loading scripts from the 'scripts' subdirectory
        /// // with file extension '.rhai' (the default).
        /// let resolver = FileModuleResolver::new_with_path("./scripts");
        ///
        /// let mut engine = Engine::new();
        /// engine.set_module_resolver(Some(resolver));
        /// ```
        pub fn new_with_path<P: Into<PathBuf>>(path: P) -> Self {
            Self::new_with_path_and_extension(path, "rhai")
        }

        /// Create a new `FileModuleResolver` with a specific base path and file extension.
        ///
        /// The default extension is `.rhai`.
        ///
        /// # Examples
        ///
        /// ```
        /// use rhai::Engine;
        /// use rhai::module_resolvers::FileModuleResolver;
        ///
        /// // Create a new 'FileModuleResolver' loading scripts from the 'scripts' subdirectory
        /// // with file extension '.x'.
        /// let resolver = FileModuleResolver::new_with_path_and_extension("./scripts", "x");
        ///
        /// let mut engine = Engine::new();
        /// engine.set_module_resolver(Some(resolver));
        /// ```
        pub fn new_with_path_and_extension<P: Into<PathBuf>, E: Into<String>>(
            path: P,
            extension: E,
        ) -> Self {
            Self {
                path: path.into(),
                extension: extension.into(),
                cache: Default::default(),
            }
        }

        /// Create a new `FileModuleResolver` with the current directory as base path.
        ///
        /// # Examples
        ///
        /// ```
        /// use rhai::Engine;
        /// use rhai::module_resolvers::FileModuleResolver;
        ///
        /// // Create a new 'FileModuleResolver' loading scripts from the current directory
        /// // with file extension '.rhai' (the default).
        /// let resolver = FileModuleResolver::new();
        ///
        /// let mut engine = Engine::new();
        /// engine.set_module_resolver(Some(resolver));
        /// ```
        pub fn new() -> Self {
            Default::default()
        }

        /// Create a `Module` from a file path.
        pub fn create_module<P: Into<PathBuf>>(
            &self,
            engine: &Engine,
            path: &str,
        ) -> Result<Module, Box<EvalAltResult>> {
            self.resolve(engine, path, Default::default())
        }
    }

    impl ModuleResolver for FileModuleResolver {
        fn resolve(
            &self,
            engine: &Engine,
            path: &str,
            pos: Position,
        ) -> Result<Module, Box<EvalAltResult>> {
            // Construct the script file path
            let mut file_path = self.path.clone();
            file_path.push(path);
            file_path.set_extension(&self.extension); // Force extension

            let scope = Default::default();

            // See if it is cached
            let (module, ast) = {
                #[cfg(not(feature = "sync"))]
                let c = self.cache.borrow();
                #[cfg(feature = "sync")]
                let c = self.cache.read().unwrap();

                match c.get(&file_path) {
                    Some(ast) => (
                        Module::eval_ast_as_new(scope, ast, engine)
                            .map_err(|err| err.new_position(pos))?,
                        None,
                    ),
                    None => {
                        // Load the file and compile it if not found
                        let ast = engine
                            .compile_file(file_path.clone())
                            .map_err(|err| err.new_position(pos))?;

                        (
                            Module::eval_ast_as_new(scope, &ast, engine)
                                .map_err(|err| err.new_position(pos))?,
                            Some(ast),
                        )
                    }
                }
            };

            if let Some(ast) = ast {
                // Put it into the cache
                #[cfg(not(feature = "sync"))]
                self.cache.borrow_mut().insert(file_path, ast);
                #[cfg(feature = "sync")]
                self.cache.write().unwrap().insert(file_path, ast);
            }

            Ok(module)
        }
    }
}

/// Static module resolver.
#[cfg(not(feature = "no_module"))]
mod stat {
    use super::*;

    /// Module resolution service that serves modules added into it.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Engine, Module};
    /// use rhai::module_resolvers::StaticModuleResolver;
    ///
    /// let mut resolver = StaticModuleResolver::new();
    ///
    /// let module = Module::new();
    /// resolver.insert("hello".to_string(), module);
    ///
    /// let mut engine = Engine::new();
    /// engine.set_module_resolver(Some(resolver));
    /// ```
    #[derive(Debug, Clone, Default)]
    pub struct StaticModuleResolver(HashMap<String, Module>);

    impl StaticModuleResolver {
        /// Create a new `StaticModuleResolver`.
        ///
        /// # Examples
        ///
        /// ```
        /// use rhai::{Engine, Module};
        /// use rhai::module_resolvers::StaticModuleResolver;
        ///
        /// let mut resolver = StaticModuleResolver::new();
        ///
        /// let module = Module::new();
        /// resolver.insert("hello", module);
        ///
        /// let mut engine = Engine::new();
        /// engine.set_module_resolver(Some(resolver));
        /// ```
        pub fn new() -> Self {
            Default::default()
        }
    }

    impl StaticModuleResolver {
        /// Add a module keyed by its path.
        pub fn insert<S: Into<String>>(&mut self, path: S, mut module: Module) {
            module.index_all_sub_modules();
            self.0.insert(path.into(), module);
        }
        /// Remove a module given its path.
        pub fn remove(&mut self, path: &str) -> Option<Module> {
            self.0.remove(path)
        }
        /// Does the path exist?
        pub fn contains_path(&self, path: &str) -> bool {
            self.0.contains_key(path)
        }
        /// Get an iterator of all the modules.
        pub fn iter(&self) -> impl Iterator<Item = (&str, &Module)> {
            self.0.iter().map(|(k, v)| (k.as_str(), v))
        }
        /// Get a mutable iterator of all the modules.
        pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut Module)> {
            self.0.iter_mut().map(|(k, v)| (k.as_str(), v))
        }
        /// Get an iterator of all the module paths.
        pub fn paths(&self) -> impl Iterator<Item = &str> {
            self.0.keys().map(String::as_str)
        }
        /// Get an iterator of all the modules.
        pub fn values(&self) -> impl Iterator<Item = &Module> {
            self.0.values()
        }
        /// Get a mutable iterator of all the modules.
        pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Module> {
            self.0.values_mut()
        }
        /// Remove all modules.
        pub fn clear(&mut self) {
            self.0.clear();
        }
    }

    impl ModuleResolver for StaticModuleResolver {
        fn resolve(
            &self,
            _: &Engine,
            path: &str,
            pos: Position,
        ) -> Result<Module, Box<EvalAltResult>> {
            self.0
                .get(path)
                .cloned()
                .ok_or_else(|| EvalAltResult::ErrorModuleNotFound(path.into(), pos).into())
        }
    }
}

/// Module resolver collection.
#[cfg(not(feature = "no_module"))]
mod collection {
    use super::*;

    /// Module resolution service that holds a collection of module resolves,
    /// to be searched in sequential order.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Engine, Module};
    /// use rhai::module_resolvers::{StaticModuleResolver, ModuleResolversCollection};
    ///
    /// let mut collection = ModuleResolversCollection::new();
    ///
    /// let resolver = StaticModuleResolver::new();
    /// collection.push(resolver);
    ///
    /// let mut engine = Engine::new();
    /// engine.set_module_resolver(Some(collection));
    /// ```
    #[derive(Default)]
    pub struct ModuleResolversCollection(Vec<Box<dyn ModuleResolver>>);

    impl ModuleResolversCollection {
        /// Create a new `ModuleResolversCollection`.
        ///
        /// # Examples
        ///
        /// ```
        /// use rhai::{Engine, Module};
        /// use rhai::module_resolvers::{StaticModuleResolver, ModuleResolversCollection};
        ///
        /// let mut collection = ModuleResolversCollection::new();
        ///
        /// let resolver = StaticModuleResolver::new();
        /// collection.push(resolver);
        ///
        /// let mut engine = Engine::new();
        /// engine.set_module_resolver(Some(collection));
        /// ```
        pub fn new() -> Self {
            Default::default()
        }
    }

    impl ModuleResolversCollection {
        /// Add a module keyed by its path.
        pub fn push(&mut self, resolver: impl ModuleResolver + 'static) {
            self.0.push(Box::new(resolver));
        }
        /// Get an iterator of all the module resolvers.
        pub fn iter(&self) -> impl Iterator<Item = &dyn ModuleResolver> {
            self.0.iter().map(|v| v.as_ref())
        }
        /// Remove all module resolvers.
        pub fn clear(&mut self) {
            self.0.clear();
        }
    }

    impl ModuleResolver for ModuleResolversCollection {
        fn resolve(
            &self,
            engine: &Engine,
            path: &str,
            pos: Position,
        ) -> Result<Module, Box<EvalAltResult>> {
            for resolver in self.0.iter() {
                if let Ok(module) = resolver.resolve(engine, path, pos) {
                    return Ok(module);
                }
            }

            EvalAltResult::ErrorModuleNotFound(path.into(), pos).into()
        }
    }
}
