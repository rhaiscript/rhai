//! Reaching a compiled chunk from inside a native function.
//!
//! `[1, 2, 3].map(|x| x * 2)` leaves this VM in the middle of a call. `map` is
//! Rhai's, and the pointer it calls back is resolved by Rhai's dispatch, which
//! looks in `global.lib` and the engine's modules. Our chunks are in neither:
//! [`Op::Call`](crate::bytecode::Op::Call) finds one by a name *index* that
//! only the compiler and the call site share, and a `FnPtr` carries a string.
//!
//! So a program that hands a pointer out stacks a module of its compiled
//! functions for the length of the run. Either every function is registered
//! in it up front, or, lazily, the module looks each one up in the program
//! only when Rhai asks for it, so a run pays for the functions it calls back,
//! not for every function it has. Direct dispatch is untouched — this is
//! somewhere for Rhai to look, not somewhere we look.

#[cfg(not(feature = "no_function"))]
use crate::{grain::program::SharedProgram, module::GrainFunctions, Module};

/// A module of the compiled functions, for Rhai to resolve a pointer against,
/// with every function registered up front unless `lazy`.
///
/// Built once per run rather than cached on the program: the definitions it
/// makes hold the program, so anything the program held back would be a
/// cycle.
#[cfg(not(feature = "no_function"))]
pub(crate) fn wrappers(program: &SharedProgram, lazy: bool) -> Module {
    let mut module = Module::from_grain_functions(GrainFunctions::new(program));
    if !lazy {
        module.register_lazy_functions();
        module.build_index();
    }
    module
}
