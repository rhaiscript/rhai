//! Randomised loading, on the assumption that an artifact is hostile.
//!
//! `Program::read` is the only place untrusted bytes enter, and what it hands
//! back is executed in place — so the claim it has to support is total: any
//! byte string either fails to load or loads into a chunk the VM can run
//! without panicking, looping forever, or reading outside itself.
//!
//! `no_single_bit_flip_can_panic_or_smuggle_a_bad_chunk` in `format.rs` is the
//! exhaustive half, over one-bit corruption of one artifact. This is the wide
//! half: many mutations, several of them structural, over a corpus of real
//! artifacts. Seeded rather than random, so a failure is reproducible and CI
//! does not go intermittent — the seed is printed and any interesting input is
//! dumped as hex.
//!
//! It is not a substitute for `cargo fuzz` (see `fuzz/`), which explores with
//! coverage feedback. It is what runs on every `cargo test`.

// Only the sources are wanted here; the names belong to the harnesses that
// report per-case results.
#[allow(dead_code)]
mod corpus;

use rhai::{Dynamic, Engine, Scope};
use rhaigrain::{Compiler, Program, Vm};

use corpus::generate::{Generator, Rng};

/// Ways an artifact can arrive wrong. Truncation and splicing matter as much
/// as corruption: a length field that disagrees with what follows is how a
/// loader is talked into reading past the end.
fn mutate(rng: &mut Rng, bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    if out.is_empty() {
        return out;
    }

    match rng.below(6) {
        // Corrupt a run of bytes, which reaches multi-byte fields that
        // single-bit flipping cannot.
        0 | 1 => {
            let at = rng.below(out.len());
            let run = 1 + rng.below((out.len() - at).min(8));
            for byte in &mut out[at..at + run] {
                *byte = rng.next() as u8;
            }
        }
        // Cut it short.
        2 => out.truncate(rng.below(out.len())),
        // Splice out a slice, so every following offset is wrong.
        3 => {
            let at = rng.below(out.len());
            let run = 1 + rng.below((out.len() - at).min(16));
            out.drain(at..at + run);
        }
        // Insert junk, the same problem in the other direction.
        4 => {
            let at = rng.below(out.len());
            let junk: Vec<u8> = (0..1 + rng.below(16)).map(|_| rng.next() as u8).collect();
            out.splice(at..at, junk);
        }
        // Set a byte to an edge value: the lengths and counts are varints, and
        // 0xff runs are what make one disagree with reality.
        _ => {
            let at = rng.below(out.len());
            out[at] = [0x00, 0x01, 0x7f, 0x80, 0xff][rng.below(5)];
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The claim, over a lot of inputs.
///
/// Running what survives is the point rather than a bonus: a loader that
/// accepts a chunk it should not have has done nothing visible until something
/// executes it.
#[test]
fn mutated_artifacts_load_or_fail_but_never_misbehave() {
    let writer = corpus::engine();

    // Every artifact the corpus can produce, so the mutations land on real
    // structure — a hand-written sample would only ever exercise its own
    // shape.
    let seeds: Vec<Vec<u8>> = corpus::CASES
        .iter()
        .filter_map(|case| {
            let ast = writer.compile(case.source).ok()?;
            Compiler::new().compile(&ast).write().ok()
        })
        .collect();

    assert!(
        seeds.len() >= 50,
        "only {} artifacts to mutate, which is too few to prove anything",
        seeds.len(),
    );

    // A budget, because verification proves structure and not termination: a
    // corrupted-but-in-range jump target is a valid infinite loop.
    let mut engine = corpus::engine();
    engine.set_max_operations(10_000);
    engine.set_max_string_size(4096);
    engine.set_max_array_size(1024);

    const SEED: u64 = 0x5eed_1234_abcd_0001;
    // Enough to be worth running on every `cargo test` and not so many that
    // anyone is tempted to stop. Depth is `cargo fuzz`'s job — see `fuzz/`.
    const ROUNDS: usize = 20;

    let mut rng = Rng::new(SEED);
    let mut loaded = 0usize;

    for round in 0..ROUNDS {
        for original in &seeds {
            let corrupt = mutate(&mut rng, original);

            let Ok(program) = Program::read(&corrupt) else {
                continue;
            };
            loaded += 1;

            // Anything `read` returns must already have verified — that is the
            // contract the VM's missing bounds checks rest on.
            assert!(
                program.verify().is_ok(),
                "seed {SEED:#x} round {round}: read returned a chunk that does \
                 not verify: {}",
                hex(&corrupt),
            );

            // And must then run without taking the process down. The result is
            // free to be anything at all.
            let _ = Vm::new(&engine).run(&program, &mut Scope::new());
        }
    }

    println!(
        "{loaded} of {} mutations loaded and ran",
        ROUNDS * seeds.len(),
    );
    assert!(
        loaded > 0,
        "no mutation survived, so nothing was actually executed",
    );
}

/// Bytes that were never an artifact, which is the other way in.
#[test]
fn arbitrary_bytes_never_load_into_something_that_misbehaves() {
    let mut engine = Engine::new();
    engine.set_max_operations(10_000);

    let mut rng = Rng::new(0x5eed_0000_0000_0002);

    for _ in 0..20_000 {
        let len = rng.below(64);
        let mut bytes: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();

        // Half of them get the magic, so the loader is reached rather than
        // rejected at the first four bytes every time.
        if rng.below(2) == 0 && bytes.len() >= 4 {
            bytes[..4].copy_from_slice(b"RGRN");
        }

        if let Ok(program) = Program::read(&bytes) {
            assert!(program.verify().is_ok(), "unverified: {}", hex(&bytes));
            let _ = Vm::new(&engine).run(&program, &mut Scope::new());
        }
    }
}

/// What a script produced, in a form two runs can be compared on.
#[derive(PartialEq, Eq)]
struct Outcome {
    result: Result<String, String>,
    scope: Vec<(String, String)>,
}

impl std::fmt::Debug for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.result {
            Ok(value) => write!(f, "{value} | scope {:?}", self.scope),
            Err(err) => write!(f, "!{err} | scope {:?}", self.scope),
        }
    }
}

fn snapshot(scope: &Scope, result: Result<Dynamic, Box<rhai::EvalAltResult>>) -> Outcome {
    Outcome {
        result: result
            .map(|value| format!("{value:?}"))
            .map_err(|err| format!("{err:?}")),
        scope: scope
            .iter_raw()
            .map(|(name, _, value)| (name.to_string(), format!("{value:?}")))
            .collect(),
    }
}

/// Whether an outcome is one the two sides are not expected to reach together.
///
/// Both enforce these limits; neither counts towards them in lockstep, because
/// a VM instruction is not a walker node and a callback boundary costs a
/// different number of call levels. A script that runs into one has stopped
/// saying anything about lowering, so it is dropped rather than compared —
/// and the drop rate is asserted, so this cannot quietly become the answer for
/// everything.
fn hit_a_limit(outcome: &Outcome) -> bool {
    let Err(err) = &outcome.result else {
        return false;
    };
    ["ErrorTooManyOperations", "ErrorStackOverflow", "ErrorTooManyVariables"]
        .iter()
        .any(|limit| err.contains(limit))
}

/// Run something that may panic, without the panic reaching the console.
///
/// The hook is swapped rather than left alone because a fuzzing run can trip
/// the same upstream panic thousands of times, and the backtrace for each one
/// is noise. Single-threaded, and restored immediately.
fn quietly<T>(body: impl FnOnce() -> T) -> Option<T> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).ok();
    std::panic::set_hook(hook);
    out
}

/// A rhai bug, not one of ours, found by `cargo fuzz run generated`.
///
/// A `switch` as the last statement of a block makes rhai's optimizer delete a
/// `let` in that block and flatten what is left into the enclosing statement
/// list — while the reads of that local keep the scope index the parser gave
/// them. The index is counted back from the end of the scope, so it now names
/// whatever moved into its place.
///
/// Two symptoms, and the quiet one is the dangerous one:
///
/// * with something else at that index, rhai answers with **another variable's
///   value** and reports nothing at all;
/// * with nothing there, `scope.len() - index` underflows (`eval/expr.rs:131`)
///   — a panic in a debug build and a wild index in a release one.
///
/// Both are three lines of ordinary rhai with none of this involved. We resolve
/// locals by name, so we say the variable is missing, which is the closest
/// thing to right available: there is no agreeing with an AST that refers to a
/// local it does not declare.
///
/// Pinned because `generated_scripts_agree_with_the_walker` and both `cargo
/// fuzz` targets have to skip these, and every one of those skips should go the
/// day this test starts failing.
#[test]
fn rhai_drops_a_local_its_optimizer_still_refers_to() {
    // The read lands on `a`, so rhai answers 1 where the script says 99.
    const WRONG: &str = "let a = 1; { let b = 99; switch b { _ => b } }";
    // The same shape with nothing left at that index.
    const PANIC: &str = "{ let b = 99; switch b { _ => b } }";

    let engine = corpus::engine();
    let mut plain = corpus::engine();
    plain.set_optimization_level(rhai::OptimizationLevel::None);

    let ast = engine.compile(WRONG).expect("it parses");
    let value = engine
        .eval_ast::<Dynamic>(&ast)
        .expect("rhai runs it, which is the problem");
    assert_eq!(
        value.as_int().ok(),
        Some(1),
        "rhai no longer reads the wrong variable — delete the optimizer skips \
         in the fuzzers and this test with them",
    );

    let ast = engine.compile(PANIC).expect("it parses");
    assert!(
        quietly(|| engine.eval_ast::<Dynamic>(&ast)).is_none(),
        "rhai no longer underflows here — delete the walker skip in \
         `generated_scripts_agree_with_the_walker`",
    );

    // The same scripts with the optimizer out of the way, which is what says
    // the fault is in the optimizer rather than in them.
    for source in [WRONG, PANIC] {
        let ast = plain.compile(source).expect("it parses either way");
        let value = quietly(|| plain.eval_ast::<Dynamic>(&ast))
            .expect("with the optimizer off it should run")
            .expect("and it should not fail");
        assert_eq!(value.as_int().ok(), Some(99), "{source:?}");
    }
}

/// Whether the two sides agree once rhai's optimizer is out of the way.
///
/// The optimizer is what makes a dropped local reachable, so agreeing without
/// it and disagreeing with it says the AST is at fault rather than the
/// lowering. Only ever asked about a divergence that already looks like one.
fn agree_unoptimised(source: &str) -> bool {
    let mut plain = corpus::engine();
    plain.set_optimization_level(rhai::OptimizationLevel::None);
    let Ok(ast) = plain.compile(source) else {
        return false;
    };

    let mut walker_scope = Scope::new();
    let Some(walked) = quietly(|| plain.eval_ast_with_scope::<Dynamic>(&mut walker_scope, &ast))
    else {
        return false;
    };

    let program = Compiler::new().compile(&ast);
    let mut vm_scope = Scope::new();
    let ours = if program.makes_fn_pointers() {
        let program = program.into_shared();
        Vm::new(&plain).run_with_callbacks(&program, &mut vm_scope)
    } else {
        Vm::new(&plain).run(&program, &mut vm_scope)
    };

    snapshot(&walker_scope, walked) == snapshot(&vm_scope, ours)
}

/// The claim the corpus makes, over scripts nobody wrote.
///
/// `tests/differential.rs` pins the constructs someone thought of. This is the
/// same comparison — value, error variant, error position, and what was left in
/// the scope — over combinations of them that no one did.
#[test]
fn generated_scripts_agree_with_the_walker() {
    // Budgets, not correctness limits: a generated script can be quadratic in
    // a way no corpus case is, and there is no reason to wait for it.
    let mut engine = corpus::engine();
    engine.set_max_operations(200_000);
    engine.set_max_array_size(2048);
    engine.set_max_string_size(8192);
    // Pinned, because rhai's defaults for these are `debug_assertions`-gated —
    // 32/16 and 8 in a debug build against 64/64 and 64 in a release one
    // (`api/limits.rs:10-36`). Left alone, `cargo test` and
    // `cargo test --release` parse different halves of the same seeded corpus
    // and compare different scripts, which is not a thing a differential
    // harness may do. The release numbers, because they admit more.
    engine.set_max_expr_depths(64, 64);
    engine.set_max_call_levels(64);

    const SEED: u64 = 0x5eed_9a11_0000_0001;
    const SCRIPTS: usize = 4000;

    let mut parsed = 0usize;
    let mut ran = 0usize;
    let mut valued = 0usize;
    let mut skipped = 0usize;
    let mut walker_panics = 0usize;
    let mut too_deep = 0usize;
    let mut lost_a_local = 0usize;
    let mut unparsed: Vec<String> = Vec::new();
    let mut failures = Vec::new();

    for n in 0..SCRIPTS {
        // Seeded per script rather than from one long stream, so a failing
        // script can be reproduced on its own by its index.
        let source = Generator::new(SEED ^ n as u64).script();

        let ast = match engine.compile(&source) {
            Ok(ast) => ast,
            Err(err) => {
                if matches!(err.err_type(), rhai::ParseErrorType::ExprTooDeep) {
                    too_deep += 1;
                } else {
                    unparsed.push(format!("\n  {err}\n  {source}"));
                }
                continue;
            }
        };
        parsed += 1;

        // The scope is snapshotted after the run, not during: what a script
        // leaves behind is half of what is being compared.
        //
        // Guarded because the walker can panic on a script of its own accord —
        // see `rhai_underflows_a_scope_index_under_its_own_optimizer`. There is
        // nothing to compare against a side that did not finish, so those are
        // counted and dropped rather than blamed on the VM.
        let mut walker_scope = Scope::new();
        let Some(walked) = quietly(|| engine.eval_ast_with_scope::<Dynamic>(&mut walker_scope, &ast))
        else {
            walker_panics += 1;
            continue;
        };
        let walked = snapshot(&walker_scope, walked);

        if hit_a_limit(&walked) {
            skipped += 1;
            continue;
        }
        ran += 1;
        if walked.result.is_ok() {
            valued += 1;
        }

        let program = Compiler::new().compile(&ast);
        let mut vm_scope = Scope::new();
        let ours = if program.makes_fn_pointers() {
            let program = program.into_shared();
            Vm::new(&engine).run_with_callbacks(&program, &mut vm_scope)
        } else {
            Vm::new(&engine).run(&program, &mut vm_scope)
        };
        let ours = snapshot(&vm_scope, ours);

        if hit_a_limit(&ours) {
            skipped += 1;
            continue;
        }

        if walked == ours {
            continue;
        }
        // Not every disagreement is one to have: rhai's optimizer can delete a
        // `let` whose variable is still read, and then there is no agreeing
        // with it. See `rhai_drops_a_local_its_optimizer_still_refers_to`.
        if format!("{ours:?}").contains("ErrorVariableNotFound")
            && agree_unoptimised(&source)
        {
            lost_a_local += 1;
            continue;
        }

        if failures.len() < 5 {
            failures.push(format!(
                "\n=== script {n} (seed {:#x}) ===\n  {source}\n  rhai: {walked:?}\n  vm:   {ours:?}",
                SEED ^ n as u64,
            ));
        }
    }

    println!(
        "{parsed} of {SCRIPTS} generated scripts parsed, {ran} compared, \
         {valued} produced a value, {skipped} hit a limit, \
         {walker_panics} panicked the walker, \
         {too_deep} were too complex to parse, \
         {lost_a_local} lost a local to rhai's optimizer",
    );

    // The walker panicking is upstream's problem, but it is also a hole in this
    // test's coverage, so it is worth knowing if it ever becomes common.
    assert!(
        walker_panics * 50 < SCRIPTS,
        "{walker_panics} of {SCRIPTS} scripts panicked the walker, which is too \
         many to keep skipping — see \
         `rhai_underflows_a_scope_index_under_its_own_optimizer`",
    );

    // A generator that mostly emits garbage would pass this test by comparing
    // two identical parse failures a few thousand times. These are the numbers
    // that say it is doing work, and they are what to look at first if this
    // file ever stops finding anything.
    //
    // Invalid *syntax* is the thing to catch, and a raw parse rate does not
    // catch it: a script rejected for exceeding rhai's complexity limit is
    // well-formed, and how many do is a property of how much the generator
    // packs into one script rather than of whether it can write the language.
    // So the two are counted apart, and this is the one that means something.
    assert!(
        unparsed.is_empty(),
        "{} of {SCRIPTS} scripts are not valid rhai, so the generator is \
         emitting syntax rather than testing it:{}",
        unparsed.len(),
        unparsed.iter().take(5).cloned().collect::<String>(),
    );
    assert!(
        parsed * 10 >= SCRIPTS * 7,
        "only {parsed} of {SCRIPTS} scripts parsed, and {too_deep} were rejected \
         as too complex — the generator is packing more into a script than rhai \
         will take, so most of what it writes is never run",
    );
    // Against the whole corpus rather than against what ran, because the two
    // move for opposite reasons. A script that errors is still compared — the
    // variant, the position and the scope all have to match — so a *rising*
    // error rate is usually the generator reaching further, and measuring
    // against `ran` would read that as a regression and push it back toward
    // scripts too simple to catch anything. What this is here to catch is the
    // generator producing nothing that works at all.
    assert!(
        valued * 5 >= SCRIPTS,
        "only {valued} of {SCRIPTS} scripts reached a value; the rest failed at \
         run time, which tests error parity and little else",
    );
    assert!(
        skipped * 20 < SCRIPTS,
        "{skipped} of {SCRIPTS} scripts hit an engine limit, so the budgets are \
         deciding the outcome rather than the code",
    );

    assert!(
        failures.is_empty(),
        "generated scripts diverged:{}",
        failures.join(""),
    );
}
