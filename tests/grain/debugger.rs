//! A compiled program must be visible to the debugger, and stoppable.
//!
//! Two claims. The first is that a compiled call is a call: `back_trace` reads
//! `global.debugger.call_stack()`, which Rhai fills from `call_script_fn`
//! (`func/script.rs:78`), and a VM that skipped the push would answer a script
//! that asks where it is with an empty array rather than a wrong one — silent,
//! and only wrong under a feature most builds do not carry.
//!
//! The second is that a chunk can be stopped in. Rhai runs the callback per AST
//! node and a chunk has no nodes, so the compiler marks where the statements
//! were (`Op::Statement`) and the VM stops there. Statements are as fine as it
//! gets: an expression is not a thing a chunk still has, so the walker stops in
//! places the VM cannot. Every test below therefore says which engine's answer
//! it is pinning, and compares to the walker only where both can agree.
//!
//! Outside the differential corpus on purpose: registering a debugger changes
//! what *every* script does, and the corpus is about other subjects.
//!
//! Frame counts are compared against the walker rather than written down. Rhai
//! decides what a frame is — `back_trace` filters its own, and a `catch` may or
//! may not leave one behind — so a literal here would pin this harness to
//! today's answer instead of to agreement.

use std::sync::{Arc, Mutex};

use rhai::debugger::{BreakPoint, DebuggerCommand, DebuggerEvent};
use rhai::grain::{Compiler, Program, Vm};
use rhai::{Array, Dynamic, Engine, Scope, INT};
// Only a break-point placed by line needs one, and those tests go with the
// positions.
#[cfg(not(feature = "no_position"))]
use rhai::Position;

/// One stop, as the callback saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stop {
    /// What stopped it.
    event: &'static str,
    /// The line of the node it stopped at, or 0 where there is none.
    line: usize,
    /// What a function-exit event carried, rendered. Empty for the rest.
    detail: String,
}

/// Where a run stopped, in order.
type Log = Arc<Mutex<Vec<Stop>>>;

fn describe(event: DebuggerEvent) -> (&'static str, String) {
    match event {
        DebuggerEvent::Start => ("start", String::new()),
        DebuggerEvent::Step => ("step", String::new()),
        DebuggerEvent::BreakPoint(..) => ("break", String::new()),
        DebuggerEvent::FunctionExitWithValue(value) => ("exit", value.to_string()),
        DebuggerEvent::FunctionExitWithError(err) => ("raised", err.to_string()),
        DebuggerEvent::End => ("end", String::new()),
        _ => ("unknown", String::new()),
    }
}

/// An engine whose debugger records every stop and answers `reply`.
///
/// `reply` sees the stops so far with the current one last, which is how a test
/// says "step until you are inside, then let it run". The log is shared rather
/// than owned because the interface is `Fn` — and `Send + Sync` under `sync`.
fn recording(reply: impl Fn(&[Stop]) -> DebuggerCommand + Send + Sync + 'static) -> (Engine, Log) {
    breaking(Vec::new(), reply)
}

/// The same, with break-points in place before the run.
fn breaking(points: Vec<BreakPoint>, reply: impl Fn(&[Stop]) -> DebuggerCommand + Send + Sync + 'static) -> (Engine, Log) {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let recorded = log.clone();

    let mut engine = Engine::new();
    #[allow(deprecated)] // not deprecated but unstable
    engine.register_debugger(
        move |_, mut dbg| {
            dbg.break_points_mut().extend(points.iter().cloned());
            dbg
        },
        move |_, event, _, _, pos| {
            let (event, detail) = describe(event);
            let mut stops = recorded.lock().expect("nothing else holds the log");
            stops.push(Stop { event, line: pos.line().unwrap_or(0), detail });
            Ok(reply(&stops))
        },
    );

    (engine, log)
}

fn stops(log: &Log) -> Vec<Stop> {
    log.lock().expect("the run is over").clone()
}

/// The lines a run stopped at, in order, dropping the events that carry no
/// position of their own.
#[cfg(not(feature = "no_position"))]
fn lines(log: &Log) -> Vec<usize> {
    stops(log).iter().filter(|stop| stop.line > 0).map(|stop| stop.line).collect()
}

fn events(log: &Log) -> Vec<&'static str> {
    stops(log).iter().map(|stop| stop.event).collect()
}

/// The program `source` lowers to, having first established that it is wholly
/// the VM's: a fragment is evaluated by the walker, which stops at its own nodes
/// and would put them in the log beside ours.
fn compiled(engine: &Engine, source: &str) -> Program<'static> {
    let ast = engine.compile(source).expect("must compile");
    let program = Compiler::new().compile(&ast);

    assert_eq!(program.residual_count(), 0, "{source:?} must be fully lowered, or the stops counted are the walker's: {:?}", program.first_unsupported(),);

    program
}

fn vm_eval(engine: &Engine, source: &str) -> Dynamic {
    Vm::new(engine).eval_with_scope(&mut Scope::new(), &compiled(engine, source)).expect("must run")
}

fn walker_eval(engine: &Engine, source: &str) -> Dynamic {
    walker_eval_with(engine, &mut Scope::new(), source)
}

/// The same against a scope the caller keeps, for a test that reads what the run
/// left in it.
fn walker_eval_with(engine: &Engine, scope: &mut Scope, source: &str) -> Dynamic {
    let ast = engine.compile(source).expect("must compile");
    engine.eval_ast_with_scope::<Dynamic>(scope, &ast).expect("must run under Rhai too")
}

/// Run it for the stops rather than for the answer.
fn vm_run(engine: &Engine, source: &str) {
    let _ = vm_eval(engine, source);
}

fn walker_run(engine: &Engine, source: &str) {
    let _ = walker_eval(engine, source);
}

fn vm_trace(engine: &Engine, source: &str) -> Array {
    vm_eval(engine, source).into_array().expect("back_trace hands back an array")
}

fn walker_trace(engine: &Engine, source: &str) -> Array {
    walker_eval(engine, source).into_array().expect("back_trace hands back an array")
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
    let (engine, _) = recording(|_| DebuggerCommand::Continue);

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
    let (engine, _) = recording(|_| DebuggerCommand::Continue);

    let ours = vm_trace(&engine, AFTER_A_CAUGHT_THROW);
    let walker = walker_trace(&engine, AFTER_A_CAUGHT_THROW);

    assert_eq!(ours.len(), walker.len(), "frames from the unwound calls are still on the stack",);
}

/// Three statements, one per line, and nothing the optimizer can fold away.
///
/// Every use is a claim about *where* it stopped, so a build with no positions
/// has nothing to ask of it.
#[cfg(not(feature = "no_position"))]
const THREE: &str = "\
let a = 1;
a += 1;
a
";

/// A call, so a step can go into one and a `next` can go over one.
///
/// Line 1 is the body's opening brace, line 2 its only statement, and the call
/// is on line 5.
const CALLING: &str = "\
fn inner(x) {
    x + 1
}
let a = 1;
let b = inner(a);
b
";

#[test]
#[cfg(not(feature = "no_position"))]
fn stepping_stops_at_every_statement() {
    let (engine, log) = recording(|_| DebuggerCommand::StepInto);
    vm_run(&engine, THREE);

    assert_eq!(lines(&log), vec![1, 2, 3], "one stop per statement, in order");
    assert_eq!(events(&log), vec!["start", "step", "step", "end"], "the run must announce both of its ends",);

    // The walker stops at expressions too, so it can only be asked for a
    // superset — that the VM stops nowhere Rhai would not.
    let (walker_engine, walker) = recording(|_| DebuggerCommand::StepInto);
    walker_run(&walker_engine, THREE);
    for line in lines(&log) {
        assert!(lines(&walker).contains(&line), "line {line} is a stop the walker does not have: {:?}", lines(&walker),);
    }
}

#[test]
#[cfg(not(feature = "no_position"))]
fn a_break_point_stops_a_compiled_statement() {
    let point = || BreakPoint::AtPosition {
        source: None,
        pos: Position::new(2, 0),
        enabled: true,
    };

    let (engine, log) = breaking(vec![point()], |_| DebuggerCommand::Continue);
    vm_run(&engine, THREE);

    assert_eq!(stops(&log).iter().filter(|stop| stop.event == "break").map(|stop| stop.line).collect::<Vec<_>>(), vec![2], "the break-point on line 2 fired once, at line 2",);

    // Rhai's own count is not one: a line matches the statement's node and
    // every expression node on it. Agreement is that the line stops at all.
    let (walker_engine, walker) = breaking(vec![point()], |_| DebuggerCommand::Continue);
    walker_run(&walker_engine, THREE);
    assert!(stops(&walker).iter().any(|stop| stop.event == "break"), "the walker did not stop for the same break-point: {:?}", stops(&walker),);
}

#[test]
#[cfg(not(feature = "no_position"))]
fn a_next_steps_over_a_call() {
    let (engine, log) = recording(|_| DebuggerCommand::Next);
    vm_run(&engine, CALLING);

    assert_eq!(lines(&log), vec![4, 5, 6], "the call's own line, not its body");

    let (walker_engine, walker) = recording(|_| DebuggerCommand::Next);
    walker_run(&walker_engine, CALLING);
    for log in [&log, &walker] {
        assert!(!lines(log).contains(&2), "a `next` stopped inside the callee: {:?}", lines(log),);
    }
}

/// A block, so a `next` has something to step over that is not a call.
///
/// Lines 3 and 4 are inside it, and line 6 is after it.
#[cfg(not(feature = "no_position"))]
const BLOCKED: &str = "\
let a = 1;
if a == 1 {
    a += 1;
    a += 1;
}
a
";

/// The reason a marker carries its nesting depth: without it a `next` at the
/// `if` would stop at the first statement inside it.
///
/// Line 6 is where the two engines part. It is a bare expression statement,
/// which Rhai treats as transitive — it stops *inside* one rather than at it,
/// and a `next` only stops at statements, so it does not stop there at all. The
/// VM's marker is the statement, so it stops once. What the test is about, they
/// agree on: neither goes into the block.
#[test]
#[cfg(not(feature = "no_position"))]
fn a_next_steps_over_a_block() {
    let (engine, log) = recording(|_| DebuggerCommand::Next);
    vm_run(&engine, BLOCKED);

    assert_eq!(lines(&log), vec![1, 2, 6], "the `if`, then what follows it");

    let (walker_engine, walker) = recording(|_| DebuggerCommand::Next);
    walker_run(&walker_engine, BLOCKED);
    for log in [&log, &walker] {
        assert!(!lines(log).iter().any(|line| [3, 4].contains(line)), "a `next` stopped inside the block: {:?}", lines(log),);
    }
}

#[test]
#[cfg(not(feature = "no_position"))]
fn a_step_goes_into_a_call_and_reports_leaving_it() {
    let (engine, log) = recording(|_| DebuggerCommand::StepInto);
    vm_run(&engine, CALLING);

    assert_eq!(lines(&log), vec![4, 5, 1, 2, 5, 6], "entering the body, running its statement, and coming back out",);
    assert_eq!(stops(&log).iter().find(|stop| stop.event == "exit").map(|stop| stop.detail.clone()), Some("2".to_string()), "leaving `inner` must report what it returned",);

    // The value a body ended with is the one thing about leaving it that both
    // engines can state exactly.
    let (walker_engine, walker) = recording(|_| DebuggerCommand::StepInto);
    walker_run(&walker_engine, CALLING);
    assert_eq!(stops(&walker).iter().find(|stop| stop.event == "exit").map(|stop| stop.detail.clone()), Some("2".to_string()), "the walker reports the same body's value",);
}

/// A `throw` out of a call, caught outside it, so the run still finishes.
const RAISING: &str = "\
fn boom() {
    throw 42
}
let a = 0;
try { a = boom(); } catch { a = 1; }
a
";

#[test]
fn leaving_a_call_by_raising_is_reported_as_an_error() {
    let (engine, log) = recording(|_| DebuggerCommand::StepInto);
    vm_run(&engine, RAISING);

    let raised: Vec<_> = stops(&log).iter().filter(|stop| stop.event == "raised").map(|stop| stop.detail.clone()).collect();

    assert_eq!(raised.len(), 1, "one call raised, so one such event: {:?}", stops(&log));
    assert!(raised[0].contains("42"), "the event must carry the error the caller sees, got {:?}", raised[0],);

    let (walker_engine, walker) = recording(|_| DebuggerCommand::StepInto);
    walker_run(&walker_engine, RAISING);
    assert!(stops(&walker).iter().any(|stop| stop.event == "raised" && stop.detail.contains("42")), "the walker reports the same error: {:?}", stops(&walker),);
}

#[test]
#[cfg(not(feature = "no_position"))]
fn a_function_exit_runs_the_rest_of_the_body() {
    // Step until the body's statement is reached, ask to leave, then let the
    // program finish. Keyed on where it is rather than on how many stops that
    // took, because the two engines do not take the same number.
    let reply = |stops: &[Stop]| {
        let inside = |stop: &Stop| stop.line == 2;
        match stops.last() {
            Some(stop) if inside(stop) => DebuggerCommand::FunctionExit,
            _ if stops.iter().any(inside) => DebuggerCommand::Continue,
            _ => DebuggerCommand::StepInto,
        }
    };

    let (engine, log) = recording(reply);
    vm_run(&engine, CALLING);

    assert_eq!(
        stops(&log).iter().find(|stop| stop.event == "exit").map(|stop| stop.detail.clone()),
        Some("2".to_string()),
        "asking to leave `inner` must stop again as it is left: {:?}",
        stops(&log),
    );

    let (walker_engine, walker) = recording(reply);
    walker_run(&walker_engine, CALLING);
    assert_eq!(stops(&walker).iter().find(|stop| stop.event == "exit").map(|stop| stop.detail.clone()), Some("2".to_string()), "the walker leaves it the same way",);
}

/// What a callback declares must not disturb the program it stopped.
///
/// A stop can now happen part way through a chunk, so a callback that pushes
/// onto the scope is a callback that moves every slot the rest of the chunk has
/// yet to declare. Rhai searches by name once that has happened; a chunk cannot,
/// so it takes the entry back off instead — see `rewind_after_stop`. The two
/// engines agree on the program's value for different reasons, and the injected
/// variable is the visible difference between them.
///
/// The run's last stop is the exception, and the callback here skips it: it
/// happens after the chunk, where there are no slots left to move, so what is
/// declared there is the caller's to keep as it is under Rhai.
#[test]
fn what_a_callback_declares_does_not_outlive_the_stop() {
    let mut engine = Engine::new();
    #[allow(deprecated)] // not deprecated but unstable
    engine.register_debugger(
        |_, dbg| dbg,
        |mut context, event, _, _, _| {
            if describe(event).0 != "end" && context.scope().get_value::<INT>("injected").is_none() {
                context.scope_mut().push("injected", 99 as INT);
            }
            Ok(DebuggerCommand::Continue)
        },
    );

    // Declarations after the first stop, so a slot that moved reads the
    // injected value instead of the variable it names.
    const DECLARING: &str = "\
let a = 1;
let b = 2;
a + b
";

    let mut ours = Scope::new();
    let value = Vm::new(&engine).eval_with_scope(&mut ours, &compiled(&engine, DECLARING)).expect("must run");

    let mut walked = Scope::new();
    let expected = walker_eval_with(&engine, &mut walked, DECLARING);

    assert_eq!(format!("{value:?}"), format!("{expected:?}"), "a stop moved the slots underneath the statements after it",);
    assert!(ours.get_value::<INT>("injected").is_none(), "the callback's variable outlived the stop, so the slots are its problem now",);
    assert!(walked.get_value::<INT>("injected").is_some(), "Rhai no longer keeps what a callback declares, so this is not the difference",);
}

/// A break-point on a *name* has nothing to match under the VM, and saying so
/// here is the point: it is the one kind of stop compiling away the tree costs.
///
/// Rhai matches these against the call node itself (`eval/debugger.rs:334`), and
/// the node a marker hands the callback is a synthetic `Noop` — a chunk keeps no
/// call expression to offer, and inventing one would hand a debugger arguments
/// the script never wrote. Position break-points cover the same line.
#[test]
fn a_break_point_on_a_function_name_cannot_fire_under_the_vm() {
    let point = || BreakPoint::AtFunctionName { name: "inner".into(), enabled: true };

    let (engine, log) = breaking(vec![point()], |_| DebuggerCommand::Continue);
    vm_run(&engine, CALLING);
    assert_eq!(events(&log), vec!["start", "end"], "a name break-point stopped the VM, so this limitation is stale",);

    let (walker_engine, walker) = breaking(vec![point()], |_| DebuggerCommand::Continue);
    walker_run(&walker_engine, CALLING);
    assert!(events(&walker).contains(&"break"), "the walker no longer stops on a name either, so this is not the VM's gap",);
}
