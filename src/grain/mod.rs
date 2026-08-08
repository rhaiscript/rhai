//! A bytecode VM for Rhai.
//!
//! Rhai evaluates by walking its AST, which the parser allocates a node at a
//! time — so holding a script costs in proportion to how much program it is,
//! and the parser's peak is higher again than what it settles at. That is what
//! caps script size on a small target long before anything else does.
//! rhaigrain compiles the tree to a flat instruction stream that can be
//! produced elsewhere and loaded without a parser.
//!
//! `tests/grain/allocation.rs` measures both ends of that with a tracking
//! allocator.
//!
//! Execution reuses the host `Engine`: `Dynamic` stays the value type and every
//! registered function is dispatched by rhai itself. Only control flow, local
//! variable access and operator fast paths are reimplemented.
//!
//! A program that has been lowered all the way through can be written out with
//! [`Program::write`] and read back with [`Program::read`] — see [`mod@format`].
//! That is the artifact the device loads, and the reason the tree never has to
//! exist there.
//!
//! Coverage is total from the start, by construction rather than by effort.
//! Anything the compiler cannot yet lower is kept as an AST fragment and handed
//! back to rhai's walker through [`bytecode::Op::EvalAst`], so a `Program`
//! always means the same thing as the `AST` it came from. Progress is measured
//! by [`Program::residual_count`] falling, not by constructs becoming legal.
//!
//! # Compiling and running
//!
//! The `Engine` does the parsing and, at runtime, all the dispatching; the VM
//! only replaces the walk between those two.
//!
//! ```
//! use rhai::grain::{Compiler, Vm};
//! use rhai::{Engine, Scope};
//!
//! let engine = Engine::new();
//! let ast = engine.compile("let total = 0; for i in 0..10 { total += i; } total")?;
//!
//! let program = Compiler::new().compile(&ast);
//!
//! // The `Scope` is the caller's locals: a script declares are left in it,
//! // exactly as `Engine::eval_with_scope` would.
//! let mut scope = Scope::new();
//! let value = Vm::new(&engine).eval_with_scope(&mut scope, &program)?;
//!
//! assert_eq!(value.as_int().unwrap(), 45);
//! # Ok::<_, Box<rhai::EvalAltResult>>(())
//! ```
//!
//! # Shipping an artifact
//!
//! The point of the byte encoding: compile on a host, run somewhere that never
//! sees the source. A loaded `Program` borrows its instructions from the bytes,
//! so nothing it retains grows with how long the script is.
//!
//! ```
//! use rhai::grain::{Compiler, Program, Vm};
//! use rhai::{Engine, Scope};
//!
//! let engine = Engine::new();
//!
//! // On the host.
//! let ast = engine.compile("let x = 6; x * 7")?;
//! let program = Compiler::new().compile(&ast);
//!
//! // `write` refuses a program still holding AST fragments, so this is also
//! // the check that the script lowered all the way through.
//! assert_eq!(program.residual_count(), 0);
//! let bytes = program.write().expect("no residuals, so it is writable");
//!
//! // On the device, with no parser and no `AST` in sight.
//! let loaded = Program::read(&bytes).expect("written by this build");
//! let value = Vm::new(&engine).eval(&loaded)?;
//!
//! assert_eq!(value.as_int().unwrap(), 42);
//! # Ok::<_, Box<rhai::EvalAltResult>>(())
//! ```
//!
//! Diagnostics are separable: [`Program::write_stripped`] hands back the
//! artifact and its position table separately, so the device carries only the
//! first and a failure there comes back as an instruction address the host
//! turns into a source position with [`pos::resolve`].

// A VM that runs untrusted bytecode has no business containing any, and saying
// so here makes it the compiler's problem rather than a promise. `crates/
// rhaigrain-pos` declares the same.
#![forbid(unsafe_code)]

pub mod bytecode;
mod compile;
pub mod format;
pub mod pos;
mod program;
mod vm;

pub use compile::Compiler;
pub use program::Program;
pub use vm::Vm;
