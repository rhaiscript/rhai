//! Trait to build a custom type for use with [`Engine`].
#![allow(deprecated)]

use crate::{types::dynamic::Variant, Engine, Identifier, RegisterNativeFunction};
use std::marker::PhantomData;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
use crate::func::register::Mut;

/// Trait to build the API of a custom type for use with an [`Engine`]
/// (i.e. register the type and its getters, setters, methods, etc.).
///
/// # WARNING - Volatile Trait
///
/// This API is volatile and may change in the future.
///
/// # Example
///
/// ```
/// use rhai::{CustomType, TypeBuilder, Engine};
///
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
///     fn get_value(&mut self) -> i64 {
///         self.field
///     }
///     fn set_value(&mut self, value: i64) {
///        self.field = value;
///     }
/// }
///
/// impl CustomType for TestStruct {
///     fn build(mut builder: TypeBuilder<Self>) {
///         builder
///             .with_name("TestStruct")
///             .with_fn("new_ts", Self::new)
///             .with_fn("update", Self::update);
///     }
/// }
///
/// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
///
/// let mut engine = Engine::new();
///
/// // Register API for the custom type.
/// engine.build_type::<TestStruct>();
///
///
/// # #[cfg(not(feature = "no_object"))]
/// assert_eq!(
///     engine.eval::<TestStruct>("let x = new_ts(); x.update(41); x")?,
///     TestStruct { field: 42 }
/// );
/// # Ok(())
/// # }
/// ```
#[deprecated = "This trait is NOT deprecated, but it is considered volatile and may change in the future."]
pub trait CustomType: Variant + Clone {
    /// Builds the custom type for use with the [`Engine`].
    ///
    /// Methods, property getters/setters, indexers etc. should be registered in this function.
    fn build(builder: TypeBuilder<Self>);
}

impl Engine {
    /// Build the API of a custom type for use with the [`Engine`].
    ///
    /// The custom type must implement [`CustomType`].
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    #[inline]
    pub fn build_type<T: CustomType>(&mut self) -> &mut Self {
        T::build(TypeBuilder::new(self));
        self
    }
}

/// Builder to build the API of a custom type for use with an [`Engine`].
///
/// The type is automatically registered when this builder is dropped.
///
/// ## Pretty-Print Name
///
/// By default the type is registered with [`Engine::register_type`] (i.e. without a pretty-print name).
///
/// To define a pretty-print name, call [`with_name`][`TypeBuilder::with_name`],
/// to use [`Engine::register_type_with_name`] instead.
#[deprecated = "This type is NOT deprecated, but it is considered volatile and may change in the future."]
pub struct TypeBuilder<'a, T: Variant + Clone> {
    engine: &'a mut Engine,
    name: Option<&'static str>,
    _marker: PhantomData<T>,
}

impl<'a, T: Variant + Clone> TypeBuilder<'a, T> {
    /// Create a [`TypeBuilder`] linked to a particular [`Engine`] instance.
    #[inline(always)]
    fn new(engine: &'a mut Engine) -> Self {
        Self {
            engine,
            name: None,
            _marker: PhantomData::default(),
        }
    }
}

impl<'a, T: Variant + Clone> TypeBuilder<'a, T> {
    /// Set a pretty-print name for the `type_of` function.
    #[inline(always)]
    pub fn with_name(&mut self, name: &'static str) -> &mut Self {
        self.name = Some(name);
        self
    }

    /// Register a custom function.
    #[inline(always)]
    pub fn with_fn<A, R, S>(
        &mut self,
        name: impl AsRef<str> + Into<Identifier>,
        method: impl RegisterNativeFunction<A, R, S>,
    ) -> &mut Self {
        self.engine.register_fn(name, method);
        self
    }
}

impl<'a, T> TypeBuilder<'a, T>
where
    T: Variant + Clone + IntoIterator,
    <T as IntoIterator>::Item: Variant + Clone,
{
    /// Register a type iterator.
    /// This is an advanced API.
    #[inline(always)]
    pub fn is_iterable(&mut self) -> &mut Self {
        self.engine.register_iterator::<T>();
        self
    }
}

#[cfg(not(feature = "no_object"))]
impl<'a, T: Variant + Clone> TypeBuilder<'a, T> {
    /// Register a getter function.
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under `no_object`.
    #[inline(always)]
    pub fn with_get<V: Variant + Clone, S>(
        &mut self,
        name: impl AsRef<str>,
        get_fn: impl RegisterNativeFunction<(Mut<T>,), V, S> + crate::func::SendSync + 'static,
    ) -> &mut Self {
        self.engine.register_get(name, get_fn);
        self
    }

    /// Register a setter function.
    ///
    /// Not available under `no_object`.
    #[inline(always)]
    pub fn with_set<V: Variant + Clone, S>(
        &mut self,
        name: impl AsRef<str>,
        set_fn: impl RegisterNativeFunction<(Mut<T>, V), (), S> + crate::func::SendSync + 'static,
    ) -> &mut Self {
        self.engine.register_set(name, set_fn);
        self
    }

    /// Short-hand for registering both getter and setter functions.
    ///
    /// All function signatures must start with `&mut self` and not `&self`.
    ///
    /// Not available under `no_object`.
    #[inline(always)]
    pub fn with_get_set<V: Variant + Clone, S1, S2>(
        &mut self,
        name: impl AsRef<str>,
        get_fn: impl RegisterNativeFunction<(Mut<T>,), V, S1> + crate::func::SendSync + 'static,
        set_fn: impl RegisterNativeFunction<(Mut<T>, V), (), S2> + crate::func::SendSync + 'static,
    ) -> &mut Self {
        self.engine.register_get_set(name, get_fn, set_fn);
        self
    }
}

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
impl<'a, T: Variant + Clone> TypeBuilder<'a, T> {
    /// Register an index getter.
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under both `no_index` and `no_object`.
    #[inline(always)]
    pub fn with_indexer_get<X: Variant + Clone, V: Variant + Clone, S>(
        &mut self,
        get_fn: impl RegisterNativeFunction<(Mut<T>, X), V, S> + crate::func::SendSync + 'static,
    ) -> &mut Self {
        self.engine.register_indexer_get(get_fn);
        self
    }

    /// Register an index setter.
    ///
    /// Not available under both `no_index` and `no_object`.
    #[inline(always)]
    pub fn with_indexer_set<X: Variant + Clone, V: Variant + Clone, S>(
        &mut self,
        set_fn: impl RegisterNativeFunction<(Mut<T>, X, V), (), S> + crate::func::SendSync + 'static,
    ) -> &mut Self {
        self.engine.register_indexer_set(set_fn);
        self
    }

    /// Short-hand for registering both index getter and setter functions.
    ///
    /// Not available under both `no_index` and `no_object`.
    #[inline(always)]
    pub fn with_indexer_get_set<X: Variant + Clone, V: Variant + Clone, S1, S2>(
        &mut self,
        get_fn: impl RegisterNativeFunction<(Mut<T>, X), V, S1> + crate::func::SendSync + 'static,
        set_fn: impl RegisterNativeFunction<(Mut<T>, X, V), (), S2> + crate::func::SendSync + 'static,
    ) -> &mut Self {
        self.engine.register_indexer_get_set(get_fn, set_fn);
        self
    }
}

impl<'a, T: Variant + Clone> Drop for TypeBuilder<'a, T> {
    #[inline]
    fn drop(&mut self) {
        if let Some(name) = self.name {
            self.engine.register_type_with_name::<T>(name);
        } else {
            self.engine.register_type::<T>();
        }
    }
}
