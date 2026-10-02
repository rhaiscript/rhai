//! Module defining on-demand (lazy) loading of functions.

use super::plugin::ModuleManifest;
use super::{locked_read, locked_write, FnCallArgs, Locked, RhaiFunc, SendSync};
use crate::eval::Caches;
use crate::{Dynamic, Engine, Module, RhaiResultOf, StaticVec};
use std::any::TypeId;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// A request to load the function(s) matching a call that is not found.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct FnLoadRequest<'a> {
    /// Namespace path below the root the source is registered under (empty for unqualified calls).
    pub path: &'a [&'a str],
    /// Name of the function being called.
    pub name: &'a str,
    /// [`TypeId`]s of the arguments. For method calls, the first is the object.
    pub arg_types: &'a [TypeId],
    /// Is this a method-style call (`obj.method()`)?
    pub is_method_call: bool,
    /// Only functions in the [global][crate::FnNamespace::Global] namespace may be returned
    /// (an unqualified call into a namespaced source).
    pub global_only: bool,
    /// Only native Rust functions can be used (script-defined functions should not be returned).
    pub native_only: bool,
}

/// A source of functions that are loaded on demand, one at a time, when a function call is not found.
pub trait LazyFunctionSource: SendSync {
    /// Load the function(s) matching a call into a new [`Module`].
    ///
    /// Return `Ok(None)` if there is no matching function.
    fn load(&self, engine: &Engine, request: &FnLoadRequest) -> RhaiResultOf<Option<Module>>;

    /// Is there a script-defined function with a name and number of parameters (used by `is_def_fn`)?
    ///
    /// This must not load the function.
    ///
    /// * `this_type`: the type a method is bound to (e.g. `fn Type.name()`), if any.
    /// * `global_only`: `true` if only functions in the [global][crate::FnNamespace::Global]
    ///   namespace count (a namespaced source).
    #[allow(unused_variables)]
    #[inline(always)]
    fn has_script_fn(
        &self,
        name: &str,
        num_params: usize,
        this_type: Option<&str>,
        global_only: bool,
    ) -> bool {
        false
    }

    /// Load a variable for a qualified variable access (e.g. `module::VARIABLE`) that is not found.
    ///
    /// Return `Ok(None)` if there is no such variable.
    #[allow(unused_variables)]
    #[inline(always)]
    fn load_var(
        &self,
        engine: &Engine,
        path: &[&str],
        name: &str,
    ) -> RhaiResultOf<Option<Dynamic>> {
        Ok(None)
    }
}

impl LazyFunctionSource for &'static ModuleManifest {
    fn load(&self, _engine: &Engine, request: &FnLoadRequest) -> RhaiResultOf<Option<Module>> {
        let FnLoadRequest {
            path,
            name,
            arg_types,
            global_only,
            ..
        } = *request;
        let mut manifest: &ModuleManifest = self;

        for &segment in path {
            match manifest.find_sub_module(segment) {
                Some(m) => manifest = m,
                None => return Ok(None),
            }
        }

        Ok(manifest.find_fn(name, arg_types, global_only).map(|entry| {
            let mut module = Module::new();
            (entry.register)(&mut module);
            module
        }))
    }
}

/// Plugin module manifests of a package, with all sub-modules flattened.
pub(crate) struct LazyPackage(pub Vec<&'static ModuleManifest>);

impl LazyFunctionSource for LazyPackage {
    fn load(&self, _engine: &Engine, request: &FnLoadRequest) -> RhaiResultOf<Option<Module>> {
        let FnLoadRequest {
            path,
            name,
            arg_types,
            global_only,
            ..
        } = *request;
        type Best = Option<(usize, &'static super::plugin::FnManifestEntry)>;

        fn search(
            manifest: &'static ModuleManifest,
            name: &str,
            arg_types: &[TypeId],
            global_only: bool,
            best: &mut Best,
        ) {
            if let Some(f) = manifest.find_fn(name, arg_types, global_only) {
                let wildcards = (f.matches)(arg_types).unwrap_or(usize::MAX);
                // Later registrations override earlier ones
                if best.map_or(true, |(w, ..)| wildcards <= w) {
                    *best = Some((wildcards, f));
                }
            }
            for (.., sub_module) in manifest.sub_modules {
                search(sub_module, name, arg_types, global_only, best);
            }
        }

        if !path.is_empty() {
            return Ok(None);
        }

        let mut best = None;
        for &manifest in &self.0 {
            search(manifest, name, arg_types, global_only, &mut best);
        }

        Ok(best.map(|(.., f)| {
            let mut module = Module::new();
            (f.register)(&mut module);
            module
        }))
    }
}

/// A library of script-defined functions that are loaded on demand, one at a time.
///
/// Each loaded function carries the full environment of the library: it can call other functions
/// in the library (including private ones), and see the library's constants and imports.
///
/// The library's top-level statements (if any) are run once, when the first function is loaded
/// (or the first variable is accessed). A library without top-level statements is never evaluated.
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "no_ast"))]
pub struct ScriptLibrary {
    /// The library.
    ast: crate::AST,
    /// Public functions, keyed by name and number of parameters.
    functions: std::collections::BTreeMap<
        (crate::Identifier, usize),
        StaticVec<crate::Shared<super::ScriptFuncDef>>,
    >,
    /// Environment of the library, created on first use.
    environ: Locked<Option<ScriptLibraryEnviron>>,
}

/// Environment of a [`ScriptLibrary`].
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "no_ast"))]
#[derive(Clone)]
struct ScriptLibraryEnviron {
    /// Encapsulated environment for each function.
    env: crate::Shared<super::EncapsulatedEnviron>,
    /// Exported variables.
    variables: crate::SharedModule,
}

#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "no_ast"))]
impl std::fmt::Debug for ScriptLibrary {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptLibrary")
            .field("source", &self.ast.source())
            .field("functions", &self.functions.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "no_ast"))]
impl ScriptLibrary {
    /// Create a new [`ScriptLibrary`] from an [`AST`][crate::AST].
    #[must_use]
    pub fn new(ast: crate::AST) -> Self {
        let mut functions = std::collections::BTreeMap::<_, StaticVec<_>>::new();

        for f in ast.iter_fn_def() {
            if f.access == super::FnAccess::Private {
                continue;
            }
            functions
                .entry((f.name.as_str().into(), f.params.len()))
                .or_default()
                .push(f.clone());
        }

        Self {
            ast,
            functions,
            environ: Locked::new(None),
        }
    }
    /// Get the environment of the library, running its top-level statements if necessary.
    fn environ(&self, engine: &Engine) -> RhaiResultOf<ScriptLibraryEnviron> {
        if let Some(environ) = locked_read(&self.environ).and_then(|e| e.clone()) {
            return Ok(environ);
        }

        // Do not hold the lock while running the library
        let environ = if self.ast.statements().is_empty() {
            ScriptLibraryEnviron {
                env: super::EncapsulatedEnviron {
                    lib: std::iter::once(self.ast.shared_lib().clone()).collect(),
                    imports: crate::ThinVec::new(),
                    constants: None,
                }
                .into(),
                variables: Module::new().into(),
            }
        } else {
            let scope = &mut crate::Scope::new();
            let global = &mut engine.new_global_runtime_state();
            let (variables, env) = Module::eval_ast_environ(engine, scope, global, &self.ast)
                .map_err(|err| {
                    crate::ERR::ErrorInModule(
                        self.ast.source().unwrap_or_default().to_string(),
                        err,
                        crate::Position::NONE,
                    )
                })?;
            ScriptLibraryEnviron {
                env,
                variables: variables.into(),
            }
        };

        // The first environment stored wins
        Ok(match locked_write(&self.environ) {
            Some(mut guard) => guard.get_or_insert(environ).clone(),
            None => environ,
        })
    }
}

#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "no_ast"))]
impl LazyFunctionSource for ScriptLibrary {
    fn load(&self, engine: &Engine, request: &FnLoadRequest) -> RhaiResultOf<Option<Module>> {
        let FnLoadRequest {
            path,
            name,
            arg_types,
            global_only,
            is_method_call,
            native_only,
        } = *request;
        if native_only || !path.is_empty() {
            return Ok(None);
        }

        // Method calls bind the object to `this`
        let num_params = if is_method_call {
            arg_types.len() - 1
        } else {
            arg_types.len()
        };

        let Some(functions) = self.functions.get(&(name.into(), num_params)) else {
            return Ok(None);
        };

        // Only methods with a `this` type are in the global namespace
        #[cfg(not(feature = "no_object"))]
        let is_global = |f: &&crate::Shared<super::ScriptFuncDef>| f.this_type.is_some();
        #[cfg(feature = "no_object")]
        let is_global = |_: &&crate::Shared<super::ScriptFuncDef>| false;

        let mut functions = functions
            .iter()
            .filter(|f| !global_only || is_global(f))
            .peekable();

        if functions.peek().is_none() {
            return Ok(None);
        }

        let env = self.environ(engine)?.env;
        let mut module = Module::new();

        for f in functions {
            module.set_script_fn_with_env(f.clone(), env.clone());
        }

        Ok(Some(module))
    }

    fn has_script_fn(
        &self,
        name: &str,
        num_params: usize,
        this_type: Option<&str>,
        global_only: bool,
    ) -> bool {
        self.functions
            .get(&(name.into(), num_params))
            .map_or(false, |functions| {
                functions.iter().any(|_f| {
                    #[cfg(not(feature = "no_object"))]
                    return _f.this_type.as_ref().map(|t| t.as_str()) == this_type
                        && (!global_only || _f.this_type.is_some());
                    #[cfg(feature = "no_object")]
                    return this_type.is_none() && !global_only;
                })
            })
    }

    fn load_var(
        &self,
        engine: &Engine,
        path: &[&str],
        name: &str,
    ) -> RhaiResultOf<Option<Dynamic>> {
        if !path.is_empty() {
            return Ok(None);
        }
        Ok(self.environ(engine)?.variables.get_var(name))
    }
}

/// _(internals)_ Sources of on-demand functions registered with an [`Engine`], plus the functions loaded so far.
#[derive(Default)]
pub struct LazyFunctions {
    /// Sources for the global namespace.
    pub(crate) global: Vec<Box<dyn LazyFunctionSource>>,
    /// Sources for namespaces, keyed by root namespace.
    #[cfg(not(feature = "no_module"))]
    pub(crate) namespaces:
        std::collections::BTreeMap<crate::Identifier, Box<dyn LazyFunctionSource>>,
    /// Functions loaded so far.
    pub(crate) loaded: Locked<LoadedFunctions>,
}

impl std::fmt::Debug for LazyFunctions {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_struct("LazyFunctions");
        f.field("global", &self.global.len());
        #[cfg(not(feature = "no_module"))]
        f.field("namespaces", &self.namespaces.keys().collect::<Vec<_>>());
        f.finish()
    }
}

/// Functions loaded on demand.
#[derive(Debug, Default)]
pub struct LoadedFunctions {
    /// Functions for unqualified calls.
    pub(crate) global: Module,
    /// Functions for qualified calls, keyed by full namespace path.
    #[cfg(not(feature = "no_module"))]
    pub(crate) namespaces: std::collections::BTreeMap<crate::Identifier, Module>,
}

impl Engine {
    /// Get a function loaded on demand.
    ///
    /// `namespace` is the full namespace path for qualified calls.
    #[inline]
    #[must_use]
    pub(crate) fn get_loaded_fn(&self, namespace: Option<&str>, hash: u64) -> Option<RhaiFunc> {
        let lazy = self.lazy_functions.as_deref()?;
        let loaded = locked_read(&lazy.loaded)?;

        match namespace {
            None => loaded.global.get_fn(hash).cloned(),
            #[cfg(not(feature = "no_module"))]
            Some(ns) => loaded.namespaces.get(ns)?.get_fn(hash).cloned(),
            #[cfg(feature = "no_module")]
            Some(..) => None,
        }
    }
    /// Find a function loaded on demand for a qualified call, including versions with
    /// [`Dynamic`] parameters.
    ///
    /// `namespace` is the full namespace path.
    #[cfg(not(feature = "no_ast"))]
    #[cfg(not(feature = "no_module"))]
    #[must_use]
    pub(crate) fn find_loaded_qualified_fn(
        &self,
        namespace: &str,
        name: &str,
        args: &FnCallArgs,
    ) -> Option<RhaiFunc> {
        let lazy = self.lazy_functions.as_deref()?;
        let loaded = locked_read(&lazy.loaded)?;
        let module = loaded.namespaces.get(namespace)?;

        let hash_script = super::calc_fn_hash(None, name, args.len());

        // Script-defined functions first, then native functions
        if let Some(f) = module.get_fn(hash_script).or_else(|| {
            module.get_fn(super::calc_fn_hash_full(
                hash_script,
                args.iter().map(|a| a.type_id()),
            ))
        }) {
            return Some(f.clone());
        }

        if args.is_empty() || !module.may_contain_dynamic_fn(hash_script) {
            return None;
        }

        // Try all permutations with `Dynamic` wildcards
        let max_dynamic_count = usize::min(
            args.len(),
            crate::api::default_limits::MAX_DYNAMIC_PARAMETERS,
        );

        (1..(1usize << max_dynamic_count)).find_map(|bitmask| {
            let hash = super::calc_fn_hash_full(
                hash_script,
                args.iter().enumerate().map(|(i, a)| {
                    if i < max_dynamic_count
                        && bitmask & (1usize << (max_dynamic_count - i - 1)) != 0
                    {
                        TypeId::of::<Dynamic>()
                    } else {
                        a.type_id()
                    }
                }),
            );
            module.get_fn(hash).cloned()
        })
    }
    /// Can a function loaded on demand for unqualified calls have [`Dynamic`] parameters?
    #[inline]
    #[must_use]
    pub(crate) fn may_contain_loaded_dynamic_fn(&self, hash_script: u64) -> bool {
        self.lazy_functions
            .as_deref()
            .and_then(|lazy| locked_read(&lazy.loaded))
            .map_or(false, |loaded| {
                loaded.global.may_contain_dynamic_fn(hash_script)
            })
    }
    /// Load a function that is not found from the on-demand sources.
    ///
    /// `namespace` is the namespace path for qualified calls.
    ///
    /// Returns `true` if a function was loaded. All function resolution caches are then cleared.
    pub(crate) fn load_missing_fn(
        &self,
        caches: &mut Caches,
        namespace: Option<&[&str]>,
        name: &str,
        args: &FnCallArgs,
        is_method_call: bool,
        native_only: bool,
    ) -> RhaiResultOf<bool> {
        let Some(lazy) = self.lazy_functions.as_deref() else {
            return Ok(false);
        };

        let arg_types = args.iter().map(|a| a.type_id()).collect::<StaticVec<_>>();
        let request = FnLoadRequest {
            path: &[],
            name,
            arg_types: &arg_types,
            is_method_call,
            global_only: false,
            native_only,
        };

        let module = match namespace {
            None => {
                let mut module = None;

                for source in &lazy.global {
                    module = source.load(self, &request)?;
                    if module.is_some() {
                        break;
                    }
                }

                // Global functions in namespaced sources are also available to unqualified calls
                #[cfg(not(feature = "no_module"))]
                if module.is_none() {
                    for source in lazy.namespaces.values() {
                        module = source.load(
                            self,
                            &FnLoadRequest {
                                global_only: true,
                                ..request
                            },
                        )?;
                        if module.is_some() {
                            break;
                        }
                    }
                }

                module
            }
            #[cfg(not(feature = "no_module"))]
            Some(path) => match path.split_first() {
                Some((root, path)) => match lazy.namespaces.get(*root) {
                    Some(source) => source.load(self, &FnLoadRequest { path, ..request })?,
                    None => None,
                },
                None => None,
            },
            #[cfg(feature = "no_module")]
            Some(..) => None,
        };

        let Some(module) = module else {
            return Ok(false);
        };

        let Some(mut loaded) = locked_write(&lazy.loaded) else {
            return Ok(false);
        };

        match namespace {
            None => {
                loaded.global.combine(module);
            }
            #[cfg(not(feature = "no_module"))]
            Some(path) => {
                let key = path.join(crate::engine::NAMESPACE_SEPARATOR);
                loaded
                    .namespaces
                    .entry(key.into())
                    .or_default()
                    .combine(module);
            }
            #[cfg(feature = "no_module")]
            Some(..) => unreachable!(),
        }

        drop(loaded);

        // Cached misses may hide the newly-loaded function
        caches.clear_fn_resolution_caches();

        Ok(true)
    }
    /// Is there a script-defined function that can be loaded on demand for unqualified calls?
    #[cfg(not(feature = "no_function"))]
    #[must_use]
    pub(crate) fn has_lazy_script_fn(
        &self,
        name: &str,
        num_params: usize,
        this_type: Option<&str>,
    ) -> bool {
        let Some(lazy) = self.lazy_functions.as_deref() else {
            return false;
        };

        let found = lazy
            .global
            .iter()
            .any(|s| s.has_script_fn(name, num_params, this_type, false));

        #[cfg(not(feature = "no_module"))]
        let found = found
            || lazy
                .namespaces
                .values()
                .any(|s| s.has_script_fn(name, num_params, this_type, true));

        found
    }
    /// Load a variable that is not found in a qualified variable access from the on-demand sources.
    #[cfg(not(feature = "no_ast"))]
    #[cfg(not(feature = "no_module"))]
    pub(crate) fn load_missing_var(
        &self,
        namespace: &[&str],
        name: &str,
    ) -> RhaiResultOf<Option<Dynamic>> {
        let Some(lazy) = self.lazy_functions.as_deref() else {
            return Ok(None);
        };
        let Some((root, path)) = namespace.split_first() else {
            return Ok(None);
        };
        match lazy.namespaces.get(*root) {
            Some(source) => source.load_var(self, path, name),
            None => Ok(None),
        }
    }
    /// Is there an on-demand source registered for a root namespace?
    #[cfg(not(feature = "no_ast"))]
    #[cfg(not(feature = "no_module"))]
    #[inline]
    #[must_use]
    pub(crate) fn has_lazy_namespace(&self, root: &str) -> bool {
        self.lazy_functions
            .as_deref()
            .map_or(false, |lazy| lazy.namespaces.contains_key(root))
    }
}
