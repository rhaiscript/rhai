//! Module that defines the public file-based API of [`Engine`].
#![cfg(not(feature = "no_std"))]
#![cfg(any(not(target_family = "wasm"), not(target_os = "unknown")))]

use crate::types::dynamic::Variant;
use crate::{Engine, RhaiResultOf, Scope, AST, ERR};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

impl Engine {
    /// Read the contents of a file into a string.
    fn read_file(path: impl AsRef<Path>) -> RhaiResultOf<String> {
        let path = path.as_ref();

        let mut f = File::open(path).map_err(|err| {
            ERR::ErrorSystem(
                format!("Cannot open script file '{}'", path.to_string_lossy()),
                err.into(),
            )
        })?;

        let mut contents = String::new();

        f.read_to_string(&mut contents).map_err(|err| {
            ERR::ErrorSystem(
                format!("Cannot read script file '{}'", path.to_string_lossy()),
                err.into(),
            )
        })?;

        if contents.starts_with("#!") {
            // Remove shebang
            match contents.find('\n') {
                Some(n) => {
                    contents.drain(0..n).count();
                }
                None => contents.clear(),
            }
        }

        Ok(contents)
    }
    /// Compile a script file into an [`AST`], which can be used later for evaluation.
    ///
    /// Not available under `no_std` or `WASM`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// // Compile a script file to an AST and store it for later evaluation.
    /// // Notice that a PathBuf is required which can easily be constructed from a string.
    /// let ast = engine.compile_file("script.rhai".into())?;
    ///
    /// for _ in 0..42 {
    ///     engine.eval_ast::<i64>(&ast)?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn compile_file(&self, path: PathBuf) -> RhaiResultOf<AST> {
        self.compile_file_with_scope(&Scope::new(), path)
    }
    /// Compile a script file into an [`AST`] using own scope, which can be used later for evaluation.
    ///
    /// Not available under `no_std` or `WASM`.
    ///
    /// ## Constants Propagation
    ///
    /// If not [`OptimizationLevel::None`][crate::OptimizationLevel::None], constants defined within
    /// the scope are propagated throughout the script _including_ functions.
    ///
    /// This allows functions to be optimized based on dynamic global constants.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # #[cfg(not(feature = "no_optimize"))]
    /// # {
    /// use rhai::{Engine, Scope, OptimizationLevel};
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push_constant("x", 42_i64);   // 'x' is a constant
    ///
    /// // Compile a script to an AST and store it for later evaluation.
    /// // Notice that a PathBuf is required which can easily be constructed from a string.
    /// let ast = engine.compile_file_with_scope(&scope, "script.rhai".into())?;
    ///
    /// let result = engine.eval_ast::<i64>(&ast)?;
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn compile_file_with_scope(&self, scope: &Scope, path: PathBuf) -> RhaiResultOf<AST> {
        Self::read_file(&path).and_then(|contents| {
            let mut ast = self.compile_with_scope(scope, contents)?;
            ast.set_source(path.to_string_lossy().as_ref());
            Ok(ast)
        })
    }
    /// Evaluate a script file, returning the result value or an error.
    ///
    /// Not available under `no_std` or `WASM`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// // Notice that a PathBuf is required which can easily be constructed from a string.
    /// let result = engine.eval_file::<i64>("script.rhai".into())?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn eval_file<T: Variant + Clone>(&self, path: PathBuf) -> RhaiResultOf<T> {
        Self::read_file(path).and_then(|contents| self.eval::<T>(&contents))
    }
    /// Evaluate a script file with own scope, returning the result value or an error.
    ///
    /// Not available under `no_std` or `WASM`.
    ///
    /// ## Constants Propagation
    ///
    /// If not [`OptimizationLevel::None`][crate::OptimizationLevel::None], constants defined within
    /// the scope are propagated throughout the script _including_ functions.
    ///
    /// This allows functions to be optimized based on dynamic global constants.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Scope};
    ///
    /// let engine = Engine::new();
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push("x", 42_i64);
    ///
    /// // Notice that a PathBuf is required which can easily be constructed from a string.
    /// let result = engine.eval_file_with_scope::<i64>(&mut scope, "script.rhai".into())?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn eval_file_with_scope<T: Variant + Clone>(
        &self,
        scope: &mut Scope,
        path: PathBuf,
    ) -> RhaiResultOf<T> {
        Self::read_file(path).and_then(|contents| self.eval_with_scope(scope, &contents))
    }
    /// Evaluate a file.
    ///
    /// Not available under `no_std` or `WASM`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// // Notice that a PathBuf is required which can easily be constructed from a string.
    /// engine.run_file("script.rhai".into())?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn run_file(&self, path: PathBuf) -> RhaiResultOf<()> {
        Self::read_file(path).and_then(|contents| self.run(&contents))
    }
    /// Evaluate a file with own scope.
    ///
    /// Not available under `no_std` or `WASM`.
    ///
    /// ## Constants Propagation
    ///
    /// If not [`OptimizationLevel::None`][crate::OptimizationLevel::None], constants defined within
    /// the scope are propagated throughout the script _including_ functions.
    ///
    /// This allows functions to be optimized based on dynamic global constants.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Scope};
    ///
    /// let engine = Engine::new();
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push("x", 42_i64);
    ///
    /// // Notice that a PathBuf is required which can easily be constructed from a string.
    /// engine.run_file_with_scope(&mut scope, "script.rhai".into())?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn run_file_with_scope(&self, scope: &mut Scope, path: PathBuf) -> RhaiResultOf<()> {
        Self::read_file(path).and_then(|contents| self.run_with_scope(scope, &contents))
    }
}

/// Evaluate a script file, returning the result value or an error.
///
/// Not available under `no_std` or `WASM`.
///
/// # Example
///
/// ```no_run
/// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
/// let result = rhai::eval_file::<i64>("script.rhai")?;
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn eval_file<T: Variant + Clone>(path: impl AsRef<Path>) -> RhaiResultOf<T> {
    Engine::read_file(path).and_then(|contents| Engine::new().eval::<T>(&contents))
}

/// Evaluate a file.
///
/// Not available under `no_std` or `WASM`.
///
/// # Example
///
/// ```no_run
/// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
/// rhai::run_file("script.rhai")?;
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn run_file(path: impl AsRef<Path>) -> RhaiResultOf<()> {
    Engine::read_file(path).and_then(|contents| Engine::new().run(&contents))
}
