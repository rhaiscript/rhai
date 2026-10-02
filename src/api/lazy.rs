//! Module that defines the API for loading functions on demand.

use crate::func::lazy::{LazyFunctionSource, LazyFunctions};
use crate::func::locked_write;
use crate::plugin::ModuleManifest;
use crate::Engine;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

impl Engine {
    /// Get the on-demand functions registry, creating it if necessary.
    #[inline]
    fn lazy_functions_mut(&mut self) -> &mut LazyFunctions {
        self.lazy_functions.get_or_insert_with(Default::default)
    }
    /// Register a source of functions that are loaded on demand.
    ///
    /// When a function call is not found, each source is asked, in order of registration, to load
    /// a function matching the call. The loaded function is kept for the lifetime of the [`Engine`].
    ///
    /// * `namespace = None`: functions are available to unqualified calls (like
    ///   [`register_global_module`][Engine::register_global_module]).
    /// * `namespace = Some(name)`: functions are available to qualified calls under `name`
    ///   (e.g. `name::func()`), and [global][crate::FnNamespace::Global] functions are also
    ///   available to unqualified calls (like [`register_static_module`][Engine::register_static_module]).
    pub fn register_lazy_source(
        &mut self,
        namespace: Option<&str>,
        source: impl LazyFunctionSource + 'static,
    ) -> &mut Self {
        let source: Box<dyn LazyFunctionSource> = Box::new(source);

        match namespace {
            None => self.lazy_functions_mut().global.push(source),
            #[cfg(not(feature = "no_module"))]
            Some(name) => {
                self.lazy_functions_mut()
                    .namespaces
                    .insert(name.trim().into(), source);
            }
            #[cfg(feature = "no_module")]
            Some(..) => (),
        }

        self
    }
    /// Register a plugin module's [manifest][ModuleManifest] into the global namespace.
    /// Functions are loaded on demand, one at a time, when they are called.
    ///
    /// Constants and custom types in the plugin module are registered immediately.
    ///
    /// Use [`exported_manifest!`][crate::plugin::exported_manifest] to get the manifest of a
    /// plugin module defined via `#[export_module(manifest)]`.
    pub fn register_lazy_global_module(&mut self, manifest: &'static ModuleManifest) -> &mut Self {
        (manifest.init_eager)(self.global_namespace_mut());
        self.register_lazy_source(None, manifest)
    }
    /// Register a plugin module's [manifest][ModuleManifest] as a static module namespace.
    /// Functions are loaded on demand, one at a time, when they are called.
    ///
    /// Constants and custom types in the plugin module (and its sub-modules) are registered
    /// immediately.
    ///
    /// Use [`exported_manifest!`][crate::plugin::exported_manifest] to get the manifest of a
    /// plugin module defined via `#[export_module(manifest)]`.
    #[cfg(not(feature = "no_module"))]
    pub fn register_lazy_static_module(
        &mut self,
        name: impl AsRef<str>,
        manifest: &'static ModuleManifest,
    ) -> &mut Self {
        fn make_stub(manifest: &ModuleManifest) -> crate::Module {
            let mut module = crate::Module::new();
            (manifest.init_eager)(&mut module);
            for (name, sub_module) in manifest.sub_modules {
                module.set_sub_module(*name, make_stub(sub_module));
            }
            module
        }

        let name = name.as_ref();
        self.register_static_module(name, make_stub(manifest).into());
        self.register_lazy_source(Some(name), manifest)
    }
    /// Register a [package][crate::packages::Package] into the global namespace, loading its
    /// functions on demand, one at a time, when they are called.
    ///
    /// Functions in plugin modules combined into the package via
    /// [`Module::combine_manifest`][crate::Module::combine_manifest] are loaded on demand.
    /// Everything else in the package (e.g. iterators, constants, custom types and other functions)
    /// is registered immediately.
    ///
    /// # Example
    ///
    /// ```
    /// # use rhai::Engine;
    /// use rhai::packages::StandardPackage;
    ///
    /// let mut engine = Engine::new_raw();
    /// engine.register_lazy_package::<StandardPackage>();
    ///
    /// assert_eq!(engine.eval::<String>(r#"to_upper("hello")"#).unwrap(), "HELLO");
    /// ```
    pub fn register_lazy_package<P: crate::packages::Package>(&mut self) -> &mut Self {
        let mut module = crate::Module::new();
        module.collect_manifests();
        P::init(&mut module);
        let manifests = module.take_manifests();
        module.build_index();

        P::init_engine(self);
        self.register_global_module(module.into());

        if !manifests.is_empty() {
            self.register_lazy_source(None, crate::func::lazy::LazyPackage(manifests));
        }
        self
    }
    /// Remove all functions loaded on demand so far.
    /// They will be loaded again when called.
    pub fn clear_loaded_functions(&self) -> &Self {
        if let Some(mut loaded) = self
            .lazy_functions
            .as_deref()
            .and_then(|lazy| locked_write(&lazy.loaded))
        {
            *loaded = Default::default();
        }
        self
    }
}
