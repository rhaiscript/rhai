//! Module that defines the extern API of [`Engine`].

use crate::dynamic::Variant;
use crate::engine::{EvalContext, EvalState, Imports};
use crate::fn_call::FnCallArgs;
use crate::fn_native::SendSync;
use crate::fn_register::RegisterNativeFunction;
use crate::optimize::OptimizationLevel;
use crate::parse::ParseState;
use crate::{
    scope::Scope, Dynamic, Engine, EvalAltResult, FnAccess, FnNamespace, Identifier, Module,
    NativeCallContext, ParseError, Position, RhaiResult, Shared, AST,
};
use std::any::{type_name, TypeId};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_index"))]
use crate::Array;

#[cfg(not(feature = "no_object"))]
use crate::Map;

impl Engine {
    /// Get the global namespace module (which is the last module in `global_modules`).
    #[inline(always)]
    #[allow(dead_code)]
    pub(crate) fn global_namespace(&self) -> &Module {
        self.global_modules
            .first()
            .expect("global_modules contains at least one module")
    }
    /// Get a mutable reference to the global namespace module
    /// (which is the first module in `global_modules`).
    #[inline(always)]
    pub(crate) fn global_namespace_mut(&mut self) -> &mut Module {
        Shared::get_mut(
            self.global_modules
                .first_mut()
                .expect("global_modules contains at least one module"),
        )
        .expect("global namespace module is never shared")
    }
    /// Register a custom function with the [`Engine`].
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// // Normal function
    /// fn add(x: i64, y: i64) -> i64 {
    ///     x + y
    /// }
    ///
    /// let mut engine = Engine::new();
    ///
    /// engine.register_fn("add", add);
    ///
    /// assert_eq!(engine.eval::<i64>("add(40, 2)")?, 42);
    ///
    /// // You can also register a closure.
    /// engine.register_fn("sub", |x: i64, y: i64| x - y );
    ///
    /// assert_eq!(engine.eval::<i64>("sub(44, 2)")?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn register_fn<N, A, F>(&mut self, name: N, func: F) -> &mut Self
    where
        N: AsRef<str> + Into<Identifier>,
        F: RegisterNativeFunction<A, ()>,
    {
        let param_types = F::param_types();

        #[cfg(feature = "metadata")]
        let mut param_type_names: crate::StaticVec<_> = F::param_names()
            .iter()
            .map(|ty| format!("_: {}", self.map_type_name(ty)))
            .collect();

        #[cfg(feature = "metadata")]
        if F::return_type() != TypeId::of::<()>() {
            param_type_names.push(self.map_type_name(F::return_type_name()).into());
        }

        #[cfg(feature = "metadata")]
        let param_type_names: Option<crate::StaticVec<_>> =
            Some(param_type_names.iter().map(|ty| ty.as_str()).collect());

        #[cfg(not(feature = "metadata"))]
        let param_type_names: Option<[&str; 0]> = None;

        self.global_namespace_mut().set_fn(
            name,
            FnNamespace::Global,
            FnAccess::Public,
            param_type_names.as_ref().map(|v| v.as_ref()),
            &param_types,
            func.into_callable_function(),
        );
        self
    }
    /// Register a custom fallible function with the [`Engine`].
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Engine, EvalAltResult};
    ///
    /// // Normal function
    /// fn div(x: i64, y: i64) -> Result<i64, Box<EvalAltResult>> {
    ///     if y == 0 {
    ///         // '.into()' automatically converts to 'Box<EvalAltResult::ErrorRuntime>'
    ///         Err("division by zero!".into())
    ///     } else {
    ///         Ok(x / y)
    ///     }
    /// }
    ///
    /// let mut engine = Engine::new();
    ///
    /// engine.register_result_fn("div", div);
    ///
    /// engine.eval::<i64>("div(42, 0)")
    ///       .expect_err("expecting division by zero error!");
    /// ```
    #[inline]
    pub fn register_result_fn<N, A, F, R>(&mut self, name: N, func: F) -> &mut Self
    where
        N: AsRef<str> + Into<Identifier>,
        F: RegisterNativeFunction<A, Result<R, Box<EvalAltResult>>>,
    {
        let param_types = F::param_types();

        #[cfg(feature = "metadata")]
        let param_type_names: crate::StaticVec<_> = F::param_names()
            .iter()
            .map(|ty| format!("_: {}", self.map_type_name(ty)))
            .chain(std::iter::once(
                self.map_type_name(F::return_type_name()).into(),
            ))
            .collect();

        #[cfg(feature = "metadata")]
        let param_type_names: Option<crate::StaticVec<_>> =
            Some(param_type_names.iter().map(|ty| ty.as_str()).collect());

        #[cfg(not(feature = "metadata"))]
        let param_type_names: Option<[&str; 0]> = None;

        self.global_namespace_mut().set_fn(
            name,
            FnNamespace::Global,
            FnAccess::Public,
            param_type_names.as_ref().map(|v| v.as_ref()),
            &param_types,
            func.into_callable_function(),
        );
        self
    }
    /// Register a function of the [`Engine`].
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.  It takes a list of [`TypeId`][std::any::TypeId]'s indicating the actual types of the parameters.
    ///
    /// Arguments are simply passed in as a mutable array of [`&mut Dynamic`][Dynamic],
    /// The arguments are guaranteed to be of the correct types matching the [`TypeId`][std::any::TypeId]'s.
    ///
    /// To access a primary argument value (i.e. cloning is cheap), use: `args[n].as_xxx().unwrap()`
    ///
    /// To access an argument value and avoid cloning, use `std::mem::take(args[n]).cast::<T>()`.
    /// Notice that this will _consume_ the argument, replacing it with `()`.
    ///
    /// To access the first mutable parameter, use `args.get_mut(0).unwrap()`
    #[deprecated = "this function is volatile and may change"]
    #[inline(always)]
    pub fn register_raw_fn<N, T>(
        &mut self,
        name: N,
        arg_types: &[TypeId],
        func: impl Fn(NativeCallContext, &mut FnCallArgs) -> Result<T, Box<EvalAltResult>>
            + SendSync
            + 'static,
    ) -> &mut Self
    where
        N: AsRef<str> + Into<Identifier>,
        T: Variant + Clone,
    {
        self.global_namespace_mut().set_raw_fn(
            name,
            FnNamespace::Global,
            FnAccess::Public,
            arg_types,
            func,
        );
        self
    }
    /// Register a custom type for use with the [`Engine`].
    /// The type must implement [`Clone`].
    ///
    /// # Example
    ///
    /// ```
    /// #[derive(Debug, Clone, Eq, PartialEq)]
    /// struct TestStruct {
    ///     field: i64
    /// }
    ///
    /// impl TestStruct {
    ///     fn new() -> Self { Self { field: 1 } }
    ///     fn update(&mut self, offset: i64) { self.field += offset; }
    /// }
    ///
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register API for the custom type.
    /// engine
    ///     .register_type::<TestStruct>()
    ///     .register_fn("new_ts", TestStruct::new)
    ///     // Use `register_fn` to register methods on the type.
    ///     .register_fn("update", TestStruct::update);
    ///
    /// # #[cfg(not(feature = "no_object"))]
    /// assert_eq!(
    ///     engine.eval::<TestStruct>("let x = new_ts(); x.update(41); x")?,
    ///     TestStruct { field: 42 }
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn register_type<T: Variant + Clone>(&mut self) -> &mut Self {
        self.register_type_with_name::<T>(type_name::<T>())
    }
    /// Register a custom type for use with the [`Engine`], with a pretty-print name
    /// for the `type_of` function. The type must implement [`Clone`].
    ///
    /// # Example
    ///
    /// ```
    /// #[derive(Clone)]
    /// struct TestStruct {
    ///     field: i64
    /// }
    ///
    /// impl TestStruct {
    ///     fn new() -> Self { Self { field: 1 } }
    /// }
    ///
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register API for the custom type.
    /// engine
    ///     .register_type::<TestStruct>()
    ///     .register_fn("new_ts", TestStruct::new);
    ///
    /// assert_eq!(
    ///     engine.eval::<String>("let x = new_ts(); type_of(x)")?,
    ///     "rust_out::TestStruct"
    /// );
    ///
    /// // Re-register the custom type with a name.
    /// engine.register_type_with_name::<TestStruct>("Hello");
    ///
    /// assert_eq!(
    ///     engine.eval::<String>("let x = new_ts(); type_of(x)")?,
    ///     "Hello"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn register_type_with_name<T: Variant + Clone>(&mut self, name: &str) -> &mut Self {
        // Add the pretty-print type name into the map
        self.type_names
            .insert(type_name::<T>().into(), Box::new(name.into()));
        self
    }
    /// Register an type iterator for an iterable type with the [`Engine`].
    /// This is an advanced feature.
    #[inline(always)]
    pub fn register_iterator<T>(&mut self) -> &mut Self
    where
        T: Variant + Clone + IntoIterator,
        <T as IntoIterator>::Item: Variant + Clone,
    {
        self.global_namespace_mut().set_iterable::<T>();
        self
    }
    /// Register a getter function for a member of a registered type with the [`Engine`].
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under `no_object`.
    ///
    /// # Example
    ///
    /// ```
    /// #[derive(Clone)]
    /// struct TestStruct {
    ///     field: i64
    /// }
    ///
    /// impl TestStruct {
    ///     fn new() -> Self { Self { field: 1 } }
    ///     // Even a getter must start with `&mut self` and not `&self`.
    ///     fn get_field(&mut self) -> i64  { self.field }
    /// }
    ///
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register API for the custom type.
    /// engine
    ///     .register_type::<TestStruct>()
    ///     .register_fn("new_ts", TestStruct::new)
    ///     // Register a getter on a property (notice it doesn't have to be the same name).
    ///     .register_get("xyz", TestStruct::get_field);
    ///
    /// assert_eq!(engine.eval::<i64>("let a = new_ts(); a.xyz")?, 1);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn register_get<T: Variant + Clone, V: Variant + Clone>(
        &mut self,
        name: &str,
        get_fn: impl Fn(&mut T) -> V + SendSync + 'static,
    ) -> &mut Self {
        self.register_fn(&crate::engine::make_getter(name), get_fn)
    }
    /// Register a getter function for a member of a registered type with the [`Engine`].
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under `no_object`.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Engine, Dynamic, EvalAltResult};
    ///
    /// #[derive(Clone)]
    /// struct TestStruct {
    ///     field: i64
    /// }
    ///
    /// impl TestStruct {
    ///     fn new() -> Self { Self { field: 1 } }
    ///     // Even a getter must start with `&mut self` and not `&self`.
    ///     fn get_field(&mut self) -> Result<i64, Box<EvalAltResult>> {
    ///         Ok(self.field)
    ///     }
    /// }
    ///
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// let mut engine = Engine::new();
    ///
    /// // Register API for the custom type.
    /// engine
    ///     .register_type::<TestStruct>()
    ///     .register_fn("new_ts", TestStruct::new)
    ///     // Register a getter on a property (notice it doesn't have to be the same name).
    ///     .register_get_result("xyz", TestStruct::get_field);
    ///
    /// assert_eq!(engine.eval::<i64>("let a = new_ts(); a.xyz")?, 1);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn register_get_result<T: Variant + Clone, V: Variant + Clone>(
        &mut self,
        name: &str,
        get_fn: impl Fn(&mut T) -> Result<V, Box<EvalAltResult>> + SendSync + 'static,
    ) -> &mut Self {
        self.register_result_fn(&crate::engine::make_getter(name), get_fn)
    }
    /// Register a setter function for a member of a registered type with the [`Engine`].
    ///
    /// Not available under `no_object`.
    ///
    /// # Example
    ///
    /// ```
    /// #[derive(Debug, Clone, Eq, PartialEq)]
    /// struct TestStruct {
    ///     field: i64
    /// }
    ///
    /// impl TestStruct {
    ///     fn new() -> Self { Self { field: 1 } }
    ///     fn set_field(&mut self, new_val: i64) { self.field = new_val; }
    /// }
    ///
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register API for the custom type.
    /// engine
    ///     .register_type::<TestStruct>()
    ///     .register_fn("new_ts", TestStruct::new)
    ///     // Register a setter on a property (notice it doesn't have to be the same name)
    ///     .register_set("xyz", TestStruct::set_field);
    ///
    /// // Notice that, with a getter, there is no way to get the property value
    /// assert_eq!(
    ///     engine.eval::<TestStruct>("let a = new_ts(); a.xyz = 42; a")?,
    ///     TestStruct { field: 42 }
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn register_set<T: Variant + Clone, V: Variant + Clone>(
        &mut self,
        name: &str,
        set_fn: impl Fn(&mut T, V) + SendSync + 'static,
    ) -> &mut Self {
        self.register_fn(&crate::engine::make_setter(name), set_fn)
    }
    /// Register a setter function for a member of a registered type with the [`Engine`].
    ///
    /// Not available under `no_object`.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Engine, Dynamic, EvalAltResult};
    ///
    /// #[derive(Debug, Clone, Eq, PartialEq)]
    /// struct TestStruct {
    ///     field: i64
    /// }
    ///
    /// impl TestStruct {
    ///     fn new() -> Self { Self { field: 1 } }
    ///     fn set_field(&mut self, new_val: i64) -> Result<(), Box<EvalAltResult>> {
    ///         self.field = new_val;
    ///         Ok(())
    ///     }
    /// }
    ///
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// let mut engine = Engine::new();
    ///
    /// // Register API for the custom type.
    /// engine
    ///     .register_type::<TestStruct>()
    ///     .register_fn("new_ts", TestStruct::new)
    ///     // Register a setter on a property (notice it doesn't have to be the same name)
    ///     .register_set_result("xyz", TestStruct::set_field);
    ///
    /// // Notice that, with a getter, there is no way to get the property value
    /// assert_eq!(
    ///     engine.eval::<TestStruct>("let a = new_ts(); a.xyz = 42; a")?,
    ///     TestStruct { field: 42 }
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn register_set_result<T: Variant + Clone, V: Variant + Clone>(
        &mut self,
        name: &str,
        set_fn: impl Fn(&mut T, V) -> Result<(), Box<EvalAltResult>> + SendSync + 'static,
    ) -> &mut Self {
        self.register_result_fn(&crate::engine::make_setter(name), set_fn)
    }
    /// Short-hand for registering both getter and setter functions
    /// of a registered type with the [`Engine`].
    ///
    /// All function signatures must start with `&mut self` and not `&self`.
    ///
    /// Not available under `no_object`.
    ///
    /// # Example
    ///
    /// ```
    /// #[derive(Clone)]
    /// struct TestStruct {
    ///     field: i64
    /// }
    ///
    /// impl TestStruct {
    ///     fn new() -> Self { Self { field: 1 } }
    ///     // Even a getter must start with `&mut self` and not `&self`.
    ///     fn get_field(&mut self) -> i64 { self.field }
    ///     fn set_field(&mut self, new_val: i64) { self.field = new_val; }
    /// }
    ///
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register API for the custom type.
    /// engine
    ///     .register_type::<TestStruct>()
    ///     .register_fn("new_ts", TestStruct::new)
    ///     // Register both a getter and a setter on a property
    ///     // (notice it doesn't have to be the same name)
    ///     .register_get_set("xyz", TestStruct::get_field, TestStruct::set_field);
    ///
    /// assert_eq!(engine.eval::<i64>("let a = new_ts(); a.xyz = 42; a.xyz")?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn register_get_set<T: Variant + Clone, V: Variant + Clone>(
        &mut self,
        name: &str,
        get_fn: impl Fn(&mut T) -> V + SendSync + 'static,
        set_fn: impl Fn(&mut T, V) + SendSync + 'static,
    ) -> &mut Self {
        self.register_get(name, get_fn).register_set(name, set_fn)
    }
    /// Register an index getter for a custom type with the [`Engine`].
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`], [`Map`], [`String`],
    /// [`ImmutableString`][crate::ImmutableString], `&str` or [`INT`][crate::INT].
    /// Indexers for arrays, object maps, strings and integers cannot be registered.
    ///
    /// # Example
    ///
    /// ```
    /// #[derive(Clone)]
    /// struct TestStruct {
    ///     fields: Vec<i64>
    /// }
    ///
    /// impl TestStruct {
    ///     fn new() -> Self { Self { fields: vec![1, 2, 3, 4, 5] } }
    ///     // Even a getter must start with `&mut self` and not `&self`.
    ///     fn get_field(&mut self, index: i64) -> i64 { self.fields[index as usize] }
    /// }
    ///
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register API for the custom type.
    /// # #[cfg(not(feature = "no_object"))]
    /// engine.register_type::<TestStruct>();
    ///
    /// engine
    ///     .register_fn("new_ts", TestStruct::new)
    ///     // Register an indexer.
    ///     .register_indexer_get(TestStruct::get_field);
    ///
    /// # #[cfg(not(feature = "no_index"))]
    /// assert_eq!(engine.eval::<i64>("let a = new_ts(); a[2]")?, 3);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline]
    pub fn register_indexer_get<T: Variant + Clone, X: Variant + Clone, V: Variant + Clone>(
        &mut self,
        get_fn: impl Fn(&mut T, X) -> V + SendSync + 'static,
    ) -> &mut Self {
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<T>() == TypeId::of::<Array>() {
            panic!("Cannot register indexer for arrays.");
        }
        #[cfg(not(feature = "no_object"))]
        if TypeId::of::<T>() == TypeId::of::<Map>() {
            panic!("Cannot register indexer for object maps.");
        }
        if TypeId::of::<T>() == TypeId::of::<String>()
            || TypeId::of::<T>() == TypeId::of::<&str>()
            || TypeId::of::<T>() == TypeId::of::<crate::ImmutableString>()
        {
            panic!("Cannot register indexer for strings.");
        }
        if TypeId::of::<T>() == TypeId::of::<crate::INT>() {
            panic!("Cannot register indexer for integers.");
        }

        self.register_fn(crate::engine::FN_IDX_GET, get_fn)
    }
    /// Register an index getter for a custom type with the [`Engine`].
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`], [`Map`], [`String`],
    /// [`ImmutableString`][crate::ImmutableString], `&str` or [`INT`][crate::INT].
    /// Indexers for arrays, object maps, strings and integers cannot be registered.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Engine, Dynamic, EvalAltResult};
    ///
    /// #[derive(Clone)]
    /// struct TestStruct {
    ///     fields: Vec<i64>
    /// }
    ///
    /// impl TestStruct {
    ///     fn new() -> Self { Self { fields: vec![1, 2, 3, 4, 5] } }
    ///     // Even a getter must start with `&mut self` and not `&self`.
    ///     fn get_field(&mut self, index: i64) -> Result<i64, Box<EvalAltResult>> {
    ///         Ok(self.fields[index as usize])
    ///     }
    /// }
    ///
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// let mut engine = Engine::new();
    ///
    /// // Register API for the custom type.
    /// # #[cfg(not(feature = "no_object"))]
    /// engine.register_type::<TestStruct>();
    ///
    /// engine
    ///     .register_fn("new_ts", TestStruct::new)
    ///     // Register an indexer.
    ///     .register_indexer_get_result(TestStruct::get_field);
    ///
    /// # #[cfg(not(feature = "no_index"))]
    /// assert_eq!(engine.eval::<i64>("let a = new_ts(); a[2]")?, 3);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline]
    pub fn register_indexer_get_result<
        T: Variant + Clone,
        X: Variant + Clone,
        V: Variant + Clone,
    >(
        &mut self,
        get_fn: impl Fn(&mut T, X) -> Result<V, Box<EvalAltResult>> + SendSync + 'static,
    ) -> &mut Self {
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<T>() == TypeId::of::<Array>() {
            panic!("Cannot register indexer for arrays.");
        }
        #[cfg(not(feature = "no_object"))]
        if TypeId::of::<T>() == TypeId::of::<Map>() {
            panic!("Cannot register indexer for object maps.");
        }
        if TypeId::of::<T>() == TypeId::of::<String>()
            || TypeId::of::<T>() == TypeId::of::<&str>()
            || TypeId::of::<T>() == TypeId::of::<crate::ImmutableString>()
        {
            panic!("Cannot register indexer for strings.");
        }
        if TypeId::of::<T>() == TypeId::of::<crate::INT>() {
            panic!("Cannot register indexer for integers.");
        }

        self.register_result_fn(crate::engine::FN_IDX_GET, get_fn)
    }
    /// Register an index setter for a custom type with the [`Engine`].
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`], [`Map`], [`String`],
    /// [`ImmutableString`][crate::ImmutableString], `&str` or [`INT`][crate::INT].
    /// Indexers for arrays, object maps, strings and integers cannot be registered.
    ///
    /// # Example
    ///
    /// ```
    /// #[derive(Clone)]
    /// struct TestStruct {
    ///     fields: Vec<i64>
    /// }
    ///
    /// impl TestStruct {
    ///     fn new() -> Self { Self { fields: vec![1, 2, 3, 4, 5] } }
    ///     fn set_field(&mut self, index: i64, value: i64) { self.fields[index as usize] = value; }
    /// }
    ///
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register API for the custom type.
    /// # #[cfg(not(feature = "no_object"))]
    /// engine.register_type::<TestStruct>();
    ///
    /// engine
    ///     .register_fn("new_ts", TestStruct::new)
    ///     // Register an indexer.
    ///     .register_indexer_set(TestStruct::set_field);
    ///
    /// # #[cfg(not(feature = "no_index"))]
    /// assert_eq!(
    ///     engine.eval::<TestStruct>("let a = new_ts(); a[2] = 42; a")?.fields[2],
    ///     42
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline]
    pub fn register_indexer_set<T: Variant + Clone, X: Variant + Clone, V: Variant + Clone>(
        &mut self,
        set_fn: impl Fn(&mut T, X, V) + SendSync + 'static,
    ) -> &mut Self {
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<T>() == TypeId::of::<Array>() {
            panic!("Cannot register indexer for arrays.");
        }
        #[cfg(not(feature = "no_object"))]
        if TypeId::of::<T>() == TypeId::of::<Map>() {
            panic!("Cannot register indexer for object maps.");
        }
        if TypeId::of::<T>() == TypeId::of::<String>()
            || TypeId::of::<T>() == TypeId::of::<&str>()
            || TypeId::of::<T>() == TypeId::of::<crate::ImmutableString>()
        {
            panic!("Cannot register indexer for strings.");
        }
        if TypeId::of::<T>() == TypeId::of::<crate::INT>() {
            panic!("Cannot register indexer for integers.");
        }

        self.register_fn(crate::engine::FN_IDX_SET, set_fn)
    }
    /// Register an index setter for a custom type with the [`Engine`].
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`], [`Map`], [`String`],
    /// [`ImmutableString`][crate::ImmutableString], `&str` or [`INT`][crate::INT].
    /// Indexers for arrays, object maps, strings and integers cannot be registered.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Engine, Dynamic, EvalAltResult};
    ///
    /// #[derive(Clone)]
    /// struct TestStruct {
    ///     fields: Vec<i64>
    /// }
    ///
    /// impl TestStruct {
    ///     fn new() -> Self { Self { fields: vec![1, 2, 3, 4, 5] } }
    ///     fn set_field(&mut self, index: i64, value: i64) -> Result<(), Box<EvalAltResult>> {
    ///         self.fields[index as usize] = value;
    ///         Ok(())
    ///     }
    /// }
    ///
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// let mut engine = Engine::new();
    ///
    /// // Register API for the custom type.
    /// # #[cfg(not(feature = "no_object"))]
    /// engine.register_type::<TestStruct>();
    ///
    /// engine
    ///     .register_fn("new_ts", TestStruct::new)
    ///     // Register an indexer.
    ///     .register_indexer_set_result(TestStruct::set_field);
    ///
    /// # #[cfg(not(feature = "no_index"))]
    /// assert_eq!(
    ///     engine.eval::<TestStruct>("let a = new_ts(); a[2] = 42; a")?.fields[2],
    ///     42
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline]
    pub fn register_indexer_set_result<
        T: Variant + Clone,
        X: Variant + Clone,
        V: Variant + Clone,
    >(
        &mut self,
        set_fn: impl Fn(&mut T, X, V) -> Result<(), Box<EvalAltResult>> + SendSync + 'static,
    ) -> &mut Self {
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<T>() == TypeId::of::<Array>() {
            panic!("Cannot register indexer for arrays.");
        }
        #[cfg(not(feature = "no_object"))]
        if TypeId::of::<T>() == TypeId::of::<Map>() {
            panic!("Cannot register indexer for object maps.");
        }
        if TypeId::of::<T>() == TypeId::of::<String>()
            || TypeId::of::<T>() == TypeId::of::<&str>()
            || TypeId::of::<T>() == TypeId::of::<crate::ImmutableString>()
        {
            panic!("Cannot register indexer for strings.");
        }
        if TypeId::of::<T>() == TypeId::of::<crate::INT>() {
            panic!("Cannot register indexer for integers.");
        }

        self.register_result_fn(crate::engine::FN_IDX_SET, set_fn)
    }
    /// Short-hand for registering both index getter and setter functions for a custom type with the [`Engine`].
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`], [`Map`], [`String`],
    /// [`ImmutableString`][crate::ImmutableString], `&str` or [`INT`][crate::INT].
    /// Indexers for arrays, object maps, strings and integers cannot be registered.
    ///
    /// # Example
    ///
    /// ```
    /// #[derive(Clone)]
    /// struct TestStruct {
    ///     fields: Vec<i64>
    /// }
    ///
    /// impl TestStruct {
    ///     fn new() -> Self { Self { fields: vec![1, 2, 3, 4, 5] } }
    ///     // Even a getter must start with `&mut self` and not `&self`.
    ///     fn get_field(&mut self, index: i64) -> i64 { self.fields[index as usize] }
    ///     fn set_field(&mut self, index: i64, value: i64) { self.fields[index as usize] = value; }
    /// }
    ///
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register API for the custom type.
    /// # #[cfg(not(feature = "no_object"))]
    /// engine.register_type::<TestStruct>();
    ///
    /// engine
    ///     .register_fn("new_ts", TestStruct::new)
    ///     // Register an indexer.
    ///     .register_indexer_get_set(TestStruct::get_field, TestStruct::set_field);
    ///
    /// # #[cfg(not(feature = "no_index"))]
    /// assert_eq!(engine.eval::<i64>("let a = new_ts(); a[2] = 42; a[2]")?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline(always)]
    pub fn register_indexer_get_set<T: Variant + Clone, X: Variant + Clone, V: Variant + Clone>(
        &mut self,
        get_fn: impl Fn(&mut T, X) -> V + SendSync + 'static,
        set_fn: impl Fn(&mut T, X, V) + SendSync + 'static,
    ) -> &mut Self {
        self.register_indexer_get(get_fn)
            .register_indexer_set(set_fn)
    }
    /// Register a shared [`Module`] into the global namespace of [`Engine`].
    ///
    /// All functions and type iterators are automatically available to scripts without namespace
    /// qualifications.
    ///
    /// Sub-modules and variables are **ignored**.
    ///
    /// When searching for functions, modules loaded later are preferred. In other words, loaded
    /// modules are searched in reverse order.
    #[inline(always)]
    pub fn register_global_module(&mut self, module: Shared<Module>) -> &mut Self {
        // Insert the module into the front.
        // The first module is always the global namespace.
        self.global_modules.insert(1, module);
        self
    }
    /// Register a shared [`Module`] as a static module namespace with the [`Engine`].
    ///
    /// Functions marked [`FnNamespace::Global`] and type iterators are exposed to scripts without
    /// namespace qualifications.
    ///
    /// Not available under `no_module`.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Shared, Module};
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Create the module
    /// let mut module = Module::new();
    /// module.set_native_fn("calc", |x: i64| Ok(x + 1));
    ///
    /// let module: Shared<Module> = module.into();
    ///
    /// engine
    ///     // Register the module as a fixed sub-module
    ///     .register_static_module("foo::bar::baz", module.clone())
    ///     // Multiple registrations to the same partial path is also OK!
    ///     .register_static_module("foo::bar::hello", module.clone())
    ///     .register_static_module("CalcService", module);
    ///
    /// assert_eq!(engine.eval::<i64>("foo::bar::baz::calc(41)")?, 42);
    /// assert_eq!(engine.eval::<i64>("foo::bar::hello::calc(41)")?, 42);
    /// assert_eq!(engine.eval::<i64>("CalcService::calc(41)")?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_module"))]
    pub fn register_static_module(
        &mut self,
        name: impl AsRef<str> + Into<Identifier>,
        module: Shared<Module>,
    ) -> &mut Self {
        fn register_static_module_raw(
            root: &mut std::collections::BTreeMap<Identifier, Shared<Module>>,
            name: impl AsRef<str> + Into<Identifier>,
            module: Shared<Module>,
        ) {
            let separator = crate::token::Token::DoubleColon.syntax();

            if !name.as_ref().contains(separator.as_ref()) {
                if !module.is_indexed() {
                    // Index the module (making a clone copy if necessary) if it is not indexed
                    let mut module = crate::fn_native::shared_take_or_clone(module);
                    module.build_index();
                    root.insert(name.into(), module.into());
                } else {
                    root.insert(name.into(), module);
                }
            } else {
                let mut iter = name.as_ref().splitn(2, separator.as_ref());
                let sub_module = iter.next().expect("name contains separator").trim();
                let remainder = iter.next().expect("name contains separator").trim();

                if !root.contains_key(sub_module) {
                    let mut m = Module::new();
                    register_static_module_raw(m.sub_modules_mut(), remainder, module);
                    m.build_index();
                    root.insert(sub_module.into(), m.into());
                } else {
                    let m = root
                        .remove(sub_module)
                        .expect("root contains the sub-module");
                    let mut m = crate::fn_native::shared_take_or_clone(m);
                    register_static_module_raw(m.sub_modules_mut(), remainder, module);
                    m.build_index();
                    root.insert(sub_module.into(), m.into());
                }
            }
        }

        register_static_module_raw(&mut self.global_sub_modules, name, module);
        self
    }

    /// Compile a string into an [`AST`], which can be used later for evaluation.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// // Compile a script to an AST and store it for later evaluation
    /// let ast = engine.compile("40 + 2")?;
    ///
    /// for _ in 0..42 {
    ///     assert_eq!(engine.eval_ast::<i64>(&ast)?, 42);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn compile(&self, script: &str) -> Result<AST, ParseError> {
        self.compile_with_scope(&Default::default(), script)
    }
    /// Compile a string into an [`AST`] using own scope, which can be used later for evaluation.
    ///
    /// The scope is useful for passing constants into the script for optimization
    /// when using [`OptimizationLevel::Full`].
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # #[cfg(not(feature = "no_optimize"))]
    /// # {
    /// use rhai::{Engine, Scope, OptimizationLevel};
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Set optimization level to 'Full' so the Engine can fold constants
    /// // into function calls and operators.
    /// engine.set_optimization_level(OptimizationLevel::Full);
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push_constant("x", 42_i64);   // 'x' is a constant
    ///
    /// // Compile a script to an AST and store it for later evaluation.
    /// // Notice that `Full` optimization is on, so constants are folded
    /// // into function calls and operators.
    /// let ast = engine.compile_with_scope(&mut scope,
    ///             "if x > 40 { x } else { 0 }"    // all 'x' are replaced with 42
    /// )?;
    ///
    /// // Normally this would have failed because no scope is passed into the 'eval_ast'
    /// // call and so the variable 'x' does not exist.  Here, it passes because the script
    /// // has been optimized and all references to 'x' are already gone.
    /// assert_eq!(engine.eval_ast::<i64>(&ast)?, 42);
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn compile_with_scope(&self, scope: &Scope, script: &str) -> Result<AST, ParseError> {
        self.compile_scripts_with_scope(scope, &[script])
    }
    /// Compile a string into an [`AST`] using own scope, which can be used later for evaluation,
    /// embedding all imported modules.
    ///
    /// Not available under `no_module`.
    ///
    /// Modules referred by `import` statements containing literal string paths are eagerly resolved
    /// via the current [module resolver][crate::ModuleResolver] and embedded into the resultant
    /// [`AST`]. When it is evaluated later, `import` statement directly recall pre-resolved
    /// [modules][Module] and the resolution process is not performed again.
    #[cfg(not(feature = "no_module"))]
    pub fn compile_into_self_contained(
        &self,
        scope: &Scope,
        script: &str,
    ) -> Result<AST, Box<EvalAltResult>> {
        use crate::{
            ast::{ASTNode, Expr, Stmt},
            fn_native::shared_take_or_clone,
            module::resolvers::StaticModuleResolver,
        };
        use std::collections::BTreeSet;

        fn collect_imports(
            ast: &AST,
            resolver: &StaticModuleResolver,
            imports: &mut BTreeSet<Identifier>,
        ) {
            ast.walk(
                &mut |path| match path.last().expect("`path` contains the current node") {
                    // Collect all `import` statements with a string constant path
                    ASTNode::Stmt(Stmt::Import(Expr::StringConstant(s, _), _, _))
                        if !resolver.contains_path(s) && !imports.contains(s.as_str()) =>
                    {
                        imports.insert(s.clone().into());
                        true
                    }
                    _ => true,
                },
            );
        }

        let mut ast = self.compile_scripts_with_scope(scope, &[script])?;

        if let Some(ref module_resolver) = self.module_resolver {
            let mut resolver = StaticModuleResolver::new();
            let mut imports = Default::default();

            collect_imports(&ast, &resolver, &mut imports);

            if !imports.is_empty() {
                while let Some(path) = imports.iter().next() {
                    let path = path.clone();

                    match module_resolver.resolve_ast(self, None, &path, Position::NONE) {
                        Some(Ok(module_ast)) => {
                            collect_imports(&module_ast, &resolver, &mut imports)
                        }
                        Some(err) => return err,
                        None => (),
                    }

                    let module = module_resolver.resolve(self, None, &path, Position::NONE)?;
                    let module = shared_take_or_clone(module);

                    imports.remove(&path);
                    resolver.insert(path, module);
                }
                ast.set_resolver(resolver);
            }
        }

        Ok(ast)
    }
    /// When passed a list of strings, first join the strings into one large script,
    /// and then compile them into an [`AST`] using own scope, which can be used later for evaluation.
    ///
    /// The scope is useful for passing constants into the script for optimization
    /// when using [`OptimizationLevel::Full`].
    ///
    /// ## Note
    ///
    /// All strings are simply parsed one after another with nothing inserted in between, not even
    /// a newline or space.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # #[cfg(not(feature = "no_optimize"))]
    /// # {
    /// use rhai::{Engine, Scope, OptimizationLevel};
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Set optimization level to 'Full' so the Engine can fold constants
    /// // into function calls and operators.
    /// engine.set_optimization_level(OptimizationLevel::Full);
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push_constant("x", 42_i64);   // 'x' is a constant
    ///
    /// // Compile a script made up of script segments to an AST and store it for later evaluation.
    /// // Notice that `Full` optimization is on, so constants are folded
    /// // into function calls and operators.
    /// let ast = engine.compile_scripts_with_scope(&mut scope, &[
    ///             "if x > 40",            // all 'x' are replaced with 42
    ///             "{ x } el",
    ///             "se { 0 }"              // segments do not need to be valid scripts!
    /// ])?;
    ///
    /// // Normally this would have failed because no scope is passed into the 'eval_ast'
    /// // call and so the variable 'x' does not exist.  Here, it passes because the script
    /// // has been optimized and all references to 'x' are already gone.
    /// assert_eq!(engine.eval_ast::<i64>(&ast)?, 42);
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn compile_scripts_with_scope(
        &self,
        scope: &Scope,
        scripts: &[&str],
    ) -> Result<AST, ParseError> {
        self.compile_with_scope_and_optimization_level(scope, scripts, self.optimization_level)
    }
    /// Join a list of strings and compile into an [`AST`] using own scope at a specific optimization level.
    #[inline]
    pub(crate) fn compile_with_scope_and_optimization_level(
        &self,
        scope: &Scope,
        scripts: &[&str],
        optimization_level: OptimizationLevel,
    ) -> Result<AST, ParseError> {
        let (stream, tokenizer_control) =
            self.lex_raw(scripts, self.token_mapper.as_ref().map(Box::as_ref));
        let mut state = ParseState::new(self, tokenizer_control);
        self.parse(
            &mut stream.peekable(),
            &mut state,
            scope,
            optimization_level,
        )
    }
    /// Read the contents of a file into a string.
    #[cfg(not(feature = "no_std"))]
    #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
    fn read_file(path: std::path::PathBuf) -> Result<String, Box<EvalAltResult>> {
        use std::io::Read;

        let mut f = std::fs::File::open(path.clone()).map_err(|err| {
            EvalAltResult::ErrorSystem(
                format!("Cannot open script file '{}'", path.to_string_lossy()),
                err.into(),
            )
        })?;

        let mut contents = String::new();

        f.read_to_string(&mut contents).map_err(|err| {
            EvalAltResult::ErrorSystem(
                format!("Cannot read script file '{}'", path.to_string_lossy()),
                err.into(),
            )
        })?;

        if contents.starts_with("#!") {
            // Remove shebang
            if let Some(n) = contents.find('\n') {
                contents.drain(0..n).count();
            } else {
                contents.clear();
            }
        };

        Ok(contents)
    }
    /// Compile a script file into an [`AST`], which can be used later for evaluation.
    ///
    /// Not available under `no_std` or `WASM`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// // Compile a script file to an AST and store it for later evaluation.
    /// // Notice that a PathBuf is required which can easily be constructed from a string.
    /// let ast = engine.compile_file("script.rhai".into())?;
    ///
    /// for _ in 0..42 {
    ///     engine.eval_ast::<i64>(&ast)?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_std"))]
    #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
    #[inline(always)]
    pub fn compile_file(&self, path: std::path::PathBuf) -> Result<AST, Box<EvalAltResult>> {
        self.compile_file_with_scope(&Default::default(), path)
    }
    /// Compile a script file into an [`AST`] using own scope, which can be used later for evaluation.
    ///
    /// Not available under `no_std` or `WASM`.
    ///
    /// The scope is useful for passing constants into the script for optimization
    /// when using [`OptimizationLevel::Full`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # #[cfg(not(feature = "no_optimize"))]
    /// # {
    /// use rhai::{Engine, Scope, OptimizationLevel};
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Set optimization level to 'Full' so the Engine can fold constants.
    /// engine.set_optimization_level(OptimizationLevel::Full);
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push_constant("x", 42_i64);   // 'x' is a constant
    ///
    /// // Compile a script to an AST and store it for later evaluation.
    /// // Notice that a PathBuf is required which can easily be constructed from a string.
    /// let ast = engine.compile_file_with_scope(&mut scope, "script.rhai".into())?;
    ///
    /// let result = engine.eval_ast::<i64>(&ast)?;
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_std"))]
    #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
    #[inline(always)]
    pub fn compile_file_with_scope(
        &self,
        scope: &Scope,
        path: std::path::PathBuf,
    ) -> Result<AST, Box<EvalAltResult>> {
        Self::read_file(path).and_then(|contents| Ok(self.compile_with_scope(scope, &contents)?))
    }
    /// Parse a JSON string into an [object map][`Map`].
    /// This is a light-weight alternative to using, say,
    /// [`serde_json`](https://crates.io/crates/serde_json) to deserialize the JSON.
    ///
    /// Not available under `no_object`.
    ///
    /// The JSON string must be an object hash.  It cannot be a simple scalar value.
    ///
    /// Set `has_null` to `true` in order to map `null` values to `()`.
    /// Setting it to `false` will cause an [`ErrorVariableNotFound`][EvalAltResult::ErrorVariableNotFound] error during parsing.
    ///
    /// # JSON With Sub-Objects
    ///
    /// This method assumes no sub-objects in the JSON string.  That is because the syntax
    /// of a JSON sub-object (or object hash), `{ .. }`, is different from Rhai's syntax, `#{ .. }`.
    /// Parsing a JSON string with sub-objects will cause a syntax error.
    ///
    /// If it is certain that the character `{` never appears in any text string within the JSON object,
    /// which is a valid assumption for many use cases, then globally replace `{` with `#{` before calling this method.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Map};
    ///
    /// let engine = Engine::new();
    ///
    /// let map = engine.parse_json(
    ///     r#"{"a":123, "b":42, "c":{"x":false, "y":true}, "d":null}"#
    ///         .replace("{", "#{").as_str(),
    /// true)?;
    ///
    /// assert_eq!(map.len(), 4);
    /// assert_eq!(map["a"].as_int().expect("a should exist"), 123);
    /// assert_eq!(map["b"].as_int().expect("b should exist"), 42);
    /// assert!(map["d"].is::<()>());
    ///
    /// let c = map["c"].read_lock::<Map>().expect("c should exist");
    /// assert_eq!(c["x"].as_bool().expect("x should be bool"), false);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn parse_json(
        &self,
        json: impl AsRef<str>,
        has_null: bool,
    ) -> Result<Map, Box<EvalAltResult>> {
        use crate::token::Token;

        fn parse_json_inner(
            engine: &Engine,
            json: &str,
            has_null: bool,
        ) -> Result<Map, Box<EvalAltResult>> {
            let mut scope = Scope::new();
            let json_text = json.trim_start();
            let scripts = if json_text.starts_with(Token::MapStart.literal_syntax()) {
                [json_text, ""]
            } else if json_text.starts_with(Token::LeftBrace.literal_syntax()) {
                ["#", json_text]
            } else {
                return Err(crate::ParseErrorType::MissingToken(
                    Token::LeftBrace.syntax().into(),
                    "to start a JSON object hash".into(),
                )
                .into_err(Position::new(1, (json.len() - json_text.len() + 1) as u16))
                .into());
            };
            let (stream, tokenizer_control) = engine.lex_raw(
                &scripts,
                if has_null {
                    Some(&|token, _, _| {
                        match token {
                            // If `null` is present, make sure `null` is treated as a variable
                            Token::Reserved(s) if s == "null" => Token::Identifier(s),
                            _ => token,
                        }
                    })
                } else {
                    None
                },
            );
            let mut state = ParseState::new(engine, tokenizer_control);
            let ast = engine.parse_global_expr(
                &mut stream.peekable(),
                &mut state,
                &scope,
                OptimizationLevel::None,
            )?;
            if has_null {
                scope.push_constant("null", ());
            }
            engine.eval_ast_with_scope(&mut scope, &ast)
        }

        parse_json_inner(self, json.as_ref(), has_null)
    }
    /// Compile a string containing an expression into an [`AST`],
    /// which can be used later for evaluation.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// // Compile a script to an AST and store it for later evaluation
    /// let ast = engine.compile_expression("40 + 2")?;
    ///
    /// for _ in 0..42 {
    ///     assert_eq!(engine.eval_ast::<i64>(&ast)?, 42);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn compile_expression(&self, script: &str) -> Result<AST, ParseError> {
        self.compile_expression_with_scope(&Default::default(), script)
    }
    /// Compile a string containing an expression into an [`AST`] using own scope,
    /// which can be used later for evaluation.
    ///
    /// The scope is useful for passing constants into the script for optimization
    /// when using [`OptimizationLevel::Full`].
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # #[cfg(not(feature = "no_optimize"))]
    /// # {
    /// use rhai::{Engine, Scope, OptimizationLevel};
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Set optimization level to 'Full' so the Engine can fold constants
    /// // into function calls and operators.
    /// engine.set_optimization_level(OptimizationLevel::Full);
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push_constant("x", 10_i64);   // 'x' is a constant
    ///
    /// // Compile a script to an AST and store it for later evaluation.
    /// // Notice that `Full` optimization is on, so constants are folded
    /// // into function calls and operators.
    /// let ast = engine.compile_expression_with_scope(&mut scope,
    ///             "2 + (x + x) * 2"    // all 'x' are replaced with 10
    /// )?;
    ///
    /// // Normally this would have failed because no scope is passed into the 'eval_ast'
    /// // call and so the variable 'x' does not exist.  Here, it passes because the script
    /// // has been optimized and all references to 'x' are already gone.
    /// assert_eq!(engine.eval_ast::<i64>(&ast)?, 42);
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn compile_expression_with_scope(
        &self,
        scope: &Scope,
        script: &str,
    ) -> Result<AST, ParseError> {
        let scripts = [script];
        let (stream, tokenizer_control) =
            self.lex_raw(&scripts, self.token_mapper.as_ref().map(Box::as_ref));

        let mut peekable = stream.peekable();
        let mut state = ParseState::new(self, tokenizer_control);
        self.parse_global_expr(&mut peekable, &mut state, scope, self.optimization_level)
    }
    /// Evaluate a script file.
    ///
    /// Not available under `no_std` or `WASM`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// // Notice that a PathBuf is required which can easily be constructed from a string.
    /// let result = engine.eval_file::<i64>("script.rhai".into())?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_std"))]
    #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
    #[inline(always)]
    pub fn eval_file<T: Variant + Clone>(
        &self,
        path: std::path::PathBuf,
    ) -> Result<T, Box<EvalAltResult>> {
        Self::read_file(path).and_then(|contents| self.eval::<T>(&contents))
    }
    /// Evaluate a script file with own scope.
    ///
    /// Not available under `no_std` or `WASM`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Scope};
    ///
    /// let engine = Engine::new();
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push("x", 42_i64);
    ///
    /// // Notice that a PathBuf is required which can easily be constructed from a string.
    /// let result = engine.eval_file_with_scope::<i64>(&mut scope, "script.rhai".into())?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_std"))]
    #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
    #[inline(always)]
    pub fn eval_file_with_scope<T: Variant + Clone>(
        &self,
        scope: &mut Scope,
        path: std::path::PathBuf,
    ) -> Result<T, Box<EvalAltResult>> {
        Self::read_file(path).and_then(|contents| self.eval_with_scope::<T>(scope, &contents))
    }
    /// Evaluate a string.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// assert_eq!(engine.eval::<i64>("40 + 2")?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn eval<T: Variant + Clone>(&self, script: &str) -> Result<T, Box<EvalAltResult>> {
        self.eval_with_scope(&mut Default::default(), script)
    }
    /// Evaluate a string with own scope.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Scope};
    ///
    /// let engine = Engine::new();
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push("x", 40_i64);
    ///
    /// assert_eq!(engine.eval_with_scope::<i64>(&mut scope, "x += 2; x")?, 42);
    /// assert_eq!(engine.eval_with_scope::<i64>(&mut scope, "x += 2; x")?, 44);
    ///
    /// // The variable in the scope is modified
    /// assert_eq!(scope.get_value::<i64>("x").expect("variable x should exist"), 44);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn eval_with_scope<T: Variant + Clone>(
        &self,
        scope: &mut Scope,
        script: &str,
    ) -> Result<T, Box<EvalAltResult>> {
        let ast = self.compile_with_scope_and_optimization_level(
            scope,
            &[script],
            self.optimization_level,
        )?;
        self.eval_ast_with_scope(scope, &ast)
    }
    /// Evaluate a string containing an expression.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// assert_eq!(engine.eval_expression::<i64>("40 + 2")?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn eval_expression<T: Variant + Clone>(
        &self,
        script: &str,
    ) -> Result<T, Box<EvalAltResult>> {
        self.eval_expression_with_scope(&mut Default::default(), script)
    }
    /// Evaluate a string containing an expression with own scope.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Scope};
    ///
    /// let engine = Engine::new();
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push("x", 40_i64);
    ///
    /// assert_eq!(engine.eval_expression_with_scope::<i64>(&mut scope, "x + 2")?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn eval_expression_with_scope<T: Variant + Clone>(
        &self,
        scope: &mut Scope,
        script: &str,
    ) -> Result<T, Box<EvalAltResult>> {
        let scripts = [script];
        let (stream, tokenizer_control) =
            self.lex_raw(&scripts, self.token_mapper.as_ref().map(Box::as_ref));
        let mut state = ParseState::new(self, tokenizer_control);

        // No need to optimize a lone expression
        let ast = self.parse_global_expr(
            &mut stream.peekable(),
            &mut state,
            scope,
            OptimizationLevel::None,
        )?;

        self.eval_ast_with_scope(scope, &ast)
    }
    /// Evaluate an [`AST`].
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// // Compile a script to an AST and store it for later evaluation
    /// let ast = engine.compile("40 + 2")?;
    ///
    /// // Evaluate it
    /// assert_eq!(engine.eval_ast::<i64>(&ast)?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn eval_ast<T: Variant + Clone>(&self, ast: &AST) -> Result<T, Box<EvalAltResult>> {
        self.eval_ast_with_scope(&mut Default::default(), ast)
    }
    /// Evaluate an [`AST`] with own scope.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Scope};
    ///
    /// let engine = Engine::new();
    ///
    /// // Compile a script to an AST and store it for later evaluation
    /// let ast = engine.compile("x + 2")?;
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push("x", 40_i64);
    ///
    /// // Compile a script to an AST and store it for later evaluation
    /// let ast = engine.compile("x += 2; x")?;
    ///
    /// // Evaluate it
    /// assert_eq!(engine.eval_ast_with_scope::<i64>(&mut scope, &ast)?, 42);
    /// assert_eq!(engine.eval_ast_with_scope::<i64>(&mut scope, &ast)?, 44);
    ///
    /// // The variable in the scope is modified
    /// assert_eq!(scope.get_value::<i64>("x").expect("variable x should exist"), 44);
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn eval_ast_with_scope<T: Variant + Clone>(
        &self,
        scope: &mut Scope,
        ast: &AST,
    ) -> Result<T, Box<EvalAltResult>> {
        let mods = &mut Default::default();

        let result = self.eval_ast_with_scope_raw(scope, mods, ast, 0)?;

        let typ = self.map_type_name(result.type_name());

        result.try_cast::<T>().ok_or_else(|| {
            EvalAltResult::ErrorMismatchOutputType(
                self.map_type_name(type_name::<T>()).into(),
                typ.into(),
                Position::NONE,
            )
            .into()
        })
    }
    /// Evaluate an [`AST`] with own scope.
    #[inline]
    pub(crate) fn eval_ast_with_scope_raw<'a>(
        &self,
        scope: &mut Scope,
        mods: &mut Imports,
        ast: &'a AST,
        level: usize,
    ) -> RhaiResult {
        let mut state = EvalState::new();
        state.source = ast.source_raw().cloned();
        #[cfg(not(feature = "no_module"))]
        {
            state.embedded_module_resolver = ast.resolver();
        }

        let statements = ast.statements();

        if statements.is_empty() {
            return Ok(Dynamic::UNIT);
        }

        let lib = &[ast.lib()];
        self.eval_global_statements(scope, mods, &mut state, statements, lib, level)
    }
    /// Evaluate a file, returning any error (if any).
    ///
    /// Not available under `no_std` or `WASM`.
    #[cfg(not(feature = "no_std"))]
    #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
    #[inline(always)]
    pub fn run_file(&self, path: std::path::PathBuf) -> Result<(), Box<EvalAltResult>> {
        Self::read_file(path).and_then(|contents| self.run(&contents))
    }
    /// Evaluate a file with own scope, returning any error (if any).
    ///
    /// Not available under `no_std` or `WASM`.
    #[cfg(not(feature = "no_std"))]
    #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
    #[inline(always)]
    pub fn run_file_with_scope(
        &self,
        scope: &mut Scope,
        path: std::path::PathBuf,
    ) -> Result<(), Box<EvalAltResult>> {
        Self::read_file(path).and_then(|contents| self.run_with_scope(scope, &contents))
    }
    /// Evaluate a script, returning any error (if any).
    #[inline(always)]
    pub fn run(&self, script: &str) -> Result<(), Box<EvalAltResult>> {
        self.run_with_scope(&mut Default::default(), script)
    }
    /// Evaluate a script with own scope, returning any error (if any).
    #[inline]
    pub fn run_with_scope(
        &self,
        scope: &mut Scope,
        script: &str,
    ) -> Result<(), Box<EvalAltResult>> {
        let scripts = [script];
        let (stream, tokenizer_control) =
            self.lex_raw(&scripts, self.token_mapper.as_ref().map(Box::as_ref));
        let mut state = ParseState::new(self, tokenizer_control);

        let ast = self.parse(
            &mut stream.peekable(),
            &mut state,
            scope,
            self.optimization_level,
        )?;

        self.run_ast_with_scope(scope, &ast)
    }
    /// Evaluate an AST, returning any error (if any).
    #[inline(always)]
    pub fn run_ast(&self, ast: &AST) -> Result<(), Box<EvalAltResult>> {
        self.run_ast_with_scope(&mut Default::default(), ast)
    }
    /// Evaluate an [`AST`] with own scope, returning any error (if any).
    #[inline]
    pub fn run_ast_with_scope(
        &self,
        scope: &mut Scope,
        ast: &AST,
    ) -> Result<(), Box<EvalAltResult>> {
        let mods = &mut Default::default();
        let mut state = EvalState::new();
        state.source = ast.source_raw().cloned();
        #[cfg(not(feature = "no_module"))]
        {
            state.embedded_module_resolver = ast.resolver();
        }

        let statements = ast.statements();
        if !statements.is_empty() {
            let lib = &[ast.lib()];
            self.eval_global_statements(scope, mods, &mut state, statements, lib, 0)?;
        }
        Ok(())
    }
    /// Call a script function defined in an [`AST`] with multiple arguments.
    /// Arguments are passed as a tuple.
    ///
    /// Not available under `no_function`.
    ///
    /// The [`AST`] is evaluated before calling the function.
    /// This allows a script to load the necessary modules.
    /// This is usually desired. If not, a specialized [`AST`] can be prepared that contains only
    /// function definitions without any body script via [`AST::clear_statements`].
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # #[cfg(not(feature = "no_function"))]
    /// # {
    /// use rhai::{Engine, Scope};
    ///
    /// let engine = Engine::new();
    ///
    /// let ast = engine.compile("
    ///     fn add(x, y) { len(x) + y + foo }
    ///     fn add1(x)   { len(x) + 1 + foo }
    ///     fn bar()     { foo/2 }
    /// ")?;
    ///
    /// let mut scope = Scope::new();
    /// scope.push("foo", 42_i64);
    ///
    /// // Call the script-defined function
    /// let result: i64 = engine.call_fn(&mut scope, &ast, "add", ( "abc", 123_i64 ) )?;
    /// assert_eq!(result, 168);
    ///
    /// let result: i64 = engine.call_fn(&mut scope, &ast, "add1", ( "abc", ) )?;
    /// //                                                         ^^^^^^^^^^ tuple of one
    /// assert_eq!(result, 46);
    ///
    /// let result: i64 = engine.call_fn(&mut scope, &ast, "bar", () )?;
    /// assert_eq!(result, 21);
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub fn call_fn<T: Variant + Clone>(
        &self,
        scope: &mut Scope,
        ast: &AST,
        name: impl AsRef<str>,
        args: impl crate::FuncArgs,
    ) -> Result<T, Box<EvalAltResult>> {
        let mut arg_values = crate::StaticVec::new();
        args.parse(&mut arg_values);
        let mut args: crate::StaticVec<_> = arg_values.iter_mut().collect();
        let name = name.as_ref();

        let result = self.call_fn_dynamic_raw(scope, ast, true, name, &mut None, &mut args)?;

        let typ = self.map_type_name(result.type_name());

        result.try_cast().ok_or_else(|| {
            EvalAltResult::ErrorMismatchOutputType(
                self.map_type_name(type_name::<T>()).into(),
                typ.into(),
                Position::NONE,
            )
            .into()
        })
    }
    /// Call a script function defined in an [`AST`] with multiple [`Dynamic`] arguments
    /// and optionally a value for binding to the `this` pointer.
    ///
    /// Not available under `no_function`.
    ///
    /// There is an option to evaluate the [`AST`] to load necessary modules before calling the function.
    ///
    /// # WARNING
    ///
    /// All the arguments are _consumed_, meaning that they're replaced by `()`.
    /// This is to avoid unnecessarily cloning the arguments.
    /// Do not use the arguments after this call. If they are needed afterwards,
    /// clone them _before_ calling this function.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # #[cfg(not(feature = "no_function"))]
    /// # {
    /// use rhai::{Engine, Scope, Dynamic};
    ///
    /// let engine = Engine::new();
    ///
    /// let ast = engine.compile("
    ///     fn add(x, y) { len(x) + y + foo }
    ///     fn add1(x)   { len(x) + 1 + foo }
    ///     fn bar()     { foo/2 }
    ///     fn action(x) { this += x; }         // function using 'this' pointer
    /// ")?;
    ///
    /// let mut scope = Scope::new();
    /// scope.push("foo", 42_i64);
    ///
    /// // Call the script-defined function
    /// let result = engine.call_fn_dynamic(&mut scope, &ast, true, "add", None, [ "abc".into(), 123_i64.into() ])?;
    /// //                                                                 ^^^^ no 'this' pointer
    /// assert_eq!(result.cast::<i64>(), 168);
    ///
    /// let result = engine.call_fn_dynamic(&mut scope, &ast, true, "add1", None, [ "abc".into() ])?;
    /// assert_eq!(result.cast::<i64>(), 46);
    ///
    /// let result = engine.call_fn_dynamic(&mut scope, &ast, true, "bar", None, [])?;
    /// assert_eq!(result.cast::<i64>(), 21);
    ///
    /// let mut value: Dynamic = 1_i64.into();
    /// let result = engine.call_fn_dynamic(&mut scope, &ast, true, "action", Some(&mut value), [ 41_i64.into() ])?;
    /// //                                                                    ^^^^^^^^^^^^^^^^ binding the 'this' pointer
    /// assert_eq!(value.as_int().expect("value should be INT"), 42);
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub fn call_fn_dynamic(
        &self,
        scope: &mut Scope,
        ast: &AST,
        eval_ast: bool,
        name: impl AsRef<str>,
        mut this_ptr: Option<&mut Dynamic>,
        mut arg_values: impl AsMut<[Dynamic]>,
    ) -> RhaiResult {
        let name = name.as_ref();
        let mut args: crate::StaticVec<_> = arg_values.as_mut().iter_mut().collect();

        self.call_fn_dynamic_raw(scope, ast, eval_ast, name, &mut this_ptr, &mut args)
    }
    /// Call a script function defined in an [`AST`] with multiple [`Dynamic`] arguments.
    ///
    /// # WARNING
    ///
    /// All the arguments are _consumed_, meaning that they're replaced by `()`.
    /// This is to avoid unnecessarily cloning the arguments.
    /// Do not use the arguments after this call. If they are needed afterwards,
    /// clone them _before_ calling this function.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub(crate) fn call_fn_dynamic_raw(
        &self,
        scope: &mut Scope,
        ast: &AST,
        eval_ast: bool,
        name: &str,
        this_ptr: &mut Option<&mut Dynamic>,
        args: &mut FnCallArgs,
    ) -> RhaiResult {
        let state = &mut EvalState::new();
        let mods = &mut Default::default();
        let lib = &[ast.lib()];
        let statements = ast.statements();

        if eval_ast && !statements.is_empty() {
            // Make sure new variables introduced at global level do not _spill_ into the function call
            let orig_scope_len = scope.len();
            self.eval_global_statements(scope, mods, state, statements, lib, 0)?;
            scope.rewind(orig_scope_len);
        }

        let fn_def = ast
            .lib()
            .get_script_fn(name, args.len())
            .ok_or_else(|| EvalAltResult::ErrorFunctionNotFound(name.into(), Position::NONE))?;

        // Check for data race.
        #[cfg(not(feature = "no_closure"))]
        crate::fn_call::ensure_no_data_race(name, args, false)?;

        self.call_script_fn(
            scope,
            mods,
            state,
            lib,
            this_ptr,
            fn_def,
            args,
            Position::NONE,
            0,
        )
    }
    /// Optimize the [`AST`] with constants defined in an external Scope.
    /// An optimized copy of the [`AST`] is returned while the original [`AST`] is consumed.
    ///
    /// Not available under `no_optimize`.
    ///
    /// Although optimization is performed by default during compilation, sometimes it is necessary to
    /// _re_-optimize an [`AST`]. For example, when working with constants that are passed in via an
    /// external scope, it will be more efficient to optimize the [`AST`] once again to take advantage
    /// of the new constants.
    ///
    /// With this method, it is no longer necessary to recompile a large script.
    /// The script [`AST`] can be compiled just once. Before evaluation,
    /// constants are passed into the [`Engine`] via an external scope
    /// (i.e. with [`Scope::push_constant`]).
    /// Then, the [`AST`] is cloned and the copy re-optimized before running.
    #[cfg(not(feature = "no_optimize"))]
    #[inline]
    #[must_use]
    pub fn optimize_ast(
        &self,
        scope: &Scope,
        mut ast: AST,
        optimization_level: OptimizationLevel,
    ) -> AST {
        #[cfg(not(feature = "no_function"))]
        let lib = ast
            .lib()
            .iter_fn()
            .filter(|f| f.func.is_script())
            .map(|f| {
                f.func
                    .get_script_fn_def()
                    .expect("scripted function")
                    .clone()
            })
            .collect();

        #[cfg(feature = "no_function")]
        let lib = Default::default();

        let stmt = std::mem::take(ast.statements_mut());
        crate::optimize::optimize_into_ast(self, scope, stmt, lib, optimization_level)
    }
    /// _(metadata)_ Generate a list of all registered functions.
    /// Exported under the `metadata` feature only.
    ///
    /// Functions from the following sources are included, in order:
    /// 1) Functions registered into the global namespace
    /// 2) Functions in registered sub-modules
    /// 3) Functions in packages (optional)
    #[cfg(feature = "metadata")]
    #[inline]
    #[must_use]
    pub fn gen_fn_signatures(&self, include_packages: bool) -> Vec<String> {
        let mut signatures = Vec::with_capacity(64);

        signatures.extend(self.global_namespace().gen_fn_signatures());

        self.global_sub_modules.iter().for_each(|(name, m)| {
            signatures.extend(m.gen_fn_signatures().map(|f| format!("{}::{}", name, f)))
        });

        if include_packages {
            signatures.extend(
                self.global_modules
                    .iter()
                    .take(self.global_modules.len() - 1)
                    .flat_map(|m| m.gen_fn_signatures()),
            );
        }

        signatures
    }
    /// Provide a callback that will be invoked before each variable access.
    ///
    /// # Callback Function Signature
    ///
    /// The callback function signature takes the following form:
    ///
    /// > `Fn(name: &str, index: usize, context: &EvalContext)`  
    /// > `   -> Result<Option<Dynamic>, Box<EvalAltResult>> + 'static`
    ///
    /// where:
    /// * `index`: an offset from the bottom of the current [`Scope`] that the variable is supposed
    ///   to reside. Offsets start from 1, with 1 meaning the last variable in the current
    ///   [`Scope`].  Essentially the correct variable is at position `scope.len() - index`.
    ///   If `index` is zero, then there is no pre-calculated offset position and a search through the
    ///   current [`Scope`] must be performed.
    ///
    /// * `context`: the current [evaluation context][`EvalContext`].
    ///
    /// ## Return value
    ///
    /// * `Ok(None)`: continue with normal variable access.
    /// * `Ok(Some(Dynamic))`: the variable's value.
    ///
    /// ## Raising errors
    ///
    /// Return `Err(...)` if there is an error.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register a variable resolver.
    /// engine.on_var(|name, _, _| {
    ///     match name {
    ///         "MYSTIC_NUMBER" => Ok(Some(42_i64.into())),
    ///         _ => Ok(None)
    ///     }
    /// });
    ///
    /// engine.eval::<i64>("MYSTIC_NUMBER")?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn on_var(
        &mut self,
        callback: impl Fn(&str, usize, &EvalContext) -> Result<Option<Dynamic>, Box<EvalAltResult>>
            + SendSync
            + 'static,
    ) -> &mut Self {
        self.resolve_var = Some(Box::new(callback));
        self
    }
    /// _(internals)_ Provide a callback that will be invoked during parsing to remap certain tokens.
    /// Exported under the `internals` feature only.
    ///
    /// # Callback Function Signature
    ///
    /// The callback function signature takes the following form:
    ///
    /// > `Fn(token: Token, pos: Position, state: &TokenizeState) -> Token`
    ///
    /// where:
    /// * [`token`][crate::token::Token]: current token parsed
    /// * [`pos`][`Position`]: location of the token
    /// * [`state`][crate::token::TokenizeState]: current state of the tokenizer
    ///
    /// ## Raising errors
    ///
    /// It is possible to raise a parsing error by returning
    /// [`Token::LexError`][crate::token::Token::LexError] as the mapped token.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Token};
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register a token mapper.
    /// engine.on_parse_token(|token, _, _| {
    ///     match token {
    ///         // Convert all integer literals to strings
    ///         Token::IntegerConstant(n) => Token::StringConstant(n.to_string()),
    ///         // Convert 'begin' .. 'end' to '{' .. '}'
    ///         Token::Identifier(s) if &s == "begin" => Token::LeftBrace,
    ///         Token::Identifier(s) if &s == "end" => Token::RightBrace,
    ///         // Pass through all other tokens unchanged
    ///         _ => token
    ///     }
    /// });
    ///
    /// assert_eq!(engine.eval::<String>("42")?, "42");
    /// assert_eq!(engine.eval::<bool>("true")?, true);
    /// assert_eq!(engine.eval::<String>("let x = 42; begin let x = 0; end; x")?, "42");
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "internals")]
    #[inline(always)]
    pub fn on_parse_token(
        &mut self,
        callback: impl Fn(crate::token::Token, Position, &crate::token::TokenizeState) -> crate::token::Token
            + SendSync
            + 'static,
    ) -> &mut Self {
        use std::string::ParseError;

        self.token_mapper = Some(Box::new(callback));
        self
    }
    /// Register a callback for script evaluation progress.
    ///
    /// Not available under `unchecked`.
    ///
    /// # Callback Function Signature
    ///
    /// The callback function signature takes the following form:
    ///
    /// > `Fn(counter: u64) -> Option<Dynamic>`
    ///
    /// ## Return value
    ///
    /// * `None`: continue running the script.
    /// * `Some(Dynamic)`: terminate the script with the specified exception value.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # use std::sync::RwLock;
    /// # use std::sync::Arc;
    /// use rhai::Engine;
    ///
    /// let result = Arc::new(RwLock::new(0_u64));
    /// let logger = result.clone();
    ///
    /// let mut engine = Engine::new();
    ///
    /// engine.on_progress(move |ops| {
    ///     if ops > 10000 {
    ///         Some("Over 10,000 operations!".into())
    ///     } else if ops % 800 == 0 {
    ///         *logger.write().unwrap() = ops;
    ///         None
    ///     } else {
    ///         None
    ///     }
    /// });
    ///
    /// engine.run("for x in range(0, 50000) {}")
    ///       .expect_err("should error");
    ///
    /// assert_eq!(*result.read().unwrap(), 9600);
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "unchecked"))]
    #[inline(always)]
    pub fn on_progress(
        &mut self,
        callback: impl Fn(u64) -> Option<Dynamic> + SendSync + 'static,
    ) -> &mut Self {
        self.progress = Some(Box::new(callback));
        self
    }
    /// Override default action of `print` (print to stdout using [`println!`])
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # use std::sync::RwLock;
    /// # use std::sync::Arc;
    /// use rhai::Engine;
    ///
    /// let result = Arc::new(RwLock::new(String::new()));
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Override action of 'print' function
    /// let logger = result.clone();
    /// engine.on_print(move |s| logger.write().unwrap().push_str(s));
    ///
    /// engine.run("print(40 + 2);")?;
    ///
    /// assert_eq!(*result.read().unwrap(), "42");
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn on_print(&mut self, callback: impl Fn(&str) + SendSync + 'static) -> &mut Self {
        self.print = Some(Box::new(callback));
        self
    }
    /// Override default action of `debug` (print to stdout using [`println!`])
    ///
    /// # Callback Function Signature
    ///
    /// The callback function signature passed takes the following form:
    ///
    /// > `Fn(text: &str, source: Option<&str>, pos: Position)`
    ///
    /// where:
    /// * `text`: the text to display
    /// * `source`: current source, if any
    /// * [`pos`][`Position`]: location of the `debug` call
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # use std::sync::RwLock;
    /// # use std::sync::Arc;
    /// use rhai::Engine;
    ///
    /// let result = Arc::new(RwLock::new(String::new()));
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Override action of 'print' function
    /// let logger = result.clone();
    /// engine.on_debug(move |s, src, pos| logger.write().unwrap().push_str(
    ///                     &format!("{} @ {:?} > {}", src.unwrap_or("unknown"), pos, s)
    ///                ));
    ///
    /// let mut ast = engine.compile(r#"let x = "hello"; debug(x);"#)?;
    /// ast.set_source("world");
    /// engine.run_ast(&ast)?;
    ///
    /// #[cfg(not(feature = "no_position"))]
    /// assert_eq!(*result.read().unwrap(), r#"world @ 1:18 > "hello""#);
    /// #[cfg(feature = "no_position")]
    /// assert_eq!(*result.read().unwrap(), r#"world @ none > "hello""#);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn on_debug(
        &mut self,
        callback: impl Fn(&str, Option<&str>, Position) + SendSync + 'static,
    ) -> &mut Self {
        self.debug = Some(Box::new(callback));
        self
    }
}
