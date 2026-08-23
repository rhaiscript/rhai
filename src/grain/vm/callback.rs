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
//!
//! # What being a native costs
//!
//! Rhai reaches its own closures through a `Fn*` pointer carrying the body, and
//! that shortcut is what these wrappers cannot have. One consequence:
//!
//! * **Speed, and call budget.** Rhai resolves a wrapper by name and type on
//!   every element, from a cache it builds fresh per crossing, where its own
//!   pointer skips resolution entirely. `native callbacks` in
//!   `examples/bench.rs` measures 0.34x — the one case the VM loses — and the
//!   two extra dispatch layers cost 5 call levels per crossing against the
//!   walker's 2.
//!
//! Neither touches a pointer called directly from compiled code, which is
//! `Op::CallFnPtr` and never comes through here.

#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_function"))]
use crate::{
    ast::script_fn::{ScriptFuncDef, ScriptFuncPayload},
    grain::program::SharedProgram,
    FnAccess, Module,
};

/// The most parameters a wrapper is registered for.
///
/// A wrapper takes `Dynamic` throughout, and Rhai only reaches a `Dynamic`
/// parameter by permuting the call's own argument types towards it — a search
/// it caps at `MAX_DYNAMIC_PARAMETERS`, 16 (`func/call.rs:235`). A wider
/// wrapper would be registered and never found, so it is left out rather than
/// silently dead. Direct dispatch has no such bound; this limits only what a
/// native can call back into.
#[cfg(not(feature = "no_function"))]
const MAX_PARAMS: usize = 16;

/// A wrapper per compiled function, for Rhai to resolve a pointer against.
///
/// Built once per run rather than cached on the program: the closures hold the
/// program, so anything the program held back would be a cycle.
#[cfg(not(feature = "no_function"))]
pub(super) fn wrappers(program: &SharedProgram) -> Module {
    let mut module = Module::new();

    // What Rhai reports as the source of a function it found here. Its own
    // script library is the AST's, so this is the same string by the same
    // route.
    if let Some(source) = program.source() {
        module.set_id(source.clone());
    }

    for function in program.functions() {
        let arity = function.params.len();
        if arity > MAX_PARAMS {
            continue;
        }
        let Some(name) = program.name(function.name) else {
            continue;
        };

        let owner = program.clone();

        let params = function
            .params
            .iter()
            .map(|&index| program.names().get(index).unwrap_or(""))
            .map(Into::into)
            .collect();

        module.set_script_fn(ScriptFuncDef {
            body: ScriptFuncPayload::GrainVM(owner),
            name: name.into(),
            access: FnAccess::Private,
            #[cfg(not(feature = "no_object"))]
            this_type: None,
            params,
            #[cfg(feature = "metadata")]
            comments: Default::default(),
        });
    }

    module
}
