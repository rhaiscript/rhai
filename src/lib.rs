//! # Rhai - embedded scripting for Rust
//!
//! Rhai is a tiny, simple and very fast embedded scripting language for Rust
//! that gives you a safe and easy way to add scripting to your applications.
//! It provides a familiar syntax based on JavaScript and Rust and a simple Rust interface.
//! Here is a quick example.
//!
//! First, the contents of `my_script.rhai`:
//!
//! ```,ignore
//! // Brute force factorial function
//! fn factorial(x) {
//!     if x == 1 { return 1; }
//!     x * factorial(x - 1)
//! }
//!
//! // Calling an external function 'compute'
//! compute(factorial(10))
//! ```
//!
//! And the Rust part:
//!
//! ```,no_run
//! use rhai::{Engine, EvalAltResult, RegisterFn};
//!
//! fn main() -> Result<(), Box<EvalAltResult>>
//! {
//!     // Define external function
//!     fn compute_something(x: i64) -> bool {
//!         (x % 40) == 0
//!     }
//!
//!     // Create scripting engine
//!     let mut engine = Engine::new();
//!
//!     // Register external function as 'compute'
//!     engine.register_fn("compute", compute_something);
//!
//! #   #[cfg(not(feature = "no_std"))]
//! #   #[cfg(not(target_arch = "wasm32"))]
//!     assert_eq!(
//!         // Evaluate the script, expects a 'bool' return
//!         engine.eval_file::<bool>("my_script.rhai".into())?,
//!         true
//!     );
//!
//!     Ok(())
//! }
//! ```
//!
//! # Documentation
//!
//! See [The Rhai Book](https://schungx.github.io/rhai) for details on the Rhai script engine and language.

#![cfg_attr(feature = "no_std", no_std)]

#[cfg(feature = "no_std")]
extern crate alloc;

mod any;
mod api;
mod engine;
mod error;
mod fn_args;
mod fn_call;
mod fn_func;
mod fn_native;
mod fn_register;
mod module;
mod optimize;
pub mod packages;
mod parser;
pub mod plugin;
mod result;
mod scope;
#[cfg(feature = "serde")]
mod serde;
mod settings;
mod stdlib;
mod syntax;
mod token;
mod r#unsafe;
mod utils;

pub use any::Dynamic;
pub use engine::Engine;
pub use error::{ParseError, ParseErrorType};
pub use fn_native::{FnPtr, IteratorFn};
pub use fn_register::{RegisterFn, RegisterPlugin, RegisterResultFn};
pub use module::Module;
pub use parser::{ImmutableString, AST, INT};
pub use result::EvalAltResult;
pub use scope::Scope;
pub use syntax::{EvalContext, Expression};
pub use token::Position;
pub use utils::calc_fn_spec as calc_fn_hash;

pub use rhai_codegen::*;

#[cfg(not(feature = "no_function"))]
pub use parser::FnAccess;
#[cfg(feature = "no_function")]
pub use parser::FnAccess;

#[cfg(not(feature = "no_function"))]
pub use fn_func::Func;

#[cfg(not(feature = "no_index"))]
pub use engine::Array;

#[cfg(not(feature = "no_object"))]
pub use engine::Map;

#[cfg(not(feature = "no_float"))]
pub use parser::FLOAT;

#[cfg(not(feature = "no_module"))]
pub use module::ModuleResolver;

/// Module containing all built-in _module resolvers_ available to Rhai.
///
/// Not available under the `no_module` feature.
#[cfg(not(feature = "no_module"))]
pub mod module_resolvers {
    pub use crate::module::resolvers::*;
}

/// Serialization support for [`serde`](https://crates.io/crates/serde).
///
/// Requires the `serde` feature.
#[cfg(feature = "serde")]
pub mod ser {
    pub use crate::serde::ser::to_dynamic;
}
/// Deserialization support for [`serde`](https://crates.io/crates/serde).
///
/// Requires the `serde` feature.
#[cfg(feature = "serde")]
pub mod de {
    pub use crate::serde::de::from_dynamic;
}

#[cfg(not(feature = "no_optimize"))]
pub use optimize::OptimizationLevel;

// Expose internal data structures.

#[cfg(feature = "internals")]
#[deprecated(note = "this type is volatile and may change")]
pub use error::LexError;

#[cfg(feature = "internals")]
#[deprecated(note = "this type is volatile and may change")]
pub use token::{get_next_token, parse_string_literal, InputStream, Token, TokenizeState};

#[cfg(feature = "internals")]
#[deprecated(note = "this type is volatile and may change")]
pub use parser::{CustomExpr, Expr, FloatWrapper, ReturnType, ScriptFnDef, Stmt};

#[cfg(feature = "internals")]
#[deprecated(note = "this type is volatile and may change")]
pub use engine::{Imports, Limits, State as EvalState};

#[cfg(feature = "internals")]
#[deprecated(note = "this type is volatile and may change")]
pub use module::ModuleRef;

#[cfg(feature = "internals")]
#[deprecated(note = "this type is volatile and may change")]
pub use utils::StaticVec;
