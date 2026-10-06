//! Reaching a compiled chunk from inside a native function.
//!
//! `[1, 2, 3].map(|x| x * 2)` leaves this VM in the middle of a call. `map` is
//! Rhai's, and the pointer it calls back is resolved by Rhai's dispatch, which
//! looks in `global.lib` and the engine's modules. Our chunks are in neither:
//! [`Op::Call`](crate::bytecode::Op::Call) finds one by a name *index* that
//! only the compiler and the call site share, and a `FnPtr` carries a string.
//!
//! So a program that hands a pointer out registers one native wrapper per
//! compiled function for the length of the run. Direct dispatch is untouched —
//! this is somewhere for Rhai to look, not somewhere we look.

#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_function"))]
use crate::{
    func::{ScriptFuncDef, ScriptFuncPayload},
    grain::program::SharedProgram,
    types::Span,
    Engine, Module,
};

/// A wrapper per compiled function, for Rhai to resolve a pointer against.
///
/// Built once per run rather than cached on the program: the closures hold the
/// program, so anything the program held back would be a cycle.
#[cfg(not(feature = "no_function"))]
pub(crate) fn wrappers(engine: &Engine, program: &SharedProgram) -> Module {
    let mut module = Module::new();

    // What Rhai reports as the source of a function it found here. Its own
    // script library is the AST's, so this is the same string by the same
    // route.
    if let Some(source) = program.source() {
        module.set_id(source.clone());
    }

    for function in program.functions() {
        let Some(name) = program.name(function.name) else {
            continue;
        };

        let params = function
            .params
            .iter()
            .map(|&index| program.name(index).unwrap_or(""))
            .map(|name| engine.get_interned_string(name))
            .collect();

        module.set_script_fn(ScriptFuncDef {
            body: ScriptFuncPayload::GrainVM {
                program: program.clone(),
                params: function.params.iter().copied().collect(),
                chunk: function.chunk,
                span: Span::new(
                    program.position(function.chunk.entry() as usize),
                    program.position(function.chunk.end() as usize),
                ),
            },
            name: engine.get_interned_string(name),
            access: function.access,
            #[cfg(not(feature = "no_object"))]
            this_type: function
                .this_type
                .and_then(|index| program.name(index))
                .map(|name| engine.get_interned_string(name)),
            params,
            #[cfg(feature = "metadata")]
            comments: Default::default(),
        });
    }

    module
}
