//! Module defining external-loaded modules for Rhai.

use crate::ast::FnAccess;
use crate::func::{
    shared_take_or_clone, CallableFunction, FnCallArgs, IteratorFn, RegisterNativeFunction,
    SendSync,
};
use crate::types::{dynamic::Variant, CustomTypesCollection};
use crate::{
    calc_fn_hash, calc_fn_params_hash, calc_qualified_fn_hash, combine_hashes, Dynamic, Identifier,
    ImmutableString, NativeCallContext, RhaiResultOf, Shared, SmartString, StaticVec,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::TypeId,
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fmt,
    ops::{Add, AddAssign},
};

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
use crate::func::register::Mut;

/// A type representing the namespace of a function.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "metadata", derive(serde::Serialize))]
#[cfg_attr(feature = "metadata", serde(rename_all = "camelCase"))]
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
    pub fn is_module_namespace(self) -> bool {
        match self {
            Self::Internal => true,
            Self::Global => false,
        }
    }
    /// Is this a global namespace?
    #[inline(always)]
    #[must_use]
    pub fn is_global_namespace(self) -> bool {
        match self {
            Self::Internal => false,
            Self::Global => true,
        }
    }
}

/// A type containing all metadata for a registered function.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub struct FnMetadata {
    /// Function namespace.
    pub namespace: FnNamespace,
    /// Function access mode.
    pub access: FnAccess,
    /// Function name.
    pub name: Identifier,
    /// Number of parameters.
    pub params: usize,
    /// Parameter names and types (if available).
    #[cfg(feature = "metadata")]
    pub params_info: StaticVec<Identifier>,
    /// Return type name.
    #[cfg(feature = "metadata")]
    pub return_type: Identifier,
    /// Comments.
    #[cfg(feature = "metadata")]
    pub comments: Box<[Box<str>]>,
}

impl PartialOrd for FnMetadata {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FnMetadata {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.name.cmp(&other.name) {
            #[cfg(feature = "metadata")]
            Ordering::Equal => match self.params.cmp(&other.params) {
                Ordering::Equal => self.params_info.cmp(&other.params_info),
                cmp => cmp,
            },
            #[cfg(not(feature = "metadata"))]
            Ordering::Equal => self.params.cmp(&other.params),
            cmp => cmp,
        }
    }
}

/// A type containing a single registered function.
#[derive(Debug, Clone)]
pub struct FuncInfo {
    /// Function instance.
    pub func: CallableFunction,
    /// Parameter types (if applicable).
    pub param_types: StaticVec<TypeId>,
    /// Function metadata.
    pub metadata: FnMetadata,
}

impl FuncInfo {
    /// Format a return type to be display-friendly.
    ///
    /// `()` is cleared.
    /// [`RhaiResult`][crate::RhaiResult] and [`RhaiResultOf<T>`] are expanded.
    #[cfg(feature = "metadata")]
    pub fn format_type(typ: &str, is_return_type: bool) -> std::borrow::Cow<str> {
        const RHAI_RESULT_TYPE: &str = "RhaiResult";
        const RHAI_RESULT_TYPE_EXPAND: &str = "Result<Dynamic, Box<EvalAltResult>>";
        const RHAI_RESULT_OF_TYPE: &str = "RhaiResultOf<";
        const RHAI_RESULT_OF_TYPE_EXPAND: &str = "Result<{}, Box<EvalAltResult>>";
        const RHAI_RANGE: &str = "ExclusiveRange";
        const RHAI_RANGE_TYPE: &str = "Range<";
        const RHAI_RANGE_EXPAND: &str = "Range<{}>";
        const RHAI_INCLUSIVE_RANGE: &str = "InclusiveRange";
        const RHAI_INCLUSIVE_RANGE_TYPE: &str = "RangeInclusive<";
        const RHAI_INCLUSIVE_RANGE_EXPAND: &str = "RangeInclusive<{}>";

        let typ = typ.trim();

        if let Some(x) = typ.strip_prefix("rhai::") {
            return Self::format_type(x, is_return_type);
        } else if let Some(x) = typ.strip_prefix("&mut ") {
            let r = Self::format_type(x, false);
            return if r == x {
                typ.into()
            } else {
                format!("&mut {r}").into()
            };
        }

        match typ {
            "" | "()" if is_return_type => "".into(),
            "INT" => std::any::type_name::<crate::INT>().into(),
            #[cfg(not(feature = "no_float"))]
            "FLOAT" => std::any::type_name::<crate::FLOAT>().into(),
            RHAI_RANGE => RHAI_RANGE_EXPAND
                .replace("{}", std::any::type_name::<crate::INT>())
                .into(),
            RHAI_INCLUSIVE_RANGE => RHAI_INCLUSIVE_RANGE_EXPAND
                .replace("{}", std::any::type_name::<crate::INT>())
                .into(),
            RHAI_RESULT_TYPE => RHAI_RESULT_TYPE_EXPAND.into(),
            ty if ty.starts_with(RHAI_RANGE_TYPE) && ty.ends_with('>') => {
                let inner = &ty[RHAI_RANGE_TYPE.len()..ty.len() - 1];
                RHAI_RANGE_EXPAND
                    .replace("{}", Self::format_type(inner, false).trim())
                    .into()
            }
            ty if ty.starts_with(RHAI_INCLUSIVE_RANGE_TYPE) && ty.ends_with('>') => {
                let inner = &ty[RHAI_INCLUSIVE_RANGE_TYPE.len()..ty.len() - 1];
                RHAI_INCLUSIVE_RANGE_EXPAND
                    .replace("{}", Self::format_type(inner, false).trim())
                    .into()
            }
            ty if ty.starts_with(RHAI_RESULT_OF_TYPE) && ty.ends_with('>') => {
                let inner = &ty[RHAI_RESULT_OF_TYPE.len()..ty.len() - 1];
                RHAI_RESULT_OF_TYPE_EXPAND
                    .replace("{}", Self::format_type(inner, false).trim())
                    .into()
            }
            ty => ty.into(),
        }
    }
    /// _(metadata)_ Generate a signature of the function.
    /// Exported under the `metadata` feature only.
    #[cfg(feature = "metadata")]
    #[must_use]
    pub fn gen_signature(&self) -> String {
        let mut sig = format!("{}(", self.metadata.name);

        let return_type = Self::format_type(&self.metadata.return_type, true);

        if self.metadata.params_info.is_empty() {
            for x in 0..self.metadata.params {
                sig.push('_');
                if x < self.metadata.params - 1 {
                    sig.push_str(", ");
                }
            }
            sig.push(')');
        } else {
            let params: StaticVec<_> = self
                .metadata
                .params_info
                .iter()
                .map(|s| {
                    let mut seg = s.splitn(2, ':');
                    let name = match seg.next().unwrap().trim() {
                        "" => "_",
                        s => s,
                    };
                    let result: std::borrow::Cow<str> = match seg.next() {
                        Some(typ) => {
                            format!("{name}: {}", FuncInfo::format_type(typ, false)).into()
                        }
                        None => name.into(),
                    };
                    result
                })
                .collect();
            sig.push_str(&params.join(", "));
            sig.push(')');
        }

        if !self.func.is_script() && !return_type.is_empty() {
            sig.push_str(" -> ");
            sig.push_str(&return_type);
        }

        sig
    }
}

/// _(internals)_ Calculate a non-zero [`u64`] hash key from a namespace-qualified function name and parameter types.
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
    modules: impl IntoIterator<Item = &'a str>,
    fn_name: &str,
    params: &[TypeId],
) -> u64 {
    let hash_script = calc_qualified_fn_hash(modules, fn_name, params.len());
    let hash_params = calc_fn_params_hash(params.iter().copied());
    combine_hashes(hash_script, hash_params)
}

/// A module which may contain variables, sub-modules, external Rust functions,
/// and/or script-defined functions.
#[derive(Clone)]
pub struct Module {
    /// ID identifying the module.
    /// No ID if string is empty.
    id: Identifier,
    /// Module documentation.
    #[cfg(feature = "metadata")]
    doc: crate::SmartString,
    /// Is this module internal?
    pub(crate) internal: bool,
    /// Is this module part of a standard library?
    pub(crate) standard: bool,
    /// Custom types.
    custom_types: CustomTypesCollection,
    /// Sub-modules.
    modules: BTreeMap<Identifier, Shared<Module>>,
    /// [`Module`] variables.
    variables: BTreeMap<Identifier, Dynamic>,
    /// Flattened collection of all [`Module`] variables, including those in sub-modules.
    all_variables: BTreeMap<u64, Dynamic>,
    /// Functions (both native Rust and scripted).
    functions: BTreeMap<u64, Box<FuncInfo>>,
    /// Flattened collection of all functions, native Rust and scripted.
    /// including those in sub-modules.
    all_functions: BTreeMap<u64, CallableFunction>,
    /// Native Rust functions (in scripted hash format) that contain [`Dynamic`] parameters.
    dynamic_functions: BTreeSet<u64>,
    /// Iterator functions, keyed by the type producing the iterator.
    type_iterators: BTreeMap<TypeId, Shared<IteratorFn>>,
    /// Flattened collection of iterator functions, including those in sub-modules.
    all_type_iterators: BTreeMap<TypeId, Shared<IteratorFn>>,
    /// Is the [`Module`] indexed?
    indexed: bool,
    /// Does the [`Module`] contain indexed functions that have been exposed to the global namespace?
    contains_indexed_global_functions: bool,
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

        d.field("id", &self.id)
            .field(
                "modules",
                &self
                    .modules
                    .keys()
                    .map(SmartString::as_str)
                    .collect::<BTreeSet<_>>(),
            )
            .field("vars", &self.variables)
            .field(
                "functions",
                &self
                    .iter_fn()
                    .map(|f| f.func.to_string())
                    .collect::<BTreeSet<_>>(),
            );

        #[cfg(feature = "metadata")]
        d.field("doc", &self.doc);

        d.finish()
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
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: Identifier::new_const(),
            #[cfg(feature = "metadata")]
            doc: crate::SmartString::new_const(),
            internal: false,
            standard: false,
            custom_types: CustomTypesCollection::new(),
            modules: BTreeMap::new(),
            variables: BTreeMap::new(),
            all_variables: BTreeMap::new(),
            functions: BTreeMap::new(),
            all_functions: BTreeMap::new(),
            dynamic_functions: BTreeSet::new(),
            type_iterators: BTreeMap::new(),
            all_type_iterators: BTreeMap::new(),
            indexed: true,
            contains_indexed_global_functions: false,
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
        if self.id_raw().is_empty() {
            None
        } else {
            Some(self.id_raw())
        }
    }

    /// Get the ID of the [`Module`] as an [`Identifier`], if any.
    #[inline(always)]
    #[must_use]
    pub(crate) const fn id_raw(&self) -> &Identifier {
        &self.id
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
    pub fn set_id(&mut self, id: impl Into<Identifier>) -> &mut Self {
        self.id = id.into();
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
        self.id.clear();
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
    #[inline]
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
        self.id.clear();
        #[cfg(feature = "metadata")]
        self.doc.clear();
        self.internal = false;
        self.standard = false;
        self.custom_types.clear();
        self.modules.clear();
        self.variables.clear();
        self.all_variables.clear();
        self.functions.clear();
        self.all_functions.clear();
        self.dynamic_functions.clear();
        self.type_iterators.clear();
        self.all_type_iterators.clear();
        self.indexed = false;
        self.contains_indexed_global_functions = false;
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
    /// assert_eq!(module.get_custom_type(name), Some("MyType"));
    /// ```
    #[inline(always)]
    pub fn set_custom_type<T>(&mut self, name: &str) -> &mut Self {
        self.custom_types.add_type::<T>(name);
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
    /// assert_eq!(module.get_custom_type(name), Some("MyType"));
    /// ```
    #[inline(always)]
    pub fn set_custom_type_raw(
        &mut self,
        type_name: impl Into<Identifier>,
        name: impl Into<Identifier>,
    ) -> &mut Self {
        self.custom_types.add(type_name, name);
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
    /// assert_eq!(module.get_custom_type(name), Some("MyType"));
    /// ```
    #[inline]
    #[must_use]
    pub fn get_custom_type(&self, key: &str) -> Option<&str> {
        self.custom_types.get(key).map(|t| t.display_name.as_str())
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
        self.indexed
            && !self.contains_indexed_global_functions
            && self.functions.is_empty()
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

    /// _(metadata)_ Generate signatures for all the non-private functions in the [`Module`].
    /// Exported under the `metadata` feature only.
    #[cfg(feature = "metadata")]
    #[inline]
    pub fn gen_fn_signatures(&self) -> impl Iterator<Item = String> + '_ {
        self.iter_fn()
            .filter(|&f| match f.metadata.access {
                FnAccess::Public => true,
                FnAccess::Private => false,
            })
            .map(FuncInfo::gen_signature)
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
        if self.variables.is_empty() {
            false
        } else {
            self.variables.contains_key(name)
        }
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
        if self.variables.is_empty() {
            None
        } else {
            self.variables.get(name).cloned()
        }
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
            let hash_var = crate::calc_qualified_var_hash(Some(""), &ident);
            self.all_variables.insert(hash_var, value.clone());
        }
        self.variables.insert(ident, value);
        self
    }

    /// Get a namespace-qualified [`Module`] variable as a [`Dynamic`].
    #[cfg(not(feature = "no_module"))]
    #[inline]
    pub(crate) fn get_qualified_var(&self, hash_var: u64) -> Option<Dynamic> {
        if self.all_variables.is_empty() {
            None
        } else {
            self.all_variables.get(&hash_var).cloned()
        }
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
        #[cfg(feature = "metadata")]
        let params_info = fn_def.params.iter().map(Into::into).collect();
        self.functions.insert(
            hash_script,
            FuncInfo {
                metadata: FnMetadata {
                    name: fn_def.name.as_str().into(),
                    namespace: FnNamespace::Internal,
                    access: fn_def.access,
                    params: num_params,
                    #[cfg(feature = "metadata")]
                    params_info,
                    #[cfg(feature = "metadata")]
                    return_type: "".into(),
                    #[cfg(feature = "metadata")]
                    comments: Box::default(),
                },
                func: fn_def.into(),
                param_types: StaticVec::new_const(),
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
        name: impl AsRef<str>,
        num_params: usize,
    ) -> Option<&Shared<crate::ast::ScriptFnDef>> {
        if self.functions.is_empty() {
            None
        } else {
            let name = name.as_ref();

            self.iter_fn()
                .find(|f| f.metadata.params == num_params && f.metadata.name == name)
                .and_then(|f| f.func.get_script_fn_def())
        }
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
        if self.modules.is_empty() {
            false
        } else {
            self.modules.contains_key(name)
        }
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
    pub fn get_sub_module(&self, name: &str) -> Option<&Module> {
        if self.modules.is_empty() {
            None
        } else {
            self.modules.get(name).map(|m| &**m)
        }
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
        if self.functions.is_empty() {
            false
        } else {
            self.functions.contains_key(&hash_fn)
        }
    }

    /// _(metadata)_ Update the metadata (parameter names/types and return type) of a registered function.
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
    pub fn update_fn_metadata<S: AsRef<str>>(
        &mut self,
        hash_fn: u64,
        arg_names: impl AsRef<[S]>,
    ) -> &mut Self {
        let mut param_names: StaticVec<_> = arg_names
            .as_ref()
            .iter()
            .map(|s| s.as_ref().into())
            .collect();

        if let Some(f) = self.functions.get_mut(&hash_fn) {
            let (param_names, return_type_name) = if param_names.len() > f.metadata.params {
                let return_type = param_names.pop().unwrap();
                (param_names, return_type)
            } else {
                (param_names, crate::SmartString::new_const())
            };
            f.metadata.params_info = param_names;
            f.metadata.return_type = return_type_name;
        }

        self
    }

    /// _(metadata)_ Update the metadata (parameter names/types, return type and doc-comments) of a registered function.
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
    /// The _last entry_ in the list should be the _return type_ of the function. In other words,
    /// the number of entries should be one larger than the number of parameters.
    ///
    /// ## Comments
    ///
    /// Block doc-comments should be kept in a single line.
    ///
    /// Line doc-comments should be kept in one string slice per line without the termination line-break.
    ///
    /// Leading white-spaces should be stripped, and each string slice always starts with the corresponding
    /// doc-comment leader: `///` or `/**`.
    #[cfg(feature = "metadata")]
    #[inline]
    pub fn update_fn_metadata_with_comments<A: AsRef<str>, C: AsRef<str>>(
        &mut self,
        hash_fn: u64,
        arg_names: impl AsRef<[A]>,
        comments: impl AsRef<[C]>,
    ) -> &mut Self {
        self.update_fn_metadata(hash_fn, arg_names);

        let comments = comments.as_ref();

        if !comments.is_empty() {
            let f = self.functions.get_mut(&hash_fn).unwrap();
            f.metadata.comments = comments.iter().map(|s| s.as_ref().into()).collect();
        }

        self
    }

    /// Update the namespace of a registered function.
    ///
    /// The [`u64`] hash is returned by the [`set_native_fn`][Module::set_native_fn] call.
    #[inline]
    pub fn update_fn_namespace(&mut self, hash_fn: u64, namespace: FnNamespace) -> &mut Self {
        if let Some(f) = self.functions.get_mut(&hash_fn) {
            f.metadata.namespace = namespace;
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

    /// Set a Rust function into the [`Module`], returning a non-zero hash key.
    ///
    /// If there is an existing Rust function of the same hash, it is replaced.
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.
    ///
    /// ## Parameter Names and Types
    ///
    /// Each parameter name/type pair should be a single string of the format: `var_name: type`.
    ///
    /// ## Return Type
    ///
    /// The _last entry_ in the list should be the _return type_ of the function.
    /// In other words, the number of entries should be one larger than the number of parameters.
    #[inline]
    pub fn set_fn(
        &mut self,
        name: impl AsRef<str>,
        namespace: FnNamespace,
        access: FnAccess,
        arg_names: Option<&[&str]>,
        arg_types: impl AsRef<[TypeId]>,
        func: CallableFunction,
    ) -> u64 {
        let _arg_names = arg_names;
        let is_method = func.is_method();

        let mut param_types: StaticVec<_> = arg_types
            .as_ref()
            .iter()
            .copied()
            .enumerate()
            .map(|(i, type_id)| Self::map_type(!is_method || i > 0, type_id))
            .collect();
        param_types.shrink_to_fit();

        let is_dynamic = param_types
            .iter()
            .any(|&type_id| type_id == TypeId::of::<Dynamic>());

        #[cfg(feature = "metadata")]
        let (param_names, return_type_name) = {
            let mut names = _arg_names
                .iter()
                .flat_map(|&p| p.iter())
                .map(|&s| s.into())
                .collect::<StaticVec<_>>();
            let return_type = if names.len() > arg_types.as_ref().len() {
                names.pop().unwrap()
            } else {
                crate::SmartString::new_const()
            };
            names.shrink_to_fit();
            (names, return_type)
        };

        let name = name.as_ref();
        let hash_fn = calc_native_fn_hash(None, name, &param_types);

        if is_dynamic {
            self.dynamic_functions
                .insert(calc_fn_hash(name, param_types.len()));
        }

        self.functions.insert(
            hash_fn,
            FuncInfo {
                metadata: FnMetadata {
                    name: name.into(),
                    namespace,
                    access,
                    params: param_types.len(),
                    #[cfg(feature = "metadata")]
                    params_info: param_names,
                    #[cfg(feature = "metadata")]
                    return_type: return_type_name,
                    #[cfg(feature = "metadata")]
                    comments: Box::default(),
                },
                func,
                param_types,
            }
            .into(),
        );

        self.indexed = false;
        self.contains_indexed_global_functions = false;

        hash_fn
    }

    /// _(metadata)_ Set a Rust function into the [`Module`], returning a non-zero hash key.
    /// Exported under the `metadata` feature only.
    ///
    /// If there is an existing Rust function of the same hash, it is replaced.
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.
    ///
    /// ## Parameter Names and Types
    ///
    /// Each parameter name/type pair should be a single string of the format: `var_name: type`.
    ///
    /// ## Return Type
    ///
    /// The _last entry_ in the list should be the _return type_ of the function.
    /// In other words, the number of entries should be one larger than the number of parameters.
    ///
    /// ## Comments
    ///
    /// Block doc-comments should be kept in a single line.
    ///
    /// Line doc-comments should be kept in one string slice per line without the termination line-break.
    ///
    /// Leading white-spaces should be stripped, and each string slice always starts with the corresponding
    /// doc-comment leader: `///` or `/**`.
    #[cfg(feature = "metadata")]
    #[inline]
    pub fn set_fn_with_comments<S: AsRef<str>>(
        &mut self,
        name: impl AsRef<str>,
        namespace: FnNamespace,
        access: FnAccess,
        arg_names: Option<&[&str]>,
        arg_types: impl AsRef<[TypeId]>,
        comments: impl AsRef<[S]>,
        func: CallableFunction,
    ) -> u64 {
        let hash = self.set_fn(name, namespace, access, arg_names, arg_types, func);

        let comments = comments.as_ref();

        if !comments.is_empty() {
            let f = self.functions.get_mut(&hash).unwrap();
            f.metadata.comments = comments.iter().map(|s| s.as_ref().into()).collect();
        }

        hash
    }

    /// Set a Rust function taking a reference to the scripting [`Engine`][crate::Engine],
    /// the current set of functions, plus a list of mutable [`Dynamic`] references
    /// into the [`Module`], returning a non-zero hash key.
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
    /// No metadata for the function is registered. Use [`update_fn_metadata`][Module::update_fn_metadata] to add metadata.
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
    ///                     Ok(orig)                // return RhaiResult<T>
    ///                 });
    ///
    /// assert!(module.contains_fn(hash));
    /// ```
    #[inline(always)]
    pub fn set_raw_fn<T: Variant + Clone>(
        &mut self,
        name: impl AsRef<str>,
        namespace: FnNamespace,
        access: FnAccess,
        arg_types: impl AsRef<[TypeId]>,
        func: impl Fn(NativeCallContext, &mut FnCallArgs) -> RhaiResultOf<T> + SendSync + 'static,
    ) -> u64 {
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

    /// Set a Rust function into the [`Module`], returning a non-zero hash key.
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
    pub fn set_native_fn<A, T, F, S>(
        &mut self,
        name: impl AsRef<str> + Into<Identifier>,
        func: F,
    ) -> u64
    where
        T: Variant + Clone,
        F: RegisterNativeFunction<A, T, RhaiResultOf<S>>,
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

    /// Set a Rust getter function taking one mutable parameter, returning a non-zero hash key.
    /// This function is automatically exposed to the global namespace.
    ///
    /// If there is a similar existing Rust getter function, it is replaced.
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
    /// let hash = module.set_getter_fn("value", |x: &mut i64| { Ok(*x) });
    /// assert!(module.contains_fn(hash));
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn set_getter_fn<A, T, F, S>(&mut self, name: impl AsRef<str>, func: F) -> u64
    where
        A: Variant + Clone,
        T: Variant + Clone,
        F: RegisterNativeFunction<(Mut<A>,), T, RhaiResultOf<S>> + SendSync + 'static,
    {
        self.set_fn(
            crate::engine::make_getter(name.as_ref()).as_str(),
            FnNamespace::Global,
            FnAccess::Public,
            None,
            &F::param_types(),
            func.into_callable_function(),
        )
    }

    /// Set a Rust setter function taking two parameters (the first one mutable) into the [`Module`],
    /// returning a non-zero hash key.
    /// This function is automatically exposed to the global namespace.
    ///
    /// If there is a similar existing setter Rust function, it is replaced.
    ///
    /// # Function Metadata
    ///
    /// No metadata for the function is registered.
    /// Use [`update_fn_metadata`][Module::update_fn_metadata] to add metadata.
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
    pub fn set_setter_fn<A, T, F, S>(&mut self, name: impl AsRef<str>, func: F) -> u64
    where
        A: Variant + Clone,
        T: Variant + Clone,
        F: RegisterNativeFunction<(Mut<A>, T), (), RhaiResultOf<S>> + SendSync + 'static,
    {
        self.set_fn(
            crate::engine::make_setter(name.as_ref()).as_str(),
            FnNamespace::Global,
            FnAccess::Public,
            None,
            &F::param_types(),
            func.into_callable_function(),
        )
    }

    /// Set a pair of Rust getter and setter functions into the [`Module`], returning both non-zero hash keys.
    /// This is a short-hand for [`set_getter_fn`][Module::set_getter_fn] and [`set_setter_fn`][Module::set_setter_fn].
    ///
    /// These function are automatically exposed to the global namespace.
    ///
    /// If there are similar existing Rust functions, they are replaced.
    ///
    /// # Function Metadata
    ///
    /// No metadata for the function is registered.
    /// Use [`update_fn_metadata`][Module::update_fn_metadata] to add metadata.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Module, ImmutableString};
    ///
    /// let mut module = Module::new();
    /// let (hash_get, hash_set) = module.set_getter_setter_fn("value",
    ///                                 |x: &mut i64| { Ok(x.to_string().into()) },
    ///                                 |x: &mut i64, y: ImmutableString| {
    ///                                     *x = y.len() as i64;
    ///                                     Ok(())
    ///                                 }
    /// );
    /// assert!(module.contains_fn(hash_get));
    /// assert!(module.contains_fn(hash_set));
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn set_getter_setter_fn<A: Variant + Clone, T: Variant + Clone, S1, S2>(
        &mut self,
        name: impl AsRef<str>,
        getter: impl RegisterNativeFunction<(Mut<A>,), T, RhaiResultOf<S1>> + SendSync + 'static,
        setter: impl RegisterNativeFunction<(Mut<A>, T), (), RhaiResultOf<S2>> + SendSync + 'static,
    ) -> (u64, u64) {
        (
            self.set_getter_fn(name.as_ref(), getter),
            self.set_setter_fn(name.as_ref(), setter),
        )
    }

    /// Set a Rust index getter taking two parameters (the first one mutable) into the [`Module`],
    /// returning a non-zero hash key.
    /// This function is automatically exposed to the global namespace.
    ///
    /// If there is a similar existing setter Rust function, it is replaced.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`][crate::Array] or [`Map`][crate::Map].
    /// Indexers for arrays, object maps and strings cannot be registered.
    ///
    /// # Function Metadata
    ///
    /// No metadata for the function is registered.
    /// Use [`update_fn_metadata`][Module::update_fn_metadata] to add metadata.
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
    pub fn set_indexer_get_fn<A, B, T, F, S>(&mut self, func: F) -> u64
    where
        A: Variant + Clone,
        B: Variant + Clone,
        T: Variant + Clone,
        F: RegisterNativeFunction<(Mut<A>, B), T, RhaiResultOf<S>> + SendSync + 'static,
    {
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<A>() == TypeId::of::<crate::Array>() {
            panic!("Cannot register indexer for arrays.");
        }
        #[cfg(not(feature = "no_object"))]
        if TypeId::of::<A>() == TypeId::of::<crate::Map>() {
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
    /// returning a non-zero hash key.
    /// This function is automatically exposed to the global namespace.
    ///
    /// If there is a similar existing Rust function, it is replaced.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`][crate::Array] or [`Map`][crate::Map].
    /// Indexers for arrays, object maps and strings cannot be registered.
    ///
    /// # Function Metadata
    ///
    /// No metadata for the function is registered.
    /// Use [`update_fn_metadata`][Module::update_fn_metadata] to add metadata.
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
    pub fn set_indexer_set_fn<A, B, T, F, S>(&mut self, func: F) -> u64
    where
        A: Variant + Clone,
        B: Variant + Clone,
        T: Variant + Clone,
        F: RegisterNativeFunction<(Mut<A>, B, T), (), RhaiResultOf<S>> + SendSync + 'static,
    {
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<A>() == TypeId::of::<crate::Array>() {
            panic!("Cannot register indexer for arrays.");
        }
        #[cfg(not(feature = "no_object"))]
        if TypeId::of::<A>() == TypeId::of::<crate::Map>() {
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

    /// Set a pair of Rust index getter and setter functions into the [`Module`], returning both non-zero hash keys.
    /// This is a short-hand for [`set_indexer_get_fn`][Module::set_indexer_get_fn] and
    /// [`set_indexer_set_fn`][Module::set_indexer_set_fn].
    ///
    /// These functions are automatically exposed to the global namespace.
    ///
    /// If there are similar existing Rust functions, they are replaced.
    ///
    /// # Panics
    ///
    /// Panics if the type is [`Array`][crate::Array] or [`Map`][crate::Map].
    /// Indexers for arrays, object maps and strings cannot be registered.
    ///
    /// # Function Metadata
    ///
    /// No metadata for the function is registered.
    /// Use [`update_fn_metadata`][Module::update_fn_metadata] to add metadata.
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
    pub fn set_indexer_get_set_fn<
        A: Variant + Clone,
        B: Variant + Clone,
        T: Variant + Clone,
        S1,
        S2,
    >(
        &mut self,
        get_fn: impl RegisterNativeFunction<(Mut<A>, B), T, RhaiResultOf<S1>> + SendSync + 'static,
        set_fn: impl RegisterNativeFunction<(Mut<A>, B, T), (), RhaiResultOf<S2>> + SendSync + 'static,
    ) -> (u64, u64) {
        (
            self.set_indexer_get_fn(get_fn),
            self.set_indexer_set_fn(set_fn),
        )
    }

    /// Look up a Rust function by hash.
    ///
    /// The [`u64`] hash is returned by the [`set_native_fn`][Module::set_native_fn] call.
    #[inline]
    #[must_use]
    pub(crate) fn get_fn(&self, hash_native: u64) -> Option<&CallableFunction> {
        if self.functions.is_empty() {
            None
        } else {
            self.functions.get(&hash_native).map(|f| &f.func)
        }
    }

    /// Does the particular function with [`Dynamic`] parameter(s) exist in the [`Module`]?
    #[inline(always)]
    #[must_use]
    pub(crate) fn contains_dynamic_fn(&self, hash_script: u64) -> bool {
        if self.dynamic_functions.is_empty() {
            false
        } else {
            self.dynamic_functions.contains(&hash_script)
        }
    }

    /// Does the particular namespace-qualified function exist in the [`Module`]?
    ///
    /// The [`u64`] hash is calculated by [`build_index`][Module::build_index].
    #[inline(always)]
    #[must_use]
    pub fn contains_qualified_fn(&self, hash_fn: u64) -> bool {
        if self.all_functions.is_empty() {
            false
        } else {
            self.all_functions.contains_key(&hash_fn)
        }
    }

    /// Get a namespace-qualified function.
    ///
    /// The [`u64`] hash is calculated by [`build_index`][Module::build_index].
    #[cfg(not(feature = "no_module"))]
    #[inline]
    #[must_use]
    pub(crate) fn get_qualified_fn(&self, hash_qualified_fn: u64) -> Option<&CallableFunction> {
        if self.all_functions.is_empty() {
            None
        } else {
            self.all_functions.get(&hash_qualified_fn)
        }
    }

    /// Combine another [`Module`] into this [`Module`].
    /// The other [`Module`] is _consumed_ to merge into this [`Module`].
    #[inline]
    pub fn combine(&mut self, other: Self) -> &mut Self {
        self.modules.extend(other.modules.into_iter());
        self.variables.extend(other.variables.into_iter());
        self.functions.extend(other.functions.into_iter());
        self.dynamic_functions
            .extend(other.dynamic_functions.into_iter());
        self.type_iterators.extend(other.type_iterators.into_iter());
        self.all_functions.clear();
        self.all_variables.clear();
        self.all_type_iterators.clear();
        self.indexed = false;
        self.contains_indexed_global_functions = false;

        #[cfg(feature = "metadata")]
        if !other.doc.is_empty() {
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
        for (.., m) in other.modules {
            self.combine_flatten(shared_take_or_clone(m));
        }
        self.variables.extend(other.variables.into_iter());
        self.functions.extend(other.functions.into_iter());
        self.dynamic_functions
            .extend(other.dynamic_functions.into_iter());
        self.type_iterators.extend(other.type_iterators.into_iter());
        self.all_functions.clear();
        self.all_variables.clear();
        self.all_type_iterators.clear();
        self.indexed = false;
        self.contains_indexed_global_functions = false;

        #[cfg(feature = "metadata")]
        if !other.doc.is_empty() {
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
        for (&k, v) in &other.functions {
            self.functions.entry(k).or_insert_with(|| v.clone());
        }
        self.dynamic_functions
            .extend(other.dynamic_functions.iter().copied());
        for (&k, v) in &other.type_iterators {
            self.type_iterators.entry(k).or_insert_with(|| v.clone());
        }
        self.all_functions.clear();
        self.all_variables.clear();
        self.all_type_iterators.clear();
        self.indexed = false;
        self.contains_indexed_global_functions = false;

        #[cfg(feature = "metadata")]
        if !other.doc.is_empty() {
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
        self.modules
            .extend(other.modules.iter().map(|(k, v)| (k.clone(), v.clone())));

        self.variables
            .extend(other.variables.iter().map(|(k, v)| (k.clone(), v.clone())));

        self.functions.extend(
            other
                .functions
                .iter()
                .filter(|&(.., f)| {
                    _filter(
                        f.metadata.namespace,
                        f.metadata.access,
                        f.func.is_script(),
                        f.metadata.name.as_str(),
                        f.metadata.params,
                    )
                })
                .map(|(&k, v)| (k, v.clone())),
        );
        // This may introduce entries that are superfluous because the function has been filtered away.
        self.dynamic_functions
            .extend(other.dynamic_functions.iter().copied());

        self.type_iterators
            .extend(other.type_iterators.iter().map(|(&k, v)| (k, v.clone())));
        self.all_functions.clear();
        self.all_variables.clear();
        self.all_type_iterators.clear();
        self.indexed = false;
        self.contains_indexed_global_functions = false;

        #[cfg(feature = "metadata")]
        if !other.doc.is_empty() {
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
        self.functions = std::mem::take(&mut self.functions)
            .into_iter()
            .filter(|(.., f)| {
                if f.func.is_script() {
                    filter(
                        f.metadata.namespace,
                        f.metadata.access,
                        f.metadata.name.as_str(),
                        f.metadata.params,
                    )
                } else {
                    false
                }
            })
            .collect();

        self.all_functions.clear();
        self.dynamic_functions.clear();
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
    #[inline]
    pub fn iter_sub_modules(&self) -> impl Iterator<Item = (&str, &Shared<Module>)> {
        self.modules.iter().map(|(k, m)| (k.as_str(), m))
    }

    /// Get an iterator to the variables in the [`Module`].
    #[inline]
    pub fn iter_var(&self) -> impl Iterator<Item = (&str, &Dynamic)> {
        self.variables.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Get an iterator to the functions in the [`Module`].
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn iter_fn(&self) -> impl Iterator<Item = &FuncInfo> {
        self.functions.values().map(<_>::as_ref)
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
    #[inline]
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
        self.iter_fn().filter(|&f| f.func.is_script()).map(|f| {
            (
                f.metadata.namespace,
                f.metadata.access,
                f.metadata.name.as_str(),
                f.metadata.params,
                f.func.get_script_fn_def().expect("script-defined function"),
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
        self.iter_fn().filter(|&f| f.func.is_script()).map(|f| {
            (
                f.metadata.namespace,
                f.metadata.access,
                f.metadata.name.as_str(),
                f.metadata.params,
            )
        })
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
        let global = &mut crate::eval::GlobalRuntimeState::new(engine);

        Self::eval_ast_as_new_raw(engine, scope, global, ast)
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
        scope: crate::Scope,
        global: &mut crate::eval::GlobalRuntimeState,
        ast: &crate::AST,
    ) -> RhaiResultOf<Self> {
        let mut scope = scope;

        // Save global state
        let orig_imports_len = global.num_imports();
        let orig_source = global.source.clone();
        #[cfg(not(feature = "no_function"))]
        let orig_constants = std::mem::take(&mut global.constants);

        // Run the script
        let result = engine.eval_ast_with_scope_raw(&mut scope, global, ast, 0);

        // Create new module
        let mut module = Module::new();

        // Extra modules left become sub-modules
        let mut imports = StaticVec::new_const();

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
        global.source = orig_source;

        result?;

        // Variables with an alias left in the scope become module variables
        for (_name, value, mut aliases) in scope {
            // It is an error to export function pointers that refer to encapsulated local functions
            #[cfg(not(feature = "no_function"))]
            if let Some(fn_ptr) = value.downcast_ref::<crate::FnPtr>() {
                if ast.iter_fn_def().any(|f| f.name == fn_ptr.fn_name()) {
                    return Err(crate::ERR::ErrorMismatchDataType(
                        String::new(),
                        if fn_ptr.is_anonymous() {
                            format!("cannot export closure in variable {_name}")
                        } else {
                            format!(
                                "cannot export function pointer to local function '{}' in variable {_name}",
                                fn_ptr.fn_name()
                            )
                        },
                        crate::Position::NONE,
                    )
                    .into());
                }
            }

            match aliases.len() {
                0 => (),
                1 => {
                    let alias = aliases.pop().unwrap();
                    module.set_var(alias, value);
                }
                _ => {
                    let last_alias = aliases.pop().unwrap();
                    for alias in aliases {
                        module.set_var(alias, value.clone());
                    }
                    // Avoid cloning the last value
                    module.set_var(last_alias, value);
                }
            }
        }

        // Non-private functions defined become module functions
        #[cfg(not(feature = "no_function"))]
        {
            let environ = Shared::new(crate::ast::EncapsulatedEnviron {
                lib: ast.shared_lib().clone(),
                imports: imports.into_boxed_slice(),
                constants,
            });

            ast.shared_lib()
                .iter_fn()
                .filter(|&f| match f.metadata.access {
                    FnAccess::Public => true,
                    FnAccess::Private => false,
                })
                .filter(|&f| f.func.is_script())
                .for_each(|f| {
                    let mut func = f
                        .func
                        .get_script_fn_def()
                        .expect("script-defined function")
                        .as_ref()
                        .clone();

                    // Encapsulate AST environment
                    func.environ = Some(environ.clone());

                    module.set_script_fn(func);
                });
        }

        module.set_id(ast.source_raw().clone());

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
            functions: &mut BTreeMap<u64, CallableFunction>,
            type_iterators: &mut BTreeMap<TypeId, Shared<IteratorFn>>,
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
                let hash_var = crate::calc_qualified_var_hash(path.iter().copied(), var_name);
                variables.insert(hash_var, value.clone());
            }

            // Index type iterators
            for (&type_id, func) in &module.type_iterators {
                type_iterators.insert(type_id, func.clone());
                contains_indexed_global_functions = true;
            }

            // Index all Rust functions
            for (&hash, f) in &module.functions {
                match f.metadata.namespace {
                    FnNamespace::Global => {
                        // Flatten all functions with global namespace
                        functions.insert(hash, f.func.clone());
                        contains_indexed_global_functions = true;
                    }
                    FnNamespace::Internal => (),
                }
                match f.metadata.access {
                    FnAccess::Public => (),
                    FnAccess::Private => continue, // Do not index private functions
                }

                if !f.func.is_script() {
                    let hash_qualified_fn = calc_native_fn_hash(
                        path.iter().copied(),
                        f.metadata.name.as_str(),
                        &f.param_types,
                    );
                    functions.insert(hash_qualified_fn, f.func.clone());
                } else if cfg!(not(feature = "no_function")) {
                    let hash_qualified_script = crate::calc_qualified_fn_hash(
                        path.iter().copied(),
                        f.metadata.name.as_str(),
                        f.metadata.params,
                    );
                    functions.insert(hash_qualified_script, f.func.clone());
                }
            }

            contains_indexed_global_functions
        }

        if !self.indexed {
            let mut path = Vec::with_capacity(4);
            let mut variables = BTreeMap::new();
            let mut functions = BTreeMap::new();
            let mut type_iterators = BTreeMap::new();

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
        if self.all_type_iterators.is_empty() {
            false
        } else {
            self.all_type_iterators.contains_key(&id)
        }
    }

    /// Does a type iterator exist in the module?
    #[inline(always)]
    #[must_use]
    pub fn contains_iter(&self, id: TypeId) -> bool {
        if self.type_iterators.is_empty() {
            false
        } else {
            self.type_iterators.contains_key(&id)
        }
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
        if self.indexed {
            self.all_type_iterators.insert(type_id, func.clone());
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
    pub(crate) fn get_qualified_iter(&self, id: TypeId) -> Option<&IteratorFn> {
        if self.all_type_iterators.is_empty() {
            None
        } else {
            self.all_type_iterators.get(&id).map(|f| &**f)
        }
    }

    /// Get the specified type iterator.
    #[inline]
    #[must_use]
    pub(crate) fn get_iter(&self, id: TypeId) -> Option<&IteratorFn> {
        if self.type_iterators.is_empty() {
            None
        } else {
            self.type_iterators.get(&id).map(|f| &**f)
        }
    }
}

/// Module containing all built-in [module resolvers][ModuleResolver].
#[cfg(not(feature = "no_module"))]
pub mod resolvers;

#[cfg(not(feature = "no_module"))]
pub use resolvers::ModuleResolver;
