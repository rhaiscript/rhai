//! # Rhai - embedded scripting for Rust
//!
//! Rhai is a tiny, simple and very fast embedded scripting language for Rust
//! that gives you a safe and easy way to add scripting to your applications.
//! It provides a familiar syntax based on JS and Rust and a simple Rust interface.
//! Here is a quick example. First, the contents of `my_script.rhai`:
//!
//! ```rust,ignore
//! fn factorial(x) {
//!     if x == 1 { return 1; }
//!	    x * factorial(x - 1)
//! }
//!
//! compute_something(factorial(10))
//! ```
//!
//! And the Rust part:
//!
//! ```rust,no_run
//! use rhai::{Engine, RegisterFn};
//!
//! fn compute_something(x: i64) -> bool {
//!	    (x % 40) == 0
//! }
//!
//! let mut engine = Engine::new();
//! engine.register_fn("compute_something", compute_something);
//! assert_eq!(engine.eval_file::<bool>("my_script.rhai"), Ok(true));
//! ```
//!
//! [Check out the README on GitHub for more information!](https://github.com/jonathandturner/rhai)

// lints required by Rhai
#![allow(warnings, unknown_lints, type_complexity, new_without_default_derive,
         needless_pass_by_value, too_many_arguments)]

// needs to be here, because order matters for macros
macro_rules! debug_println {
    () => (#[cfg(feature = "debug_msgs")] {print!("\n")});
    ($fmt:expr) => (#[cfg(feature = "debug_msgs")] {print!(concat!($fmt, "\n"))});
    ($fmt:expr, $($arg:tt)*) => (#[cfg(feature = "debug_msgs")] {print!(concat!($fmt, "\n"), $($arg)*)});
}

mod any;
mod call;
mod engine;
mod fn_register;
mod parser;

pub use any::Any;
pub use engine::{
    Engine,
    EvalAltResult,
    Scope
};
pub use fn_register::RegisterFn;


