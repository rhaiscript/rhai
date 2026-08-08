//! Variables the caller already had, which no slot can name.
//!
//! Slots are indices into the caller's `Scope` counted from a base taken when
//! the program starts, so everything the host put there beforehand sits below
//! every slot. Those reads and writes go by name instead, and this is where
//! that is held to rhai's behaviour — including the three places rhai looks
//! and the order it looks in.
//!
//! It matters more than the node count suggests: a script that reads anything
//! its host supplied used to be a fragment, and a program with a fragment in
//! it cannot be written to an artifact at all.

// `on_var` carries rhai's "volatile, may change" marker rather than a real
// deprecation — the same one `eval_expression_tree_raw` carries. Registering a
// resolver is the only way to test that the VM consults one.
#![allow(deprecated)]

// Only the engine is wanted here; the corpus scripts belong to the harnesses
// that run all of them.
use super::corpus;

use rhai::grain::{Compiler, Vm};
use rhai::{Dynamic, Engine, Module, Scope};

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

/// Run `source` under rhai and under the VM, from the same starting scope, and
/// require they agree on the value, the error and what the scope holds after.
///
/// `writable` is the point of the exercise rather than a detail: a program
/// that still fragments cannot cross a wire, so a case that passes while
/// fragmenting has proved nothing about the feature.
#[track_caller]
fn agree(source: &str, build: impl Fn(&mut Scope), writable: bool) {
    agree_with(&corpus::engine(), source, build, writable);
}

/// The same, against an engine the caller has set up — a variable resolver, a
/// published module. Both change what a name resolves to, which is most of what
/// this file is about.
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
        let result = Vm::new(engine).run(&program, &mut run);
        capture(&run, result)
    };

    assert_eq!(actual, expected, "{source:?}");
}

fn lit(value: i64) -> Dynamic {
    Dynamic::from(value)
}

#[test]
fn a_caller_variable_can_be_read() {
    agree(
        "brightness * 2",
        |s| {
            s.push("brightness", 21_i64);
        },
        true,
    );
    agree(
        "mode + \"!\"",
        |s| {
            s.push("mode", "chase".to_string());
        },
        true,
    );
}

#[test]
fn a_caller_variable_can_be_written() {
    agree(
        "brightness = 7; brightness",
        |s| {
            s.push("brightness", 1_i64);
        },
        true,
    );
    agree(
        "brightness += 5; brightness",
        |s| {
            s.push("brightness", 1_i64);
        },
        true,
    );
    // The op-assignment expansion path: no `-=` for strings, so rhai falls
    // back to `x = x - y` and fails there rather than reporting no `-=`.
    agree(
        "mode += \"!\"; mode",
        |s| {
            s.push("mode", "go".to_string());
        },
        true,
    );
}

/// Rhai's method-call rewrite reaches a caller's variable too.
///
/// `f(x, ..)` means `x.f(..)`, so a `&mut` first parameter mutates the
/// variable. A local is addressed by slot and one the caller supplied is not,
/// but the rule is the same for both — which is the point, since whether a
/// script's variable is a local is not something the script says.
#[test]
fn a_caller_variable_in_first_argument_position_is_taken_by_reference() {
    agree(
        "push(log, 2); log",
        |s| {
            s.push("log", vec![Dynamic::from(1_i64)]);
        },
        true,
    );
    agree(
        "bump(w); w.level",
        |s| {
            s.push("w", corpus::Widget::default());
        },
        true,
    );
    // A constant is not a place rhai will hand out, so the mutation is
    // discarded — and the caller's entry has to come back untouched.
    agree(
        "push(log, 2); log",
        |s| {
            s.push_constant("log", vec![Dynamic::from(1_i64)]);
        },
        true,
    );
    // Read after the other arguments, so it is the second name reported
    // missing rather than the first.
    agree("nosuch(gone, missing)", |_| {}, true);
    agree(
        "nosuch(gone, brightness)",
        |s| {
            s.push("brightness", 1_i64);
        },
        true,
    );
}

/// A chain rooted at a caller's variable, which is the last root shape that
/// did not lower.
///
/// Whether such a root can be written through is not known until it is looked
/// up, and rhai decides at the same moment and the same way: `search_namespace`
/// hands back a `Target`, and a scope entry becomes a reference where anything
/// else becomes a read-only value (`eval/expr.rs:120-155`). One case per arm of
/// that, because each fails differently.
#[test]
fn a_chain_can_be_rooted_at_a_caller_variable() {
    let array = || vec![Dynamic::from(1_i64)];

    // A writable entry: read, mutate through a method, and assign through.
    agree(
        "host[0]",
        |s| {
            s.push("host", array());
        },
        true,
    );
    agree(
        "host.push(2); host",
        |s| {
            s.push("host", array());
        },
        true,
    );
    agree(
        "host[0] = 9; host",
        |s| {
            s.push("host", array());
        },
        true,
    );
    agree(
        "host.level",
        |s| {
            s.push("host", corpus::Widget::default());
        },
        true,
    );
    agree(
        "host.level = 3; host.level",
        |s| {
            s.push("host", corpus::Widget::default());
        },
        true,
    );

    // A constant is not a place. Rhai refuses the assignment outright and
    // refuses a mutating method too, because it never hands out a reference to
    // one and a non-pure native will not take a read-only first argument
    // (`func/call.rs:405`).
    agree(
        "host[0] = 9; host",
        |s| {
            s.push_constant("host", array());
        },
        true,
    );
    agree(
        "host.push(2); host",
        |s| {
            s.push_constant("host", array());
        },
        true,
    );
    agree(
        "host[0]",
        |s| {
            s.push_constant("host", array());
        },
        true,
    );

    // A shared entry walks through its cell's guard, so the mutation lands
    // where every holder of the cell can see it. The closure is made in a
    // block so the compared scope does not end up holding a pointer, which the
    // two sides render differently on purpose.
    agree(
        "{ let keep = || host.len(); } host.push(2); host",
        |s| {
            s.push("host", array());
        },
        true,
    );

    // No entry at all, reported against the variable rather than the chain.
    agree("nowhere[0]", |_| {}, true);
    agree("nowhere.push(1)", |_| {}, true);
}

/// The other two things a name can resolve to, neither of which is a place.
#[test]
fn a_chain_rooted_at_a_resolved_name_cannot_be_written_through() {
    let mut engine = corpus::engine();
    engine.on_var(|name, _, _| {
        Ok(match name {
            "injected" => Some(Dynamic::from(vec![Dynamic::from(7_i64)])),
            _ => None,
        })
    });

    let mut module = rhai::Module::new();
    module.set_var("published", vec![Dynamic::from(5_i64)]);
    engine.register_global_module(module.into());

    for source in [
        // A resolver's answer is read-only, so both the write and the mutating
        // method are refused.
        "injected[0]",
        "injected[0] = 9; injected",
        "injected.push(2); injected",
        // A module's constant is not, so both go into a copy and are discarded.
        // The two constants are not the same constant, which is the thing worth
        // pinning here (`eval/expr.rs:151` against `:122`).
        "published[0]",
        "published[0] = 9",
        "published.push(2)",
    ] {
        agree_with(&engine, source, |_| {}, true);
    }
}

/// A closure can capture a variable the caller supplied, and capturing it means
/// binding the cell rather than a copy of what is in it.
///
/// Both halves of that were wrong. Sharing found the entry by walking
/// `Scope::iter_raw`, which runs from the top down, and used the position as if
/// it counted from the bottom — so with more than one caller entry it shared
/// the wrong variable outright. And the read that binds the capture went
/// through the flattening one, so even the right variable was captured by
/// value: a write afterwards was invisible to the closure.
#[test]
fn a_closure_can_capture_a_caller_variable() {
    let seed = |scope: &mut Scope| {
        // Two of them, and the interesting one is not last: the index bug is
        // invisible with a single entry.
        scope.push("first", vec![Dynamic::from(7_i64)]);
        scope.push("second", vec![Dynamic::from(1_i64)]);
    };

    // The write happens after the closure is made, so a captured copy answers
    // with the old length.
    agree("let n = 0; { let f = || first.len(); first.push(9); n = f.call(); } n", seed, true);
    agree("let n = 0; { let f = || second.len(); second.push(9); n = f.call(); } n", seed, true);
    // And the capture is what shares it, which `is_shared` can see.
    agree("{ let f = || first.len(); } is_shared(first)", seed, true);
    agree("{ let f = || first.len(); } is_shared(second)", seed, true);
}

/// A local of the same name hides the caller's and must not write through to
/// it — the caller's entry has to come back untouched.
#[test]
fn a_local_shadows_the_caller_without_disturbing_it() {
    agree(
        "let brightness = 1; brightness += 1; brightness",
        |s| {
            s.push("brightness", 100_i64);
        },
        true,
    );
    // And the other order: read before the local exists, so the same name is
    // two different variables in one script.
    agree(
        "let first = brightness; let brightness = 1; [first, brightness]",
        |s| {
            s.push("brightness", 100_i64);
        },
        true,
    );
}

#[test]
fn a_caller_constant_cannot_be_assigned_to() {
    let constant = |s: &mut Scope| {
        s.push_constant("mode", lit(1));
    };

    agree("mode = 2; mode", constant, true);
    agree("mode += 1; mode", constant, true);
    // Reading one is fine, which is the half that must keep working.
    agree("mode + 1", constant, true);
}

#[test]
fn a_name_that_is_nowhere_is_reported_the_same_way() {
    agree("nope + 1", |_| {}, true);
    agree("nope = 1", |_| {}, true);
    agree("nope += 1", |_| {}, true);
}

/// A bare script-function name is a function pointer with the calling
/// environment attached (`eval/expr.rs:71-99`), not a variable read, so it
/// must stay a fragment rather than becoming a name lookup that fails.
///
/// Checked on the compiled program rather than by running it, because running
/// it currently disagrees with the walker for an unrelated reason: `execute`
/// forces `always_search_scope` whenever a program has any fragment, and that
/// flag makes rhai skip the function-pointer branch entirely
/// (`eval/expr.rs:62`). Reported separately — it predates named variables, and
/// the fix is about residuals rather than about this.
#[test]
fn a_script_function_name_is_not_a_variable() {
    let engine = corpus::engine();
    let ast = engine.compile("fn helper() { 1 } let f = helper; f.call()").expect("must compile");
    let program = Compiler::new().compile(&ast);

    assert!(program.residual_count() > 0, "a function name must not become a name lookup",);
    assert!(
        !rhai::grain::bytecode::disassemble(program.code()).any(|(.., op)| matches!(op, rhai::grain::bytecode::Op::LoadNamed(..))),
        "nothing in {:?} may load `helper` by name",
        program,
    );
}

/// The last of the three places rhai looks: a constant a host published on a
/// module rather than in the scope.
#[test]
fn a_global_module_constant_resolves() {
    let mut engine = corpus::engine();
    let mut module = Module::new();
    module.set_var("CHANNELS", 512_i64);
    engine.register_global_module(module.into());

    let ast = engine.compile("CHANNELS / 2").expect("must compile");
    let program = Compiler::new().compile(&ast);
    assert_eq!(program.residual_count(), 0);

    let value = Vm::new(&engine).run(&program, &mut Scope::new()).expect("a module constant must resolve");
    assert_eq!(value.as_int().unwrap(), 256);

    // And writing to one is refused, because it is a value and not a place.
    let ast = engine.compile("CHANNELS = 1").expect("must compile");
    let program = Compiler::new().compile(&ast);
    let err = Vm::new(&engine).run(&program, &mut Scope::new()).expect_err("a module constant is not assignable");
    assert!(matches!(*err, rhai::EvalAltResult::ErrorAssignmentToConstant(..)), "got {err:?}",);
}

/// The first of the three, and the one a VM would most plausibly skip: a
/// resolver the host registered through `Engine::on_var` sees the name before
/// the scope does.
#[test]
fn a_variable_resolver_is_consulted_first() {
    let mut engine = corpus::engine();
    engine.on_var(|name, _, _| {
        Ok(match name {
            "injected" => Some(Dynamic::from(99_i64)),
            // Declining must fall through to the scope rather than fail.
            _ => None,
        })
    });

    let compile = |engine: &Engine, source: &str| {
        let ast = engine.compile(source).expect("must compile");
        let program = Compiler::new().compile(&ast);
        assert_eq!(program.residual_count(), 0, "{source:?} must not fragment");
        program
    };

    let mut scope = Scope::new();
    scope.push("ordinary", 5_i64);

    let program = compile(&engine, "injected + ordinary");
    let value = Vm::new(&engine).run(&program, &mut scope.clone()).expect("both must resolve");
    assert_eq!(value.as_int().unwrap(), 104);

    // A resolver hands back a value rather than a place, so it is read-only.
    let program = compile(&engine, "injected = 1");
    let err = Vm::new(&engine).run(&program, &mut scope.clone()).expect_err("a resolved variable is not assignable");
    assert!(matches!(*err, rhai::EvalAltResult::ErrorAssignmentToConstant(..)), "got {err:?}",);

    // And the walker agrees about all of it.
    let ast = engine.compile("injected + ordinary").expect("must compile");
    let expected = engine.eval_ast_with_scope::<Dynamic>(&mut scope.clone(), &ast).expect("the walker must agree");
    assert_eq!(expected.as_int().unwrap(), 104);
}

/// A resolver that pushes onto the scope invalidates every parse-time index,
/// and rhai stops trusting them from that point. Nothing this compiler emits
/// depends on those, but a fragment's does — so the flag still has to be set.
#[test]
fn a_resolver_that_grows_the_scope_forces_a_search() {
    let mut engine = corpus::engine();
    engine.on_var(|name, _, mut context| {
        if name == "grow" {
            context.scope_mut().push("added", 1_i64);
            return Ok(Some(Dynamic::from(1_i64)));
        }
        Ok(None)
    });

    let ast = engine.compile("let a = 10; grow + a").expect("must compile");
    let program = Compiler::new().compile(&ast);
    assert_eq!(program.residual_count(), 0);

    let mut scope = Scope::new();
    let value = Vm::new(&engine).run(&program, &mut scope).expect("must run");

    let mut walked = Scope::new();
    let expected = engine.eval_ast_with_scope::<Dynamic>(&mut walked, &ast).expect("the walker must run it too");

    assert_eq!(format!("{value:?}"), format!("{expected:?}"));
}

/// The receiver a resolver answered is not the scope entry of the same name.
///
/// This is the case the by-reference rewrite is most easily got wrong on: the
/// resolver is consulted first and hands back a *value*, so `push(shadowed, 2)`
/// mutates a temporary and the caller's entry is left alone — even though there
/// is an entry of that name sitting right there to take a reference to.
///
/// The VM tells them apart by the value already on the stack being read-only,
/// which is how `load_named` marks a resolver's answer. The alternative would
/// be running the resolver a second time, and a host can see that.
#[test]
fn a_resolved_receiver_is_not_the_scope_entry_it_shadows() {
    let mut engine = corpus::engine();
    engine.on_var(|name, _, _| {
        Ok(match name {
            "shadowed" => Some(Dynamic::from(vec![Dynamic::from(9_i64)])),
            _ => None,
        })
    });

    let source = "push(shadowed, 2); shadowed";
    let ast = engine.compile(source).expect("must compile");
    let program = Compiler::new().compile(&ast);
    assert_eq!(program.residual_count(), 0, "the call must lower");

    let start = |scope: &mut Scope| {
        scope.push("shadowed", vec![Dynamic::from(1_i64)]);
    };

    let mut walked = Scope::new();
    start(&mut walked);
    let walker = engine.eval_ast_with_scope::<Dynamic>(&mut walked, &ast);

    let mut run = Scope::new();
    start(&mut run);
    let ours = Vm::new(&engine).run(&program, &mut run);

    // Read-only all the way through, so rhai refuses the call outright rather
    // than mutating a copy — which is a sharper thing to agree on.
    assert!(matches!(walker.as_ref().unwrap_err().as_ref(), rhai::EvalAltResult::ErrorNonPureMethodCallOnConstant(..),), "got {walker:?}",);
    assert_eq!(capture(&run, ours), capture(&walked, walker));
    assert_eq!(format!("{:?}", run.get_value::<Dynamic>("shadowed").unwrap()), "[1]", "the entry the resolver shadowed must come back untouched",);
}

/// The one place a compiled closure is not the walker's closure.
///
/// Rhai's parser builds a pointer that embeds the closure's `ScriptFuncDef` —
/// the AST body — and tags it with the environment it was written in. That is
/// exactly what an artifact must not carry, so the compiler emits a
/// name-only pointer to the chunk it compiled from that same body.
///
/// Everything the closure *does* is identical; what differs is that ours is
/// late-bound, which rhai renders. Pinned here rather than left to be
/// discovered, because it is visible to a script that prints one.
#[test]
fn a_closure_pointer_is_late_bound() {
    let engine = corpus::engine();
    let source = "let n = 1; let f = |x| x + n; f";

    let ast = engine.compile(source).expect("must compile");
    let program = Compiler::new().compile(&ast);
    assert_eq!(program.residual_count(), 0, "the closure must lower");

    let ours = Vm::new(&engine).run(&program, &mut Scope::new()).expect("must run");
    let walker = engine.eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast).expect("must run under rhai too");

    let (ours, walker) = (format!("{ours:?}"), format!("{walker:?}"));
    assert!(ours.starts_with("Fn(\"anon$"), "ours is a plain named pointer: {ours}",);
    assert!(walker.starts_with("Fn*+(\"anon$"), "rhai's carries a script body and an environment: {walker}",);

    // And the difference is only in the binding: calling either gives the
    // same answer, which is what the corpus covers.
    assert_ne!(ours, walker, "if these ever match, delete this test");
}

/// Calling a compiled function from outside, which is what a native wrapper
/// will do once compiled chunks are registered for callbacks.
#[test]
fn a_compiled_function_can_be_called_by_name() {
    let engine = corpus::engine();
    let ast = engine.compile("fn add(a, b) { a + b } fn boom() { throw 7; } 0").expect("must compile");
    let program = Compiler::new().compile(&ast);

    let mut vm = Vm::new(&engine);
    let value = vm
        .call_function(&program, "add", vec![Dynamic::from(2_i64), Dynamic::from(3_i64)], 0, rhai::Position::NONE)
        .expect("must call");
    assert_eq!(value.as_int().unwrap(), 5);

    // Wrong arity is a miss, not a crash — the table is keyed on both.
    let err = vm
        .call_function(&program, "add", vec![Dynamic::from(1_i64)], 0, rhai::Position::NONE)
        .expect_err("one argument is a different function");
    assert!(matches!(*err, rhai::EvalAltResult::ErrorFunctionNotFound(..)));

    // And what the function raises comes back, wrapped as rhai wraps it.
    let err = vm.call_function(&program, "boom", Vec::new(), 0, rhai::Position::NONE).expect_err("must propagate");
    assert!(matches!(*err, rhai::EvalAltResult::ErrorInFunctionCall(..)), "got {err:?}",);

    // The operand stack is where it started, so a caller can keep using it.
    let value = vm
        .call_function(&program, "add", vec![Dynamic::from(10_i64), Dynamic::from(1_i64)], 0, rhai::Position::NONE)
        .expect("must call again");
    assert_eq!(value.as_int().unwrap(), 11);
}

/// The flag a host uses to decide whether a program has to be owned.
///
/// Registering compiled functions so a native can call one back requires a
/// `'static` wrapper, which means owning the program and giving up the
/// borrowed-from-the-artifact loading. Nobody should have to read a script to
/// find out whether that is needed — and the answer must be the same for a
/// compiled program and for the same program read back, or one of the two
/// paths quietly loses its callbacks.
#[test]
fn the_compiler_says_whether_a_program_makes_function_pointers() {
    let engine = corpus::engine();

    for (source, expected) in [
        ("let a = 1; a + 2", false),
        ("fn f(x) { x } f(1)", false),
        ("let s = 0; for i in 0..3 { s += i; } s", false),
        // Every shape that produces one.
        ("let n = \"ab\" + \"s\"; Fn(n)", true),
        ("let n = 1; let f = |x| x + n; f.call(2)", true),
        ("fn t(x) { x } let f = Fn(\"t\"); f.call(1)", true),
        ("fn a(x, y) { x } let n = \"a\"; Fn(n).curry(1)", true),
    ] {
        let ast = engine.compile(source).expect("must compile");
        let program = Compiler::new().compile(&ast);
        assert_eq!(program.makes_fn_pointers(), expected, "for {source:?}",);

        // Read off the code, so an artifact answers the same.
        if let Ok(bytes) = program.write() {
            let reloaded = rhai::grain::Program::read(&bytes).expect("must load");
            assert_eq!(reloaded.makes_fn_pointers(), expected, "after a round trip, for {source:?}",);
        }
    }
}

/// The whole reason for the opcode: these now cross a wire.
#[test]
fn a_program_reading_caller_state_can_be_written() {
    let engine = corpus::engine();
    let source = "let out = brightness; if mode == \"chase\" { out += 10 } out";

    let ast = engine.compile(source).expect("must compile");
    let program = Compiler::new().compile(&ast);
    let bytes = program.write().expect("must be writable");

    let reloaded = rhai::grain::Program::read(&bytes).expect("must load");

    let mut scope = Scope::new();
    scope.push("brightness", 5_i64);
    scope.push("mode", "chase".to_string());
    let value = Vm::new(&engine).run(&reloaded, &mut scope).expect("must run");

    assert_eq!(value.as_int().unwrap(), 15);
}
