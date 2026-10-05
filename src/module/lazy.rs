//! Functions in plugin modules, looked up from their manifests only when called.

use super::{new_hash_map, FnNamespace, Module, ModuleFlags};
use crate::func::RhaiFunc;
use crate::plugin::{FnManifestEntry, ModuleManifest};
use std::borrow::Cow;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Plugin module manifests whose functions are looked up only when called.
#[derive(Debug, Clone, Default)]
pub(super) struct LazyFunctions {
    /// Manifests, and whether their sub-modules are flattened. Later manifests take precedence.
    pub(super) manifests: Vec<(&'static ModuleManifest, bool)>,
}

impl LazyFunctions {
    /// Call a function on every function in the manifests, in order of precedence.
    pub(super) fn for_each_fn(&self, f: &mut dyn FnMut(&'static FnManifestEntry)) {
        for &(manifest, flatten) in &self.manifests {
            manifest.for_each_fn(flatten, f);
        }
    }
    /// Find the function in the manifests with the highest precedence that matches a predicate.
    fn find_fn(
        &self,
        f: &mut dyn FnMut(&FnManifestEntry) -> bool,
    ) -> Option<&'static FnManifestEntry> {
        self.manifests
            .iter()
            .rev()
            .find_map(|&(manifest, flatten)| manifest.find_fn(flatten, f))
    }
}

impl Module {
    /// Look up a function in plugin module manifests by hash, calculating the hash of each
    /// function in turn.
    #[must_use]
    pub(super) fn get_lazy_fn(&self, hash_native: u64) -> Option<RhaiFunc> {
        self.lazy_functions
            .as_ref()?
            .find_fn(&mut |f| f.calc_hash(&[]) == hash_native)
            .map(|f| RhaiFunc::StaticPlugin { func: f.func })
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
        (manifest.init_eager)(&mut module);

        for &(name, sub_module) in manifest.sub_modules {
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
            (manifest.init_eager)(module);
            for &(.., sub_module) in manifest.sub_modules {
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
        self.push_manifests([(manifest, flatten)]);
    }
    /// Add manifests whose functions are looked up only when called, taking precedence over
    /// existing ones, without updating the filter on functions with [`Dynamic`][crate::Dynamic] parameters.
    pub(super) fn push_manifests(
        &mut self,
        manifests: impl IntoIterator<Item = (&'static ModuleManifest, bool)>,
    ) {
        let lazy_functions = self.lazy_functions.get_or_insert_with(Default::default);
        lazy_functions.manifests.extend(manifests);

        self.flags
            .remove(ModuleFlags::INDEXED | ModuleFlags::INDEXED_GLOBAL_FUNCTIONS);
    }
    /// Register all functions in plugin module manifests (including sub-modules), with full
    /// metadata, so that they are no longer looked up only when called.
    pub fn register_lazy_functions(&mut self) -> &mut Self {
        if !self.has_lazy_functions() {
            return self;
        }
        if let Some(lazy_functions) = self.lazy_functions.take() {
            // Functions registered normally take precedence over functions in manifests
            let functions = self.functions.take();
            lazy_functions.for_each_fn(&mut |f| (f.register)(self));
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
    /// Does this [`Module`] (or any sub-module) contain functions in plugin module manifests,
    /// which are looked up only when called?
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
        ) -> Option<&'static FnManifestEntry> {
            // Functions in the module take precedence over those in sub-modules
            let found = module.lazy_functions.as_ref().and_then(|f| {
                f.find_fn(&mut |f| {
                    f.calc_hash(path) == hash
                        || (f.namespace == FnNamespace::Global && f.calc_hash(&[]) == hash)
                })
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

        if !self.flags.contains(ModuleFlags::INDEXED_LAZY_FUNCTIONS) {
            return None;
        }

        let path = &mut crate::StaticVec::new();
        path.push("");
        find_fn(self, path, hash_qualified_fn).map(|f| RhaiFunc::StaticPlugin { func: f.func })
    }
}
