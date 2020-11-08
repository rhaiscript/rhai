use crate::engine::Engine;
use crate::fn_native::{Locked, Shared};
use crate::module::{Module, ModuleResolver};
use crate::result::EvalAltResult;
use crate::token::Position;

use crate::stdlib::{boxed::Box, collections::HashMap, path::PathBuf, string::String};

/// Module resolution service that loads module script files from the file system.
///
/// Script files are cached so they are are not reloaded and recompiled in subsequent requests.
///
/// The `new_with_path` and `new_with_path_and_extension` constructor functions
/// allow specification of a base directory with module path used as a relative path offset
/// to the base directory. The script file is then forced to be in a specified extension
/// (default `.rhai`).
///
/// # Function Namespace
///
/// When a function within a script file module is loaded, all functions in the _global_ namespace
/// plus all those defined within the same module are _merged_ into a _unified_ namespace before
/// the call.  Therefore, functions in a module script can cross-call each other.
///
/// # Example
///
/// ```
/// use rhai::Engine;
/// use rhai::module_resolvers::FileModuleResolver;
///
/// // Create a new 'FileModuleResolver' loading scripts from the 'scripts' subdirectory
/// // with file extension '.x'.
/// let resolver = FileModuleResolver::new_with_path_and_extension("./scripts", "x");
///
/// let mut engine = Engine::new();
///
/// engine.set_module_resolver(Some(resolver));
/// ```
#[derive(Debug)]
pub struct FileModuleResolver {
    path: PathBuf,
    extension: String,
    cache: Locked<HashMap<PathBuf, Shared<Module>>>,
}

impl Default for FileModuleResolver {
    #[inline(always)]
    fn default() -> Self {
        Self::new_with_path(PathBuf::default())
    }
}

impl FileModuleResolver {
    /// Create a new `FileModuleResolver` with a specific base path.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Engine;
    /// use rhai::module_resolvers::FileModuleResolver;
    ///
    /// // Create a new 'FileModuleResolver' loading scripts from the 'scripts' subdirectory
    /// // with file extension '.rhai' (the default).
    /// let resolver = FileModuleResolver::new_with_path("./scripts");
    ///
    /// let mut engine = Engine::new();
    /// engine.set_module_resolver(Some(resolver));
    /// ```
    #[inline(always)]
    pub fn new_with_path<P: Into<PathBuf>>(path: P) -> Self {
        Self::new_with_path_and_extension(path, "rhai")
    }

    /// Create a new `FileModuleResolver` with a specific base path and file extension.
    ///
    /// The default extension is `.rhai`.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Engine;
    /// use rhai::module_resolvers::FileModuleResolver;
    ///
    /// // Create a new 'FileModuleResolver' loading scripts from the 'scripts' subdirectory
    /// // with file extension '.x'.
    /// let resolver = FileModuleResolver::new_with_path_and_extension("./scripts", "x");
    ///
    /// let mut engine = Engine::new();
    /// engine.set_module_resolver(Some(resolver));
    /// ```
    #[inline(always)]
    pub fn new_with_path_and_extension<P: Into<PathBuf>, E: Into<String>>(
        path: P,
        extension: E,
    ) -> Self {
        Self {
            path: path.into(),
            extension: extension.into(),
            cache: Default::default(),
        }
    }

    /// Create a new `FileModuleResolver` with the current directory as base path.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Engine;
    /// use rhai::module_resolvers::FileModuleResolver;
    ///
    /// // Create a new 'FileModuleResolver' loading scripts from the current directory
    /// // with file extension '.rhai' (the default).
    /// let resolver = FileModuleResolver::new();
    ///
    /// let mut engine = Engine::new();
    /// engine.set_module_resolver(Some(resolver));
    /// ```
    #[inline(always)]
    pub fn new() -> Self {
        Default::default()
    }
}

impl ModuleResolver for FileModuleResolver {
    fn resolve(
        &self,
        engine: &Engine,
        path: &str,
        pos: Position,
    ) -> Result<Shared<Module>, Box<EvalAltResult>> {
        // Construct the script file path
        let mut file_path = self.path.clone();
        file_path.push(path);
        file_path.set_extension(&self.extension); // Force extension

        let scope = Default::default();

        // See if it is cached
        let mut module = None;

        let module_ref = {
            #[cfg(not(feature = "sync"))]
            let c = self.cache.borrow();
            #[cfg(feature = "sync")]
            let c = self.cache.read().unwrap();

            if let Some(module) = c.get(&file_path) {
                module.clone()
            } else {
                // Load the file and compile it if not found
                let ast = engine.compile_file(file_path.clone()).map_err(|err| {
                    Box::new(EvalAltResult::ErrorInModule(path.to_string(), err, pos))
                })?;

                let mut m = Module::eval_ast_as_new(scope, &ast, engine).map_err(|err| {
                    Box::new(EvalAltResult::ErrorInModule(path.to_string(), err, pos))
                })?;

                m.build_index();

                let m: Shared<Module> = m.into();
                module = Some(m.clone());
                m
            }
        };

        if let Some(module) = module {
            // Put it into the cache
            #[cfg(not(feature = "sync"))]
            self.cache.borrow_mut().insert(file_path, module);
            #[cfg(feature = "sync")]
            self.cache.write().unwrap().insert(file_path, module);
        }

        Ok(module_ref)
    }
}
