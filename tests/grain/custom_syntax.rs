//! Custom syntax against the walker's own scope and control-flow rules.
//!
//! ## Notes on fragmenting
//!
//! A custom-syntax handler reaches the caller's scope through an
//! `EvalContext`, which is invisible to the slot model the same way `eval` is
//! -- that alone still forces a fragment. A `$block$`/`$expr$` input holding
//! a `return`, `break` or `continue` that escapes it, however, does *not*:
//! it is raised with `Op::BreakFlow` as a genuine
//! `EvalAltResult::Return`/`LoopBreak`, which propagates out of the input's
//! chunk and the handler. A site inside a loop traps `break`/`continue` and
//! jumps to the loop's exit; `return` propagates up to the function.

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

    // A program that lowers has to pass the verifier too.
    if let (true, Ok(artifact)) = (writable, program.write()) {
        Program::read(&artifact).unwrap_or_else(|err| panic!("{source:?} must verify: {err:?}"));
    }

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

/// `break`/`continue` inside a `$block$` input, caught by a loop *outside*
/// the block: the input raises a pseudo-error that the site traps and turns
/// into the loop's own exit.
#[test]
fn escaping_break_and_continue_lower() {
    let engine = engine_with_exec();

    for source in [
        r#"let x = 0; loop { x += 1; exec { if x > 2 { break; } } } x"#,
        r#"let count = 0; for i in 0..5 { exec { if i == 2 { continue; } } count += 1; } count"#,
        r#"let count = 0; let i = 0; while i < 6 { i += 1; exec { if i % 2 == 0 { continue; } } count += 1; } count"#,
        r#"let i = 0; do { i += 1; exec { if i == 3 { break; } } } while i < 10; i"#,
        // Nested loops: only the innermost is left.
        r#"let n = 0; for a in 0..3 { for b in 0..3 { exec { if b == 1 { break; } } n += 1; } n += 10; } n"#,
        // Not at the innermost level of the input either.
        r#"let n = 0; for a in 0..4 { exec { if a > 0 { if a < 3 { continue; } else { break; } } } n += 1; } n"#,
    ] {
        agree_with(&engine, source, |_| {}, true);
    }
}

/// A `break` value travels through the trap to the loop's result.
#[test]
fn escaping_break_with_a_value_lowers() {
    let engine = engine_with_exec();

    for source in [
        r#"let x = 0; let r = loop { x += 1; exec { if x > 2 { break x * 10; } } }; r"#,
        r#"let r = for i in 0..10 { exec { if i == 4 { break i; } } }; r"#,
        // The loop breaks with a value elsewhere, so a bare break supplies unit.
        r#"let r = loop { exec { if true { break; } } break 1; }; r"#,
        // The loop has no break value at all, so the trapped one is dropped.
        r#"let x = 0; loop { x += 1; exec { if x == 3 { break 99; } } if x > 5 { break; } } x"#,
    ] {
        agree_with(&engine, source, |_| {}, true);
    }
}

/// A trapped `break`/`continue` has to drop what the straight-line path would
/// have: a `for` iterator, an armed `try`, a `switch` subject.
#[test]
fn escaping_break_cleans_up_what_it_skips() {
    let engine = engine_with_exec();

    for source in [
        r#"let n = 0; for a in 0..3 { for b in 0..3 { try { exec { if b == 1 { continue; } } n += 1; } catch { n += 100; } } } n"#,
        r#"let n = 0; for a in 0..3 { try { exec { if a == 1 { break; } } n += 1; } catch { n += 100; } } try { throw 1; } catch (e) { n += e; } n"#,
        r#"let n = 0; for a in 0..4 { switch a { 1 => exec { continue; }, 3 => exec { break; }, _ => n += 1 } } n"#,
        r#"let r = loop { switch 1 { 1 => exec { break 5; }, _ => () } }; r"#,
        r#"let n = 0; for a in 0..4 { let k = a; exec { let z = 1; if k == 2 { break; } } n += k; } n"#,
    ] {
        agree_with(&engine, source, |_| {}, true);
    }
}

/// `return` inside a `$block$` input unwinds past the site as an error, which
/// the function (or the script) turns back into a value.
#[test]
#[cfg(not(feature = "no_function"))]
fn escaping_return_lowers() {
    let engine = engine_with_exec();

    for source in [
        r#"fn f() { exec { return 42; } 1 } f()"#,
        r#"fn f() { exec { for i in 0..3 { if i == 1 { return i; } } 99 } } f()"#,
        r#"fn f() { for i in 0..5 { exec { if i == 3 { return i * 2; } } } -1 } f()"#,
        r#"fn f() { 1 + exec { return 7; } } f() + 100"#,
        r#"fn f() { try { exec { return 7; } } catch { return 0; } 1 } f()"#,
        r#"exec { return 42; } 1"#,
        r#"let x = 1; exec { exec { return x + 1; } } x = 99; x"#,
    ] {
        let ast = engine.compile(source).expect("must compile");
        let program = Compiler::new().compile(&ast);
        if source.starts_with("fn ") {
            assert_eq!(program.num_functions(), 1, "{source:?} must lower `f`");
        }

        agree_with(&engine, source, |_| {}, true);
    }
}

/// An error that escaped one fault must not leave a stale entry that a later,
/// unrelated error then reports.
#[test]
#[cfg(not(feature = "no_function"))]
fn escaping_return_leaves_no_stale_fault() {
    let engine = engine_with_exec();
    agree_with(&engine, r#"fn f() { exec { return 1; } } let a = f(); throw a"#, |_| {}, true);
}

/// Operands already pushed for the expression around the site are still on
/// the stack when the pseudo-error arrives, so the trap stub drops them along
/// with whatever else the loop exit drops.
#[test]
fn escaping_break_with_operands_in_flight() {
    let engine = engine_with_exec();

    for source in [
        r#"let n = 0; for i in 0..4 { n += 1 + exec { if i == 2 { break; } 5 }; } n"#,
        r#"let n = 0; for i in 0..4 { n += max(10, exec { if i == 2 { continue; } i }); } n"#,
        r#"let n = 0; for i in 0..4 { n += 1 + exec { 5 }; } n"#,
        r#"let n = 0; for i in 0..3 { for j in 0..3 { n += 1 + exec { if j == 1 { continue; } if i == 2 { break; } 1 }; } } n"#,
        r#"let n = 0; for i in 0..4 { n += 1 + switch i { 2 => exec { continue; }, _ => 1 }; } n"#,
        r#"let n = 0; for i in 0..4 { n += 2 * (1 + { if i == 2 { break; } 5 }); } n"#,
        r#"1 + exec { 5 }"#,
    ] {
        agree_with(&engine, source, |_| {}, true);
    }
}

/// The same with operands held by array, map and index lowering.
#[test]
#[cfg(not(feature = "no_index"))]
#[cfg(not(feature = "no_object"))]
fn escaping_break_with_container_operands_in_flight() {
    let engine = engine_with_exec();

    for source in [
        r#"let r = loop { let x = [1, 2, exec { break 9; }]; }; r"#,
        r#"let n = 0; for i in 0..4 { let m = #{ a: 1, b: exec { if i == 1 { continue; } 2 }, c: 3 }; n += m.b; } n"#,
        r#"let n = 0; for i in 0..4 { let s = `a${1}b${exec { if i == 2 { break; } i }}c`; n += s.len; } n"#,
        r#"let n = 0; for i in 0..4 { let a = [10, 20, 30]; n += a[exec { if i == 2 { break; } 1 }]; } n"#,
        r#"let n = 0; let a = [1, 2, 3]; for i in 0..4 { a[exec { if i == 3 { break; } 0 }] += i; n += 1; } n + a[0]"#,
    ] {
        agree_with(&engine, source, |_| {}, true);
    }
}

/// Nested custom syntax: the pseudo-error crosses two sites.
#[test]
fn escaping_jump_crosses_nested_sites() {
    let engine = engine_with_exec();
    agree_with(&engine, r#"let n = 0; for i in 0..5 { exec { exec { if i == 3 { break; } } n += 1; } } n"#, |_| {}, true);
    agree_with(&engine, r#"let n = 0; for i in 0..5 { exec { exec { if i % 2 == 0 { continue; } } n += 1; } } n"#, |_| {}, true);
}

/// A custom syntax whose handler itself fails with a `break`, `continue` or
/// `return` pseudo-error, with no such statement anywhere in its inputs.
#[test]
fn handler_raising_a_pseudo_error_is_trapped() {
    use rhai::{EvalAltResult, Position};

    let mut engine = corpus::engine();
    engine
        .register_custom_syntax(["stop", "$expr$"], false, |context, inputs| {
            let value = context.eval_expression_tree(&inputs[0])?;
            Err(EvalAltResult::LoopBreak(true, value, Position::NONE).into())
        })
        .unwrap();
    engine
        .register_custom_syntax(["skip"], false, |_, _| Err(EvalAltResult::LoopBreak(false, Dynamic::UNIT, Position::NONE).into()))
        .unwrap();
    engine
        .register_custom_syntax(["leave", "$expr$"], false, |context, inputs| {
            let value = context.eval_expression_tree(&inputs[0])?;
            Err(EvalAltResult::Return(value, Position::NONE).into())
        })
        .unwrap();

    agree_with(&engine, r#"let r = loop { stop 7 }; r"#, |_| {}, true);
    agree_with(&engine, r#"let n = 0; for i in 0..5 { if i == 2 { skip } n += 1; } n"#, |_| {}, true);
    #[cfg(not(feature = "no_function"))]
    agree_with(&engine, r#"fn f() { for i in 0..5 { if i == 2 { leave i * 3 } } 0 } f()"#, |_| {}, true);
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

/// The trap address of a site inside a loop is verified and survives a
/// `write`/`read` round trip.
#[test]
fn trapping_site_survives_a_write_read_round_trip() {
    let engine = engine_with_exec();
    let ast = engine
        .compile(r#"let n = 0; for i in 0..9 { exec { if i == 3 { break; } if i == 1 { continue; } } n += 1; } n"#)
        .expect("must compile");
    let program = Compiler::new().compile(&ast);
    assert_eq!(program.residual_count(), 0, "must lower, got {:?}", program.first_unsupported());

    let artifact = program.write().expect("should be writable");
    let loaded = Program::read(&artifact).expect("artifact should load and verify");
    let value = Vm::new(&engine).eval_with_scope(&mut Scope::new(), &loaded).expect("loaded program should run");
    assert_eq!(value.cast::<INT>(), 2);
}

/// Two custom-syntax states of a custom type render the same (as the type's
/// name) but differ: each site keeps its own state rather than sharing one.
#[test]
fn custom_type_states_stay_apart() {
    #[derive(Clone)]
    struct Tag(INT);

    let mut engine = corpus::engine();
    engine.register_custom_syntax_with_state_raw(
        "tag",
        |stream, _look_ahead, state| match stream.len() {
            1 => Ok(Some("$int$".into())),
            _ => {
                *state = Dynamic::from(Tag(stream[1].parse().unwrap()));
                Ok(None)
            }
        },
        false,
        |_context, _inputs, state| Ok(Dynamic::from(state.clone_cast::<Tag>().0)),
    );

    agree_with(&engine, r#"let a = tag 1; let b = tag 2; a * 10 + b"#, |_| {}, true);
}
