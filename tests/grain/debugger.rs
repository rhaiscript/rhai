//! A compiled call must be visible to the debugger.
//!
//! `back_trace` reads `global.debugger.call_stack()`, which Rhai fills from
//! `call_script_fn` (`func/script.rs:78`). The VM's own frames come from
//! `call_compiled_body`, and a VM that skipped the push would answer a script
//! that asks where it is with an empty array rather than a wrong one — silent,
//! and only wrong under a feature most builds do not carry.
//!
//! Outside the differential corpus on purpose: registering a debugger changes
//! what the walker does for *every* script it runs, and the VM has no
//! per-statement `dbg` hook to match it with. Sharing `corpus::engine()` would
//! manufacture divergences in cases that are about something else.
//!
//! Frame counts are compared against the walker rather than written down. Rhai
//! decides what a frame is — `back_trace` filters its own, and a `catch` may or
//! may not leave one behind — so a literal here would pin this harness to
//! today's answer instead of to agreement.

use rhai::grain::{Compiler, Vm};
use rhai::{Array, Dynamic, Engine, Scope};

/// An engine that records nothing and steps nowhere.
///
/// `Continue` is the whole point: the call stack must be maintained because a
/// script can read it, not because anything is watching.
fn engine() -> Engine {
    let mut engine = Engine::new();
    engine.register_debugger(
        |_, dbg| dbg,
        |_, _, _, _, _| Ok(rhai::debugger::DebuggerCommand::Continue),
    );
    engine
}

/// The trace the VM produces, having first established that it is the VM's.
fn vm_trace(engine: &Engine, source: &str) -> Array {
    let ast = engine.compile(source).expect("must compile");
    let program = Compiler::new().compile(&ast);

    assert_eq!(
        program.residual_count(),
        0,
        "{source:?} must be fully lowered, or the frames counted are the walker's: {:?}",
        program.first_unsupported(),
    );

    Vm::new(engine)
        .eval_with_scope(&mut Scope::new(), &program)
        .expect("must run")
        .into_array()
        .expect("back_trace hands back an array")
}

fn walker_trace(engine: &Engine, source: &str) -> Array {
    let ast = engine.compile(source).expect("must compile");
    engine
        .eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast)
        .expect("must run under Rhai too")
        .into_array()
        .expect("back_trace hands back an array")
}

/// Recursion, so a single missing push cannot pass by looking like an off-by-one.
const NESTED: &str = "
    fn foo(x) {
        if x >= 5 { back_trace() } else { foo(x + 1) }
    }
    foo(0)
";

#[test]
fn a_compiled_call_appears_in_the_back_trace() {
    let engine = engine();

    let ours = vm_trace(&engine, NESTED);
    let walker = walker_trace(&engine, NESTED);

    assert!(!ours.is_empty(), "a compiled call left no frame at all");
    assert_eq!(ours.len(), walker.len(), "frame count must match Rhai's");
}

/// An error unwinding out of a call must take its frame with it.
///
/// The push and the pop are not symmetric in the code — the pop has to happen
/// on the path the body raised on as well as the one it returned on — so a
/// trace taken *after* a caught throw is the one that catches a leak.
const AFTER_A_CAUGHT_THROW: &str = r#"
    fn deep(x) {
        if x >= 3 { throw "boom" } else { deep(x + 1) }
    }
    fn trace() { back_trace() }

    try { deep(0) } catch { }

    trace()
"#;

#[test]
fn a_caught_throw_leaves_no_frames_behind() {
    let engine = engine();

    let ours = vm_trace(&engine, AFTER_A_CAUGHT_THROW);
    let walker = walker_trace(&engine, AFTER_A_CAUGHT_THROW);

    assert_eq!(
        ours.len(),
        walker.len(),
        "frames from the unwound calls are still on the stack",
    );
}
