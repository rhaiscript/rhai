//! Module that defines the public compilation API of [`Engine`].

use crate::parser::{ParseResult, ParseState};
use crate::{Engine, OptimizationLevel, Scope, AST};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

impl Engine {
    /// Compile a string into an [`AST`], which can be used later for evaluation.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// // Compile a script to an AST and store it for later evaluation
    /// let ast = engine.compile("40 + 2")?;
    ///
    /// for _ in 0..42 {
    ///     assert_eq!(engine.eval_ast::<i64>(&ast)?, 42);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn compile(&self, script: impl AsRef<str>) -> ParseResult<AST> {
        self.compile_with_scope(&Scope::new(), script)
    }
    /// Compile a string into an [`AST`] using own scope, which can be used later for evaluation.
    ///
    /// ## Constants Propagation
    ///
    /// If not [`OptimizationLevel::None`][crate::OptimizationLevel::None], constants defined within
    /// the scope are propagated throughout the script _including_ functions. This allows functions
    /// to be optimized based on dynamic global constants.
    ///
    /// # Example
    ///
    /// ```
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
    /// // Notice that `Full` optimization is on, so constants are folded
    /// // into function calls and operators.
    /// let ast = engine.compile_with_scope(&mut scope,
    ///             "if x > 40 { x } else { 0 }"    // all 'x' are replaced with 42
    /// )?;
    ///
    /// // Normally this would have failed because no scope is passed into the 'eval_ast'
    /// // call and so the variable 'x' does not exist.  Here, it passes because the script
    /// // has been optimized and all references to 'x' are already gone.
    /// assert_eq!(engine.eval_ast::<i64>(&ast)?, 42);
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn compile_with_scope(&self, scope: &Scope, script: impl AsRef<str>) -> ParseResult<AST> {
        self.compile_scripts_with_scope(scope, &[script])
    }
    /// Compile a string into an [`AST`] using own scope, which can be used later for evaluation,
    /// embedding all imported modules.
    ///
    /// Not available under `no_module`.
    ///
    /// Modules referred by `import` statements containing literal string paths are eagerly resolved
    /// via the current [module resolver][crate::ModuleResolver] and embedded into the resultant
    /// [`AST`]. When it is evaluated later, `import` statement directly recall pre-resolved
    /// [modules][crate::Module] and the resolution process is not performed again.
    #[cfg(not(feature = "no_module"))]
    pub fn compile_into_self_contained(
        &self,
        scope: &Scope,
        script: impl AsRef<str>,
    ) -> crate::RhaiResultOf<AST> {
        use crate::{
            ast::{ASTNode, Expr, Stmt},
            func::native::shared_take_or_clone,
            module::resolvers::StaticModuleResolver,
        };
        use std::collections::BTreeSet;

        fn collect_imports(
            ast: &AST,
            resolver: &StaticModuleResolver,
            imports: &mut BTreeSet<crate::Identifier>,
        ) {
            ast.walk(&mut |path| match path.last().unwrap() {
                // Collect all `import` statements with a string constant path
                ASTNode::Stmt(Stmt::Import(x, ..)) => match x.0 {
                    Expr::StringConstant(ref s, ..)
                        if !resolver.contains_path(s)
                            && (imports.is_empty() || !imports.contains(s.as_str())) =>
                    {
                        imports.insert(s.clone().into());
                        true
                    }
                    _ => true,
                },
                _ => true,
            });
        }

        let mut ast = self.compile_with_scope(scope, script)?;

        let mut resolver = StaticModuleResolver::new();
        let mut imports = BTreeSet::new();

        collect_imports(&ast, &resolver, &mut imports);

        if !imports.is_empty() {
            while let Some(path) = imports.iter().next() {
                let path = path.clone();

                match self
                    .module_resolver
                    .resolve_ast(self, None, &path, crate::Position::NONE)
                {
                    Some(Ok(module_ast)) => collect_imports(&module_ast, &resolver, &mut imports),
                    Some(err) => return err,
                    None => (),
                }

                let module =
                    self.module_resolver
                        .resolve(self, None, &path, crate::Position::NONE)?;

                let module = shared_take_or_clone(module);

                imports.remove(&path);
                resolver.insert(path, module);
            }
            ast.set_resolver(resolver);
        }

        Ok(ast)
    }
    /// When passed a list of strings, first join the strings into one large script, and then
    /// compile them into an [`AST`] using own scope, which can be used later for evaluation.
    ///
    /// The scope is useful for passing constants into the script for optimization when using
    /// [`OptimizationLevel::Full`][crate::OptimizationLevel::Full].
    ///
    /// ## Note
    ///
    /// All strings are simply parsed one after another with nothing inserted in between, not even a
    /// newline or space.
    ///
    /// ## Constants Propagation
    ///
    /// If not [`OptimizationLevel::None`][crate::OptimizationLevel::None], constants defined within
    /// the scope are propagated throughout the script _including_ functions. This allows functions
    /// to be optimized based on dynamic global constants.
    ///
    /// # Example
    ///
    /// ```
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
    /// // Compile a script made up of script segments to an AST and store it for later evaluation.
    /// // Notice that `Full` optimization is on, so constants are folded
    /// // into function calls and operators.
    /// let ast = engine.compile_scripts_with_scope(&mut scope, &[
    ///             "if x > 40",            // all 'x' are replaced with 42
    ///             "{ x } el",
    ///             "se { 0 }"              // segments do not need to be valid scripts!
    /// ])?;
    ///
    /// // Normally this would have failed because no scope is passed into the 'eval_ast'
    /// // call and so the variable 'x' does not exist.  Here, it passes because the script
    /// // has been optimized and all references to 'x' are already gone.
    /// assert_eq!(engine.eval_ast::<i64>(&ast)?, 42);
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn compile_scripts_with_scope<S: AsRef<str>>(
        &self,
        scope: &Scope,
        scripts: impl AsRef<[S]>,
    ) -> ParseResult<AST> {
        self.compile_with_scope_and_optimization_level(scope, scripts, self.optimization_level)
    }
    /// Join a list of strings and compile into an [`AST`] using own scope at a specific optimization level.
    ///
    /// ## Constants Propagation
    ///
    /// If not [`OptimizationLevel::None`], constants defined within the scope are propagated
    /// throughout the script _including_ functions. This allows functions to be optimized based on
    /// dynamic global constants.
    #[inline]
    pub(crate) fn compile_with_scope_and_optimization_level<S: AsRef<str>>(
        &self,
        scope: &Scope,
        scripts: impl AsRef<[S]>,
        optimization_level: OptimizationLevel,
    ) -> ParseResult<AST> {
        let (stream, tokenizer_control) = self.lex_raw(
            scripts.as_ref(),
            self.token_mapper.as_ref().map(<_>::as_ref),
        );
        let mut state = ParseState::new(self, scope, Default::default(), tokenizer_control);
        let mut _ast = self.parse(&mut stream.peekable(), &mut state, optimization_level)?;
        #[cfg(feature = "metadata")]
        _ast.set_doc(state.tokenizer_control.borrow().global_comments.join("\n"));
        Ok(_ast)
    }
    /// Compile a string containing an expression into an [`AST`],
    /// which can be used later for evaluation.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// // Compile a script to an AST and store it for later evaluation
    /// let ast = engine.compile_expression("40 + 2")?;
    ///
    /// for _ in 0..42 {
    ///     assert_eq!(engine.eval_ast::<i64>(&ast)?, 42);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn compile_expression(&self, script: impl AsRef<str>) -> ParseResult<AST> {
        self.compile_expression_with_scope(&Scope::new(), script)
    }
    /// Compile a string containing an expression into an [`AST`] using own scope,
    /// which can be used later for evaluation.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # #[cfg(not(feature = "no_optimize"))]
    /// # {
    /// use rhai::{Engine, Scope, OptimizationLevel};
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push_constant("x", 10_i64);   // 'x' is a constant
    ///
    /// // Compile a script to an AST and store it for later evaluation.
    /// // Notice that `Full` optimization is on, so constants are folded
    /// // into function calls and operators.
    /// let ast = engine.compile_expression_with_scope(&mut scope,
    ///             "2 + (x + x) * 2"    // all 'x' are replaced with 10
    /// )?;
    ///
    /// // Normally this would have failed because no scope is passed into the 'eval_ast'
    /// // call and so the variable 'x' does not exist.  Here, it passes because the script
    /// // has been optimized and all references to 'x' are already gone.
    /// assert_eq!(engine.eval_ast::<i64>(&ast)?, 42);
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn compile_expression_with_scope(
        &self,
        scope: &Scope,
        script: impl AsRef<str>,
    ) -> ParseResult<AST> {
        let scripts = [script];
        let (stream, tokenizer_control) =
            self.lex_raw(&scripts, self.token_mapper.as_ref().map(<_>::as_ref));

        let mut peekable = stream.peekable();
        let mut state = ParseState::new(self, scope, Default::default(), tokenizer_control);
        self.parse_global_expr(&mut peekable, &mut state, self.optimization_level)
    }
}
