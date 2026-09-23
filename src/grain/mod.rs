//! _(grain)_ A bytecode [`VM`] for Rhai.
//!
//! Exported under the `grain` feature only.
//!
//! [`AST`]: crate::AST
//! [`Engine`]: crate::Engine
//! [`VM`]: vm::Vm
//!
//! # Core Concept
//!
//! Rhai evaluates a script by walking its [`AST`], which the parser allocates
//! a node at a time — so holding a script costs in proportion to how much program
//! there is, and the parser's peak is higher again than what it settles at.
//! That is what caps script size on a small target long before anything else does.
//!
//! Rhai Grain transpiles the tree to a flat instruction stream that can be produced
//! elsewhere and loaded without a parser or even the [`AST`] interpreter.
//!
//! ### Note
//!
//! `tests/grain/allocation.rs` measures both ends with a tracking allocator.
//!
//! # Safety
//!
//! The entire Rhai Grain module contains no `unsafe` code.
//! It is marked with `#![forbid(unsafe_code)]`.
//!
//! # Reading and Writing Rhai Grain Programs
//!
//! A program that has been lowered all the way through can be written out with
//! [`Program::write`] and read back with [`Program::read`]. That is the artifact
//! the device loads, and the reason the tree never has to exist there.
//!
//! Coverage is total from the start, by construction rather than by effort.
//! Anything the transpiler cannot yet lower is kept as an _AST fragment_ and
//! handed back to Rhai's interpreter through
//! [`Op::EvalAst`][crate::grain::bytecode::Op::EvalAst], so a [`Program`] always
//! means the same thing as the [`AST`] it came from.
//!
//! # Residual Fragments
//!
//! [`Program::residual_count`] indicates how much of the [`AST`] is still residual.
//!
//! When it drops to zero, the [`AST`] has been fully lowered and the [`VM`] can execute
//! it directly without any help from the [`AST`] interpreter, which can even be removed
//! by enabling the `no_ast` feature.
//!
//! # Transpiling and Executing Bytecodes
//!
//! Bytecodes execution reuses the host [`Engine`]: [`Dynamic`][crate::Dynamic] stays
//! the value type and every registered function is dispatched by the [`Engine`] itself.
//!
//! Only control flow, local variables access and operator fast paths are reimplemented
//! by the vm.
//!
//! The [`Engine`] does the parsing to [`AST`] and, at runtime, all the functions
//! dispatching. The [`VM`] essentially flattens and threads the [`AST`] into a linear
//! instruction stream so it can be executed without recursion nor multiple layers
//! of function calls.
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
//! # Shipping Bytecodes
//!
//! The entire point of using bytecodes is:
//! - compile on a host,
//! - run somewhere that never sees the source.
//!
//! A loaded [`Program`] borrows its instructions directly from the raw bytecodes,
//!
//! Nothing it retains grows with how long the script is.
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
//! // On the device, with no parser, no interpreter and no AST in sight.
//! let loaded = Program::read(&bytes).expect("written by this build");
//! let value = Vm::new(&engine).eval(&loaded)?;
//!
//! assert_eq!(value.as_int().unwrap(), 42);
//! # Ok::<_, Box<rhai::EvalAltResult>>(())
//! ```
//!
//! # Diagnostics
//!
//! Diagnostics are separable: [`Program::write_stripped`] hands back the
//! bytecodes artifact and a [`Sidecar`] separately, so the device carries
//! only the first.
//!
//! A failure comes back as one [`Fault`] per frame, and the host resolves
//! those against the sidecar it kept — a stack of addresses on one side,
//! a symbol file on the other, as a crash reporter does.
//!
//! ### Example
//!
//! The example below needs script functions to have two frames, positions to
//! resolve them to, and a division by zero that raises rather than panicking,
//! so it is compiled only where all three exist.
#![cfg_attr(
    not(any(
        feature = "no_function",
        feature = "no_position",
        feature = "unchecked"
    )),
    doc = r##"
```
use rhai::grain::{Compiler, Program, Vm};
use rhai::{Engine, Scope};

let engine = Engine::new();
let ast = engine.compile("fn half(x) { x / 0 }\nhalf(4)")?;
let stripped = Compiler::new().compile(&ast).write_stripped().unwrap();

// The device has the artifact and nothing else. It fails, and all it can
// say is which instructions — innermost frame first.
let program = Program::read(&stripped.artifact).unwrap();
let mut vm = Vm::new(&engine);
let error = vm.eval_with_scope(&mut Scope::new(), &program).unwrap_err();
assert!(error.position().is_none());
let trace = vm.fault_trace();

// The host kept the sidecar, and turns that back into a backtrace.
let sites = stripped.sidecar.resolve(&trace);
assert_eq!(sites[0].unwrap().line, 1); // the divide, inside `half`
assert_eq!(sites[1].unwrap().line, 2); // the call to it
# Ok::<_, Box<rhai::EvalAltResult>>(())
```
"##
)]
//! # Debugging
//!
//! A `debugging` build marks every statement, and the [`VM`] stops at the markers:
//! back-trace, stepping, break-points by position as well as function-exit events
//! all work.
//!
//! However, a _statement_ is as fine as Rhai Grain gets.
//!
//! The [`AST`] interpreter stops at every *expression*, which the [`Program`] no
//! longer has — so a step lands on the next _statement_ rather than part way through
//! the one it is on.
//!
//! Similarly, a break-point on a function name or a property name etc. never matches,
//! because what the [`AST`] interpreter hands the callback a synthetic `Noop` node,
//! not the call itself. A break-point by position covers the same line.
//!
//! ### Cost of Debug Markers
//!
//! The debugging markers are the one part of a program that a shipping build
//! (intended for a [`VM`] without the `debugging` feature flag enabled) should not include:
//! an [`Engine`] with no `debugging` callbacks has nothing to stop for.
//!
//! Each marker costs about six bytes per statement where they are compiled and a
//! [`Program`] without them still runs anywhere -- it simply cannot be stopped.
//!
// A [`VM`] that runs untrusted bytecode has no business containing any debugging markers.
#![forbid(unsafe_code)]

#[cfg(feature = "internals")]
pub mod bytecode;
#[cfg(not(feature = "internals"))]
pub(crate) mod bytecode;

mod compile;
mod format;
mod pos;
mod program;
mod vm;

#[cfg(not(feature = "no_ast"))]
pub use compile::Compiler;
pub use format::{Abi, AbiMismatch, Caps, ReadError, Sidecar, Stripped, WriteError};
pub use pos::Site;
pub use program::{Program, SharedProgram};
pub use vm::{Fault, Vm};

#[cfg(feature = "internals")]
pub use program::Function;

#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "no_function"))]
pub(crate) use {program::export_functions_to_module, vm::callback_wrappers};
