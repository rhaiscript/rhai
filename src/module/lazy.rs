//! Functions looked up only when called: those in plugin module manifests, and those of Grain
//! programs.

#[cfg(feature = "grain")]
#[cfg(not(feature = "no_function"))]
use super::GrainFunctions;
use super::{new_hash_map, FnNamespace, Module, ModuleFlags};
use crate::func::RhaiFunc;
use crate::plugin::{FnManifestEntry, ModuleManifest};
use std::borrow::Cow;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// A source of functions looked up only when called.
#[derive(Clone)]
pub(super) enum LazySource {
    /// A plugin module manifest, and whether its sub-modules are flattened.
    Manifest(&'static ModuleManifest, bool),
    /// The functions of a Grain program.
    #[cfg(feature = "grain")]
    #[cfg(not(feature = "no_function"))]
    Grain(GrainFunctions),
}

/// Sources of functions looked up only when called.
#[derive(Clone)]
pub(super) struct LazyFunctions {
    /// Later sources take precedence. Never empty.
    pub(super) sources: Box<[LazySource]>,
}

impl LazyFunctions {
    /// Call a function on every function in the manifests, from the lowest precedence to the
    /// highest.
    ///
    /// Registering each function in turn leaves the one with the highest precedence in place.
    pub(super) fn for_each_fn(&self, f: &mut dyn FnMut(&'static FnManifestEntry)) {
        for source in &*self.sources {
            match *source {
                LazySource::Manifest(manifest, flatten) => manifest.for_each_fn(flatten, f),
                #[cfg(feature = "grain")]
                #[cfg(not(feature = "no_function"))]
                LazySource::Grain(..) => (),
            }
        }
    }
    /// The Grain functions, from the lowest precedence to the highest.
    #[cfg(feature = "grain")]
    #[cfg(not(feature = "no_function"))]
    pub(super) fn grain(&self) -> impl DoubleEndedIterator<Item = &GrainFunctions> {
        self.sources.iter().filter_map(|source| match source {
            LazySource::Grain(functions) => Some(functions),
            LazySource::Manifest(..) => None,
        })
    }
    /// Find the function with the highest precedence that matches.
    ///
    /// `manifest` matches a function in a manifest; `grain` looks up a function of a Grain program.
    fn find_fn(
        &self,
        manifest: &mut dyn FnMut(&FnManifestEntry) -> bool,
        #[cfg(feature = "grain")]
        #[cfg(not(feature = "no_function"))]
        grain: &mut dyn FnMut(&GrainFunctions) -> Option<RhaiFunc>,
    ) -> Option<RhaiFunc> {
        self.sources.iter().rev().find_map(|source| match *source {
            LazySource::Manifest(m, flatten) => m
                .find_fn(flatten, manifest)
                .map(|f| RhaiFunc::StaticPlugin { func: f.func() }),
            #[cfg(feature = "grain")]
            #[cfg(not(feature = "no_function"))]
            LazySource::Grain(ref functions) => grain(functions),
        })
    }
    /// Is any function in the global namespace?
    pub(super) fn has_global(&self) -> bool {
        self.sources.iter().any(|source| match *source {
            LazySource::Manifest(manifest, flatten) => manifest
                .find_fn(flatten, &mut |f| f.namespace() == FnNamespace::Global)
                .is_some(),
            #[cfg(feature = "grain")]
            #[cfg(not(feature = "no_function"))]
            LazySource::Grain(ref functions) => functions.has_global(),
        })
    }
    /// Register every function into `module`, from the lowest precedence to the highest.
    fn register_into(&self, module: &mut Module) {
        for source in &*self.sources {
            match *source {
                LazySource::Manifest(manifest, flatten) => {
                    manifest.for_each_fn(flatten, &mut |f| f.register_into(module));
                }
                #[cfg(feature = "grain")]
                #[cfg(not(feature = "no_function"))]
                LazySource::Grain(ref functions) => functions.register_into(module),
            }
        }
    }
}

impl Module {
    /// Look up a function in plugin module manifests, or among the functions of a Grain program,
    /// by hash, calculating the hash of each function in turn.
    #[must_use]
    pub(super) fn get_lazy_fn(&self, hash_native: u64) -> Option<RhaiFunc> {
        self.lazy_functions.as_ref().and_then(|f| {
            f.find_fn(
                &mut |f| f.calc_hash(&[]) == hash_native,
                #[cfg(feature = "grain")]
                #[cfg(not(feature = "no_function"))]
                &mut |g| g.get(hash_native),
            )
        })
    }
    /// Create a new [`Module`] from a plugin module's [manifest][crate::plugin::ModuleManifest].
    ///
    /// Functions are looked up in the manifest only when they are called. Constants, custom types
    /// and sub-modules are registered immediately.
    ///
    /// This is the lazy equivalent of [`exported_module!`][crate::plugin::exported_module].
    #[must_use]
    pub fn from_manifest(manifest: &'static ModuleManifest) -> Self {
        let mut module = Self::new();
        module.add_manifest(manifest, false);
        manifest.init_eager(&mut module);

        for &(name, sub_module) in manifest.sub_modules() {
            module.set_sub_module(name, Self::from_manifest(sub_module));
        }

        module.build_index();
        module
    }
    /// Combine a plugin module's [manifest][crate::plugin::ModuleManifest] into this [`Module`],
    /// flattening all sub-modules.
    ///
    /// Functions are looked up in the manifest only when they are called. Constants and custom
    /// types are registered immediately.
    ///
    /// Functions registered normally into this [`Module`] take precedence over functions in
    /// manifests.
    ///
    /// This is the lazy equivalent of
    /// [`combine_with_exported_module!`][crate::plugin::combine_with_exported_module].
    pub fn combine_manifest(&mut self, manifest: &'static ModuleManifest) -> &mut Self {
        fn init_eager(module: &mut Module, manifest: &ModuleManifest) {
            manifest.init_eager(module);
            for &(.., sub_module) in manifest.sub_modules() {
                init_eager(module, sub_module);
            }
        }

        self.add_manifest(manifest, true);
        init_eager(self, manifest);
        self
    }
    /// Add a manifest whose functions are looked up only when called, taking precedence over
    /// existing ones.
    pub(super) fn add_manifest(&mut self, manifest: &'static ModuleManifest, flatten: bool) {
        manifest.for_each_fn(flatten, &mut |f| {
            if let Some(hash_script) = f.calc_dynamic_hash() {
                self.dynamic_functions_filter.mark(hash_script);
            }
        });
        self.push_lazy_sources([LazySource::Manifest(manifest, flatten)]);
    }
    /// Add sources of functions looked up only when called, taking precedence over existing ones,
    /// without updating the filter on functions with [`Dynamic`][crate::Dynamic] parameters.
    pub(super) fn push_lazy_sources(&mut self, sources: impl IntoIterator<Item = LazySource>) {
        let existing = self
            .lazy_functions
            .take()
            .map_or_else(Vec::new, |f| f.sources.into_vec());
        let sources: Box<[_]> = existing.into_iter().chain(sources).collect();
        if !sources.is_empty() {
            self.lazy_functions = Some(LazyFunctions { sources });
        }

        self.flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);
    }
    /// Register all functions in plugin module manifests (including sub-modules), with full
    /// metadata, so that they are no longer looked up only when called.
    ///
    /// The functions of a Grain program this [`Module`] holds are registered too.
    pub fn register_lazy_functions(&mut self) -> &mut Self {
        if !self.has_lazy_functions() {
            return self;
        }
        if let Some(lazy_functions) = self.lazy_functions.take() {
            // Functions registered normally take precedence over functions looked up only when called
            let functions = self.functions.take();
            lazy_functions.register_into(self);
            if let Some(functions) = functions {
                self.functions
                    .get_or_insert_with(|| new_hash_map(functions.len()))
                    .extend(functions);
            }
        }
        for m in self.modules.values_mut() {
            if m.has_lazy_functions() {
                let m = crate::func::shared_make_mut(m);
                m.register_lazy_functions();
                m.build_index();
            }
        }
        self.flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);
        self
    }
    /// Does this [`Module`] (or any sub-module) contain functions in plugin module manifests, or
    /// the functions of a Grain program, which are looked up only when called?
    #[inline]
    #[must_use]
    pub fn has_lazy_functions(&self) -> bool {
        self.lazy_functions.is_some() || self.modules.values().any(|m| m.has_lazy_functions())
    }
    /// Get this [`Module`] with all functions in plugin module manifests registered, cloning it
    /// if necessary.
    ///
    /// Used for enumerating functions (e.g. for metadata), which is not a hot path.
    #[inline]
    #[must_use]
    pub(crate) fn materialized(&self) -> Cow<'_, Self> {
        if self.has_lazy_functions() {
            let mut module = self.clone();
            module.register_lazy_functions();
            module.build_index();
            Cow::Owned(module)
        } else {
            Cow::Borrowed(self)
        }
    }

    /// Get a namespace-qualified function in plugin module manifests, in this [`Module`] or any
    /// sub-module, calculating the hash of each function in turn.
    ///
    /// The [`u64`] hash is calculated the same as [`build_index`][Module::build_index].
    #[must_use]
    pub(super) fn get_lazy_qualified_fn(&self, hash_qualified_fn: u64) -> Option<RhaiFunc> {
        fn find_fn<'a>(
            module: &'a Module,
            path: &mut crate::StaticVec<&'a str>,
            hash: u64,
        ) -> Option<RhaiFunc> {
            // Functions in the module take precedence over those in sub-modules
            let found = module.lazy_functions.as_ref().and_then(|f| {
                f.find_fn(
                    &mut |f| {
                        f.calc_hash(path) == hash
                            || (f.namespace() == FnNamespace::Global && f.calc_hash(&[]) == hash)
                    },
                    #[cfg(feature = "grain")]
                    #[cfg(not(feature = "no_function"))]
                    &mut |g| g.get_qualified(path, hash),
                )
            });

            found.or_else(|| {
                module.modules.iter().rev().find_map(|(name, m)| {
                    path.push(name);
                    let found = find_fn(m, path, hash);
                    path.pop();
                    found
                })
            })
        }

        if !self.flags.contains(ModuleFlags::HAS_LAZY_FUNCTIONS) {
            return None;
        }

        let path = &mut crate::StaticVec::new();
        path.push("");
        find_fn(self, path, hash_qualified_fn)
    }
}
