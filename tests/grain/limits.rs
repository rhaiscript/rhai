//! Compiled code must still be stoppable.
//!
//! Rhai enforces `max_operations` and the `on_progress` interrupt from
//! `Engine::track_operation`, which the tree walker calls per AST node. A VM
//! that never called it would turn `loop {}` from a script the engine
//! terminates into one that hangs the host — a safety regression, not a
//! performance one, which is why `track_operation` is in the patch.
//!
//! These live outside the differential corpus on purpose. The walker ticks per
//! node and the VM ticks per loop back-edge, so the operation *counts* differ
//! and always will. What must hold is that the limit fires and the interrupt is
//! honoured, so that is what is asserted — not parity of counts or positions.

#![cfg(feature = "internals")]

use std::sync::{Arc, Mutex};

use rhai::grain::bytecode::Op;
use rhai::grain::{Compiler, Vm};
use rhai::{Dynamic, Engine, EvalAltResult, Scope};

/// A bare infinite loop, which the compiler lowers with nothing left over —
/// asserted below, so this cannot silently become a test of the fallback.
const SPIN: &str = "loop { }";

fn run_vm(engine: &Engine, source: &str) -> Result<Dynamic, Box<EvalAltResult>> {
    let ast = engine.compile(source).expect("must compile");
    let program = Compiler::new().compile(&ast);

    assert_eq!(program.residual_count(), 0, "{source:?} must be fully lowered, or this tests Rhai rather than the VM",);

    // Without a tick on the back-edge nothing in a compiled loop ever reaches
    // `track_operation`, and the tests below would hang rather than fail.
    assert!(program.main().ops(program.code()).any(|(_, op)| op == Op::Tick), "{source:?} lowered to a loop with no operation tick",);

    Vm::new(engine).eval_with_scope(&mut Scope::new(), &program)
}

#[test]
fn compiled_loop_hits_the_operation_limit() {
    let mut engine = Engine::new();
    engine.set_max_operations(10_000);

    let err = run_vm(&engine, SPIN).expect_err("an unbounded loop must be stopped");

    assert!(matches!(*err, EvalAltResult::ErrorTooManyOperations(..)), "expected ErrorTooManyOperations, got {err:?}",);
}

#[test]
fn compiled_loop_honours_the_progress_interrupt() {
    // A `Mutex` rather than an `AtomicU64`, so this file builds on targets
    // without 64-bit atomics.
    let ticks = Arc::new(Mutex::new(0));
    let seen = ticks.clone();

    let mut engine = Engine::new();
    engine.on_progress(move |count| {
        *seen.lock().unwrap() = count;
        // Stand-in for a host's abort flag.
        (count >= 500).then(|| Dynamic::from("terminated"))
    });

    let err = run_vm(&engine, SPIN).expect_err("the interrupt must stop the loop");

    assert!(matches!(*err, EvalAltResult::ErrorTerminated(..)), "expected ErrorTerminated, got {err:?}",);
    assert!(*ticks.lock().unwrap() >= 500, "on_progress should have been called on every back-edge",);
}

/// The walker and the VM must agree that the script *fails*, even though they
/// disagree about after how many operations.
#[test]
fn the_walker_agrees_the_loop_is_stopped() {
    let mut engine = Engine::new();
    engine.set_max_operations(10_000);

    let ast = engine.compile(SPIN).expect("must compile");
    let err = engine.eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast).expect_err("rhai must stop it too");

    assert!(matches!(*err, EvalAltResult::ErrorTooManyOperations(..)), "expected ErrorTooManyOperations, got {err:?}",);
}

/// `max_string_size` is a host's defense, and interpolation is the easiest way
/// to walk past it — Rhai checks the running total after *every* segment
/// rather than once at the end, so a script cannot build a huge string and
/// hand it over.
///
/// The position is checked too, because it is the one thing a single
/// instruction might not be able to reproduce: Rhai blames the segment that
/// tipped the total over, and the VM has one position-table entry per
/// instruction.
#[test]
fn interpolation_respects_the_string_limit() {
    let mut engine = Engine::new();
    engine.set_max_string_size(16);

    let source = r#"let a = "0123456789"; `${a}${a}${a}`"#;
    let ast = engine.compile(source).expect("must compile");
    let program = Compiler::new().compile(&ast);
    assert_eq!(program.residual_count(), 0, "must be lowered, not walked");

    let walker = engine.eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast).expect_err("the walker must refuse it");
    let vm = Vm::new(&engine).eval_with_scope(&mut Scope::new(), &program).expect_err("and so must the VM");

    assert!(matches!(*vm, EvalAltResult::ErrorDataTooLarge(..)), "got {vm:?}",);
    assert_eq!(format!("{vm:?}"), format!("{walker:?}"), "including the position of the segment that went over",);
}

/// A map literal's constant values are in its template rather than evaluated,
/// but they still count towards the literal's total: the VM must refuse what
/// the walker refuses, against the same entry.
#[test]
#[cfg(not(feature = "no_object"))]
fn map_constants_count_towards_the_limits() {
    let mut engine = Engine::new();
    engine.set_max_string_size(10);
    engine.set_max_map_size(10);

    for source in [r#"let x = "abc"; #{a: "abcdef", b: "ghijk", c: x}"#, r#"let x = 1; #{a: #{b:1,c:2,d:3,e:4,f:5}, g: #{h:1,i:2,j:3,k:4}, l: x}"#] {
        let ast = engine.compile(source).expect("must compile");
        let program = Compiler::new().compile(&ast);
        assert_eq!(program.residual_count(), 0, "{source:?} must be lowered, not walked");

        let walker = engine.eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast).expect_err("the walker must refuse it");
        let vm = Vm::new(&engine).eval_with_scope(&mut Scope::new(), &program).expect_err("and so must the VM");

        assert!(matches!(*vm, EvalAltResult::ErrorDataTooLarge(..)), "{source:?}: got {vm:?}");
        assert_eq!(format!("{vm:?}"), format!("{walker:?}"), "{source:?}");
    }

    // Under the limits, they agree on the value.
    let source = r#"let x = "abc"; #{a: "ab", b: #{c: 1}, d: x}"#;
    let ast = engine.compile(source).expect("must compile");
    let program = Compiler::new().compile(&ast);
    let walker = engine.eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast).unwrap();
    let vm = Vm::new(&engine).eval_with_scope(&mut Scope::new(), &program).unwrap();
    assert_eq!(format!("{vm:?}"), format!("{walker:?}"));
}

/// A loop that does terminate must not be killed by the tick itself, and must
/// still produce the value Rhai produces.
#[test]
fn ticking_does_not_disturb_a_bounded_loop() {
    let mut engine = Engine::new();
    engine.set_max_operations(10_000);

    let source = "let i = 0; loop { i += 1; if i > 100 { break i; } }";
    let ast = engine.compile(source).expect("must compile");

    let program = Compiler::new().compile(&ast);
    let vm = Vm::new(&engine).eval_with_scope(&mut Scope::new(), &program).expect("bounded loop must finish");

    let walker = engine.eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast).expect("bounded loop must finish under Rhai too");

    assert_eq!(format!("{vm:?}"), format!("{walker:?}"));
}

/// A closure that never returns, handed to a native. The native calls it back
/// through `call_script_fn`, which runs the chunk in a nested VM and hands the
/// operation count back to the caller afterwards.
#[cfg(not(any(feature = "no_index", feature = "no_object", feature = "no_function")))]
const SPIN_IN_CALLBACK: &str = "let a = [1, 2, 3]; a.map(|x| { loop { } })";

/// Stop a run after this many `on_progress` calls, whatever the count says.
///
/// Without it, a counter that never advances turns these tests into hangs.
#[cfg(not(any(feature = "no_index", feature = "no_object", feature = "no_function")))]
const BACKSTOP_CALLS: u64 = 100_000;

/// Compile `SPIN_IN_CALLBACK` and run it with the callback wrappers installed.
#[cfg(not(any(feature = "no_index", feature = "no_object", feature = "no_function")))]
fn run_callback(engine: &Engine) -> Result<Dynamic, Box<EvalAltResult>> {
    let ast = engine.compile(SPIN_IN_CALLBACK).expect("must compile");
    let program = Compiler::new().compile(&ast);

    assert_eq!(program.residual_count(), 0, "must be lowered, or this tests Rhai rather than the VM");
    assert!(program.makes_fn_pointers(), "the closure must reach `map` as a pointer to a compiled chunk");

    Vm::new(engine).eval_with_callbacks(&mut Scope::new(), &program.into_shared())
}

/// The error a callback raised, with the `ErrorInFunctionCall` layers Rhai
/// wraps it in removed.
#[cfg(not(any(feature = "no_index", feature = "no_object", feature = "no_function")))]
fn innermost(err: &EvalAltResult) -> &EvalAltResult {
    match err {
        EvalAltResult::ErrorInFunctionCall(.., inner, _) => innermost(inner),
        err => err,
    }
}

/// The limit must fire inside a compiled function a native calls back.
///
/// With `sync` but without 64-bit atomics the count is a plain `u64`, with
/// separate increment and hand-back paths. A counter that never advances would
/// never trip the limit and would reach the backstop instead.
#[cfg(not(any(feature = "no_index", feature = "no_object", feature = "no_function")))]
#[test]
fn a_callback_hits_the_operation_limit() {
    let calls = Arc::new(Mutex::new(0_u64));
    let counted = calls.clone();

    let mut engine = Engine::new();
    engine.set_max_operations(10_000);
    engine.on_progress(move |_| {
        let mut calls = counted.lock().unwrap();
        *calls += 1;
        (*calls >= BACKSTOP_CALLS).then(|| Dynamic::from("backstop"))
    });

    let err = run_callback(&engine).expect_err("an unbounded callback must be stopped");

    assert!(matches!(innermost(&err), EvalAltResult::ErrorTooManyOperations(..)), "expected ErrorTooManyOperations, got {err:?}");
}

/// `on_progress` must see the count advance inside a callback, by one per
/// operation.
///
/// With `sync` but without 64-bit atomics, only the steady state inside the
/// callback is checked. A native's first attempt runs against a clone of the
/// global state whose count cannot be written back through a shared reference,
/// so the count may repeat once there.
#[cfg(not(any(feature = "no_index", feature = "no_object", feature = "no_function")))]
#[test]
fn a_callback_advances_the_progress_count() {
    let counts = Arc::new(Mutex::new(Vec::new()));
    let seen = counts.clone();

    let mut engine = Engine::new();
    engine.on_progress(move |count| {
        let mut counts = seen.lock().unwrap();
        counts.push(count);
        (counts.len() >= 1_000).then(|| Dynamic::from("stopped"))
    });

    let err = run_callback(&engine).expect_err("the interrupt must stop the callback");
    assert!(matches!(innermost(&err), EvalAltResult::ErrorTerminated(..)), "expected ErrorTerminated, got {err:?}");

    let counts = counts.lock().unwrap();
    let counts = if cfg!(any(not(feature = "sync"), atomic_ops_count)) { counts.as_slice() } else { &counts[counts.len() - 100..] };
    assert!(counts.windows(2).all(|w| w[1] == w[0] + 1), "the count did not advance by one per operation: {counts:?}");
}
