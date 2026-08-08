//! Calling one compiled function, with the environment and receiver rhai gives
//! it.
//!
//! A program's library, source and module resolver used to be installed around
//! its *main chunk* only, so a function reached through `call_fn` ran without
//! them. That is not a corner: the compiler leaves anything it cannot lower as
//! an AST in the library, and rhai finds it only in `global.lib`.
//!
//! The other half is `this`. An event handler bound to its state through
//! `bind_this_ptr` is the common shape of a `call_fn` caller, and the whole of
//! what these check is that a `Vm` answers such a call the way an `Engine`
//! does — including that a write through `this` lands in the caller's value.

use rhai::grain::format::WriteError;
use rhai::grain::{Compiler, Vm};
use rhai::{CallFnOptions, Dynamic, Engine, EvalAltResult, Scope, INT};

/// A receiver for the methods below.
fn holder(count: INT) -> Dynamic {
    Dynamic::from_map([("count".into(), Dynamic::from(count))].into_iter().collect())
}

/// A bare `eval` statement is what keeps the callee an AST in the program's
/// library — it can declare into the caller's scope, which no slot model can
/// account for — and the library is the only thing that can answer the method
/// call below. In *expression* position it becomes a fragment instead, which
/// leaves the function compiled and would not exercise this at all.
///
/// It used to be `this` here. That stopped being unlowerable, which would have
/// left this test passing while checking nothing.
const PROGRAM: &str = r#"
    fn labelled() { eval("1"); 42 }
    fn outer(m) { m.labelled() }
"#;

/// The same shape, but the un-lowered function's name is also a rhai built-in
/// (`Dynamic::tag`). Without the library the call does not fail — it silently
/// resolves to the built-in getter and answers 0.
const SHADOWED: &str = r#"
    fn tag() { eval("1"); 42 }
    fn outer(m) { m.tag() }
"#;

fn walked(engine: &Engine, source: &str) -> INT {
    let ast = engine.compile(source).unwrap();
    engine.call_fn(&mut Scope::new(), &ast, "outer", (holder(7),)).unwrap()
}

fn run(engine: &Engine, source: &str) -> INT {
    let ast = engine.compile(source).unwrap();
    let program = Compiler::new().compile(&ast);
    // The premise: the callee really is one the compiler left behind, so the
    // library has to be installed for the call to resolve at all. Refusing to
    // write for *that* reason is the public way to see it.
    assert!(matches!(program.write(), Err(WriteError::HasScriptFunctions)), "{source:?} no longer keeps a library, so this would prove nothing (write said {:?})", program.write(),);
    Vm::new(engine).call_fn(&mut Scope::new(), &program, "outer", (holder(7),)).unwrap()
}

#[test]
fn a_call_reaches_a_function_the_compiler_left_to_rhai() {
    let engine = Engine::new();
    assert_eq!(walked(&engine, PROGRAM), 42);
    assert_eq!(run(&engine, PROGRAM), walked(&engine, PROGRAM));
}

#[test]
fn a_missing_library_cannot_be_answered_by_a_builtin_of_the_same_name() {
    let engine = Engine::new();
    assert_eq!(walked(&engine, SHADOWED), 42);
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
        .call_fn_with_options(CallFnOptions::new().rewind_scope(false), &mut scope, &program, "outer", ())
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
        .call_fn_with_options(CallFnOptions::new().eval_ast(false).rewind_scope(false), &mut scope, &program, "outer", ())
        .unwrap();

    assert!(scope.get_value::<INT>("started").is_none());
}

#[test]
fn an_error_inside_a_call_carries_the_program_source() {
    let engine = Engine::new();
    let mut ast = engine.compile(r#"fn outer() { throw "boom" }"#).unwrap();
    ast.set_source("handlers.rhai");
    let program = Compiler::new().compile(&ast);

    let err = *Vm::new(&engine).call_fn::<Dynamic>(&mut Scope::new(), &program, "outer", ()).unwrap_err();

    match err {
        EvalAltResult::ErrorInFunctionCall(name, source, ..) => {
            assert_eq!(name, "outer");
            assert_eq!(source, "handlers.rhai");
        }
        other => panic!("expected a wrapped call error, got {other:?}"),
    }
}

/// Both sides of one `call_fn` with a bound receiver: the value it answers, and
/// the caller's `Dynamic` afterwards.
fn bound(engine: &Engine, source: &str, name: &str, state: &mut Dynamic) -> Result<INT, String> {
    let ast = engine.compile(source).unwrap();
    let program = Compiler::new().compile(&ast);

    Vm::new(engine)
        .call_fn_with_options(CallFnOptions::new().bind_this_ptr(state), &mut Scope::new(), &program, name, (1 as INT,))
        .map_err(|err| format!("{err:?}"))
}

/// The same through the walker, for the comparison to mean anything.
fn walked_bound(engine: &Engine, source: &str, name: &str, state: &mut Dynamic) -> Result<INT, String> {
    let ast = engine.compile(source).unwrap();
    engine
        .call_fn_with_options(CallFnOptions::new().bind_this_ptr(state), &mut Scope::new(), &ast, name, (1 as INT,))
        .map_err(|err| format!("{err:?}"))
}

/// The shape this work exists for: a handler with its state on `this`.
#[test]
fn a_handler_bound_to_its_state_answers_as_the_walker_does() {
    let engine = Engine::new();
    const HANDLER: &str = "fn bump(n) { this.count += n; this.count }";

    let mut state = holder(1);
    let mut walked_state = holder(1);

    assert_eq!(bound(&engine, HANDLER, "bump", &mut state), Ok(2));
    assert_eq!(walked_bound(&engine, HANDLER, "bump", &mut walked_state), Ok(2));
    // And the write reached the caller's own `Dynamic`, on both sides.
    assert_eq!(format!("{state:?}"), format!("{walked_state:?}"));
    assert_eq!(format!("{state:?}"), r#"#{"count": 2}"#);
}

/// Rhai reaches `this` through the caller's storage, so a handler that writes
/// and then fails has already written.
#[test]
fn a_write_before_a_failure_reaches_the_caller() {
    let engine = Engine::new();
    const HANDLER: &str = r#"fn bump(n) { this.count += n; throw "boom" }"#;

    let mut state = holder(1);
    let mut walked_state = holder(1);

    assert!(bound(&engine, HANDLER, "bump", &mut state).is_err());
    assert!(walked_bound(&engine, HANDLER, "bump", &mut walked_state).is_err());

    assert_eq!(format!("{state:?}"), format!("{walked_state:?}"));
    assert_eq!(format!("{state:?}"), r#"#{"count": 2}"#);
}

/// A handler is a chunk now, so a program full of them is an artifact.
#[test]
fn a_program_of_handlers_is_writable() {
    let engine = Engine::new();
    let ast = engine.compile("fn on_open(n) { this.count += n } fn on_close() { this.count = 0 }").unwrap();
    let program = Compiler::new().compile(&ast);

    assert_eq!(program.residual_count(), 0);
    assert_eq!(program.functions().len(), 2);
    assert!(program.write().is_ok(), "got {:?}", program.write());
}
