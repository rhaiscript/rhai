//! Grammar-directed scripts through the pipeline, checked against the walker.
//!
//! `roundtrip` hands libfuzzer's bytes straight to the parser, which for a
//! language with real syntax means most of them are rejected before anything
//! interesting runs. This spends the same bytes on *grammar decisions* instead,
//! so every input is a valid script and coverage feedback is steering the shape
//! of the program rather than the spelling of it.
//!
//! `tests/fuzz.rs` runs the same generator over a few thousand seeded scripts
//! on every `cargo test`. This is the one that runs for hours.
//!
//! `cargo fuzz run generated`

#![no_main]

use libfuzzer_sys::fuzz_target;
use rhai::grain::{Compiler, Vm};
use rhai::{Dynamic, Engine, Scope};

// The generator lives with the tests because that is the only thing that needs
// it, and putting it in the library would make it public API. This crate is
// outside the workspace and cannot depend on a test target, so it takes the
// source directly — inside a module, because the file opens with `//!`.
mod generate {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/grain/corpus/generate.rs"
    ));
}

use generate::Generator;

/// Both sides budgeted identically, or a script that runs out of operations on
/// one and not the other reads as a divergence.
fn engine() -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(200_000);
    engine.set_max_string_size(8192);
    engine.set_max_array_size(2048);
    engine.set_max_map_size(64);
    // Pinned for the reason `tests/fuzz.rs` pins them: rhai's defaults are
    // `debug_assertions`-gated (`api/limits.rs:10-36`), so an unpinned harness
    // explores one script space here and a different one under `cargo test`.
    // Same numbers in both, so a finding from one reproduces in the other.
    engine.set_max_expr_depths(64, 64);
    engine.set_max_call_levels(64);
    engine
}

/// Run the walker, surviving a panic of its own making.
///
/// Rhai's optimizer can leave a local's parse-time scope index stale, and
/// evaluating it then underflows (`eval/expr.rs:131`). It is reachable from an
/// ordinary script with none of this involved — `tests/fuzz.rs` pins a
/// reproducer — and a fuzzing run trips it often enough to end the run in
/// minutes, which costs far more than it finds. There is nothing to compare
/// against a side that did not finish, so those inputs are dropped.
///
/// The hook is swapped for the duration because libfuzzer installs one that
/// aborts, which would defeat `catch_unwind`. It is swapped back immediately,
/// so a panic anywhere else in this target still ends the run as it should.
fn walk(engine: &Engine, ast: &rhai::AST, scope: &mut Scope) -> Option<RhaiOutcome> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine.eval_ast_with_scope::<Dynamic>(scope, ast)
    }));
    std::panic::set_hook(hook);
    result.ok()
}

type RhaiOutcome = Result<Dynamic, Box<rhai::EvalAltResult>>;

/// A run reduced to something two of them can be compared on, scope included:
/// a VM that produced the right value while leaving the scope a different shape
/// has still got it wrong, and that is how the slot model fails.
fn outcome(scope: &Scope, result: RhaiOutcome) -> Option<String> {
    let scope: Vec<String> = scope
        .iter_raw()
        .map(|(name, _, value)| format!("{name}={value:?}"))
        .collect();

    match result {
        Ok(value) => Some(format!("ok {value:?} [{}]", scope.join(","))),
        // The limits neither side counts towards in lockstep. `tests/fuzz.rs`
        // documents why and asserts they stay rare; here they are simply
        // dropped, because there is no way to assert a rate over one input.
        Err(err)
            if matches!(
                *err,
                rhai::EvalAltResult::ErrorTooManyOperations(..)
                    | rhai::EvalAltResult::ErrorStackOverflow(..)
                    | rhai::EvalAltResult::ErrorTooManyVariables(..)
            ) =>
        {
            None
        }
        Err(err) => Some(format!("err {err:?} [{}]", scope.join(","))),
    }
}

/// Run a script both ways under one engine. `None` if there is nothing to
/// compare — it did not parse, the walker did not survive it, or a limit
/// decided it.
fn compare(engine: &Engine, source: &str) -> Option<(String, String)> {
    let ast = engine.compile(source).ok()?;

    let mut walker_scope = Scope::new();
    let walked = walk(engine, &ast, &mut walker_scope)?;
    let expected = outcome(&walker_scope, walked)?;

    let program = Compiler::new().compile(&ast);
    let mut vm_scope = Scope::new();
    let ours = if program.makes_fn_pointers() {
        let program = program.into_shared();
        Vm::new(engine).run_with_callbacks(&program, &mut vm_scope)
    } else {
        Vm::new(engine).run(&program, &mut vm_scope)
    };
    Some((outcome(&vm_scope, ours)?, expected))
}

/// Whether a divergence is rhai's optimizer losing a local rather than a bug of
/// ours — see `rhai_drops_a_local_its_optimizer_still_refers_to` in
/// `tests/fuzz.rs`.
///
/// The optimizer can delete a `let` whose variable is still read, leaving the
/// read pointing at whatever now sits at that scope index. Rhai answers with
/// another variable's value; we resolve by name and report it missing. There is
/// no agreeing with an AST that refers to a local it does not declare.
///
/// Turning the optimizer off is what tells the two apart, and it runs only on a
/// divergence that already looks like this one — a fuzzer that quietly stopped
/// comparing would be worse than one that stops.
fn optimizer_lost_a_local(source: &str, ours: &str) -> bool {
    if !ours.contains("ErrorVariableNotFound") {
        return false;
    }
    let mut plain = engine();
    plain.set_optimization_level(rhai::OptimizationLevel::None);
    compare(&plain, source).is_some_and(|(ours, expected)| ours == expected)
}

fuzz_target!(|data: &[u8]| {
    // Too few bytes to steer with, and the PRNG fallback would make every such
    // input the same script.
    if data.len() < 8 {
        return;
    }

    let source = Generator::from_bytes(data).script();
    let Some((ours, expected)) = compare(&engine(), &source) else {
        return;
    };
    if ours == expected || optimizer_lost_a_local(&source, &ours) {
        return;
    }

    assert_eq!(
        ours, expected,
        "the VM disagrees with the walker on:\n{source}"
    );
});
