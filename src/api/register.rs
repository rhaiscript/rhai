//! Module that defines the public function/module registration API of [`Engine`].

use crate::func::{FnCallArgs, RhaiFunc, RhaiNativeFunc, SendSync};
use crate::module::{FuncRegistration, ModuleFlags};
use crate::types::dynamic::Variant;
use crate::{
    Dynamic, Engine, FnNamespace, Identifier, Module, NativeCallContext, RhaiResultOf, Shared,
    SharedModule,
};
use std::any::{type_name, TypeId};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
use crate::func::register::Mut;

impl Engine {
    /// Get a mutable reference to the global namespace module
    /// (which is the first module in `global_modules`).
    #[inline(always)]
    #[must_use]
    pub(crate) fn global_namespace_mut(&mut self) -> &mut Module {
        if self.global_modules.is_empty() {
            let mut global_namespace = Module::new();
            global_namespace.flags |= ModuleFlags::INTERNAL;
            self.global_modules.push(global_namespace.into());
        }

        Shared::get_mut(self.global_modules.first_mut().unwrap()).unwrap()
    }
    /// Register a custom function with the [`Engine`].
    ///
    /// # Assumptions
    ///
    /// * **Accessibility**: The function namespace is [`FnNamespace::Global`].
    ///
    /// * **Purity**: The function is assumed to be _pure_ unless it is a property setter or an index setter.
    ///
    /// * **Volatility**: The function is assumed to be _non-volatile_ -- i.e. it guarantees the same result for the same input(s).
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
    pub fn register_fn<
        A: 'static,
        const N: usize,
        const X: bool,
        R: Variant + Clone,
        const F: bool,
        FUNC: RhaiNativeFunc<A, N, X, R, F> + SendSync + 'static,
    >(
        &mut self,
        name: impl AsRef<str> + Into<Identifier>,
        func: FUNC,
    ) -> &mut Self {
        let reg = FuncRegistration::new(name.into()).with_namespace(FnNamespace::Global);

        #[cfg(feature = "metadata")]
        let reg = {
            let mut param_type_names = FUNC::param_names()
                .iter()
                .map(|ty| format!("_: {}", self.format_param_type(ty)))
                .collect::<crate::FnArgsVec<_>>();

            if FUNC::return_type() != TypeId::of::<()>() {
                param_type_names.push(self.format_param_type(FUNC::return_type_name()).into());
            }

            let param_type_names = param_type_names
                .iter()
                .map(String::as_str)
                .collect::<crate::FnArgsVec<_>>();

            reg.with_params_info(param_type_names)
        };

        reg.set_into_module(self.global_namespace_mut(), func);

        self
    }
    /// Register a function of the [`Engine`].
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.  It takes a list of [`TypeId`][std::any::TypeId]'s
    /// indicating the actual types of the parameters.
    ///
    /// # Arguments
    ///
    /// Arguments are simply passed in as a mutable array of [`&mut Dynamic`][crate::Dynamic].
    /// The arguments are guaranteed to be of the correct types matching the [`TypeId`][std::any::TypeId]'s.
    ///
    /// To access a primary argument value (i.e. cloning is cheap), use: `args[n].as_xxx().unwrap()`
    ///
    /// To access an argument value and avoid cloning, use `args[n].take().cast::<T>()`.
    /// Notice that this will _consume_ the argument, replacing it with `()`.
    ///
    /// To access the first mutable parameter, use `args.get_mut(0).unwrap()`
    #[inline(always)]
    pub fn register_raw_fn<T: Variant + Clone>(
        &mut self,
        name: impl AsRef<str> + Into<Identifier>,
        arg_types: impl AsRef<[TypeId]>,
        func: impl Fn(NativeCallContext, &mut FnCallArgs) -> RhaiResultOf<T> + SendSync + 'static,
    ) -> &mut Self {
        let name = name.into();
        let arg_types = arg_types.as_ref();
        let is_pure = true;

        #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
        let is_pure = is_pure && (arg_types.len() != 3 || name != crate::engine::FN_IDX_SET);
        #[cfg(not(feature = "no_object"))]
        let is_pure = is_pure && (arg_types.len() != 2 || !name.starts_with(crate::engine::FN_SET));

        FuncRegistration::new(name)
            .with_namespace(FnNamespace::Global)
            .set_into_module_raw(
                self.global_namespace_mut(),
                arg_types,
                RhaiFunc::Method {
                    func: Shared::new(
                        move |ctx: Option<NativeCallContext>, args: &mut FnCallArgs| {
                            func(ctx.unwrap(), args).map(Dynamic::from)
                        },
                    ),
                    has_context: true,
                    is_pure,
                    is_volatile: true,
                },
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
    ///     fn new() -> Self {
    ///         Self { field: 1 }
    ///     }
    ///     fn update(&mut self, offset: i64) {
    ///         self.field += offset;
    ///     }
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
    ///     fn new() -> Self {
    ///         Self { field: 1 }
    ///     }
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
        self.global_namespace_mut().set_custom_type::<T>(name);
        self
    }
    /// Register a custom type for use with the [`Engine`], with a pretty-print name
    /// for the `type_of` function. The type must implement [`Clone`].
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is low level.
    #[inline(always)]
    pub fn register_type_with_name_raw(
        &mut self,
        type_path: impl Into<Identifier>,
        name: impl Into<Identifier>,
    ) -> &mut Self {
        // Add the pretty-print type name into the map
        self.global_namespace_mut()
            .set_custom_type_raw(type_path, name);
        self
    }
    /// Register a type iterator for an iterable type with the [`Engine`].
    /// This is an advanced API.
    #[inline(always)]
    pub fn register_iterator<T>(&mut self) -> &mut Self
    where
        T: Variant + Clone + IntoIterator,
        <T as IntoIterator>::Item: Variant + Clone,
    {
        self.global_namespace_mut().set_iterable::<T>();
        self
    }
    /// Register a fallible type iterator for an iterable type with the [`Engine`].
    /// This is an advanced API.
    #[inline(always)]
    pub fn register_iterator_result<T, R>(&mut self) -> &mut Self
    where
        T: Variant + Clone + IntoIterator<Item = RhaiResultOf<R>>,
        R: Variant + Clone,
    {
        self.global_namespace_mut().set_iterable_result::<T, R>();
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
    ///     fn new() -> Self {
    ///         Self { field: 1 }
    ///     }
    ///     // Even a getter must start with `&mut self` and not `&self`.
    ///     fn get_field(&mut self) -> i64  {
    ///         self.field
    ///     }
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
    pub fn register_get<T: Variant + Clone, const X: bool, R: Variant + Clone, const F: bool>(
        &mut self,
        name: impl AsRef<str>,
        get_fn: impl RhaiNativeFunc<(Mut<T>,), 1, X, R, F> + SendSync + 'static,
    ) -> &mut Self {
        self.register_fn(crate::engine::make_getter(name.as_ref()), get_fn)
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
    ///     fn new() -> Self {
    ///         Self { field: 1 }
    ///     }
    ///     fn set_field(&mut self, new_val: i64) {
    ///         self.field = new_val;
    ///     }
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
    pub fn register_set<T: Variant + Clone, const X: bool, R: Variant + Clone, const F: bool>(
        &mut self,
        name: impl AsRef<str>,
        set_fn: impl RhaiNativeFunc<(Mut<T>, R), 2, X, (), F> + SendSync + 'static,
    ) -> &mut Self {
        self.register_fn(crate::engine::make_setter(name.as_ref()), set_fn)
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
    ///     fn new() -> Self {
    ///         Self { field: 1 }
    ///     }
    ///     // Even a getter must start with `&mut self` and not `&self`.
    ///     fn get_field(&mut self) -> i64 {
    ///         self.field
    ///     }
    ///     fn set_field(&mut self, new_val: i64) {
    ///         self.field = new_val;
    ///     }
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
    pub fn register_get_set<
        T: Variant + Clone,
        const X1: bool,
        const X2: bool,
        R: Variant + Clone,
        const F1: bool,
        const F2: bool,
    >(
        &mut self,
        name: impl AsRef<str>,
        get_fn: impl RhaiNativeFunc<(Mut<T>,), 1, X1, R, F1> + SendSync + 'static,
        set_fn: impl RhaiNativeFunc<(Mut<T>, R), 2, X2, (), F2> + SendSync + 'static,
    ) -> &mut Self {
        self.register_get(&name, get_fn).register_set(&name, set_fn)
    }
    /// Register an index getter for a custom type with the [`Engine`].
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`][crate::Array], [`Map`][crate::Map], [`String`],
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
    ///     fn new() -> Self {
    ///         Self { fields: vec![1, 2, 3, 4, 5] }
    ///     }
    ///     // Even a getter must start with `&mut self` and not `&self`.
    ///     fn get_field(&mut self, index: i64) -> i64 {
    ///         self.fields[index as usize]
    ///     }
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
    #[inline(always)]
    pub fn register_indexer_get<
        T: Variant + Clone,
        IDX: Variant + Clone,
        const X: bool,
        R: Variant + Clone,
        const F: bool,
    >(
        &mut self,
        get_fn: impl RhaiNativeFunc<(Mut<T>, IDX), 2, X, R, F> + SendSync + 'static,
    ) -> &mut Self {
        self.register_fn(crate::engine::FN_IDX_GET, get_fn)
    }
    /// Register an index setter for a custom type with the [`Engine`].
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`][crate::Array], [`Map`][crate::Map], [`String`],
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
    ///     fn new() -> Self {
    ///         Self { fields: vec![1, 2, 3, 4, 5] }
    ///     }
    ///     fn set_field(&mut self, index: i64, value: i64) {
    ///         self.fields[index as usize] = value;
    ///     }
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
    /// let result = engine.eval::<TestStruct>("let a = new_ts(); a[2] = 42; a")?;
    ///
    /// # #[cfg(not(feature = "no_index"))]
    /// assert_eq!(result.fields[2], 42);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline(always)]
    pub fn register_indexer_set<
        T: Variant + Clone,
        IDX: Variant + Clone,
        const X: bool,
        R: Variant + Clone,
        const F: bool,
    >(
        &mut self,
        set_fn: impl RhaiNativeFunc<(Mut<T>, IDX, R), 3, X, (), F> + SendSync + 'static,
    ) -> &mut Self {
        self.register_fn(crate::engine::FN_IDX_SET, set_fn)
    }
    /// Short-hand for registering both index getter and setter functions for a custom type with the [`Engine`].
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`][crate::Array], [`Map`][crate::Map], [`String`],
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
    ///     fn new() -> Self {
    ///         Self { fields: vec![1, 2, 3, 4, 5] }
    ///     }
    ///     // Even a getter must start with `&mut self` and not `&self`.
    ///     fn get_field(&mut self, index: i64) -> i64 {
    ///         self.fields[index as usize]
    ///     }
    ///     fn set_field(&mut self, index: i64, value: i64) {
    ///         self.fields[index as usize] = value;
    ///     }
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
    pub fn register_indexer_get_set<
        T: Variant + Clone,
        IDX: Variant + Clone,
        const X1: bool,
        const X2: bool,
        R: Variant + Clone,
        const F1: bool,
        const F2: bool,
    >(
        &mut self,
        get_fn: impl RhaiNativeFunc<(Mut<T>, IDX), 2, X1, R, F1> + SendSync + 'static,
        set_fn: impl RhaiNativeFunc<(Mut<T>, IDX, R), 3, X2, (), F2> + SendSync + 'static,
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
    pub fn register_global_module(&mut self, module: SharedModule) -> &mut Self {
        // Make sure the global namespace is created.
        let _ = self.global_namespace_mut();

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
        name: impl AsRef<str>,
        module: SharedModule,
    ) -> &mut Self {
        use std::collections::BTreeMap;

        fn register_static_module_raw(
            root: &mut BTreeMap<Identifier, SharedModule>,
            name: &str,
            module: SharedModule,
        ) {
            let separator = crate::engine::NAMESPACE_SEPARATOR;

            if name.contains(separator) {
                let mut iter = name.splitn(2, separator);
                let sub_module = iter.next().unwrap().trim();
                let remainder = iter.next().unwrap().trim();

                if root.is_empty() || !root.contains_key(sub_module) {
                    let mut m = Module::new();
                    register_static_module_raw(m.get_sub_modules_mut(), remainder, module);
                    m.build_index();
                    root.insert(sub_module.into(), m.into());
                } else {
                    let m = root.remove(sub_module).unwrap();
                    let mut m = crate::func::shared_take_or_clone(m);
                    register_static_module_raw(m.get_sub_modules_mut(), remainder, module);
                    m.build_index();
                    root.insert(sub_module.into(), m.into());
                }
            } else if module.is_indexed() {
                root.insert(name.into(), module);
            } else {
                // Index the module (making a clone copy if necessary) if it is not indexed
                let mut module = crate::func::shared_take_or_clone(module);
                module.build_index();
                root.insert(name.into(), module.into());
            }
        }

        register_static_module_raw(&mut self.global_sub_modules, name.as_ref(), module);
        self
    }
    /// _(metadata)_ Generate a list of all registered functions.
    /// Exported under the `metadata` feature only.
    ///
    /// Functions from the following sources are included, in order:
    /// 1) Functions registered into the global namespace
    /// 2) Functions in registered sub-modules
    /// 3) Functions in registered packages
    /// 4) Functions in standard packages (optional)
    #[cfg(feature = "metadata")]
    #[inline]
    #[must_use]
    pub fn gen_fn_signatures(&self, include_packages: bool) -> Vec<String> {
        let mut signatures = Vec::with_capacity(64);

        if let Some(global_namespace) = self.global_modules.first() {
            signatures.extend(
                global_namespace.gen_fn_signatures_with_mapper(|s| self.format_param_type(s)),
            );
        }

        #[cfg(not(feature = "no_module"))]
        for (name, m) in &self.global_sub_modules {
            signatures.extend(
                m.gen_fn_signatures_with_mapper(|s| self.format_param_type(s))
                    .map(|f| format!("{name}::{f}")),
            );
        }

        let exclude_flags = if include_packages {
            crate::module::ModuleFlags::INTERNAL
        } else {
            crate::module::ModuleFlags::INTERNAL | crate::module::ModuleFlags::STANDARD_LIB
        };

        signatures.extend(
            self.global_modules
                .iter()
                .skip(1)
                .filter(|m| !m.flags.intersects(exclude_flags))
                .flat_map(|m| m.gen_fn_signatures_with_mapper(|s| self.format_param_type(s))),
        );

        signatures
    }
}
