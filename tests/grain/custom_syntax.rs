//! Custom syntax against the walker's own scope and control-flow rules.
//!
//! ## Notes on fragmenting
//!
//! A custom-syntax handler reaches the caller's scope through an
//! `EvalContext`, which is invisible to the slot model the same way `eval` is
//! -- that alone still forces a fragment. A `$block$`/`$expr$` input holding
//! a `return`, `break` or `continue` that escapes it, however, does *not*:
//! it runs, at call time, on its own separate `Vm`, so an escaping jump
//! lowers to a genuine `EvalAltResult::Return`/`LoopBreak` (a `PseudoJump`
//! marker underneath `Op::Return`) that propagates back out through the
//! ordinary error-unwinding path, rather than to a direct jump (which would
//! have no valid target in the input's own chunk) or to a fragment.

// Only the engine is wanted here; the corpus scripts belong to the harnesses
// that run all of them.
use super::corpus;

use rhai::grain::{Compiler, Program, Vm};
use rhai::{Dynamic, Engine, Scope, INT};

/// What a run produced, in a form two runs can be compared on.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    result: Result<String, String>,
    scope: Vec<(String, String)>,
}

fn capture(scope: &Scope, result: Result<Dynamic, Box<rhai::EvalAltResult>>) -> Outcome {
    Outcome {
        result: result.map(|value| format!("{value:?}")).map_err(|err| format!("{err:?}")),
        scope: scope.iter_raw().map(|(name, _, value)| (name.to_string(), format!("{value:?}"))).collect(),
    }
}

/// Run `source` under Rhai and under the VM, against an engine the caller has
/// set up (with the custom syntax the case needs registered), and require
/// they agree on the value, the error and what the scope holds after.
///
/// `writable` is the point of the exercise rather than a detail: a program
/// that still fragments cannot cross a wire, so a case that passes while
/// fragmenting has proved nothing about the feature.
#[track_caller]
fn agree_with(engine: &Engine, source: &str, build: impl Fn(&mut Scope), writable: bool) {
    let ast = engine.compile(source).expect("must compile");
    let program = Compiler::new().compile(&ast);

    assert_eq!(program.residual_count() == 0, writable, "{source:?} fragments: {:?}", program.first_unsupported(),);

    let mut walked = Scope::new();
    build(&mut walked);
    let expected = capture(&walked.clone(), engine.eval_ast_with_scope::<Dynamic>(&mut walked, &ast));
    let expected = Outcome {
        scope: capture(&walked, Ok(Dynamic::UNIT)).scope,
        ..expected
    };

    let mut run = Scope::new();
    build(&mut run);
    let actual = {
        let result = Vm::new(engine).eval_with_scope(&mut run, &program);
        capture(&run, result)
    };

    assert_eq!(actual, expected, "{source:?}");
}

/// The same as `eval`'s `let x` outliving it: what a custom syntax declares
/// into the caller's scope through `EvalContext::scope_mut` outlives it too,
/// which is likewise invisible to the slot model.
#[test]
fn custom_syntax_keeps_the_walkers_answer() {
    let mut engine = corpus::engine();
    engine
        .register_custom_syntax(["declare", "$ident$", "=", "$int$"], true, |context, inputs| {
            let name = inputs[0].get_string_value().unwrap().to_string();
            let value = inputs[1].get_literal_value::<INT>().unwrap();
            context.scope_mut().push(name, value);
            Ok(Dynamic::UNIT)
        })
        .expect("the custom syntax must register");

    for source in [r#"declare foo = 41; foo + 1"#, r#"declare bar = 5; 10"#] {
        agree_with(&engine, source, |_| {}, false);
    }
}

#[test]
fn lowerable_custom_syntax_runs_as_grain() {
    let mut engine = corpus::engine();
    engine
        .register_custom_syntax(["twice", "$expr$"], false, |context, inputs| {
            let value = context.eval_expression_tree(&inputs[0])?.cast::<INT>();
            Ok(Dynamic::from(value * 2))
        })
        .expect("the custom syntax must register");
    engine
        .register_custom_syntax(["literal", "$int$"], false, |_context, inputs| Ok(Dynamic::from(inputs[0].get_literal_value::<INT>().expect("literal input"))))
        .expect("the literal custom syntax must register");

    agree_with(&engine, r#"twice 21"#, |_| {}, true);
    agree_with(&engine, r#"literal 21"#, |_| {}, true);

    let ast = engine.compile(r#"literal 21"#).expect("must compile");
    let program = Compiler::new().compile(&ast);
    let artifact = program.write().expect("custom syntax should be writable");
    let loaded = Program::read(&artifact).expect("custom syntax artifact should load");
    let value = Vm::new(&engine).eval_with_scope(&mut Scope::new(), &loaded).expect("loaded custom syntax should run");
    assert_eq!(value.cast::<INT>(), 21);
}

/// An engine with an `exec $block$` custom syntax registered: it evaluates its
/// block input and hands back whatever value the block produced. Used by the
/// tests below to exercise a `$block$` input specifically, rather than the
/// `$expr$` one [`lowerable_custom_syntax_runs_as_grain`] already covers.
fn engine_with_exec() -> Engine {
    let mut engine = corpus::engine();
    engine
        .register_custom_syntax(["exec", "$block$"], false, |context, inputs| context.eval_expression_tree(&inputs[0]))
        .expect("the exec custom syntax must register");
    engine
}

#[test]
fn block_custom_syntax_input_lowers() {
    let engine = engine_with_exec();
    agree_with(&engine, r#"exec { let a = 1; let b = 2; a + b }"#, |_| {}, true);
}

/// A custom syntax used inside a function body, so its input lowers against a
/// non-zero slot base rather than the program's own (always-zero) one.
#[test]
#[cfg(not(feature = "no_function"))]
fn custom_syntax_inside_a_function_lowers() {
    let engine = engine_with_exec();
    agree_with(&engine, r#"fn f() { let z = 5; exec { z + 1 } } f()"#, |_| {}, true);
}

/// `this` read from inside a custom-syntax input.
#[test]
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_object"))]
fn this_inside_custom_syntax_input_lowers() {
    let engine = engine_with_exec();
    agree_with(&engine, r#"fn "i64".bump() { exec { this + 1 } } 41.bump()"#, |_| {}, true);
}

/// One custom syntax's `$block$` input containing another use of the same
/// custom syntax.
#[test]
fn nested_custom_syntax_lowers() {
    let engine = engine_with_exec();
    agree_with(&engine, r#"exec { exec { 5 } + 1 }"#, |_| {}, true);
}

/// `break`/`continue` inside a loop that is itself entirely inside the
/// `$block$` input targets that loop's own exit -- a jump inside the same
/// chunk -- so lowering still succeeds.
#[test]
fn break_continue_caught_by_a_loop_inside_the_block_still_lowers() {
    let engine = engine_with_exec();
    agree_with(&engine, r#"exec { let s = 0; for i in 0..5 { if i > 2 { break; } if i == 1 { continue; } s += i; } s }"#, |_| {}, true);
}

/// `break` inside a `$block$` input, but caught by a loop *outside* the
/// block, has no valid jump target inside the input's own chunk: the chunk
/// runs independently of the surrounding loop. Grain has to fragment this
/// custom syntax rather than lower it, and the fragmented (walker) result is
/// what has to come out.
#[test]
fn escaping_break_forces_a_fragment() {
    let engine = engine_with_exec();
    agree_with(&engine, r#"let x = 0; loop { x += 1; exec { if x > 2 { break; } } } x"#, |_| {}, false);
}

/// The `continue` counterpart of [`escaping_break_forces_a_fragment`].
#[test]
fn escaping_continue_forces_a_fragment() {
    let engine = engine_with_exec();
    agree_with(&engine, r#"let count = 0; for i in 0..5 { exec { if i == 2 { continue; } } count += 1; } count"#, |_| {}, false);
}

/// `return` inside a `$block$` input always escapes: it unwinds past the
/// function the custom syntax is used in, not just whatever loop happens to
/// enclose it, so no amount of loop nesting inside the block can catch it the
/// way `break`/`continue` can. A `return` this cannot lower around forces the
/// *whole enclosing function* to fall back to the walker -- unlike a
/// top-level escape, which only turns the custom-syntax expression itself
/// into a residual -- so this checks [`Program::num_functions`] rather than
/// [`Program::residual_count`]: the function that contains the escaping
/// `return` must not appear among the compiled functions at all.
#[test]
#[cfg(not(feature = "no_function"))]
fn escaping_return_forces_a_fragment() {
    let engine = engine_with_exec();

    for source in [
        r#"fn f() { exec { return 42; } 1 } f()"#,
        // A `return` still forces a fragment even when every loop between it
        // and the top of the block already catches its own `break`/`continue`
        // -- the presence of a loop does not mask `return`, only
        // `break`/`continue`.
        r#"fn f() { exec { for i in 0..3 { if i == 1 { return i; } } 99 } } f()"#,
    ] {
        let ast = engine.compile(source).expect("must compile");
        let program = Compiler::new().compile(&ast);
        assert_eq!(program.num_functions(), 0, "{source:?} must skip lowering `f`, not compile it");

        // Even with `f` skipped, calling it still has to agree with the walker.
        agree_with(&engine, source, |_| {}, true);
    }
}

/// A `write`/`read` round trip for a program containing a custom syntax with
/// a `$block$` input, exercising the verifier on the input chunk the same as
/// [`lowerable_custom_syntax_runs_as_grain`] does for a plain `$expr$` one.
#[test]
fn block_custom_syntax_survives_a_write_read_round_trip() {
    let engine = engine_with_exec();
    let ast = engine.compile(r#"exec { 20 + 1 }"#).expect("must compile");
    let program = Compiler::new().compile(&ast);
    assert_eq!(program.residual_count(), 0, "must lower, got {:?}", program.first_unsupported());

    let artifact = program.write().expect("custom syntax with a block input should be writable");
    let loaded = Program::read(&artifact).expect("custom syntax artifact should load and verify");
    let value = Vm::new(&engine).eval_with_scope(&mut Scope::new(), &loaded).expect("loaded custom syntax should run");
    assert_eq!(value.cast::<INT>(), 21);
}
