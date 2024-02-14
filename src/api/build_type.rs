//! Trait to build a custom type for use with [`Engine`].
use crate::func::SendSync;
use crate::packages::string_basic::{FUNC_TO_DEBUG, FUNC_TO_STRING};
use crate::{types::dynamic::Variant, Engine, Identifier, RhaiNativeFunc};
use std::marker::PhantomData;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
use crate::func::register::Mut;

/// Trait to build the API of a custom type for use with an [`Engine`]
/// (i.e. register the type and its getters, setters, methods, etc.).
///
/// # Example
///
/// ```
/// # #[cfg(not(feature = "no_object"))]
/// # {
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
///             // Register pretty-print name of the type
///             .with_name("TestStruct")
///             // Register display functions
///             .on_print(|v| format!("TestStruct({})", v.field))
///             .on_debug(|v| format!("{v:?}"))
///             // Register a constructor function
///             .with_fn("new_ts", Self::new)
///             // Register the 'update' method
///             .with_fn("update", Self::update)
///             // Register the 'value' property
///             .with_get_set("value", Self::get_value, Self::set_value);
///     }
/// }
///
/// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
/// let mut engine = Engine::new();
///
/// // Register API for the custom type.
/// engine.build_type::<TestStruct>();
///
/// assert_eq!(
///     engine.eval::<TestStruct>("let x = new_ts(); x.update(41); print(x); x")?,
///     TestStruct { field: 42 }
/// );
/// # Ok(())
/// # }
/// # }
/// ```
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
pub struct TypeBuilder<'a, T: Variant + Clone> {
    engine: &'a mut Engine,
    _marker: PhantomData<T>,
}

impl<'a, T: Variant + Clone> TypeBuilder<'a, T> {
    /// Create a [`TypeBuilder`] linked to a particular [`Engine`] instance.
    #[inline(always)]
    fn new(engine: &'a mut Engine) -> Self {
        Self {
            engine,
            _marker: PhantomData,
        }
    }
}

impl<'s, T: Variant + Clone> TypeBuilder<'_, T> {
    /// Set a pretty-print name for the `type_of` function.
    #[inline(always)]
    pub fn with_name(&mut self, name: &str) -> &mut Self {
        self.engine.register_type_with_name::<T>(name);
        self
    }

    /// Pretty-print this custom type.
    #[inline(always)]
    pub fn on_print(
        &mut self,
        on_print: impl Fn(&mut T) -> String + SendSync + 'static,
    ) -> &mut Self {
        self.engine.register_fn(FUNC_TO_STRING, on_print);
        self
    }

    /// Debug-print this custom type.
    #[inline(always)]
    pub fn on_debug(
        &mut self,
        on_print: impl Fn(&mut T) -> String + SendSync + 'static,
    ) -> &mut Self {
        self.engine.register_fn(FUNC_TO_DEBUG, on_print);
        self
    }

    /// Register a custom method.
    #[inline(always)]
    pub fn with_fn<A: 'static, const N: usize, const X: bool, R: Variant + Clone, const F: bool>(
        &mut self,
        name: impl AsRef<str> + Into<Identifier>,
        method: impl RhaiNativeFunc<A, N, X, R, F> + SendSync + 'static,
    ) -> &mut Self {
        self.engine.register_fn(name, method);
        self
    }
}

impl<T> TypeBuilder<'_, T>
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
impl<T: Variant + Clone> TypeBuilder<'_, T> {
    /// Register a getter function.
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under `no_object`.
    #[inline(always)]
    pub fn with_get<const X: bool, R: Variant + Clone, const F: bool>(
        &mut self,
        name: impl AsRef<str>,
        get_fn: impl RhaiNativeFunc<(Mut<T>,), 1, X, R, F> + SendSync + 'static,
    ) -> &mut Self {
        self.engine.register_get(name, get_fn);
        self
    }

    /// Register a setter function.
    ///
    /// Not available under `no_object`.
    #[inline(always)]
    pub fn with_set<const X: bool, R: Variant + Clone, const F: bool>(
        &mut self,
        name: impl AsRef<str>,
        set_fn: impl RhaiNativeFunc<(Mut<T>, R), 2, X, (), F> + SendSync + 'static,
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
    pub fn with_get_set<
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
        self.engine.register_get_set(name, get_fn, set_fn);
        self
    }
}

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
impl<T: Variant + Clone> TypeBuilder<'_, T> {
    /// Register an index getter.
    ///
    /// The function signature must start with `&mut self` and not `&self`.
    ///
    /// Not available under both `no_index` and `no_object`.
    #[inline(always)]
    pub fn with_indexer_get<
        IDX: Variant + Clone,
        const X: bool,
        R: Variant + Clone,
        const F: bool,
    >(
        &mut self,
        get_fn: impl RhaiNativeFunc<(Mut<T>, IDX), 2, X, R, F> + SendSync + 'static,
    ) -> &mut Self {
        self.engine.register_indexer_get(get_fn);
        self
    }

    /// Register an index setter.
    ///
    /// Not available under both `no_index` and `no_object`.
    #[inline(always)]
    pub fn with_indexer_set<
        IDX: Variant + Clone,
        const X: bool,
        R: Variant + Clone,
        const F: bool,
    >(
        &mut self,
        set_fn: impl RhaiNativeFunc<(Mut<T>, IDX, R), 3, X, (), F> + SendSync + 'static,
    ) -> &mut Self {
        self.engine.register_indexer_set(set_fn);
        self
    }

    /// Short-hand for registering both index getter and setter functions.
    ///
    /// Not available under both `no_index` and `no_object`.
    #[inline(always)]
    pub fn with_indexer_get_set<
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
        self.engine.register_indexer_get_set(get_fn, set_fn);
        self
    }
}
