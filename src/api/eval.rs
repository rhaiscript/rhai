//! Module that defines the public evaluation API of [`Engine`].

use crate::ast::{Expr, FnCallHashes};
use crate::eval::{Caches, GlobalRuntimeState};
use crate::parser::ParseState;
use crate::tokenizer::Token;
use crate::types::dynamic::Variant;
use crate::{
    calc_fn_hash, Dynamic, Engine, FnArgsVec, FuncArgs, Position, RhaiResult, RhaiResultOf, Scope,
    AST, ERR,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::{type_name, TypeId},
    mem,
};

impl Engine {
    /// Evaluate a string as a script, returning the result value or an error.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// assert_eq!(engine.eval::<i64>("40 + 2")?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn eval<T: Variant + Clone>(&self, script: &str) -> RhaiResultOf<T> {
        self.eval_with_scope(&mut Scope::new(), script)
    }
    /// Evaluate a string as a script with own scope, returning the result value or an error.
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
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Scope};
    ///
    /// let engine = Engine::new();
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push("x", 40_i64);
    ///
    /// assert_eq!(engine.eval_with_scope::<i64>(&mut scope, "x += 2; x")?, 42);
    /// assert_eq!(engine.eval_with_scope::<i64>(&mut scope, "x += 2; x")?, 44);
    ///
    /// // The variable in the scope is modified
    /// assert_eq!(scope.get_value::<i64>("x").expect("variable x should exist"), 44);
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn eval_with_scope<T: Variant + Clone>(
        &self,
        scope: &mut Scope,
        script: &str,
    ) -> RhaiResultOf<T> {
        let ast = self.compile_scripts_with_scope_raw(
            Some(scope),
            [script],
            #[cfg(not(feature = "no_optimize"))]
            self.optimization_level,
        )?;
        self.eval_ast_with_scope(scope, &ast)
    }
    /// Evaluate a string containing an expression, returning the result value or an error.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// assert_eq!(engine.eval_expression::<i64>("40 + 2")?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn eval_expression<T: Variant + Clone>(&self, script: &str) -> RhaiResultOf<T> {
        self.eval_expression_with_scope(&mut Scope::new(), script)
    }
    /// Evaluate a string containing an expression with own scope, returning the result value or an error.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Scope};
    ///
    /// let engine = Engine::new();
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push("x", 40_i64);
    ///
    /// assert_eq!(engine.eval_expression_with_scope::<i64>(&mut scope, "x + 2")?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn eval_expression_with_scope<T: Variant + Clone>(
        &self,
        scope: &mut Scope,
        script: &str,
    ) -> RhaiResultOf<T> {
        let scripts = [script];
        let ast = {
            let (stream, tc) = self.lex(&scripts);

            let input = &mut stream.peekable();
            let lib = &mut <_>::default();
            let state = ParseState::new(Some(scope), input, tc, lib);

            // No need to optimize a lone expression
            self.parse_global_expr(
                state,
                |_| {},
                #[cfg(not(feature = "no_optimize"))]
                crate::OptimizationLevel::None,
            )?
        };

        self.eval_ast_with_scope(scope, &ast)
    }
    /// Evaluate an [`AST`], returning the result value or an error.
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
    /// // Evaluate it
    /// assert_eq!(engine.eval_ast::<i64>(&ast)?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn eval_ast<T: Variant + Clone>(&self, ast: &AST) -> RhaiResultOf<T> {
        self.eval_ast_with_scope(&mut Scope::new(), ast)
    }
    /// Evaluate an [`AST`] with own scope, returning the result value or an error.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Scope};
    ///
    /// let engine = Engine::new();
    ///
    /// // Create initialized scope
    /// let mut scope = Scope::new();
    /// scope.push("x", 40_i64);
    ///
    /// // Compile a script to an AST and store it for later evaluation
    /// let ast = engine.compile("x += 2; x")?;
    ///
    /// // Evaluate it
    /// assert_eq!(engine.eval_ast_with_scope::<i64>(&mut scope, &ast)?, 42);
    /// assert_eq!(engine.eval_ast_with_scope::<i64>(&mut scope, &ast)?, 44);
    ///
    /// // The variable in the scope is modified
    /// assert_eq!(scope.get_value::<i64>("x").expect("variable x should exist"), 44);
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn eval_ast_with_scope<T: Variant + Clone>(
        &self,
        scope: &mut Scope,
        ast: &AST,
    ) -> RhaiResultOf<T> {
        let global = &mut self.new_global_runtime_state();
        let caches = &mut Caches::new();

        let result = self.eval_ast_with_scope_raw(global, caches, scope, ast)?;

        // Bail out early if the return type needs no cast
        if TypeId::of::<T>() == TypeId::of::<Dynamic>() {
            return Ok(reify! { result => T });
        }

        result.try_cast_result::<T>().map_err(|v| {
            let typename = match type_name::<T>() {
                typ if typ.contains("::") => self.map_type_name(typ),
                typ => typ,
            };

            ERR::ErrorMismatchOutputType(
                typename.into(),
                self.map_type_name(v.type_name()).into(),
                Position::NONE,
            )
            .into()
        })
    }
    /// Evaluate an [`AST`] with own scope, returning the result value or an error.
    #[inline]
    pub(crate) fn eval_ast_with_scope_raw(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        ast: &AST,
    ) -> RhaiResult {
        let orig_source = mem::replace(&mut global.source, ast.source_raw().cloned());

        #[cfg(not(feature = "no_function"))]
        let orig_lib_len = global.lib.len();

        #[cfg(not(feature = "no_function"))]
        global.lib.push(ast.shared_lib().clone());

        #[cfg(not(feature = "no_module"))]
        let orig_embedded_module_resolver =
            mem::replace(&mut global.embedded_module_resolver, ast.resolver.clone());

        defer! { global => move |g| {
            #[cfg(not(feature = "no_module"))]
            {
                g.embedded_module_resolver = orig_embedded_module_resolver;
            }

            #[cfg(not(feature = "no_function"))]
            g.lib.truncate(orig_lib_len);

            g.source = orig_source;
        }}

        let r = self.eval_global_statements(global, caches, scope, ast.statements(), true)?;

        #[cfg(feature = "debugging")]
        if self.is_debugger_registered() {
            global.debugger_mut().status = crate::eval::DebuggerStatus::Terminate;
            let node = &crate::ast::Stmt::Noop(Position::NONE);
            self.dbg(global, caches, scope, None, node)?;
        }

        Ok(r)
    }

    /// Evaluate a binary operator with two operands with the [`Engine`].
    ///
    /// This method is very useful for simply comparing two [`Dynamic`] values --
    /// simply use the `==` operator to compare them.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// // Compare two values
    /// let result = engine.eval_binary_op::<bool>("==", "abc", 123_i64)?;
    /// assert!(!result);
    /// # #[cfg(not(feature = "no_float"))]
    /// # {
    ///
    /// // Rhai by default equates floating-point integers with normal integers.
    /// let result = engine.eval_binary_op::<bool>("==", 123.0, 123_i64)?;
    /// assert!(result);
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn eval_binary_op<T: Variant + Clone>(
        &self,
        op: impl AsRef<str>,
        lhs: impl Into<Dynamic>,
        rhs: impl Into<Dynamic>,
    ) -> RhaiResultOf<T> {
        let lhs = lhs.into();
        let lhs = Expr::DynamicConstant(lhs.into(), Position::NONE);
        let rhs = rhs.into();
        let rhs = Expr::DynamicConstant(rhs.into(), Position::NONE);

        let op = op.as_ref();

        self.make_function_call(
            &mut self.new_global_runtime_state(),
            &mut Caches::new(),
            &mut Scope::new(),
            None,
            op,
            Token::lookup_symbol_from_syntax(op).as_ref(),
            Some(&lhs),
            &[rhs],
            FnCallHashes::from_native_only(calc_fn_hash(None, op, 2)),
            false,
            Position::NONE,
        )?
        .try_cast_result::<T>()
        .map_err(|v| {
            let typename = match type_name::<T>() {
                typ if typ.contains("::") => self.map_type_name(typ),
                typ => typ,
            };

            ERR::ErrorMismatchOutputType(
                typename.into(),
                self.map_type_name(v.type_name()).into(),
                Position::NONE,
            )
            .into()
        })
    }
    /// Evaluate an arbitrary function call in the [`Engine`].
    #[inline(always)]
    pub fn eval_fn_call<T: Variant + Clone>(
        &self,
        fn_name: impl AsRef<str>,
        this_ptr: Option<&mut Dynamic>,
        args: impl FuncArgs,
    ) -> RhaiResultOf<T> {
        let mut has_this = false;
        let arg_values = &mut FnArgsVec::new_const();
        args.parse(arg_values);
        let args = &mut arg_values.iter_mut().collect::<FnArgsVec<_>>();

        if let Some(this_ptr) = this_ptr {
            args.insert(0, this_ptr);
            has_this = true;
        }

        self.eval_fn_call_with_arguments::<T>(fn_name, args, has_this, has_this)?
            .try_cast_result::<T>()
            .map_err(|v| {
                let typename = match type_name::<T>() {
                    typ if typ.contains("::") => self.map_type_name(typ),
                    typ => typ,
                };

                ERR::ErrorMismatchOutputType(
                    typename.into(),
                    self.map_type_name(v.type_name()).into(),
                    Position::NONE,
                )
                .into()
            })
    }
    /// Evaluate a function call with the [`Engine`].
    pub(crate) fn eval_fn_call_with_arguments<T: Variant + Clone>(
        &self,
        fn_name: impl AsRef<str>,
        args: &mut [&mut Dynamic],
        is_ref_mut: bool,
        is_method_call: bool,
    ) -> RhaiResult {
        let name = fn_name.as_ref();
        let op_token = Token::lookup_symbol_from_syntax(name);

        let hashes = if is_method_call {
            #[cfg(feature = "no_function")]
            {
                panic!("method calls are not supported under `no_function`")
            }
            #[cfg(not(feature = "no_function"))]
            {
                FnCallHashes::from_script_and_native(
                    calc_fn_hash(None, name, args.len() - 1),
                    calc_fn_hash(None, name, args.len()),
                )
            }
        } else {
            FnCallHashes::from_hash(calc_fn_hash(None, name, args.len()))
        };

        self.exec_fn_call(
            &mut self.new_global_runtime_state(),
            &mut Caches::new(),
            None,
            name,
            op_token.as_ref(),
            hashes,
            args,
            is_ref_mut,
            is_method_call,
            Position::NONE,
        )
        .map(|(v, ..)| v)
    }
}

/// Evaluate a string as a script, returning the result value or an error.
///
/// # Example
///
/// ```
/// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
/// let result: i64 = rhai::eval("40 + 2")?;
///
/// assert_eq!(result, 42);
/// # Ok(())
/// # }
/// ```
#[inline(always)]
pub fn eval<T: Variant + Clone>(script: &str) -> RhaiResultOf<T> {
    Engine::new().eval(script)
}
