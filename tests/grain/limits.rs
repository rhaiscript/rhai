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

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rhai::{Dynamic, Engine, EvalAltResult, Scope};
use rhaigrain::bytecode::Op;
use rhaigrain::{Compiler, Program, Vm};

/// A bare infinite loop, which the compiler lowers with nothing left over —
/// asserted below, so this cannot silently become a test of the fallback.
const SPIN: &str = "loop { }";

fn run_vm(engine: &Engine, source: &str) -> Result<Dynamic, Box<EvalAltResult>> {
    let ast = engine.compile(source).expect("must compile");
    let program = Compiler::new().compile(&ast);

    assert_eq!(
        program.residual_count(),
        0,
        "{source:?} must be fully lowered, or this tests rhai rather than the VM",
    );

    // Without a tick on the back-edge nothing in a compiled loop ever reaches
    // `track_operation`, and the tests below would hang rather than fail.
    assert!(
        program
            .main()
            .ops(program.code())
            .any(|(_, op)| op == Op::Tick),
        "{source:?} lowered to a loop with no operation tick",
    );

    Vm::new(engine).run(&program, &mut Scope::new())
}

#[test]
fn compiled_loop_hits_the_operation_limit() {
    let mut engine = Engine::new();
    engine.set_max_operations(10_000);

    let err = run_vm(&engine, SPIN).expect_err("an unbounded loop must be stopped");

    assert!(
        matches!(*err, EvalAltResult::ErrorTooManyOperations(..)),
        "expected ErrorTooManyOperations, got {err:?}",
    );
}

#[test]
fn compiled_loop_honours_the_progress_interrupt() {
    let ticks = Arc::new(AtomicU64::new(0));
    let seen = ticks.clone();

    let mut engine = Engine::new();
    engine.on_progress(move |count| {
        seen.store(count, Ordering::SeqCst);
        // Stand-in for a host's abort flag.
        (count >= 500).then(|| Dynamic::from("terminated"))
    });

    let err = run_vm(&engine, SPIN).expect_err("the interrupt must stop the loop");

    assert!(
        matches!(*err, EvalAltResult::ErrorTerminated(..)),
        "expected ErrorTerminated, got {err:?}",
    );
    assert!(
        ticks.load(Ordering::SeqCst) >= 500,
        "on_progress should have been called on every back-edge",
    );
}

/// A chunk that loops with no tick in it must still be stopped.
///
/// Every loop this compiler emits carries an `Op::Tick` on its back-edge, so
/// nothing it produces can spin. An artifact is not required to have come from
/// it. Turning this program's tick into a no-op leaves a chunk that still
/// verifies — the jump is in range, the stack balances, every path reaches a
/// `Return` — and runs forever, which makes the engine's budget the only thing
/// between a host and a hostile file.
///
/// So the budget cannot depend on the compiler having been generous: the VM
/// charges an operation for every *backward* transfer, and a cycle always has
/// one. Found by `mutated_artifacts_load_or_fail_but_never_misbehave`, which
/// hung on a mutation rather than failing.
#[test]
fn a_loop_with_its_tick_removed_still_hits_the_limit() {
    let mut engine = Engine::new();
    engine.set_max_operations(10_000);

    let ast = engine.compile(SPIN).expect("must compile");
    let program = Compiler::new().compile(&ast);

    // Where the tick sits inside the code, and what the code looks like, so the
    // same bytes can be found again inside the finished artifact.
    let code = program.code().to_vec();
    let (tick_at, _) = program
        .main()
        .ops(program.code())
        .find(|(_, op)| *op == Op::Tick)
        .expect("the compiler ticks a loop");

    let mut bytes = program.write().expect("a lowered program must write");
    let start = bytes
        .windows(code.len())
        .position(|window| window == code)
        .expect("the artifact embeds the code verbatim");

    // `Checkpoint` is the other one-byte instruction that does nothing to the
    // stack, so this swap leaves every offset, jump target and position entry
    // exactly where it was. Only the metering goes.
    bytes[start + tick_at] = rhaigrain::bytecode::code::tag::CHECKPOINT;

    let tickless = Program::read(&bytes).expect("still a valid artifact");
    assert!(
        !tickless
            .main()
            .ops(tickless.code())
            .any(|(_, op)| op == Op::Tick),
        "the tick should be gone, or this tests nothing",
    );

    let err = Vm::new(&engine)
        .run(&tickless, &mut Scope::new())
        .expect_err("a tickless loop must still be stopped");
    assert!(
        matches!(*err, EvalAltResult::ErrorTooManyOperations(..)),
        "expected ErrorTooManyOperations, got {err:?}",
    );
}

/// The walker and the VM must agree that the script *fails*, even though they
/// disagree about after how many operations.
#[test]
fn the_walker_agrees_the_loop_is_stopped() {
    let mut engine = Engine::new();
    engine.set_max_operations(10_000);

    let ast = engine.compile(SPIN).expect("must compile");
    let err = engine
        .eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast)
        .expect_err("rhai must stop it too");

    assert!(
        matches!(*err, EvalAltResult::ErrorTooManyOperations(..)),
        "expected ErrorTooManyOperations, got {err:?}",
    );
}

/// `max_string_size` is a host's defence, and interpolation is the easiest way
/// to walk past it — rhai checks the running total after *every* segment
/// rather than once at the end, so a script cannot build a huge string and
/// hand it over.
///
/// The position is checked too, because it is the one thing a single
/// instruction might not be able to reproduce: rhai blames the segment that
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

    let walker = engine
        .eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast)
        .expect_err("the walker must refuse it");
    let vm = Vm::new(&engine)
        .run(&program, &mut Scope::new())
        .expect_err("and so must the VM");

    assert!(
        matches!(*vm, EvalAltResult::ErrorDataTooLarge(..)),
        "got {vm:?}",
    );
    assert_eq!(
        format!("{vm:?}"),
        format!("{walker:?}"),
        "including the position of the segment that went over",
    );
}

/// A loop that does terminate must not be killed by the tick itself, and must
/// still produce the value rhai produces.
#[test]
fn ticking_does_not_disturb_a_bounded_loop() {
    let mut engine = Engine::new();
    engine.set_max_operations(10_000);

    let source = "let i = 0; loop { i += 1; if i > 100 { break i; } }";
    let ast = engine.compile(source).expect("must compile");

    let program = Compiler::new().compile(&ast);
    let vm = Vm::new(&engine)
        .run(&program, &mut Scope::new())
        .expect("bounded loop must finish");

    let walker = engine
        .eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast)
        .expect("bounded loop must finish under rhai too");

    assert_eq!(format!("{vm:?}"), format!("{walker:?}"));
}
