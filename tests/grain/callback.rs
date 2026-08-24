//! Natives that call a compiled function back.
//!
//! `[1, 2, 3].map(|x| x * 2)` is the shape: `map` is Rhai's, and the pointer it
//! is handed is resolved by Rhai's dispatch rather than by ours. This is where
//! the wrappers that make that resolve are held to the walker's behaviour, and
//! where the places it still diverges are pinned rather than left to be
//! discovered.

// Only the engine is wanted here; the corpus scripts belong to the harnesses
// that run all of them.
use super::corpus;

use rhai::grain::{Compiler, Vm};
use rhai::{Dynamic, Engine, Scope};

/// Run a source through the VM with the callback wrappers installed.
fn run(engine: &Engine, source: &str) -> Result<String, String> {
    let ast = engine.compile(source).map_err(|err| format!("{err:?}"))?;
    let program = Compiler::new().compile(&ast).into_shared();
    Vm::new(engine)
        .eval_with_callbacks(&mut Scope::new(), &program)
        .map(|value| format!("{value:?}"))
        .map_err(|err| format!("{err:?}"))
}

/// The same source through the walker, which is what the answer has to be.
fn walk(engine: &Engine, source: &str) -> Result<String, String> {
    let ast = engine.compile(source).map_err(|err| format!("{err:?}"))?;
    engine.eval_ast::<Dynamic>(&ast).map(|value| format!("{value:?}")).map_err(|err| format!("{err:?}"))
}

fn agree(source: &str) {
    let engine = corpus::engine();
    assert_eq!(walk(&engine, source), run(&engine, source), "{source}");
}

/// The array is bound to a variable in every case here, and that is not
/// incidental: a chain rooted at a literal is still a fragment, and a fragment
/// hands the whole expression back to the walker — which would make every one
/// of these pass without the wrappers existing at all.
fn lowered(source: &str) {
    let engine = corpus::engine();
    let ast = engine.compile(source).unwrap();
    let program = Compiler::new().compile(&ast);
    assert!(program.residual_count() == 0, "{source} still fragments, so it does not test the callback path",);
    assert!(program.makes_fn_pointers(), "{source}");
}

#[test]
fn a_native_can_call_a_closure_back() {
    lowered("let a = [1, 2, 3]; a.map(|x| x * 2)");
    agree("let a = [1, 2, 3]; a.map(|x| x * 2)");
    agree("let a = [1, 2, 3, 4]; a.filter(|x| x % 2 == 0)");
}

#[test]
fn a_native_can_call_a_named_function_back() {
    agree("fn double(x) { x * 2 } let a = [1, 2, 3]; a.map(Fn(\"double\"))");
}

/// A bare function name is a function pointer, not a variable read.
///
/// The compiler leaves it to Rhai — it is a fragment — and Rhai used to refuse
/// it, because a program holding any fragment sets `always_search_scope` and
/// the check for a function of that name sat behind the flag. Every spelling
/// below reported `double` as an unknown variable.
#[test]
fn a_bare_function_name_is_a_pointer() {
    agree("fn double(x) { x * 2 } let a = [1, 2, 3]; a.map(double)");
    agree("fn double(x) { x * 2 } let r = 0; { let f = double; r = f.call(4); } r");
    agree("fn double(x) { x * 2 } fn apply(f, v) { f.call(v) } apply(double, 4)");
    // A variable of the same name still wins over the function.
    agree("fn double(x) { x * 2 } let double = 7; double");
}

/// A capture arrives, and has the right value.
///
/// Multiplication commutes, so this says nothing about which *position* it
/// arrives in — see `a_capturing_closure_reaches_a_native_with_its_arguments_rotated`
/// for that, which is where the answer is unwelcome.
#[test]
#[cfg(not(feature = "no_closure"))]
fn a_capturing_closure_resolves_and_sees_its_capture() {
    lowered("let n = 10; let a = [1, 2, 3]; a.map(|x| x * n)");
    agree("let n = 10; let a = [1, 2, 3]; a.map(|x| x * n)");
}

/// A capturing closure handed to a native keeps its captured values ahead of
/// the bound `this` value.
#[test]
#[cfg(not(feature = "no_closure"))]
fn a_capturing_closure_reaches_a_native_with_its_arguments_in_order() {
    let engine = corpus::engine();
    let source = "let n = 10; let a = [1, 2, 3]; a.map(|x| n - x)";

    assert_eq!(walk(&engine, source), Ok("[9, 8, 7]".to_string()));
    assert_eq!(run(&engine, source), Ok("[9, 8, 7]".to_string()));
}

#[test]
#[cfg(not(feature = "unchecked"))]
fn a_callback_reaching_a_second_one_still_resolves() {
    // The limit is raised because reaching a chunk through Rhai's dispatch
    // costs more call levels than reaching a script function does, and the
    // default in a debug build is 8. `a_callback_costs_more_call_levels`
    // measures the difference; this is about resolution, not depth.
    let mut engine = corpus::engine();
    engine.set_max_call_levels(64);
    let source = "let a = [[1, 2], [3, 4]]; a.map(|row| row.map(|x| x + 1))";
    assert_eq!(walk(&engine, source), run(&engine, source));
}

#[test]
#[cfg(not(feature = "no_closure"))]
fn a_closure_called_back_sees_a_write_made_after_it_was_made() {
    agree("let n = 1; let f = |x| x * n; n = 10; let a = [1, 2, 3]; a.map(f)");
}

#[test]
fn an_error_inside_a_callback_still_arrives() {
    let engine = corpus::engine();
    let err = run(&engine, "let a = [1, 2, 3]; a.map(|x| throw x)").unwrap_err();
    assert!(err.contains("ErrorInFunctionCall"), "{err}");
}

/// Reaching a chunk through Rhai's dispatch spends more of the call budget
/// than reaching a script function does, so a callback nests less deeply before
/// `max_call_levels` stops it.
///
/// Measured rather than asserted at a number, because the number is a property
/// of Rhai's dispatch and would drift. What has to hold is the direction: we
/// are stricter, never laxer. A program that would have run out of budget on
/// the walker must not somehow keep going here.
#[test]
#[cfg(not(feature = "unchecked"))]
fn a_callback_costs_more_call_levels() {
    let deepest = |depth: usize, run: &dyn Fn(&Engine, &str) -> Result<String, String>| {
        // `depth` closures nested through `map`, each one a boundary, over an
        // array nested to match.
        let mut source = format!("let a = {}1{}; a", "[".repeat(depth), "]".repeat(depth));
        for level in 0..depth {
            source.push_str(&format!(".map(|v{level}| v{level}"));
        }
        source.push_str(" + 1");
        source.push_str(&")".repeat(depth));
        (1..=64)
            .find(|levels| {
                let mut engine = corpus::engine();
                engine.set_max_call_levels(*levels);
                run(&engine, &source).is_ok()
            })
            .unwrap_or(usize::MAX)
    };

    for depth in 1..=3 {
        let walker = deepest(depth, &walk);
        let vm = deepest(depth, &run);
        println!("{depth} nested callback(s): walker needs {walker}, we need {vm}");
        assert!(
            vm >= walker,
            "depth {depth}: we ran at {vm} levels where the walker needed {walker}, \
             so a budget the walker respects is not being enforced",
        );
    }
}

#[test]
fn without_the_wrappers_the_pointer_does_not_resolve() {
    let engine = corpus::engine();

    const SCRIPTS: &[&str] = &[
        "let a = [1, 2, 3]; a.map(|x| x * 2)",
        "let a = [1, 2, 3]; a.map(|| this * 2)",
        "let a = [1, 2, 3]; a.filter(|x| x % 2 == 0).map(|| this * 2)",
        "let a = [1, 2, 3]; a.reduce(|a, v| if a == () { v } else { a + v })",
        "let a = [1, 2, 3]; a.reduce(|a| if a == () { this } else { a + this })",
        "let s = 0; let a = [10, 20, 30]; a.for_each(|i| s += i); s",
        "let s = 0; let a = [10, 20, 30]; a.for_each(|| s += this); s",
    ];

    for source in SCRIPTS {
        // The AST walker and VM must agree.
        agree(source);

        // Now check that the program is a callback that requires wrappers.
        let ast = engine.compile(source).unwrap();
        let program = Compiler::new().compile(&ast);

        assert!(program.makes_fn_pointers(), "{source} must be a program a host is told to run with callbacks",);

        let err = Vm::new(&engine).eval_with_scope(&mut Scope::new(), &program).unwrap_err();
        assert!(format!("{err:?}").contains("ErrorFunctionNotFound"), "{source}: {err}");
    }
}
