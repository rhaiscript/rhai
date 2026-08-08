//! A bytecode VM for Rhai.
//!
//! Rhai evaluates by walking its AST, which the parser allocates a node at a
//! time — so holding a script costs in proportion to how much program it is,
//! and the parser's peak is higher again than what it settles at. That is what
//! caps script size on a small target long before anything else does.
//! rhaigrain compiles the tree to a flat instruction stream that can be
//! produced elsewhere and loaded without a parser.
//!
//! `tests/allocation.rs` measures both ends of that with a tracking allocator.
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
