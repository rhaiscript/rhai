//! What a program contributes to `global` while [`Vm::call_fn`] runs.
//!
//! A program's library, source and module resolver used to be installed around
//! its *main chunk* only, so a function reached through `call_fn` ran without
//! them. That is not a corner: the compiler leaves anything it cannot lower as
//! an AST in the library, and rhai finds it only in `global.lib`.

use rhai::grain::{Compiler, Vm};
use rhai::{CallFnOptions, Dynamic, Engine, EvalAltResult, Scope, INT};

/// A receiver for the methods below.
fn holder(count: INT) -> Dynamic {
    Dynamic::from_map(
        [("count".into(), Dynamic::from(count))]
            .into_iter()
            .collect(),
    )
}

/// `this` is what keeps `inner` an AST in the program's library, which is the
/// only way to observe from outside whether the library is installed.
const PROGRAM: &str = "
    fn labelled() { this.count }
    fn outer(m) { m.labelled() }
";

/// The same shape, but the un-lowered function's name is also a rhai built-in
/// (`Dynamic::tag`). Without the library the call does not fail — it silently
/// resolves to the built-in getter and answers 0.
const SHADOWED: &str = "
    fn tag() { this.count }
    fn outer(m) { m.tag() }
";

fn walked(engine: &Engine, source: &str) -> INT {
    let ast = engine.compile(source).unwrap();
    engine
        .call_fn(&mut Scope::new(), &ast, "outer", (holder(7),))
        .unwrap()
}

fn run(engine: &Engine, source: &str) -> INT {
    let ast = engine.compile(source).unwrap();
    let program = Compiler::new().compile(&ast);
    Vm::new(engine)
        .call_fn(&mut Scope::new(), &program, "outer", (holder(7),))
        .unwrap()
}

#[test]
fn a_call_reaches_a_function_the_compiler_left_to_rhai() {
    let engine = Engine::new();
    assert_eq!(walked(&engine, PROGRAM), 7);
    assert_eq!(run(&engine, PROGRAM), walked(&engine, PROGRAM));
}

#[test]
fn a_missing_library_cannot_be_answered_by_a_builtin_of_the_same_name() {
    let engine = Engine::new();
    assert_eq!(walked(&engine, SHADOWED), 7);
    assert_eq!(run(&engine, SHADOWED), walked(&engine, SHADOWED));
}

/// The environment is now installed once around both halves of the call, so the
/// half that used to have it must not have lost it.
#[test]
fn the_main_chunk_still_runs_before_the_call() {
    let engine = Engine::new();
    let ast = engine.compile("fn outer() { 1 } let started = 9;").unwrap();
    let program = Compiler::new().compile(&ast);

    let mut scope = Scope::new();
    let value: INT = Vm::new(&engine)
        .call_fn_with_options(
            CallFnOptions::new().rewind_scope(false),
            &mut scope,
            &program,
            "outer",
            (),
        )
        .unwrap();

    assert_eq!(value, 1);
    // What the main chunk declared is left in the caller's scope, as
    // `Engine::eval_ast_with_scope` would leave it.
    assert_eq!(scope.get_value::<INT>("started"), Some(9));
}

#[test]
fn eval_ast_off_skips_the_main_chunk() {
    let engine = Engine::new();
    let ast = engine.compile("fn outer() { 1 } let started = 9;").unwrap();
    let program = Compiler::new().compile(&ast);

    let mut scope = Scope::new();
    let _: INT = Vm::new(&engine)
        .call_fn_with_options(
            CallFnOptions::new().eval_ast(false).rewind_scope(false),
            &mut scope,
            &program,
            "outer",
            (),
        )
        .unwrap();

    assert!(scope.get_value::<INT>("started").is_none());
}

#[test]
fn an_error_inside_a_call_carries_the_program_source() {
    let engine = Engine::new();
    let mut ast = engine.compile(r#"fn outer() { throw "boom" }"#).unwrap();
    ast.set_source("handlers.rhai");
    let program = Compiler::new().compile(&ast);

    let err = *Vm::new(&engine)
        .call_fn::<Dynamic>(&mut Scope::new(), &program, "outer", ())
        .unwrap_err();

    match err {
        EvalAltResult::ErrorInFunctionCall(name, source, ..) => {
            assert_eq!(name, "outer");
            assert_eq!(source, "handlers.rhai");
        }
        other => panic!("expected a wrapped call error, got {other:?}"),
    }
}
