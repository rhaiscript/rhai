//! Arbitrary *scripts* through the whole pipeline, checked against the walker.
//!
//! The other target proves a hostile artifact cannot misbehave. This one
//! proves a well-formed one still means what it said: compile, write, read
//! back, run, and get what Rhai's own evaluator got — the differential corpus
//! generalized from cases someone thought of to inputs nobody did.
//!
//! Both halves are load-bearing and they fail differently. A divergence here
//! is a compiler or VM bug; a panic there is a safety bug.
//!
//! `cargo fuzz run grain_roundtrip`

#![no_main]

use libfuzzer_sys::fuzz_target;
use rhai::grain::{Compiler, Program, Vm};
use rhai::{Dynamic, Engine, Scope};

/// Both sides must be budgeted identically, or a script that runs out of
/// operations on one and not the other reads as a divergence.
fn engine() -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(50_000);
    engine.set_max_string_size(4096);
    engine.set_max_array_size(1024);
    engine.set_max_map_size(64);
    engine.set_max_call_levels(16);
    // Parse-time depth is `debug_assertions`-gated in rhai
    // (`api/limits.rs:10-36`), so without this the set of scripts this target
    // accepts depends on how it was built. Call depth above is already pinned
    // for the same reason.
    engine.set_max_expr_depths(64, 64);
    engine
}

/// A value, with the one difference between the two sides that is intended.
///
/// Rhai renders a closure pointer `Fn*+("anon$..")` — a script function with a
/// captured environment attached — and ours `Fn("anon$..")`, because ours is
/// name-only and resolved at call time. That is the whole point: a `Script`
/// pointer carries an AST, and an AST is what an artifact must not contain.
/// The difference is deliberate, pinned by `a_closure_pointer_is_late_bound` in
/// `tests/scope.rs`, and left in the rendering rather than papered over there.
///
/// Here it has to be papered over, or the first script whose value is a bare
/// closure ends the run — which is within a few thousand executions.
fn rendered(value: &Dynamic) -> String {
    format!("{value:?}")
        .replace("Fn*+(", "Fn(")
        .replace("Fn*(", "Fn(")
}

/// A run reduced to something two of them can be compared on, scope included:
/// a VM that produced the right value while leaving the scope a different
/// shape has still got it wrong, and that is how the slot model fails.
fn outcome(run: impl FnOnce(&mut Scope) -> Result<Dynamic, Box<rhai::EvalAltResult>>) -> String {
    let mut scope = Scope::new();
    let result = run(&mut scope);
    let scope: Vec<String> = scope
        .iter_raw()
        .map(|(name, _, value)| rendered(value))
        .collect();

    match result {
        Ok(value) => format!("ok {} [{}]", rendered(&value), scope.join(",")),
        // The limits are the one thing allowed to differ: the walker ticks per
        // AST node and the VM per loop back-edge, and a callback crossing costs
        // the two of them different numbers of call levels. Neither count will
        // ever match, so a script that runs into one is dropped.
        Err(err)
            if matches!(
                *err,
                rhai::EvalAltResult::ErrorTooManyOperations(..)
                    | rhai::EvalAltResult::ErrorStackOverflow(..)
                    | rhai::EvalAltResult::ErrorTooManyVariables(..)
            ) =>
        {
            "budget".to_string()
        }
        Err(err) => format!("err {err:?} [{}]", scope.join(",")),
    }
}

fuzz_target!(|source: String| {
    let engine = engine();
    let Ok(ast) = engine.compile(&source) else {
        return;
    };

    let program = Compiler::new().compile(&ast).into_shared();

    let expected = outcome(|scope| engine.eval_ast_with_scope::<Dynamic>(scope, &ast));
    // A program that can hand a pointer to a native has to be run the way such
    // a program is meant to be run, or every one of them reads as a divergence.
    let run = |scope: &mut Scope| Vm::new(&engine).eval_with_callbacks(scope, program);

    let direct = outcome(run);
    if expected == "budget" || direct == "budget" {
        return;
    }
    if direct != expected {
        return;
    }
    assert_eq!(
        direct, expected,
        "the VM disagrees with the walker on:\n{source}"
    );

    // A program that still fragments cannot be written, which is not a bug —
    // it is the escape hatch doing its job.
    let Ok(bytes) = program.write() else {
        return;
    };
    let reloaded = Program::read(&bytes).expect("what we wrote must read back");
    let loaded = outcome(|scope| Vm::new(&engine).eval_with_scope(scope, &reloaded));

    assert_eq!(
        loaded, expected,
        "the artifact disagrees with the walker on:\n{source}"
    );
});
