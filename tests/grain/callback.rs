//! Natives that call a compiled function back.
//!
//! `[1, 2, 3].map(|x| x * 2)` is the shape: `map` is rhai's, and the pointer it
//! is handed is resolved by rhai's dispatch rather than by ours. This is where
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
        .run_with_callbacks(&program, &mut Scope::new())
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

/// A capturing closure handed to a native that binds `this` gets its captured
/// values *after* the element instead of before.
///
/// Not our arithmetic: it is which shape rhai tries first. A capture is a
/// curried value, and `_call_with_extra_args` (`types/fn_ptr.rs:573`) opens
/// with `[this] ++ curry ++ args` — the one order that is never right. Rhai's
/// own closures escape it because they are `Fn*` pointers with the body
/// attached, which is caught two branches earlier at `:538` and rearranged
/// into `curry ++ [this] ++ args`.
///
/// Ours are `Fn` pointers resolved by name, so rhai reaches them as *natives*
/// and takes the first shape. The proof that this is rhai's behaviour rather
/// than ours is `stock_rhai_does_the_same_to_its_own_native_pointers` below,
/// which reproduces it with no rhaigrain in the picture at all.
///
/// So: safe for a closure that captures nothing, and for one whose parameters
/// commute. Wrong, silently, otherwise. Non-commutative on purpose here —
/// `n - x` and `x - n` are the same two values in the other order, and only
/// arithmetic that cares can tell them apart.
#[test]
#[cfg(not(feature = "no_closure"))]
fn a_capturing_closure_reaches_a_native_with_its_arguments_rotated() {
    let engine = corpus::engine();
    let source = "let n = 10; let a = [1, 2, 3]; a.map(|x| n - x)";

    assert_eq!(walk(&engine, source), Ok("[9, 8, 7]".to_string()));
    assert_eq!(run(&engine, source), Ok("[-9, -8, -7]".to_string()));
}

/// The same rotation, with nothing of ours involved.
///
/// `Fn(s)` on a computed name is the one spelling that gets a name-only
/// pointer out of stock rhai, which is what every pointer we make is. Curry it,
/// point it at a native, and hand it to `map`: rhai passes the element first
/// and the curried value second, while `f.call(1)` on the very same pointer
/// passes them the other way round.
#[test]
fn stock_rhai_does_the_same_to_its_own_native_pointers() {
    let mut engine = corpus::engine();
    engine.register_fn("nsub", |a: i64, b: i64| a - b);

    let curried = "let s = \"ns\" + \"ub\"; let f = Fn(s).curry(10);";
    assert_eq!(walk(&engine, &format!("{curried} [1, 2, 3].map(f)")), Ok("[-9, -8, -7]".to_string()), "rhai puts the element before the curried value",);
    assert_eq!(walk(&engine, &format!("{curried} f.call(1)")), Ok("9".to_string()), "and the curried value first when there is no element",);
}

#[test]
fn a_callback_reaching_a_second_one_still_resolves() {
    // The limit is raised because reaching a chunk through rhai's dispatch
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

/// Reaching a chunk through rhai's dispatch spends more of the call budget
/// than reaching a script function does, so a callback nests less deeply before
/// `max_call_levels` stops it.
///
/// Measured rather than asserted at a number, because the number is a property
/// of rhai's dispatch and would drift. What has to hold is the direction: we
/// are stricter, never laxer. A program that would have run out of budget on
/// the walker must not somehow keep going here.
#[test]
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
    // The whole reason `run_with_callbacks` exists. A plain `run` leaves rhai
    // nowhere to look, and the failure is a lookup failure rather than
    // anything worse.
    let engine = corpus::engine();
    let source = "let a = [1, 2, 3]; a.map(|x| x * 2)";
    let ast = engine.compile(source).unwrap();
    let program = Compiler::new().compile(&ast);

    let err = Vm::new(&engine).run(&program, &mut Scope::new()).unwrap_err();
    assert!(format!("{err:?}").contains("ErrorFunctionNotFound"), "{err}");
}
