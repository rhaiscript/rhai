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

use core::any::TypeId;
use core::mem;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

use crate::{
    func::RhaiFunc, Dynamic, FnArgsVec, FuncRegistration, Module, NativeCallContext, Shared,
    SmartString,
};

use super::{malformed, Vm, VmResult};
use crate::grain::program::SharedProgram;

/// The most parameters a wrapper is registered for.
///
/// A wrapper takes `Dynamic` throughout, and Rhai only reaches a `Dynamic`
/// parameter by permuting the call's own argument types towards it — a search
/// it caps at `MAX_DYNAMIC_PARAMETERS`, 16 (`func/call.rs:235`). A wider
/// wrapper would be registered and never found, so it is left out rather than
/// silently dead. Direct dispatch has no such bound; this limits only what a
/// native can call back into.
const MAX_PARAMS: usize = 16;

/// A wrapper per compiled function, for Rhai to resolve a pointer against.
///
/// Built once per run rather than cached on the program: the closures hold the
/// program, so anything the program held back would be a cycle.
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
        // A `this`-taking chunk cannot be reached this way. A wrapper is
        // registered at one arity, and how many arguments Rhai asks for depends
        // on what the *native* appends beside the receiver — `map` adds an
        // index, `reduce` adds the running result — which the wrapper has no way
        // to know. Rhai's own pointer carries the body and sizes the call from
        // its declared arity (`types/fn_ptr.rs:501-535`); a name-only pointer,
        // which is all a wrapper can be, cannot. So these are left to Rhai,
        // and `Program::needs_walker` is what keeps its copy alive for them.
        if function.takes_this {
            continue;
        }
        let Some(name) = program.name(function.name) else {
            continue;
        };

        let owner = program.clone();
        let called: SmartString = name.into();

        // One closure for every arity, rather than the fixed-arity shapes
        // `Module::set_native_fn` generates. `Dynamic` parameters throughout
        // mean the types never have to line up, only the count.
        let wrapper = move |context: Option<NativeCallContext>, args: &mut [&mut Dynamic]| {
            invoke(&owner, &called, context.as_ref(), args)
        };

        FuncRegistration::new(name)
            .in_internal_namespace()
            .set_into_module_raw(
                &mut module,
                vec![TypeId::of::<Dynamic>(); arity],
                RhaiFunc::Pure {
                    func: Shared::new(wrapper),
                    has_context: true,
                    is_pure: true,
                    is_volatile: false,
                },
            );
    }

    module
}

/// Run one chunk for a native that called back into us.
fn invoke(
    program: &SharedProgram,
    name: &str,
    context: Option<&NativeCallContext>,
    args: &mut [&mut Dynamic],
) -> VmResult {
    // Registered with `has_context`, so Rhai always supplies one.
    let context =
        context.ok_or_else(|| malformed("a callback wrapper was given no context".into()))?;

    // Taken rather than cloned, as every registered function does: the
    // arguments are the caller's to give away, and it has already copied
    // anything it still needs.
    let values: FnArgsVec<Dynamic> = args.iter_mut().map(|arg| mem::take(*arg)).collect();

    let mut vm = Vm::reentrant(context);
    // A chunk reached this way can itself build a closure, and that pointer
    // needs the program as much as one built in the outer run does.
    #[cfg(not(feature = "no_function"))]
    vm.adopt(program);

    vm.call_function(
        program,
        name,
        values,
        context.call_level(),
        context.call_position(),
    )
}
