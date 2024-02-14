//! Module defining external-loaded modules for Rhai.

#[cfg(feature = "metadata")]
use crate::api::formatting::format_param_type_for_display;
use crate::ast::FnAccess;
use crate::func::{
    shared_take_or_clone, FnIterator, RhaiFunc, RhaiNativeFunc, SendSync, StraightHashMap,
};
use crate::types::{dynamic::Variant, BloomFilterU64, CustomTypeInfo, CustomTypesCollection};
use crate::{
    calc_fn_hash, calc_fn_hash_full, Dynamic, Engine, FnArgsVec, Identifier, ImmutableString,
    RhaiResultOf, Shared, SharedModule, SmartString,
};
use bitflags::bitflags;
#[cfg(feature = "no_std")]
use hashbrown::hash_map::Entry;
#[cfg(not(feature = "no_std"))]
use std::collections::hash_map::Entry;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::{type_name, TypeId},
    collections::BTreeMap,
    fmt,
    ops::{Add, AddAssign},
};

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
use crate::func::register::Mut;

/// Initial capacity of the hashmap for functions.
const FN_MAP_SIZE: usize = 16;

/// A type representing the namespace of a function.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(
    feature = "metadata",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
#[non_exhaustive]
pub enum FnNamespace {
    /// Module namespace only.
    ///
    /// Ignored under `no_module`.
    Internal,
    /// Expose to global namespace.
    Global,
}

impl FnNamespace {
    /// Is this a module namespace?
    #[inline(always)]
    #[must_use]
    pub const fn is_module_namespace(self) -> bool {
        match self {
            Self::Internal => true,
            Self::Global => false,
        }
    }
    /// Is this a global namespace?
    #[inline(always)]
    #[must_use]
    pub const fn is_global_namespace(self) -> bool {
        match self {
            Self::Internal => false,
            Self::Global => true,
        }
    }
}

/// A type containing the metadata of a single registered function.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub struct FuncMetadata {
    /// Hash value.
    pub hash: u64,
    /// Function namespace.
    pub namespace: FnNamespace,
    /// Function access mode.
    pub access: FnAccess,
    /// Function name.
    pub name: Identifier,
    /// Number of parameters.
    pub num_params: usize,
    /// Parameter types (if applicable).
    pub param_types: FnArgsVec<TypeId>,
    /// Parameter names and types (if available).
    #[cfg(feature = "metadata")]
    pub params_info: FnArgsVec<Identifier>,
    /// Return type name.
    #[cfg(feature = "metadata")]
    pub return_type: Identifier,
    /// Comments.
    #[cfg(feature = "metadata")]
    pub comments: crate::StaticVec<SmartString>,
}

impl FuncMetadata {
    /// _(metadata)_ Generate a signature of the function.
    /// Exported under the `metadata` feature only.
    #[cfg(feature = "metadata")]
    #[must_use]
    pub fn gen_signature<'a>(
        &'a self,
        type_mapper: impl Fn(&'a str) -> std::borrow::Cow<'a, str>,
    ) -> String {
        let mut signature = format!("{}(", self.name);

        let return_type = format_param_type_for_display(&self.return_type, true);

        if self.params_info.is_empty() {
            for x in 0..self.num_params {
                signature.push('_');
                if x < self.num_params - 1 {
                    signature.push_str(", ");
                }
            }
        } else {
            let params = self
                .params_info
                .iter()
                .map(|param| {
                    let mut segment = param.splitn(2, ':');
                    let name = match segment.next().unwrap().trim() {
                        "" => "_".into(),
                        s => s,
                    };
                    let result: std::borrow::Cow<_> = segment.next().map_or_else(
                        || name.into(),
                        |typ| {
                            format!(
                                "{name}: {}",
                                format_param_type_for_display(&type_mapper(typ), false)
                            )
                            .into()
                        },
                    );
                    result
                })
                .collect::<crate::FnArgsVec<_>>();
            signature.push_str(&params.join(", "));
        }
        signature.push(')');

        if !return_type.is_empty() {
            signature.push_str(" -> ");
            signature.push_str(&return_type);
        }

        signature
    }
}

/// _(internals)_ Calculate a [`u64`] hash key from a namespace-qualified function name and parameter types.
/// Exported under the `internals` feature only.
///
/// Module names are passed in via `&str` references from an iterator.
/// Parameter types are passed in via [`TypeId`] values from an iterator.
///
/// # Note
///
/// The first module name is skipped.  Hashing starts from the _second_ module in the chain.
#[inline]
pub fn calc_native_fn_hash<'a>(
    modules: impl IntoIterator<Item = &'a str, IntoIter = impl ExactSizeIterator<Item = &'a str>>,
    fn_name: &str,
    params: &[TypeId],
) -> u64 {
    calc_fn_hash_full(
        calc_fn_hash(modules, fn_name, params.len()),
        params.iter().copied(),
    )
}

/// Type for fine-tuned module function registration.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct FuncRegistration {
    /// Function metadata.
    metadata: FuncMetadata,
    /// Is the function pure?
    purity: Option<bool>,
    /// Is the function volatile?
    volatility: Option<bool>,
}

impl FuncRegistration {
    /// Create a new [`FuncRegistration`].
    ///
    /// # Defaults
    ///
    /// * **Accessibility**: The function namespace is [`FnNamespace::Internal`].
    ///
    /// * **Purity**: The function is assumed to be _pure_ unless it is a property setter or an index setter.
    ///
    /// * **Volatility**: The function is assumed to be _non-volatile_ -- i.e. it guarantees the same result for the same input(s).
    ///
    /// * **Metadata**: No metadata for the function is registered.
    ///
    /// ```
    /// # use rhai::{Module, FuncRegistration, FnNamespace};
    /// let mut module = Module::new();
    ///
    /// fn inc(x: i64) -> i64 { x + 1 }
    ///
    /// let f = FuncRegistration::new("inc")
    ///     .with_namespace(FnNamespace::Global)
    ///     .set_into_module(&mut module, inc);
    ///
    /// let hash = f.hash;
    ///
    /// assert!(module.contains_fn(hash));
    /// ```
    pub fn new(name: impl Into<Identifier>) -> Self {
        Self {
            metadata: FuncMetadata {
                hash: 0,
                name: name.into(),
                namespace: FnNamespace::Internal,
                access: FnAccess::Public,
                num_params: 0,
                param_types: <_>::default(),
                #[cfg(feature = "metadata")]
                params_info: <_>::default(),
                #[cfg(feature = "metadata")]
                return_type: "".into(),
                #[cfg(feature = "metadata")]
                comments: <_>::default(),
            },
            purity: None,
            volatility: None,
        }
    }
    /// Create a new [`FuncRegistration`] for a property getter.
    ///
    /// Not available under `no_object`.
    ///
    /// # Defaults
    ///
    /// * **Accessibility**: The function namespace is [`FnNamespace::Global`].
    ///
    /// * **Purity**: The function is assumed to be _pure_.
    ///
    /// * **Volatility**: The function is assumed to be _non-volatile_ -- i.e. it guarantees the same result for the same input(s).
    ///
    /// * **Metadata**: No metadata for the function is registered.
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn new_getter(prop: impl AsRef<str>) -> Self {
        Self::new(crate::engine::make_getter(prop.as_ref())).with_namespace(FnNamespace::Global)
    }
    /// Create a new [`FuncRegistration`] for a property setter.
    ///
    /// Not available under `no_object`.
    ///
    /// # Defaults
    ///
    /// * **Accessibility**: The function namespace is [`FnNamespace::Global`].
    ///
    /// * **Purity**: The function is assumed to be _no-pure_.
    ///
    /// * **Volatility**: The function is assumed to be _non-volatile_ -- i.e. it guarantees the same result for the same input(s).
    ///
    /// * **Metadata**: No metadata for the function is registered.
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn new_setter(prop: impl AsRef<str>) -> Self {
        Self::new(crate::engine::make_setter(prop.as_ref()))
            .with_namespace(FnNamespace::Global)
            .with_purity(false)
    }
    /// Create a new [`FuncRegistration`] for an index getter.
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Defaults
    ///
    /// * **Accessibility**: The function namespace is [`FnNamespace::Global`].
    ///
    /// * **Purity**: The function is assumed to be _pure_.
    ///
    /// * **Volatility**: The function is assumed to be _non-volatile_ -- i.e. it guarantees the same result for the same input(s).
    ///
    /// * **Metadata**: No metadata for the function is registered.
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline(always)]
    pub fn new_index_getter() -> Self {
        Self::new(crate::engine::FN_IDX_GET).with_namespace(FnNamespace::Global)
    }
    /// Create a new [`FuncRegistration`] for an index setter.
    ///
    /// Not available under both `no_index` and `no_object`.
    ///
    /// # Defaults
    ///
    /// * **Accessibility**: The function namespace is [`FnNamespace::Global`].
    ///
    /// * **Purity**: The function is assumed to be _no-pure_.
    ///
    /// * **Volatility**: The function is assumed to be _non-volatile_ -- i.e. it guarantees the same result for the same input(s).
    ///
    /// * **Metadata**: No metadata for the function is registered.
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline(always)]
    pub fn new_index_setter() -> Self {
        Self::new(crate::engine::FN_IDX_SET)
            .with_namespace(FnNamespace::Global)
            .with_purity(false)
    }
    /// Set the [namespace][`FnNamespace`] of the function.
    pub fn with_namespace(mut self, namespace: FnNamespace) -> Self {
        self.metadata.namespace = namespace;
        self
    }
    /// Set whether the function is _pure_.
    /// A pure function has no side effects.
    pub fn with_purity(mut self, pure: bool) -> Self {
        self.purity = Some(pure);
        self
    }
    /// Set whether the function is _volatile_.
    /// A volatile function does not guarantee the same result for the same input(s).
    pub fn with_volatility(mut self, volatile: bool) -> Self {
        self.volatility = Some(volatile);
        self
    }
    /// _(metadata)_ Set the function's parameter names and/or types.
    /// Exported under the `metadata` feature only.
    ///
    /// The input is a list of strings, each of which is either a parameter name or a parameter name/type pair.
    ///
    /// The _last_ string should be the _type_ of the return value.
    ///
    /// # Parameter Examples
    ///
    /// `"foo: &str"`   <- parameter name = `foo`, type = `&str`  
    /// `"bar"`         <- parameter name = `bar`, type unknown  
    /// `"_: i64"`      <- parameter name unknown, type = `i64`  
    /// `"MyType"`      <- parameter name unknown, type = `MyType`  
    #[cfg(feature = "metadata")]
    pub fn with_params_info<S: AsRef<str>>(mut self, params: impl IntoIterator<Item = S>) -> Self {
        self.metadata.params_info = params.into_iter().map(|s| s.as_ref().into()).collect();
        self
    }
    /// _(metadata)_ Set the function's doc-comments.
    /// Exported under the `metadata` feature only.
    ///
    /// The input is a list of strings, each of which is either a block of single-line doc-comments
    /// or a single block doc-comment.
    ///
    /// ## Comments
    ///
    /// Single-line doc-comments typically start with `///` and should be merged, with line-breaks,
    /// into a single string without a final termination line-break.
    ///
    /// Block doc-comments typically start with `/**` and end with `*/` and should be kept in a
    /// separate string.
    ///
    /// Leading white-spaces should be stripped, and each string should always start with the
    /// corresponding doc-comment leader: `///` or `/**`.
    ///
    /// Each line in non-block doc-comments should start with `///`.
    #[cfg(feature = "metadata")]
    pub fn with_comments<S: AsRef<str>>(mut self, comments: impl IntoIterator<Item = S>) -> Self {
        self.metadata.comments = comments.into_iter().map(|s| s.as_ref().into()).collect();
        self
    }
    /// Register the function into the specified [`Engine`].
    #[inline]
    pub fn register_into_engine<A: 'static, const N: usize, const X: bool, R, const F: bool, FUNC>(
        self,
        engine: &mut Engine,
        func: FUNC,
    ) -> &FuncMetadata
    where
        R: Variant + Clone,
        FUNC: RhaiNativeFunc<A, N, X, R, F> + SendSync + 'static,
    {
        self.with_namespace(FnNamespace::Global)
            .set_into_module(engine.global_namespace_mut(), func)
    }
    /// Register the function into the specified [`Module`].
    #[inline]
    pub fn set_into_module<A: 'static, const N: usize, const X: bool, R, const F: bool, FUNC>(
        self,
        module: &mut Module,
        func: FUNC,
    ) -> &FuncMetadata
    where
        R: Variant + Clone,
        FUNC: RhaiNativeFunc<A, N, X, R, F> + SendSync + 'static,
    {
        let is_pure = self.purity.unwrap_or_else(|| {
            // default to pure unless specified
            let is_pure = true;

            #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
            let is_pure = is_pure
                && (FUNC::num_params() != 3 || self.metadata.name != crate::engine::FN_IDX_SET);

            #[cfg(not(feature = "no_object"))]
            let is_pure = is_pure
                && (FUNC::num_params() != 2
                    || !self.metadata.name.starts_with(crate::engine::FN_SET));
            is_pure
        });
        let is_volatile = self.volatility.unwrap_or(false);

        let func = func.into_rhai_function(is_pure, is_volatile);

        // Clear flags
        let mut reg = self;
        reg.purity = None;
        reg.volatility = None;

        reg.set_into_module_raw(module, FUNC::param_types(), func)
    }
    /// Register the function into the specified [`Module`].
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.  It takes a list of [`TypeId`][std::any::TypeId]'s
    /// indicating the actual types of the parameters.
    ///
    /// # Panics
    ///
    /// Panics if the type of the first parameter is [`Array`][crate::Array], [`Map`][crate::Map],
    /// [`String`], [`ImmutableString`][crate::ImmutableString], `&str` or [`INT`][crate::INT] and
    /// the function name indicates that it is an index getter or setter.
    ///
    /// Indexers for arrays, object maps, strings and integers cannot be registered.
    #[inline]
    pub fn set_into_module_raw(
        self,
        module: &mut Module,
        arg_types: impl AsRef<[TypeId]>,
        func: RhaiFunc,
    ) -> &FuncMetadata {
        // Make sure that conflicting flags should not be set.
        debug_assert!(self.purity.is_none());
        debug_assert!(self.volatility.is_none());

        let mut f = self.metadata;

        f.num_params = arg_types.as_ref().len();
        f.param_types.extend(arg_types.as_ref().iter().copied());

        #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
        if (f.name == crate::engine::FN_IDX_GET && f.num_params == 2)
            || (f.name == crate::engine::FN_IDX_SET && f.num_params == 3)
        {
            if let Some(&type_id) = f.param_types.first() {
                #[cfg(not(feature = "no_index"))]
                assert!(
                    type_id != TypeId::of::<crate::Array>(),
                    "Cannot register indexer for arrays."
                );

                #[cfg(not(feature = "no_object"))]
                assert!(
                    type_id != TypeId::of::<crate::Map>(),
                    "Cannot register indexer for object maps."
                );

                assert!(
                    type_id != TypeId::of::<String>()
                        && type_id != TypeId::of::<&str>()
                        && type_id != TypeId::of::<crate::ImmutableString>(),
                    "Cannot register indexer for strings."
                );

                assert!(
                    type_id != TypeId::of::<crate::INT>(),
                    "Cannot register indexer for integers."
                );
            }
        }

        let is_method = func.is_method();

        f.param_types
            .iter_mut()
            .enumerate()
            .for_each(|(i, type_id)| *type_id = Module::map_type(!is_method || i > 0, *type_id));

        let is_dynamic = f
            .param_types
            .iter()
            .any(|&type_id| type_id == TypeId::of::<Dynamic>());

        #[cfg(feature = "metadata")]
        if f.params_info.len() > f.param_types.len() {
            f.return_type = f.params_info.pop().unwrap();
        }

        let hash_base = calc_fn_hash(None, &f.name, f.param_types.len());
        let hash_fn = calc_fn_hash_full(hash_base, f.param_types.iter().copied());
        f.hash = hash_fn;

        // Catch hash collisions in testing environment only.
        #[cfg(feature = "testing-environ")]
        if let Some(fx) = module.functions.as_ref().and_then(|f| f.get(&hash_base)) {
            unreachable!(
                "Hash {} already exists when registering function {}:\n{:#?}",
                hash_base, f.name, fx
            );
        }

        if is_dynamic {
            module.dynamic_functions_filter.mark(hash_base);
        }

        module
            .flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);

        let entry = match module
            .functions
            .get_or_insert_with(|| new_hash_map(FN_MAP_SIZE))
            .entry(hash_fn)
        {
            Entry::Occupied(mut entry) => {
                entry.insert((func, f.into()));
                entry.into_mut()
            }
            Entry::Vacant(entry) => entry.insert((func, f.into())),
        };

        &*entry.1
    }
}

bitflags! {
    /// Bit-flags containing all status for [`Module`].
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    pub struct ModuleFlags: u8 {
        /// Is the [`Module`] internal?
        const INTERNAL = 0b0000_0001;
        /// Is the [`Module`] part of a standard library?
        const STANDARD_LIB = 0b0000_0010;
        /// Is the [`Module`] indexed?
        const INDEXED = 0b0000_0100;
        /// Does the [`Module`] contain indexed functions that have been exposed to the global namespace?
        const INDEXED_GLOBAL_FUNCTIONS = 0b0000_1000;
    }
}

/// A module which may contain variables, sub-modules, external Rust functions,
/// and/or script-defined functions.
#[derive(Clone)]
pub struct Module {
    /// ID identifying the module.
    id: Option<ImmutableString>,
    /// Module documentation.
    #[cfg(feature = "metadata")]
    doc: SmartString,
    /// Custom types.
    custom_types: CustomTypesCollection,
    /// Sub-modules.
    modules: BTreeMap<Identifier, SharedModule>,
    /// [`Module`] variables.
    variables: BTreeMap<Identifier, Dynamic>,
    /// Flattened collection of all [`Module`] variables, including those in sub-modules.
    all_variables: Option<StraightHashMap<Dynamic>>,
    /// Functions (both native Rust and scripted).
    functions: Option<StraightHashMap<(RhaiFunc, Box<FuncMetadata>)>>,
    /// Flattened collection of all functions, native Rust and scripted.
    /// including those in sub-modules.
    all_functions: Option<StraightHashMap<RhaiFunc>>,
    /// Bloom filter on native Rust functions (in scripted hash format) that contain [`Dynamic`] parameters.
    dynamic_functions_filter: BloomFilterU64,
    /// Iterator functions, keyed by the type producing the iterator.
    type_iterators: BTreeMap<TypeId, Shared<FnIterator>>,
    /// Flattened collection of iterator functions, including those in sub-modules.
    all_type_iterators: BTreeMap<TypeId, Shared<FnIterator>>,
    /// Flags.
    pub(crate) flags: ModuleFlags,
}

impl Default for Module {
    #[inline(always)]
    #[must_use]
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Module {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Module");

        d.field("id", &self.id)
            .field(
                "custom_types",
                &self.custom_types.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            )
            .field(
                "modules",
                &self
                    .modules
                    .keys()
                    .map(SmartString::as_str)
                    .collect::<Vec<_>>(),
            )
            .field("vars", &self.variables)
            .field(
                "functions",
                &self
                    .iter_fn()
                    .map(|(_f, _m)| {
                        #[cfg(not(feature = "metadata"))]
                        return _f.to_string();
                        #[cfg(feature = "metadata")]
                        return _m.gen_signature(|s| s.into());
                    })
                    .collect::<Vec<_>>(),
            )
            .field("flags", &self.flags);

        #[cfg(feature = "metadata")]
        d.field("doc", &self.doc);

        d.finish()
    }
}

#[cfg(not(feature = "no_function"))]
impl<T: IntoIterator<Item = Shared<crate::ast::ScriptFuncDef>>> From<T> for Module {
    fn from(iter: T) -> Self {
        let mut module = Self::new();
        iter.into_iter().for_each(|fn_def| {
            module.set_script_fn(fn_def);
        });
        module
    }
}

impl<M: AsRef<Module>> Add<M> for &Module {
    type Output = Module;

    #[inline]
    fn add(self, rhs: M) -> Self::Output {
        let mut module = self.clone();
        module.merge(rhs.as_ref());
        module
    }
}

impl<M: AsRef<Self>> Add<M> for Module {
    type Output = Self;

    #[inline(always)]
    fn add(mut self, rhs: M) -> Self::Output {
        self.merge(rhs.as_ref());
        self
    }
}

impl<M: Into<Self>> AddAssign<M> for Module {
    #[inline(always)]
    fn add_assign(&mut self, rhs: M) {
        self.combine(rhs.into());
    }
}

#[inline(always)]
fn new_hash_map<T>(size: usize) -> StraightHashMap<T> {
    StraightHashMap::with_capacity_and_hasher(size, <_>::default())
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
    pub const fn new() -> Self {
        Self {
            id: None,
            #[cfg(feature = "metadata")]
            doc: SmartString::new_const(),
            custom_types: CustomTypesCollection::new(),
            modules: BTreeMap::new(),
            variables: BTreeMap::new(),
            all_variables: None,
            functions: None,
            all_functions: None,
            dynamic_functions_filter: BloomFilterU64::new(),
            type_iterators: BTreeMap::new(),
            all_type_iterators: BTreeMap::new(),
            flags: ModuleFlags::INDEXED,
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
    #[inline]
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Get the ID of the [`Module`] as an [`Identifier`], if any.
    #[inline(always)]
    #[must_use]
    pub(crate) const fn id_raw(&self) -> Option<&ImmutableString> {
        self.id.as_ref()
    }

    /// Set the ID of the [`Module`].
    ///
    /// If the string is empty, it is equivalent to clearing the ID.
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
    pub fn set_id(&mut self, id: impl Into<ImmutableString>) -> &mut Self {
        let id = id.into();
        self.id = (!id.is_empty()).then_some(id);
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

    /// Get the documentation of the [`Module`], if any.
    /// Exported under the `metadata` feature only.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// module.set_doc("//! This is my special module.");
    /// assert_eq!(module.doc(), "//! This is my special module.");
    /// ```
    #[cfg(feature = "metadata")]
    #[inline(always)]
    #[must_use]
    pub fn doc(&self) -> &str {
        &self.doc
    }

    /// Set the documentation of the [`Module`].
    /// Exported under the `metadata` feature only.
    ///
    /// If the string is empty, it is equivalent to clearing the documentation.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// module.set_doc("//! This is my special module.");
    /// assert_eq!(module.doc(), "//! This is my special module.");
    /// ```
    #[cfg(feature = "metadata")]
    #[inline(always)]
    pub fn set_doc(&mut self, doc: impl Into<crate::SmartString>) -> &mut Self {
        self.doc = doc.into();
        self
    }

    /// Clear the documentation of the [`Module`].
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// module.set_doc("//! This is my special module.");
    /// assert_eq!(module.doc(), "//! This is my special module.");
    /// module.clear_doc();
    /// assert_eq!(module.doc(), "");
    /// ```
    #[cfg(feature = "metadata")]
    #[inline(always)]
    pub fn clear_doc(&mut self) -> &mut Self {
        self.doc.clear();
        self
    }

    /// Clear the [`Module`].
    #[inline(always)]
    pub fn clear(&mut self) {
        #[cfg(feature = "metadata")]
        self.doc.clear();
        self.custom_types.clear();
        self.modules.clear();
        self.variables.clear();
        self.all_variables = None;
        self.functions = None;
        self.all_functions = None;
        self.dynamic_functions_filter.clear();
        self.type_iterators.clear();
        self.all_type_iterators.clear();
        self.flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);
    }

    /// Map a custom type to a friendly display name.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// #[derive(Clone)]
    /// struct TestStruct;
    ///
    /// let name = std::any::type_name::<TestStruct>();
    ///
    /// let mut module = Module::new();
    ///
    /// module.set_custom_type::<TestStruct>("MyType");
    ///
    /// assert_eq!(module.get_custom_type_display_by_name(name), Some("MyType"));
    /// ```
    #[inline(always)]
    pub fn set_custom_type<T>(&mut self, name: &str) -> &mut Self {
        self.custom_types.add_type::<T>(name);
        self
    }
    /// Map a custom type to a friendly display name.
    /// Exported under the `metadata` feature only.
    ///
    /// ## Comments
    ///
    /// Block doc-comments should be kept in a separate string slice.
    ///
    /// Line doc-comments should be merged, with line-breaks, into a single string slice without a final termination line-break.
    ///
    /// Leading white-spaces should be stripped, and each string slice always starts with the corresponding
    /// doc-comment leader: `///` or `/**`.
    ///
    /// Each line in non-block doc-comments should start with `///`.
    #[cfg(feature = "metadata")]
    #[inline(always)]
    pub fn set_custom_type_with_comments<T>(&mut self, name: &str, comments: &[&str]) -> &mut Self {
        self.custom_types
            .add_type_with_comments::<T>(name, comments);
        self
    }
    /// Map a custom type to a friendly display name.
    ///
    /// ```
    /// # use rhai::Module;
    /// #[derive(Clone)]
    /// struct TestStruct;
    ///
    /// let name = std::any::type_name::<TestStruct>();
    ///
    /// let mut module = Module::new();
    ///
    /// module.set_custom_type_raw(name, "MyType");
    ///
    /// assert_eq!(module.get_custom_type_display_by_name(name), Some("MyType"));
    /// ```
    #[inline(always)]
    pub fn set_custom_type_raw(
        &mut self,
        type_name: impl Into<Identifier>,
        display_name: impl Into<Identifier>,
    ) -> &mut Self {
        self.custom_types.add(type_name, display_name);
        self
    }
    /// Map a custom type to a friendly display name.
    /// Exported under the `metadata` feature only.
    ///
    /// ## Comments
    ///
    /// Block doc-comments should be kept in a separate string slice.
    ///
    /// Line doc-comments should be merged, with line-breaks, into a single string slice without a final termination line-break.
    ///
    /// Leading white-spaces should be stripped, and each string slice always starts with the corresponding
    /// doc-comment leader: `///` or `/**`.
    ///
    /// Each line in non-block doc-comments should start with `///`.
    #[cfg(feature = "metadata")]
    #[inline(always)]
    pub fn set_custom_type_with_comments_raw<C: Into<SmartString>>(
        &mut self,
        type_name: impl Into<Identifier>,
        display_name: impl Into<Identifier>,
        comments: impl IntoIterator<Item = C>,
    ) -> &mut Self {
        self.custom_types
            .add_with_comments(type_name, display_name, comments);
        self
    }
    /// Get the display name of a registered custom type.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// #[derive(Clone)]
    /// struct TestStruct;
    ///
    /// let name = std::any::type_name::<TestStruct>();
    ///
    /// let mut module = Module::new();
    ///
    /// module.set_custom_type::<TestStruct>("MyType");
    ///
    /// assert_eq!(module.get_custom_type_display_by_name(name), Some("MyType"));
    /// ```
    #[inline]
    #[must_use]
    pub fn get_custom_type_display_by_name(&self, type_name: &str) -> Option<&str> {
        self.get_custom_type_by_name_raw(type_name)
            .map(|typ| typ.display_name.as_str())
    }
    /// Get the display name of a registered custom type.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// #[derive(Clone)]
    /// struct TestStruct;
    ///
    /// let name = std::any::type_name::<TestStruct>();
    ///
    /// let mut module = Module::new();
    ///
    /// module.set_custom_type::<TestStruct>("MyType");
    ///
    /// assert_eq!(module.get_custom_type_display::<TestStruct>(), Some("MyType"));
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn get_custom_type_display<T>(&self) -> Option<&str> {
        self.get_custom_type_display_by_name(type_name::<T>())
    }
    /// _(internals)_ Get a registered custom type .
    /// Exported under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn get_custom_type_raw<T>(&self) -> Option<&CustomTypeInfo> {
        self.get_custom_type_by_name_raw(type_name::<T>())
    }
    /// Get a registered custom type .
    #[cfg(not(feature = "internals"))]
    #[inline(always)]
    #[must_use]
    pub fn get_custom_type_raw<T>(&self) -> Option<&CustomTypeInfo> {
        self.get_custom_type_by_name_raw(type_name::<T>())
    }
    /// _(internals)_ Get a registered custom type by its type name.
    /// Exported under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn get_custom_type_by_name_raw(&self, type_name: &str) -> Option<&CustomTypeInfo> {
        self.custom_types.get(type_name)
    }
    /// Get a registered custom type by its type name.
    #[cfg(not(feature = "internals"))]
    #[inline(always)]
    #[must_use]
    fn get_custom_type_by_name_raw(&self, type_name: &str) -> Option<&CustomTypeInfo> {
        self.custom_types.get(type_name)
    }

    /// Returns `true` if this [`Module`] contains no items.
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
        !self.flags.intersects(ModuleFlags::INDEXED_GLOBAL_FUNCTIONS)
            && self
                .functions
                .as_ref()
                .map_or(true, StraightHashMap::is_empty)
            && self.variables.is_empty()
            && self.modules.is_empty()
            && self.type_iterators.is_empty()
            && self
                .all_functions
                .as_ref()
                .map_or(true, StraightHashMap::is_empty)
            && self
                .all_variables
                .as_ref()
                .map_or(true, StraightHashMap::is_empty)
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
        self.flags.intersects(ModuleFlags::INDEXED)
    }

    /// _(metadata)_ Generate signatures for all the non-private functions in the [`Module`].
    /// Exported under the `metadata` feature only.
    #[cfg(feature = "metadata")]
    #[inline]
    pub fn gen_fn_signatures_with_mapper<'a>(
        &'a self,
        type_mapper: impl Fn(&'a str) -> std::borrow::Cow<'a, str> + 'a,
    ) -> impl Iterator<Item = String> + 'a {
        self.iter_fn()
            .map(|(_, m)| m)
            .filter(|&f| match f.access {
                FnAccess::Public => true,
                FnAccess::Private => false,
            })
            .map(move |m| m.gen_signature(&type_mapper))
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
    #[inline]
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

        if self.is_indexed() {
            let hash_var = crate::calc_var_hash(Some(""), &ident);

            // Catch hash collisions in testing environment only.
            #[cfg(feature = "testing-environ")]
            assert!(
                self.all_variables
                    .as_ref()
                    .map_or(true, |map| !map.contains_key(&hash_var)),
                "Hash {} already exists when registering variable {}",
                hash_var,
                ident
            );

            self.all_variables
                .get_or_insert_with(Default::default)
                .insert(hash_var, value.clone());
        }
        self.variables.insert(ident, value);
        self
    }

    /// Get a namespace-qualified [`Module`] variable as a [`Dynamic`].
    #[cfg(not(feature = "no_module"))]
    #[inline]
    pub(crate) fn get_qualified_var(&self, hash_var: u64) -> Option<Dynamic> {
        self.all_variables
            .as_ref()
            .and_then(|c| c.get(&hash_var).cloned())
    }

    /// Set a script-defined function into the [`Module`].
    ///
    /// If there is an existing function of the same name and number of arguments, it is replaced.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub fn set_script_fn(&mut self, fn_def: impl Into<Shared<crate::ast::ScriptFuncDef>>) -> u64 {
        let fn_def = fn_def.into();

        // None + function name + number of arguments.
        let namespace = FnNamespace::Internal;
        let num_params = fn_def.params.len();
        let hash_script = crate::calc_fn_hash(None, &fn_def.name, num_params);
        #[cfg(not(feature = "no_object"))]
        let (hash_script, namespace) =
            fn_def
                .this_type
                .as_ref()
                .map_or((hash_script, namespace), |this_type| {
                    (
                        crate::calc_typed_method_hash(hash_script, this_type),
                        FnNamespace::Global,
                    )
                });

        // Catch hash collisions in testing environment only.
        #[cfg(feature = "testing-environ")]
        if let Some(f) = self.functions.as_ref().and_then(|f| f.get(&hash_script)) {
            unreachable!(
                "Hash {} already exists when registering function {:#?}:\n{:#?}",
                hash_script, fn_def, f
            );
        }

        let metadata = FuncMetadata {
            hash: hash_script,
            name: fn_def.name.as_str().into(),
            namespace,
            access: fn_def.access,
            num_params,
            param_types: FnArgsVec::new_const(),
            #[cfg(feature = "metadata")]
            params_info: fn_def.params.iter().map(Into::into).collect(),
            #[cfg(feature = "metadata")]
            return_type: <_>::default(),
            #[cfg(feature = "metadata")]
            comments: crate::StaticVec::new_const(),
        };

        self.functions
            .get_or_insert_with(|| new_hash_map(FN_MAP_SIZE))
            .insert(hash_script, (fn_def.into(), metadata.into()));

        self.flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);

        hash_script
    }

    /// Get a shared reference to the script-defined function in the [`Module`] based on name
    /// and number of parameters.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    #[must_use]
    pub fn get_script_fn(
        &self,
        name: impl AsRef<str>,
        num_params: usize,
    ) -> Option<&Shared<crate::ast::ScriptFuncDef>> {
        self.functions.as_ref().and_then(|lib| {
            let name = name.as_ref();

            lib.values()
                .find(|(_, f)| f.num_params == num_params && f.name == name)
                .and_then(|(f, _)| f.get_script_fn_def())
        })
    }

    /// Get a mutable reference to the underlying [`BTreeMap`] of sub-modules,
    /// creating one if empty.
    ///
    /// # WARNING
    ///
    /// By taking a mutable reference, it is assumed that some sub-modules will be modified.
    /// Thus the [`Module`] is automatically set to be non-indexed.
    #[cfg(not(feature = "no_module"))]
    #[inline]
    #[must_use]
    pub(crate) fn get_sub_modules_mut(&mut self) -> &mut BTreeMap<Identifier, SharedModule> {
        // We must assume that the user has changed the sub-modules
        // (otherwise why take a mutable reference?)
        self.all_functions = None;
        self.all_variables = None;
        self.all_type_iterators.clear();
        self.flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);

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
    #[inline]
    #[must_use]
    pub fn get_sub_module(&self, name: &str) -> Option<&Self> {
        self.modules.get(name).map(|m| &**m)
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
        sub_module: impl Into<SharedModule>,
    ) -> &mut Self {
        self.modules.insert(name.into(), sub_module.into());
        self.flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);
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
    /// let hash = module.set_native_fn("calc", |x: i64| Ok(42 + x));
    /// assert!(module.contains_fn(hash));
    /// ```
    #[inline]
    #[must_use]
    pub fn contains_fn(&self, hash_fn: u64) -> bool {
        self.functions
            .as_ref()
            .map_or(false, |m| m.contains_key(&hash_fn))
    }

    /// _(metadata)_ Update the metadata (parameter names/types, return type and doc-comments) of a registered function.
    /// Exported under the `metadata` feature only.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use the [`FuncRegistration`] API instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.17.0", note = "use the `FuncRegistration` API instead")]
    #[cfg(feature = "metadata")]
    #[inline]
    pub fn update_fn_metadata_with_comments<A: Into<Identifier>, C: Into<SmartString>>(
        &mut self,
        hash_fn: u64,
        arg_names: impl IntoIterator<Item = A>,
        comments: impl IntoIterator<Item = C>,
    ) -> &mut Self {
        let mut params_info = arg_names
            .into_iter()
            .map(Into::into)
            .collect::<FnArgsVec<_>>();

        if let Some((_, f)) = self.functions.as_mut().and_then(|m| m.get_mut(&hash_fn)) {
            let (params_info, return_type_name) = if params_info.len() > f.num_params {
                let return_type = params_info.pop().unwrap();
                (params_info, return_type)
            } else {
                (params_info, crate::SmartString::new_const())
            };
            f.params_info = params_info;
            f.return_type = return_type_name;
            f.comments = comments.into_iter().map(Into::into).collect();
        }

        self
    }

    /// Update the namespace of a registered function.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated.
    /// Use the [`FuncRegistration`] API instead.
    ///
    /// This method will be removed in the next major version.
    #[deprecated(since = "1.17.0", note = "use the `FuncRegistration` API instead")]
    #[inline]
    pub fn update_fn_namespace(&mut self, hash_fn: u64, namespace: FnNamespace) -> &mut Self {
        if let Some((_, f)) = self.functions.as_mut().and_then(|m| m.get_mut(&hash_fn)) {
            f.namespace = namespace;
            self.flags
                .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);
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

    /// Set a native Rust function into the [`Module`] based on a [`FuncRegistration`].
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.  It takes a list of [`TypeId`][std::any::TypeId]'s
    /// indicating the actual types of the parameters.
    #[inline(always)]
    pub fn set_fn_raw_with_options(
        &mut self,
        options: FuncRegistration,
        arg_types: impl AsRef<[TypeId]>,
        func: RhaiFunc,
    ) -> &FuncMetadata {
        options.set_into_module_raw(self, arg_types, func)
    }

    /// Set a native Rust function into the [`Module`], returning a [`u64`] hash key.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Use `FuncRegistration` API
    ///
    /// It is recommended that the [`FuncRegistration`] API be used instead.
    ///
    /// Essentially, this method is a shortcut for:
    ///
    /// ```text
    /// FuncRegistration::new(name)
    ///     .with_namespace(FnNamespace::Internal)
    ///     .with_purity(true)
    ///     .with_volatility(false)
    ///     .set_into_module(module, func)
    ///     .hash
    /// ```
    ///
    /// # Assumptions
    ///
    /// * **Accessibility**: The function namespace is [`FnNamespace::Internal`].
    ///
    /// * **Purity**: The function is assumed to be _pure_ unless it is a property setter or an index setter.
    ///
    /// * **Volatility**: The function is assumed to be _non-volatile_ -- i.e. it guarantees the same result for the same input(s).
    ///
    /// * **Metadata**: No metadata for the function is registered.
    ///
    /// To change these assumptions, use the [`FuncRegistration`] API instead.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// let hash = module.set_native_fn("calc", |x: i64| Ok(42 + x));
    /// assert!(module.contains_fn(hash));
    /// ```
    #[inline]
    pub fn set_native_fn<A: 'static, const N: usize, const X: bool, R, FUNC>(
        &mut self,
        name: impl Into<Identifier>,
        func: FUNC,
    ) -> u64
    where
        R: Variant + Clone,
        FUNC: RhaiNativeFunc<A, N, X, R, true> + SendSync + 'static,
    {
        FuncRegistration::new(name)
            .with_namespace(FnNamespace::Internal)
            .with_purity(true)
            .with_volatility(false)
            .set_into_module(self, func)
            .hash
    }

    /// Set a Rust getter function taking one mutable parameter, returning a [`u64`] hash key.
    /// This function is automatically exposed to the global namespace.
    ///
    /// If there is a similar existing Rust getter function, it is replaced.
    ///
    /// # Assumptions
    ///
    /// * **Accessibility**: The function namespace is [`FnNamespace::Global`].
    ///
    /// * **Purity**: The function is assumed to be _pure_ (so it can be called on constants).
    ///
    /// * **Volatility**: The function is assumed to be _non-volatile_ -- i.e. it guarantees the same result for the same input(s).
    ///
    /// * **Metadata**: No metadata for the function is registered.
    ///
    /// To change these assumptions, use the [`FuncRegistration`] API instead.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Module;
    /// let mut module = Module::new();
    /// let hash = module.set_getter_fn("value", |x: &mut i64| Ok(*x));
    /// assert!(module.contains_fn(hash));
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn set_getter_fn<A, const X: bool, R, FUNC>(
        &mut self,
        name: impl AsRef<str>,
        func: FUNC,
    ) -> u64
    where
        A: Variant + Clone,
        R: Variant + Clone,
        FUNC: RhaiNativeFunc<(Mut<A>,), 1, X, R, true> + SendSync + 'static,
    {
        FuncRegistration::new(crate::engine::make_getter(name.as_ref()))
            .with_namespace(FnNamespace::Global)
            .with_purity(true)
            .with_volatility(false)
            .set_into_module(self, func)
            .hash
    }

    /// Set a Rust setter function taking two parameters (the first one mutable) into the [`Module`],
    /// returning a [`u64`] hash key.
    /// This function is automatically exposed to the global namespace.
    ///
    /// If there is a similar existing setter Rust function, it is replaced.
    ///
    /// # Assumptions
    ///
    /// * **Accessibility**: The function namespace is [`FnNamespace::Global`].
    ///
    /// * **Purity**: The function is assumed to be _non-pure_ (so it cannot be called on constants).
    ///
    /// * **Volatility**: The function is assumed to be _non-volatile_ -- i.e. it guarantees the same result for the same input(s).
    ///
    /// * **Metadata**: No metadata for the function is registered.
    ///
    /// To change these assumptions, use the [`FuncRegistration`] API instead.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let hash = module.set_setter_fn("value", |x: &mut i64, y: ImmutableString| {
    ///                 *x = y.len() as i64; Ok(())
    /// });
    /// assert!(module.contains_fn(hash));
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn set_setter_fn<A, const X: bool, R, FUNC>(
        &mut self,
        name: impl AsRef<str>,
        func: FUNC,
    ) -> u64
    where
        A: Variant + Clone,
        R: Variant + Clone,
        FUNC: RhaiNativeFunc<(Mut<A>, R), 2, X, (), true> + SendSync + 'static,
    {
        FuncRegistration::new(crate::engine::make_setter(name.as_ref()))
            .with_namespace(FnNamespace::Global)
            .with_purity(false)
            .with_volatility(false)
            .set_into_module(self, func)
            .hash
    }

    /// Set a pair of Rust getter and setter functions into the [`Module`], returning both [`u64`] hash keys.
    /// This is a short-hand for [`set_getter_fn`][Module::set_getter_fn] and [`set_setter_fn`][Module::set_setter_fn].
    ///
    /// These function are automatically exposed to the global namespace.
    ///
    /// If there are similar existing Rust functions, they are replaced.
    ///
    /// To change these assumptions, use the [`FuncRegistration`] API instead.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let (hash_get, hash_set) =
    ///         module.set_getter_setter_fn("value",
    ///                 |x: &mut i64| Ok(x.to_string().into()),
    ///                 |x: &mut i64, y: ImmutableString| { *x = y.len() as i64; Ok(()) }
    ///         );
    /// assert!(module.contains_fn(hash_get));
    /// assert!(module.contains_fn(hash_set));
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn set_getter_setter_fn<
        A: Variant + Clone,
        const X1: bool,
        const X2: bool,
        R: Variant + Clone,
    >(
        &mut self,
        name: impl AsRef<str>,
        getter: impl RhaiNativeFunc<(Mut<A>,), 1, X1, R, true> + SendSync + 'static,
        setter: impl RhaiNativeFunc<(Mut<A>, R), 2, X2, (), true> + SendSync + 'static,
    ) -> (u64, u64) {
        (
            self.set_getter_fn(name.as_ref(), getter),
            self.set_setter_fn(name.as_ref(), setter),
        )
    }

    /// Set a Rust index getter taking two parameters (the first one mutable) into the [`Module`],
    /// returning a [`u64`] hash key.
    /// This function is automatically exposed to the global namespace.
    ///
    /// If there is a similar existing setter Rust function, it is replaced.
    ///
    /// # Assumptions
    ///
    /// * **Accessibility**: The function namespace is [`FnNamespace::Global`].
    ///
    /// * **Purity**: The function is assumed to be _pure_ (so it can be called on constants).
    ///
    /// * **Volatility**: The function is assumed to be _non-volatile_ -- i.e. it guarantees the same result for the same input(s).
    ///
    /// * **Metadata**: No metadata for the function is registered.
    ///
    /// To change these assumptions, use the [`FuncRegistration`] API instead.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`][crate::Array], [`Map`][crate::Map], [`String`],
    /// [`ImmutableString`][crate::ImmutableString], `&str` or [`INT`][crate::INT].
    ///
    /// Indexers for arrays, object maps, strings and integers cannot be registered.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// #[derive(Clone)]
    /// struct TestStruct(i64);
    ///
    /// let mut module = Module::new();
    ///
    /// let hash = module.set_indexer_get_fn(
    ///                 |x: &mut TestStruct, y: ImmutableString| Ok(x.0 + y.len() as i64)
    ///            );
    ///
    /// assert!(module.contains_fn(hash));
    /// ```
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline]
    pub fn set_indexer_get_fn<A, B, const X: bool, R, FUNC>(&mut self, func: FUNC) -> u64
    where
        A: Variant + Clone,
        B: Variant + Clone,
        R: Variant + Clone,
        FUNC: RhaiNativeFunc<(Mut<A>, B), 2, X, R, true> + SendSync + 'static,
    {
        FuncRegistration::new(crate::engine::FN_IDX_GET)
            .with_namespace(FnNamespace::Global)
            .with_purity(true)
            .with_volatility(false)
            .set_into_module(self, func)
            .hash
    }

    /// Set a Rust index setter taking three parameters (the first one mutable) into the [`Module`],
    /// returning a [`u64`] hash key.
    /// This function is automatically exposed to the global namespace.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Assumptions
    ///
    /// * **Accessibility**: The function namespace is [`FnNamespace::Global`].
    ///
    /// * **Purity**: The function is assumed to be _non-pure_ (so it cannot be called on constants).
    ///
    /// * **Volatility**: The function is assumed to be _non-volatile_ -- i.e. it guarantees the same result for the same input(s).
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`][crate::Array], [`Map`][crate::Map], [`String`],
    /// [`ImmutableString`][crate::ImmutableString], `&str` or [`INT`][crate::INT].
    ///
    /// Indexers for arrays, object maps, strings and integers cannot be registered.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// #[derive(Clone)]
    /// struct TestStruct(i64);
    ///
    /// let mut module = Module::new();
    ///
    /// let hash = module.set_indexer_set_fn(|x: &mut TestStruct, y: ImmutableString, value: i64| {
    ///                         *x = TestStruct(y.len() as i64 + value);
    ///                         Ok(())
    ///            });
    ///
    /// assert!(module.contains_fn(hash));
    /// ```
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline]
    pub fn set_indexer_set_fn<A, B, const X: bool, R, FUNC>(&mut self, func: FUNC) -> u64
    where
        A: Variant + Clone,
        B: Variant + Clone,
        R: Variant + Clone,
        FUNC: RhaiNativeFunc<(Mut<A>, B, R), 3, X, (), true> + SendSync + 'static,
    {
        FuncRegistration::new(crate::engine::FN_IDX_SET)
            .with_namespace(FnNamespace::Global)
            .with_purity(false)
            .with_volatility(false)
            .set_into_module(self, func)
            .hash
    }

    /// Set a pair of Rust index getter and setter functions into the [`Module`], returning both [`u64`] hash keys.
    /// This is a short-hand for [`set_indexer_get_fn`][Module::set_indexer_get_fn] and
    /// [`set_indexer_set_fn`][Module::set_indexer_set_fn].
    ///
    /// These functions are automatically exposed to the global namespace.
    ///
    /// If there are similar existing Rust functions, they are replaced.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`][crate::Array], [`Map`][crate::Map], [`String`],
    /// [`ImmutableString`][crate::ImmutableString], `&str` or [`INT`][crate::INT].
    ///
    /// Indexers for arrays, object maps, strings and integers cannot be registered.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// #[derive(Clone)]
    /// struct TestStruct(i64);
    ///
    /// let mut module = Module::new();
    ///
    /// let (hash_get, hash_set) = module.set_indexer_get_set_fn(
    ///     |x: &mut TestStruct, y: ImmutableString| Ok(x.0 + y.len() as i64),
    ///     |x: &mut TestStruct, y: ImmutableString, value: i64| { *x = TestStruct(y.len() as i64 + value); Ok(()) }
    /// );
    ///
    /// assert!(module.contains_fn(hash_get));
    /// assert!(module.contains_fn(hash_set));
    /// ```
    #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
    #[inline(always)]
    pub fn set_indexer_get_set_fn<
        A: Variant + Clone,
        B: Variant + Clone,
        const X1: bool,
        const X2: bool,
        R: Variant + Clone,
    >(
        &mut self,
        get_fn: impl RhaiNativeFunc<(Mut<A>, B), 2, X1, R, true> + SendSync + 'static,
        set_fn: impl RhaiNativeFunc<(Mut<A>, B, R), 3, X2, (), true> + SendSync + 'static,
    ) -> (u64, u64) {
        (
            self.set_indexer_get_fn(get_fn),
            self.set_indexer_set_fn(set_fn),
        )
    }

    /// Look up a native Rust function by hash.
    #[inline]
    #[must_use]
    pub(crate) fn get_fn(&self, hash_native: u64) -> Option<&RhaiFunc> {
        self.functions
            .as_ref()
            .and_then(|m| m.get(&hash_native))
            .map(|(f, _)| f)
    }

    /// Can the particular function with [`Dynamic`] parameter(s) exist in the [`Module`]?
    ///
    /// A `true` return value does not automatically imply that the function _must_ exist.
    #[inline(always)]
    #[must_use]
    pub(crate) const fn may_contain_dynamic_fn(&self, hash_script: u64) -> bool {
        !self.dynamic_functions_filter.is_absent(hash_script)
    }

    /// Does the particular namespace-qualified function exist in the [`Module`]?
    ///
    /// The [`u64`] hash is calculated by [`build_index`][Module::build_index].
    #[inline(always)]
    #[must_use]
    pub fn contains_qualified_fn(&self, hash_fn: u64) -> bool {
        self.all_functions
            .as_ref()
            .map_or(false, |m| m.contains_key(&hash_fn))
    }

    /// Get a namespace-qualified function.
    ///
    /// The [`u64`] hash is calculated by [`build_index`][Module::build_index].
    #[cfg(not(feature = "no_module"))]
    #[inline]
    #[must_use]
    pub(crate) fn get_qualified_fn(&self, hash_qualified_fn: u64) -> Option<&RhaiFunc> {
        self.all_functions
            .as_ref()
            .and_then(|m| m.get(&hash_qualified_fn))
    }

    /// Combine another [`Module`] into this [`Module`].
    /// The other [`Module`] is _consumed_ to merge into this [`Module`].
    #[inline]
    pub fn combine(&mut self, other: Self) -> &mut Self {
        self.modules.extend(other.modules);
        self.variables.extend(other.variables);
        match self.functions {
            Some(ref mut m) if other.functions.is_some() => m.extend(other.functions.unwrap()),
            Some(_) => (),
            None => self.functions = other.functions,
        }
        self.dynamic_functions_filter += other.dynamic_functions_filter;
        self.type_iterators.extend(other.type_iterators);
        self.all_functions = None;
        self.all_variables = None;
        self.all_type_iterators.clear();
        self.flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);

        #[cfg(feature = "metadata")]
        {
            if !self.doc.is_empty() {
                self.doc.push('\n');
            }
            self.doc.push_str(&other.doc);
        }

        self
    }

    /// Combine another [`Module`] into this [`Module`].
    /// The other [`Module`] is _consumed_ to merge into this [`Module`].
    /// Sub-modules are flattened onto the root [`Module`], with higher level overriding lower level.
    #[inline]
    pub fn combine_flatten(&mut self, other: Self) -> &mut Self {
        for m in other.modules.into_values() {
            self.combine_flatten(shared_take_or_clone(m));
        }
        self.variables.extend(other.variables);
        match self.functions {
            Some(ref mut m) if other.functions.is_some() => m.extend(other.functions.unwrap()),
            Some(_) => (),
            None => self.functions = other.functions,
        }
        self.dynamic_functions_filter += other.dynamic_functions_filter;
        self.type_iterators.extend(other.type_iterators);
        self.all_functions = None;
        self.all_variables = None;
        self.all_type_iterators.clear();
        self.flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);

        #[cfg(feature = "metadata")]
        {
            if !self.doc.is_empty() {
                self.doc.push('\n');
            }
            self.doc.push_str(&other.doc);
        }

        self
    }

    /// Polyfill this [`Module`] with another [`Module`].
    /// Only items not existing in this [`Module`] are added.
    #[inline]
    pub fn fill_with(&mut self, other: &Self) -> &mut Self {
        for (k, v) in &other.modules {
            if !self.modules.contains_key(k) {
                self.modules.insert(k.clone(), v.clone());
            }
        }
        for (k, v) in &other.variables {
            if !self.variables.contains_key(k) {
                self.variables.insert(k.clone(), v.clone());
            }
        }
        if let Some(ref functions) = other.functions {
            let others_len = functions.len();

            for (&k, f) in functions {
                let map = self
                    .functions
                    .get_or_insert_with(|| new_hash_map(FN_MAP_SIZE));
                map.reserve(others_len);
                map.entry(k).or_insert_with(|| f.clone());
            }
        }
        self.dynamic_functions_filter += &other.dynamic_functions_filter;
        for (&k, v) in &other.type_iterators {
            self.type_iterators.entry(k).or_insert_with(|| v.clone());
        }

        self.all_functions = None;
        self.all_variables = None;
        self.all_type_iterators.clear();
        self.flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);

        #[cfg(feature = "metadata")]
        {
            if !self.doc.is_empty() {
                self.doc.push('\n');
            }
            self.doc.push_str(&other.doc);
        }

        self
    }

    /// Merge another [`Module`] into this [`Module`].
    #[inline(always)]
    pub fn merge(&mut self, other: &Self) -> &mut Self {
        self.merge_filtered(other, |_, _, _, _, _| true)
    }

    /// Merge another [`Module`] into this [`Module`] based on a filter predicate.
    pub(crate) fn merge_filtered(
        &mut self,
        other: &Self,
        _filter: impl Fn(FnNamespace, FnAccess, bool, &str, usize) -> bool + Copy,
    ) -> &mut Self {
        for (k, v) in &other.modules {
            let mut m = Self::new();
            m.merge_filtered(v, _filter);
            self.set_sub_module(k.clone(), m);
        }
        #[cfg(feature = "no_function")]
        self.modules.extend(other.modules.clone());

        self.variables.extend(other.variables.clone());

        if let Some(ref functions) = other.functions {
            match self.functions {
                Some(ref mut m) => m.extend(
                    functions
                        .iter()
                        .filter(|(.., (f, m))| {
                            _filter(m.namespace, m.access, f.is_script(), &m.name, m.num_params)
                        })
                        .map(|(&k, f)| (k, f.clone())),
                ),
                None => self.functions = other.functions.clone(),
            }
        }
        self.dynamic_functions_filter += &other.dynamic_functions_filter;

        self.type_iterators.extend(other.type_iterators.clone());
        self.all_functions = None;
        self.all_variables = None;
        self.all_type_iterators.clear();
        self.flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);

        #[cfg(feature = "metadata")]
        {
            if !self.doc.is_empty() {
                self.doc.push('\n');
            }
            self.doc.push_str(&other.doc);
        }

        self
    }

    /// Filter out the functions, retaining only some script-defined functions based on a filter predicate.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub(crate) fn retain_script_functions(
        &mut self,
        filter: impl Fn(FnNamespace, FnAccess, &str, usize) -> bool,
    ) -> &mut Self {
        self.functions = std::mem::take(&mut self.functions).map(|m| {
            m.into_iter()
                .filter(|(.., (f, m))| {
                    if f.is_script() {
                        filter(m.namespace, m.access, &m.name, m.num_params)
                    } else {
                        false
                    }
                })
                .collect()
        });

        self.dynamic_functions_filter.clear();
        self.all_functions = None;
        self.all_variables = None;
        self.all_type_iterators.clear();
        self.flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);
        self
    }

    /// Get the number of variables, functions and type iterators in the [`Module`].
    #[inline(always)]
    #[must_use]
    pub fn count(&self) -> (usize, usize, usize) {
        (
            self.variables.len(),
            self.functions.as_ref().map_or(0, StraightHashMap::len),
            self.type_iterators.len(),
        )
    }

    /// Get an iterator to the sub-modules in the [`Module`].
    #[inline(always)]
    pub fn iter_sub_modules(&self) -> impl Iterator<Item = (&str, &SharedModule)> {
        self.iter_sub_modules_raw().map(|(k, m)| (k.as_str(), m))
    }
    /// Get an iterator to the sub-modules in the [`Module`].
    #[inline(always)]
    pub(crate) fn iter_sub_modules_raw(
        &self,
    ) -> impl Iterator<Item = (&Identifier, &SharedModule)> {
        self.modules.iter()
    }

    /// Get an iterator to the variables in the [`Module`].
    #[inline(always)]
    pub fn iter_var(&self) -> impl Iterator<Item = (&str, &Dynamic)> {
        self.iter_var_raw().map(|(k, v)| (k.as_str(), v))
    }
    /// Get an iterator to the variables in the [`Module`].
    #[inline(always)]
    pub(crate) fn iter_var_raw(&self) -> impl Iterator<Item = (&Identifier, &Dynamic)> {
        self.variables.iter()
    }

    /// Get an iterator to the custom types in the [`Module`].
    #[inline(always)]
    #[allow(dead_code)]
    pub(crate) fn iter_custom_types(&self) -> impl Iterator<Item = (&str, &CustomTypeInfo)> {
        self.custom_types.iter()
    }

    /// Get an iterator to the functions in the [`Module`].
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn iter_fn(&self) -> impl Iterator<Item = (&RhaiFunc, &FuncMetadata)> {
        self.functions
            .iter()
            .flat_map(StraightHashMap::values)
            .map(|(f, m)| (f, &**m))
    }

    /// Get an iterator over all script-defined functions in the [`Module`].
    ///
    /// Function metadata includes:
    /// 1) Namespace ([`FnNamespace::Global`] or [`FnNamespace::Internal`]).
    /// 2) Access mode ([`FnAccess::Public`] or [`FnAccess::Private`]).
    /// 3) Function name (as string slice).
    /// 4) Number of parameters.
    /// 5) Shared reference to function definition [`ScriptFuncDef`][crate::ast::ScriptFuncDef].
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub(crate) fn iter_script_fn(
        &self,
    ) -> impl Iterator<
        Item = (
            FnNamespace,
            FnAccess,
            &str,
            usize,
            &Shared<crate::ast::ScriptFuncDef>,
        ),
    > + '_ {
        self.iter_fn().filter(|(f, _)| f.is_script()).map(|(f, m)| {
            (
                m.namespace,
                m.access,
                m.name.as_str(),
                m.num_params,
                f.get_script_fn_def().expect("`ScriptFuncDef`"),
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
    #[inline]
    pub fn iter_script_fn_info(
        &self,
    ) -> impl Iterator<Item = (FnNamespace, FnAccess, &str, usize)> {
        self.iter_fn()
            .filter(|(f, _)| f.is_script())
            .map(|(_, f)| (f.namespace, f.access, f.name.as_str(), f.num_params))
    }

    /// _(internals)_ Get an iterator over all script-defined functions in the [`Module`].
    /// Exported under the `internals` feature only.
    ///
    /// Function metadata includes:
    /// 1) Namespace ([`FnNamespace::Global`] or [`FnNamespace::Internal`]).
    /// 2) Access mode ([`FnAccess::Public`] or [`FnAccess::Private`]).
    /// 3) Function name (as string slice).
    /// 4) Number of parameters.
    /// 5) _(internals)_ Shared reference to function definition [`ScriptFuncDef`][crate::ast::ScriptFuncDef].
    #[cfg(not(feature = "no_function"))]
    #[cfg(feature = "internals")]
    #[inline(always)]
    pub fn iter_script_fn_info(
        &self,
    ) -> impl Iterator<
        Item = (
            FnNamespace,
            FnAccess,
            &str,
            usize,
            &Shared<crate::ast::ScriptFuncDef>,
        ),
    > {
        self.iter_script_fn()
    }

    /// Create a new [`Module`] by evaluating an [`AST`][crate::AST].
    ///
    /// The entire [`AST`][crate::AST] is encapsulated into each function, allowing functions to
    /// cross-call each other.
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
    #[inline(always)]
    pub fn eval_ast_as_new(
        scope: crate::Scope,
        ast: &crate::AST,
        engine: &crate::Engine,
    ) -> RhaiResultOf<Self> {
        let mut scope = scope;
        let global = &mut crate::eval::GlobalRuntimeState::new(engine);

        Self::eval_ast_as_new_raw(engine, &mut scope, global, ast)
    }
    /// Create a new [`Module`] by evaluating an [`AST`][crate::AST].
    ///
    /// The entire [`AST`][crate::AST] is encapsulated into each function, allowing functions to
    /// cross-call each other.
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.
    ///
    /// In particular, the [`global`][crate::GlobalRuntimeState] parameter allows the entire
    /// calling environment to be encapsulated, including automatic global constants.
    #[cfg(not(feature = "no_module"))]
    pub fn eval_ast_as_new_raw(
        engine: &crate::Engine,
        scope: &mut crate::Scope,
        global: &mut crate::eval::GlobalRuntimeState,
        ast: &crate::AST,
    ) -> RhaiResultOf<Self> {
        // Save global state
        let orig_scope_len = scope.len();
        let orig_imports_len = global.num_imports();
        let orig_source = global.source.clone();

        #[cfg(not(feature = "no_function"))]
        let orig_lib_len = global.lib.len();

        #[cfg(not(feature = "no_function"))]
        let orig_constants = std::mem::take(&mut global.constants);

        // Run the script
        let caches = &mut crate::eval::Caches::new();

        let result = engine.eval_ast_with_scope_raw(global, caches, scope, ast);

        // Create new module
        let mut module = Self::new();

        // Extra modules left become sub-modules
        let mut imports = crate::ThinVec::new();

        if result.is_ok() {
            global
                .scan_imports_raw()
                .skip(orig_imports_len)
                .for_each(|(k, m)| {
                    imports.push((k.clone(), m.clone()));
                    module.set_sub_module(k.clone(), m.clone());
                });
        }

        // Restore global state
        #[cfg(not(feature = "no_function"))]
        let constants = std::mem::replace(&mut global.constants, orig_constants);

        global.truncate_imports(orig_imports_len);

        #[cfg(not(feature = "no_function"))]
        global.lib.truncate(orig_lib_len);

        global.source = orig_source;

        // The return value is thrown away and not used
        let _ = result?;

        // Encapsulated environment
        let environ = Shared::new(crate::ast::EncapsulatedEnviron {
            #[cfg(not(feature = "no_function"))]
            lib: ast.shared_lib().clone(),
            imports,
            #[cfg(not(feature = "no_function"))]
            constants,
        });

        // Variables with an alias left in the scope become module variables
        let mut i = scope.len();
        while i > 0 {
            i -= 1;

            let (mut value, mut aliases) = if i >= orig_scope_len {
                let (_, v, a) = scope.pop_entry().unwrap();
                (v, a)
            } else {
                let (_, v, a) = scope.get_entry_by_index(i);
                (v.clone(), a.to_vec())
            };

            value.deep_scan(|v| {
                if let Some(fn_ptr) = v.downcast_mut::<crate::FnPtr>() {
                    fn_ptr.environ = Some(environ.clone());
                }
            });

            match aliases.len() {
                0 => (),
                1 => {
                    let alias = aliases.pop().unwrap();
                    if !module.contains_var(&alias) {
                        module.set_var(alias, value);
                    }
                }
                _ => {
                    // Avoid cloning the last value
                    let mut first_alias = None;

                    for alias in aliases {
                        if module.contains_var(&alias) {
                            continue;
                        }
                        if first_alias.is_none() {
                            first_alias = Some(alias);
                        } else {
                            module.set_var(alias, value.clone());
                        }
                    }

                    if let Some(alias) = first_alias {
                        module.set_var(alias, value);
                    }
                }
            }
        }

        // Non-private functions defined become module functions
        #[cfg(not(feature = "no_function"))]
        ast.iter_fn_def()
            .filter(|&f| match f.access {
                FnAccess::Public => true,
                FnAccess::Private => false,
            })
            .for_each(|f| {
                let hash = module.set_script_fn(f.clone());
                if let (
                    RhaiFunc::Script {
                        environ: ref mut e, ..
                    },
                    _,
                ) = module.functions.as_mut().unwrap().get_mut(&hash).unwrap()
                {
                    // Encapsulate AST environment
                    *e = Some(environ.clone());
                }
            });

        module.id = ast.source_raw().cloned();

        #[cfg(feature = "metadata")]
        module.set_doc(ast.doc());

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
    pub const fn contains_indexed_global_functions(&self) -> bool {
        self.flags.intersects(ModuleFlags::INDEXED_GLOBAL_FUNCTIONS)
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
            variables: &mut StraightHashMap<Dynamic>,
            functions: &mut StraightHashMap<RhaiFunc>,
            type_iterators: &mut BTreeMap<TypeId, Shared<FnIterator>>,
        ) -> bool {
            let mut contains_indexed_global_functions = false;

            for (name, m) in &module.modules {
                // Index all the sub-modules first.
                path.push(name);
                if index_module(m, path, variables, functions, type_iterators) {
                    contains_indexed_global_functions = true;
                }
                path.pop();
            }

            // Index all variables
            for (var_name, value) in &module.variables {
                let hash_var = crate::calc_var_hash(path.iter().copied(), var_name);

                // Catch hash collisions in testing environment only.
                #[cfg(feature = "testing-environ")]
                assert!(
                    !variables.contains_key(&hash_var),
                    "Hash {} already exists when indexing variable {}",
                    hash_var,
                    var_name
                );

                variables.insert(hash_var, value.clone());
            }

            // Index all type iterators
            for (&type_id, func) in &module.type_iterators {
                type_iterators.insert(type_id, func.clone());
            }

            // Index all functions
            for (&hash, (f, m)) in module.functions.iter().flatten() {
                match m.namespace {
                    FnNamespace::Global => {
                        // Catch hash collisions in testing environment only.
                        #[cfg(feature = "testing-environ")]
                        if let Some(fx) = functions.get(&hash) {
                            unreachable!(
                                "Hash {} already exists when indexing function {:#?}:\n{:#?}",
                                hash, f, fx
                            );
                        }

                        // Flatten all functions with global namespace
                        functions.insert(hash, f.clone());
                        contains_indexed_global_functions = true;
                    }
                    FnNamespace::Internal => (),
                }
                match m.access {
                    FnAccess::Public => (),
                    FnAccess::Private => continue, // Do not index private functions
                }

                if f.is_script() {
                    #[cfg(not(feature = "no_function"))]
                    {
                        let hash_script =
                            crate::calc_fn_hash(path.iter().copied(), &m.name, m.num_params);
                        #[cfg(not(feature = "no_object"))]
                        let hash_script = f
                            .get_script_fn_def()
                            .unwrap()
                            .this_type
                            .as_ref()
                            .map_or(hash_script, |this_type| {
                                crate::calc_typed_method_hash(hash_script, this_type)
                            });

                        // Catch hash collisions in testing environment only.
                        #[cfg(feature = "testing-environ")]
                        if let Some(fx) = functions.get(&hash_script) {
                            unreachable!(
                                "Hash {} already exists when indexing function {:#?}:\n{:#?}",
                                hash_script, f, fx
                            );
                        }

                        functions.insert(hash_script, f.clone());
                    }
                } else {
                    let hash_fn =
                        calc_native_fn_hash(path.iter().copied(), &m.name, &m.param_types);

                    // Catch hash collisions in testing environment only.
                    #[cfg(feature = "testing-environ")]
                    if let Some(fx) = functions.get(&hash_fn) {
                        unreachable!(
                            "Hash {} already exists when indexing function {:#?}:\n{:#?}",
                            hash_fn, f, fx
                        );
                    }

                    functions.insert(hash_fn, f.clone());
                }
            }

            contains_indexed_global_functions
        }

        if !self.is_indexed() {
            let mut path = Vec::with_capacity(4);
            let mut variables = new_hash_map(self.variables.len());
            let mut functions =
                new_hash_map(self.functions.as_ref().map_or(0, StraightHashMap::len));
            let mut type_iterators = BTreeMap::new();

            path.push("");

            let has_global_functions = index_module(
                self,
                &mut path,
                &mut variables,
                &mut functions,
                &mut type_iterators,
            );

            self.flags
                .set(ModuleFlags::INDEXED_GLOBAL_FUNCTIONS, has_global_functions);

            self.all_variables = (!variables.is_empty()).then_some(variables);
            self.all_functions = (!functions.is_empty()).then_some(functions);
            self.all_type_iterators = type_iterators;

            self.flags |= ModuleFlags::INDEXED;
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
    #[inline(always)]
    pub fn set_iter(
        &mut self,
        type_id: TypeId,
        func: impl Fn(Dynamic) -> Box<dyn Iterator<Item = Dynamic>> + SendSync + 'static,
    ) -> &mut Self {
        self.set_iter_result(type_id, move |x| {
            Box::new(func(x).map(Ok)) as Box<dyn Iterator<Item = RhaiResultOf<Dynamic>>>
        })
    }

    /// Set a fallible type iterator into the [`Module`].
    #[inline]
    pub fn set_iter_result(
        &mut self,
        type_id: TypeId,
        func: impl Fn(Dynamic) -> Box<dyn Iterator<Item = RhaiResultOf<Dynamic>>> + SendSync + 'static,
    ) -> &mut Self {
        let func = Shared::new(func);
        if self.is_indexed() {
            self.all_type_iterators.insert(type_id, func.clone());
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

    /// Set a fallible type iterator into the [`Module`].
    #[inline(always)]
    pub fn set_iterable_result<T, X>(&mut self) -> &mut Self
    where
        T: Variant + Clone + IntoIterator<Item = RhaiResultOf<X>>,
        X: Variant + Clone,
    {
        self.set_iter_result(TypeId::of::<T>(), |obj: Dynamic| {
            Box::new(obj.cast::<T>().into_iter().map(|v| v.map(Dynamic::from)))
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

    /// Set a iterator type into the [`Module`] as a fallible type iterator.
    #[inline(always)]
    pub fn set_iterator_result<T, X>(&mut self) -> &mut Self
    where
        T: Variant + Clone + Iterator<Item = RhaiResultOf<X>>,
        X: Variant + Clone,
    {
        self.set_iter_result(TypeId::of::<T>(), |obj: Dynamic| {
            Box::new(obj.cast::<T>().map(|v| v.map(Dynamic::from)))
        })
    }

    /// Get the specified type iterator.
    #[cfg(not(feature = "no_module"))]
    #[inline]
    #[must_use]
    pub(crate) fn get_qualified_iter(&self, id: TypeId) -> Option<&FnIterator> {
        self.all_type_iterators.get(&id).map(|f| &**f)
    }

    /// Get the specified type iterator.
    #[inline]
    #[must_use]
    pub(crate) fn get_iter(&self, id: TypeId) -> Option<&FnIterator> {
        self.type_iterators.get(&id).map(|f| &**f)
    }
}

/// Module containing all built-in [module resolvers][ModuleResolver].
#[cfg(not(feature = "no_module"))]
pub mod resolvers;

#[cfg(not(feature = "no_module"))]
pub use resolvers::ModuleResolver;
