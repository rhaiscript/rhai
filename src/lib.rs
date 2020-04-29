//! # Rhai - embedded scripting for Rust
//!
//! Rhai is a tiny, simple and very fast embedded scripting language for Rust
//! that gives you a safe and easy way to add scripting to your applications.
//! It provides a familiar syntax based on JS and Rust and a simple Rust interface.
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
//! ## Optional features
//!
//! | Feature       | Description                                                                                                                                              |
//! | ------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
//! | `unchecked`   | Exclude arithmetic checking (such as overflows and division by zero). Beware that a bad script may panic the entire system!                              |
//! | `no_function` | Disable script-defined functions if not needed.                                                                                                          |
//! | `no_index`    | Disable arrays and indexing features if not needed.                                                                                                      |
//! | `no_object`   | Disable support for custom types and objects.                                                                                                            |
//! | `no_float`    | Disable floating-point numbers and math if not needed.                                                                                                   |
//! | `no_optimize` | Disable the script optimizer.                                                                                                                            |
//! | `only_i32`    | Set the system integer type to `i32` and disable all other integer types. `INT` is set to `i32`.                                                         |
//! | `only_i64`    | Set the system integer type to `i64` and disable all other integer types. `INT` is set to `i64`.                                                         |
//! | `no_std`      | Build for `no-std`. Notice that additional dependencies will be pulled in to replace `std` features.                                                     |
//! | `sync`        | Restrict all values types to those that are `Send + Sync`. Under this feature, `Engine`, `Scope` and `AST` are all `Send + Sync`.                        |
//!
//! [Check out the README on GitHub for details on the Rhai language!](https://github.com/jonathandturner/rhai)

#![cfg_attr(feature = "no_std", no_std)]

#[cfg(feature = "no_std")]
extern crate alloc;

mod any;
mod api;
mod engine;
mod error;
mod fn_call;
mod fn_func;
mod fn_register;
mod optimize;
pub mod packages;
mod parser;
mod result;
mod scope;
mod stdlib;
mod token;
pub mod bytecode;

pub use any::Dynamic;
pub use engine::{calc_fn_spec as calc_fn_hash, Engine};
pub use error::{ParseError, ParseErrorType};
pub use fn_call::FuncArgs;
pub use fn_register::{RegisterDynamicFn, RegisterFn, RegisterResultFn};
pub use parser::{AST, INT};
pub use result::EvalAltResult;
pub use scope::Scope;
pub use token::Position;

#[cfg(not(feature = "no_function"))]
pub use fn_func::Func;

#[cfg(not(feature = "no_index"))]
pub use engine::Array;

#[cfg(not(feature = "no_object"))]
pub use engine::Map;

#[cfg(not(feature = "no_float"))]
pub use parser::FLOAT;

#[cfg(not(feature = "no_optimize"))]
pub use optimize::OptimizationLevel;
