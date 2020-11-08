use crate::engine::Engine;
use crate::fn_native::Shared;
use crate::module::{Module, ModuleResolver};
use crate::result::EvalAltResult;
use crate::token::Position;

use crate::stdlib::{boxed::Box, ops::AddAssign, vec::Vec};

/// Module resolution service that holds a collection of module resolves,
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
/// engine.set_module_resolver(Some(collection));
/// ```
#[derive(Default)]
pub struct ModuleResolversCollection(Vec<Box<dyn ModuleResolver>>);

impl ModuleResolversCollection {
    /// Create a new `ModuleResolversCollection`.
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
    /// engine.set_module_resolver(Some(collection));
    /// ```
    #[inline(always)]
    pub fn new() -> Self {
        Default::default()
    }
    /// Add a module keyed by its path.
    #[inline(always)]
    pub fn push(&mut self, resolver: impl ModuleResolver + 'static) {
        self.0.push(Box::new(resolver));
    }
    /// Get an iterator of all the module resolvers.
    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = &dyn ModuleResolver> {
        self.0.iter().map(|v| v.as_ref())
    }
    /// Get a mutable iterator of all the modules.
    #[inline(always)]
    pub fn into_iter(self) -> impl Iterator<Item = Box<dyn ModuleResolver>> {
        self.0.into_iter()
    }
    /// Remove all module resolvers.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.0.clear();
    }
    /// Is this `ModuleResolversCollection` empty?
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Get the number of module resolvers in this `ModuleResolversCollection`.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Add another `ModuleResolversCollection` to the end of this collection.
    /// The other `ModuleResolversCollection` is consumed.
    #[inline(always)]
    pub fn append(&mut self, other: Self) {
        self.0.extend(other.0.into_iter());
    }
}

impl ModuleResolver for ModuleResolversCollection {
    fn resolve(
        &self,
        engine: &Engine,
        path: &str,
        pos: Position,
    ) -> Result<Shared<Module>, Box<EvalAltResult>> {
        for resolver in self.0.iter() {
            match resolver.resolve(engine, path, pos) {
                Ok(module) => return Ok(module),
                Err(err) => match *err {
                    EvalAltResult::ErrorModuleNotFound(_, _) => continue,
                    EvalAltResult::ErrorInModule(_, err, _) => return Err(err),
                    _ => panic!("ModuleResolver::resolve returns error that is not ErrorModuleNotFound or ErrorInModule"),
                },
            }
        }

        EvalAltResult::ErrorModuleNotFound(path.into(), pos).into()
    }
}

impl<M: ModuleResolver + 'static> AddAssign<M> for ModuleResolversCollection {
    #[inline(always)]
    fn add_assign(&mut self, rhs: M) {
        self.push(rhs);
    }
}
