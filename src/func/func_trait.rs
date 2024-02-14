//! Module which defines the function registration mechanism.

#![cfg(not(feature = "no_function"))]
#![allow(non_snake_case)]

use crate::parser::ParseResult;
use crate::types::dynamic::Variant;
use crate::{Engine, RhaiResultOf, Scope, SmartString, AST};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Trait to create a Rust closure from a script.
///
/// Not available under `no_function`.
pub trait Func<A, R> {
    /// The closure's output type.
    type Output;

    /// Create a Rust closure from an [`AST`].
    ///
    /// The [`Engine`] and [`AST`] are consumed and basically embedded into the closure.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Func};               // use 'Func' for 'create_from_ast'
    ///
    /// let engine = Engine::new();             // create a new 'Engine' just for this
    ///
    /// let ast = engine.compile("fn calc(x, y) { x + len(y) < 42 }")?;
    ///
    /// // Func takes two type parameters:
    /// //   1) a tuple made up of the types of the script function's parameters
    /// //   2) the return type of the script function
    ///
    /// // 'func' will have type Box<dyn Fn(i64, String) -> Result<bool, Box<EvalAltResult>>> and is callable!
    /// let func = Func::<(i64, &str), bool>::create_from_ast(
    /// //                ^^^^^^^^^^^ function parameter types in tuple
    ///
    ///                                             engine, // the 'Engine' is consumed into the closure
    ///                                             ast,    // the 'AST'
    ///                                             "calc"  // the entry-point function name
    ///                                         );
    ///
    /// func(123, "hello")? == false;           // call the anonymous function
    /// # Ok(())
    /// # }
    #[must_use]
    fn create_from_ast(self, ast: AST, entry_point: &str) -> Self::Output;

    /// Create a Rust closure from a script.
    ///
    /// The [`Engine`] is consumed and basically embedded into the closure.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Func};               // use 'Func' for 'create_from_script'
    ///
    /// let engine = Engine::new();             // create a new 'Engine' just for this
    ///
    /// let script = "fn calc(x, y) { x + len(y) < 42 }";
    ///
    /// // Func takes two type parameters:
    /// //   1) a tuple made up of the types of the script function's parameters
    /// //   2) the return type of the script function
    ///
    /// // 'func' will have type Box<dyn Fn(i64, String) -> Result<bool, Box<EvalAltResult>>> and is callable!
    /// let func = Func::<(i64, &str), bool>::create_from_script(
    /// //                ^^^^^^^^^^^ function parameter types in tuple
    ///
    ///                                             engine, // the 'Engine' is consumed into the closure
    ///                                             script, // the script, notice number of parameters must match
    ///                                             "calc"  // the entry-point function name
    ///                                         )?;
    ///
    /// func(123, "hello")? == false;           // call the anonymous function
    /// # Ok(())
    /// # }
    /// ```
    fn create_from_script(self, script: &str, entry_point: &str) -> ParseResult<Self::Output>;
}

macro_rules! def_anonymous_fn {
    () => {
        def_anonymous_fn!(imp);
    };
    (imp $($par:ident),*) => {
        impl<$($par: Variant + Clone,)* RET: Variant + Clone> Func<($($par,)*), RET> for Engine
        {
            #[cfg(feature = "sync")]
            type Output = Box<dyn Fn($($par,)*) -> RhaiResultOf<RET> + Send + Sync>;
            #[cfg(not(feature = "sync"))]
            type Output = Box<dyn Fn($($par,)*) -> RhaiResultOf<RET>>;

            #[inline]
            fn create_from_ast(self, ast: AST, entry_point: &str) -> Self::Output {
                let fn_name: SmartString = entry_point.into();
                Box::new(move |$($par,)*| self.call_fn(&mut Scope::new(), &ast, &fn_name, ($($par,)*)))
            }

            #[inline]
            fn create_from_script(self, script: &str, entry_point: &str) -> ParseResult<Self::Output> {
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
