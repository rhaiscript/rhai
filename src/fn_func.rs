//! Module which defines the function registration mechanism.

#![cfg(not(feature = "no_function"))]
#![allow(non_snake_case)]

use crate::any::Variant;
use crate::engine::Engine;
use crate::error::ParseError;
use crate::parser::AST;
use crate::result::EvalAltResult;
use crate::scope::Scope;

use crate::stdlib::{boxed::Box, string::ToString};

/// Trait to create a Rust closure from a script.
pub trait Func<ARGS, RET> {
    type Output;

    /// Create a Rust closure from an `AST`.
    /// The `Engine` and `AST` are consumed and basically embedded into the closure.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Func};                       // use 'Func' for 'create_from_ast'
    ///
    /// let engine = Engine::new();                     // create a new 'Engine' just for this
    ///
    /// let ast = engine.compile("fn calc(x, y) { x + len(y) < 42 }")?;
    ///
    /// // Func takes two type parameters:
    /// //   1) a tuple made up of the types of the script function's parameters
    /// //   2) the return type of the script function
    /// //
    /// // 'func' will have type Box<dyn Fn(i64, String) -> Result<bool, Box<EvalAltResult>>> and is callable!
    /// let func = Func::<(i64, String), bool>::create_from_ast(
    /// //                ^^^^^^^^^^^^^ function parameter types in tuple
    ///
    ///                 engine,                         // the 'Engine' is consumed into the closure
    ///                 ast,                            // the 'AST'
    ///                 "calc"                          // the entry-point function name
    /// );
    ///
    /// func(123, "hello".to_string())? == false;       // call the anonymous function
    /// # Ok(())
    /// # }
    fn create_from_ast(self, ast: AST, entry_point: &str) -> Self::Output;

    /// Create a Rust closure from a script.
    /// The `Engine` is consumed and basically embedded into the closure.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Func};                       // use 'Func' for 'create_from_script'
    ///
    /// let engine = Engine::new();                     // create a new 'Engine' just for this
    ///
    /// let script = "fn calc(x, y) { x + len(y) < 42 }";
    ///
    /// // Func takes two type parameters:
    /// //   1) a tuple made up of the types of the script function's parameters
    /// //   2) the return type of the script function
    /// //
    /// // 'func' will have type Box<dyn Fn(i64, String) -> Result<bool, Box<EvalAltResult>>> and is callable!
    /// let func = Func::<(i64, String), bool>::create_from_script(
    /// //                ^^^^^^^^^^^^^ function parameter types in tuple
    ///
    ///                 engine,                         // the 'Engine' is consumed into the closure
    ///                 script,                         // the script, notice number of parameters must match
    ///                 "calc"                          // the entry-point function name
    /// )?;
    ///
    /// func(123, "hello".to_string())? == false;       // call the anonymous function
    /// # Ok(())
    /// # }
    /// ```
    fn create_from_script(
        self,
        script: &str,
        entry_point: &str,
    ) -> Result<Self::Output, ParseError>;
}

macro_rules! def_anonymous_fn {
    () => {
        def_anonymous_fn!(imp);
    };
    (imp $($par:ident),*) => {
        impl<$($par: Variant + Clone,)* RET: Variant + Clone> Func<($($par,)*), RET> for Engine
        {
            #[cfg(feature = "sync")]
            type Output = Box<dyn Fn($($par),*) -> Result<RET, Box<EvalAltResult>> + Send + Sync>;
            #[cfg(not(feature = "sync"))]
            type Output = Box<dyn Fn($($par),*) -> Result<RET, Box<EvalAltResult>>>;

            fn create_from_ast(self, ast: AST, entry_point: &str) -> Self::Output {
                let fn_name = entry_point.to_string();

                Box::new(move |$($par: $par),*| {
                    self.call_fn(&mut Scope::new(), &ast, &fn_name, ($($par,)*))
                })
            }

            fn create_from_script(self, script: &str, entry_point: &str) -> Result<Self::Output, ParseError> {
                let ast = self.compile(script)?;
                Ok(Func::<($($par,)*), RET>::create_from_ast(self, ast, entry_point))
            }
        }
    };
    ($p0:ident $(, $p:ident)*) => {
        def_anonymous_fn!(imp $p0 $(, $p)*);
        def_anonymous_fn!($($p),*);
    };
}

def_anonymous_fn!(A, B, C, D, E, F, G, H, J, K, L, M, N, P, Q, R, S, T, U, V);
