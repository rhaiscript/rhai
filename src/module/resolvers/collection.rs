use crate::{
    Engine, ModuleResolver, Position, RhaiResultOf, SharedModule, StaticVec, ERR,
    STATIC_VEC_INLINE_SIZE,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{ops::AddAssign, slice::Iter};

/// [Module][crate::Module] resolution service that holds a collection of module resolvers,
/// to be searched in sequential order.
///
/// # Example
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
/// engine.set_module_resolver(collection);
/// ```
#[derive(Default)]
pub struct ModuleResolversCollection(StaticVec<Box<dyn ModuleResolver>>);

impl ModuleResolversCollection {
    /// Create a new [`ModuleResolversCollection`].
    ///
    /// # Example
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
    /// engine.set_module_resolver(collection);
    /// ```
    #[inline(always)]
    #[must_use]
    pub const fn new() -> Self {
        Self(StaticVec::new_const())
    }
    /// Append a [module resolver][ModuleResolver] to the end.
    #[inline(always)]
    pub fn push(&mut self, resolver: impl ModuleResolver + 'static) -> &mut Self {
        self.0.push(Box::new(resolver));
        self
    }
    /// Insert a [module resolver][ModuleResolver] to an offset index.
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    #[inline(always)]
    pub fn insert(&mut self, index: usize, resolver: impl ModuleResolver + 'static) -> &mut Self {
        self.0.insert(index, Box::new(resolver));
        self
    }
    /// Remove the last [module resolver][ModuleResolver] from the end, if any.
    #[inline(always)]
    pub fn pop(&mut self) -> Option<Box<dyn ModuleResolver>> {
        self.0.pop()
    }
    /// Remove a [module resolver][ModuleResolver] at an offset index.
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    #[inline(always)]
    pub fn remove(&mut self, index: usize) -> Box<dyn ModuleResolver> {
        self.0.remove(index)
    }
    /// Get an iterator of all the [module resolvers][ModuleResolver].
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &dyn ModuleResolver> {
        self.0.iter().map(<_>::as_ref)
    }
    /// Remove all [module resolvers][ModuleResolver].
    #[inline(always)]
    pub fn clear(&mut self) -> &mut Self {
        self.0.clear();
        self
    }
    /// Returns `true` if this [`ModuleResolversCollection`] contains no module resolvers.
    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Get the number of [module resolvers][ModuleResolver] in this [`ModuleResolversCollection`].
    #[inline(always)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Add another [`ModuleResolversCollection`] to the end of this collection.
    /// The other [`ModuleResolversCollection`] is consumed.
    #[inline]
    pub fn append(&mut self, other: Self) -> &mut Self {
        self.0.extend(other.0);
        self
    }
}

impl IntoIterator for ModuleResolversCollection {
    type Item = Box<dyn ModuleResolver>;
    type IntoIter = smallvec::IntoIter<[Box<dyn ModuleResolver>; STATIC_VEC_INLINE_SIZE]>;

    #[inline(always)]
    #[must_use]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a ModuleResolversCollection {
    type Item = &'a Box<dyn ModuleResolver>;
    type IntoIter = Iter<'a, Box<dyn ModuleResolver>>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Extend<Box<dyn ModuleResolver>> for ModuleResolversCollection {
    #[inline(always)]
    fn extend<T: IntoIterator<Item = Box<dyn ModuleResolver>>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}

impl<M: ModuleResolver + 'static> AddAssign<M> for ModuleResolversCollection {
    #[inline(always)]
    fn add_assign(&mut self, rhs: M) {
        self.push(rhs);
    }
}

impl ModuleResolver for ModuleResolversCollection {
    fn resolve(
        &self,
        engine: &Engine,
        source_path: Option<&str>,
        path: &str,
        pos: Position,
    ) -> RhaiResultOf<SharedModule> {
        for resolver in &self.0 {
            match resolver.resolve(engine, source_path, path, pos) {
                Ok(module) => return Ok(module),
                Err(err) => match *err {
                    ERR::ErrorModuleNotFound(..) => continue,
                    ERR::ErrorInModule(_, err, _) => return Err(err),
                    _ => unreachable!("ModuleResolver::resolve returns error that is not ErrorModuleNotFound or ErrorInModule"),
                },
            }
        }

        Err(ERR::ErrorModuleNotFound(path.into(), pos).into())
    }
}
