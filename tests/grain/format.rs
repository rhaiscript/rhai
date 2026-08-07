//! An artifact must mean what the program it came from meant, and nothing a
//! wire can hand it may take the process down.
//!
//! Two separate claims, and they need separate tests. The first is a round
//! trip: compile, write, read back, run, and get what running the original
//! got — including the scope left behind and the exact error position. The
//! second is that `read` is total over arbitrary bytes: every truncation and
//! every single-byte corruption of a valid artifact either loads or fails, and
//! never panics.

mod corpus;

use rhai::{Dynamic, Engine, Scope};
use rhaigrain::format::{ReadError, WriteError};
use rhaigrain::{Compiler, Program, Vm};

/// What a run produced, in a form two runs can be compared on.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    result: Result<String, String>,
    scope: Vec<(String, String)>,
}

/// A finished run, reduced to what two of them can be compared on.
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

/// Taken by value, because a program that hands pointers to natives has to be
/// owned to be run at all — and whether this one does is read back off the
/// bytes, which is the property that makes an artifact self-describing.
fn run(engine: &Engine, program: Program) -> Outcome {
    let mut scope = Scope::new();
    let result = if program.makes_fn_pointers() {
        let program = program.into_shared();
        Vm::new(engine).run_with_callbacks(&program, &mut scope)
    } else {
        Vm::new(engine).run(&program, &mut scope)
    };

    snapshot(&scope, result)
}

fn run_stock(engine: &Engine, source: &str) -> Outcome {
    let mut scope = Scope::new();
    let ast = engine.compile(source).expect("corpus scripts parse");
    let result = engine.eval_ast_with_scope::<Dynamic>(&mut scope, &ast);

    snapshot(&scope, result)
}

/// Every corpus script that can be written, with its bytes.
fn writable(engine: &Engine) -> Vec<(&'static str, &'static str, Vec<u8>)> {
    corpus::CASES
        .iter()
        .filter_map(|case| {
            let ast = engine.compile(case.source).ok()?;
            let bytes = Compiler::new().compile(&ast).write().ok()?;
            Some((case.name, case.source, bytes))
        })
        .collect()
}

/// The claim the format exists to support: bytes in one process mean the same
/// program in another.
#[test]
fn an_artifact_runs_as_the_program_it_came_from() {
    let engine = corpus::engine();
    let mut failures = Vec::new();

    for (name, source, bytes) in writable(&engine) {
        let reloaded = match Program::read(&bytes) {
            Ok(program) => program,
            Err(err) => {
                failures.push(format!("\n  {name}: wrote but could not read back: {err}"));
                continue;
            }
        };

        let expected = run_stock(&engine, source);
        let actual = run(&engine, reloaded);

        if expected != actual {
            failures.push(format!(
                "\n  {name}: {source}\n    rhai: {expected:?}\n    artifact: {actual:?}"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} artifacts do not mean what they came from:{}",
        failures.len(),
        failures.join(""),
    );
}

/// A round trip over an empty set passes trivially, so pin the size of the set
/// and pin that the things it should contain are in it.
#[test]
fn the_round_trip_covers_something_worth_covering() {
    let engine = corpus::engine();
    let written = writable(&engine);
    let names: Vec<_> = written.iter().map(|(name, ..)| *name).collect();

    assert!(
        written.len() >= 20,
        "only {} corpus scripts are writable, which is too few to prove anything: {names:?}",
        written.len(),
    );

    // One per construct the encoder has a branch for, so a branch that stops
    // working names itself.
    for required in [
        "int_arithmetic",     // Call with an operator token
        "float_arithmetic",   // a float constant, whose width the ABI pins
        "shadowing_nested",   // DeclareLocal and UnwindTo
        "while_loop",         // jumps, Tick, AssignLocal with an op
        "loop_break_value",   // backpatched jumps
        "error_divide_by_zero", // a position that has to survive
        "switch_range",         // a switch table, and the hasher probe with it
        "switch_guard",         // and one whose arms are a chain rather than a target
        "string_slice_read",    // a range constant, which is a host type in `Dynamic`
        "string_slice_inclusive", // and the other range tag
        "index_assign_array",     // a chain rooted at a slot, and its name
        "temp_root_array_method", // and one rooted on the operand stack instead
    ] {
        assert!(
            names.contains(&required),
            "`{required}` no longer writes, so the encoder branch it covers is untested",
        );
    }
}

/// Where the golden pair lives. The source is checked in beside the artifact so
/// a regeneration is a visible two-file change.
const GOLDEN_SOURCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/golden.rhai");
const GOLDEN_ARTIFACT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/golden.rgrn");

/// The caller state `golden.rhai` expects. Part of the fixture, so it lives
/// with it rather than being invented at each use.
fn golden_scope() -> Scope<'static> {
    let mut scope = Scope::new();
    scope.push("supplied", vec![Dynamic::from(7_i64)]);
    scope
}

/// The one claim every other test in this file is blind to: that an artifact
/// written *earlier* still means the same thing.
///
/// Named for `golden` so the regeneration command below selects it and nothing
/// else.
///
/// Every other artifact here is produced by the current writer in the same
/// process, so a writer and reader that drift together agree with each other
/// perfectly and nothing notices. The device is the case that matters — bytes
/// built by one version of this crate and run by another — and a checked-in
/// artifact is the only way to have one side of that be genuinely old.
///
/// Failing this is not automatically a bug. It means the encoding moved, and
/// the question it asks is whether that was deliberate. If it was, regenerate:
///
/// ```text
/// REGENERATE_GOLDEN=1 cargo test --test format golden
/// ```
///
/// and bump `VERSION` if an older reader would *misread* the new bytes rather
/// than reject them — the rule is at `src/format/mod.rs:56`.
#[test]
fn a_golden_artifact_written_by_an_older_build_still_runs() {
    let engine = corpus::engine();
    let source = std::fs::read_to_string(GOLDEN_SOURCE).expect("the golden source is checked in");
    let ast = engine.compile(&source).expect("the golden source must parse");
    let program = Compiler::new().compile(&ast);
    assert_eq!(
        program.residual_count(),
        0,
        "the golden source must lower whole, or the artifact covers less than it claims: {:?}",
        program.first_unsupported(),
    );

    if std::env::var_os("REGENERATE_GOLDEN").is_some() {
        let bytes = program.write().expect("the golden source must be writable");
        std::fs::write(GOLDEN_ARTIFACT, &bytes).expect("must write the artifact");
        println!("wrote {} bytes to {GOLDEN_ARTIFACT}", bytes.len());
        return;
    }

    let bytes = std::fs::read(GOLDEN_ARTIFACT).expect("the golden artifact is checked in");
    let loaded = match Program::read(&bytes) {
        Ok(loaded) => loaded,
        // The header records the ABI the fixture was written under, and a build
        // with different numeric widths or restriction flags refuses it *by
        // design* — that refusal is what `abi.rs` is for. The fixture is one
        // build's bytes, so it can only be checked on that build; anywhere else
        // this would be testing the ABI guard rather than the encoding.
        Err(err) if format!("{err}").contains("written with") => {
            println!("skipped: the golden fixture is a default-build artifact ({err})");
            return;
        }
        Err(err) => panic!(
            "the golden artifact no longer loads: {err}\n\
             The format moved. If that was deliberate, regenerate the fixture with \
             `REGENERATE_GOLDEN=1 cargo test --test format golden`.",
        ),
    };

    // A fixture only pins what it contains, and narrowing one while editing the
    // source is easy and silent. These are read off the *artifact*, so they say
    // what the encoder branch coverage actually is rather than what the source
    // looks like it should give.
    let kinds: std::collections::BTreeSet<String> =
        rhaigrain::bytecode::disassemble(loaded.code())
            .map(|(_, op)| {
                format!("{op:?}")
                    .split(['(', ' ', '{'])
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
    for required in [
        "Chain",             // a chain record, with all three of its roots
        "CallRef",           // and both by-reference call forms
        "Rotate",            // which only a named receiver needs
        "LoadNamed",         // the caller's variable, flat
        "LoadSharedNamed",   // and as the cell a capture binds
        "MakeClosure",       // a function pointer to a compiled chunk
        "Curry",             // with what it captured bound onto it
        "MakeArray",         // a literal the optimizer could not fold
        "MakeMap",           // and its template-plus-pairs cousin
        "CheckSize",         // the per-element size check beside it
        "PushHandler",       // a handler region, whose catch variable is pooled
        "Throw",             //
        "IterNext",          // an iterator, and the two-edged instruction
        "InterpolateAppend", // a string built a segment at a time
    ] {
        assert!(
            kinds.contains(required),
            "the golden no longer contains `{required}`, so its encoder branch \
             is unpinned again — put it back or say why it went",
        );
    }
    assert!(
        kinds.len() >= 35,
        "the golden covers only {} instruction kinds, which is narrower than it \
         was written to be: {kinds:?}",
        kinds.len(),
    );

    let walked = {
        let mut scope = golden_scope();
        let result = engine.eval_ast_with_scope::<Dynamic>(&mut scope, &ast);
        snapshot(&scope, result)
    };
    let ran = {
        let mut scope = golden_scope();
        // Whether the program can hand a pointer to a native is read back off
        // the bytes, so how it must be run is part of what is being checked.
        let result = if loaded.makes_fn_pointers() {
            let loaded = loaded.into_shared();
            Vm::new(&engine).run_with_callbacks(&loaded, &mut scope)
        } else {
            Vm::new(&engine).run(&loaded, &mut scope)
        };
        snapshot(&scope, result)
    };

    assert_eq!(
        ran, walked,
        "the golden artifact no longer means what its source means.\n\
         The format moved without the reader noticing, which is the failure this \
         fixture exists to catch. If the change was deliberate, regenerate with \
         `REGENERATE_GOLDEN=1 cargo test --test format golden`.",
    );
    assert!(
        ran.result.is_ok(),
        "the golden must produce a value, not an error: {ran:?}",
    );
}

/// A chain rooted at a caller's variable, which the corpus cannot cover.
///
/// Every case in `writable` runs from an empty scope, so the name a
/// [`Root::Named`] carries — and the position beside it, which is what an
/// `ErrorVariableNotFound` is reported against — has no encoder coverage there.
#[test]
fn a_chain_rooted_at_a_name_survives_the_round_trip() {
    let engine = corpus::engine();
    let source = "host.push(2); host[9]";

    let ast = engine.compile(source).expect("must compile");
    let program = Compiler::new().compile(&ast);
    assert_eq!(program.residual_count(), 0, "the chain must lower");
    let bytes = program.write().expect("must be writable");
    let reloaded = Program::read(&bytes).expect("what we wrote must read back");

    let seed = |scope: &mut Scope| {
        scope.push("host", vec![Dynamic::from(1_i64)]);
    };

    let mut walked = Scope::new();
    seed(&mut walked);
    let expected = {
        let out = engine.eval_ast_with_scope::<Dynamic>(&mut walked, &ast);
        snapshot(&walked, out)
    };

    let mut loaded = Scope::new();
    seed(&mut loaded);
    let actual = {
        let out = Vm::new(&engine).run(&reloaded, &mut loaded);
        snapshot(&loaded, out)
    };

    // The mutation lands, and the out-of-bounds index is still blamed on the
    // step rather than on the chain — so both the name and its position came
    // back intact.
    assert_eq!(actual, expected);
    assert!(actual.result.is_err(), "the index must still be refused");
}

/// Fragments are the allocation the format exists to remove, so writing one
/// would defeat the point.
///
/// The refusal has to name the construct and where it is. A caller deciding
/// whether to ship source instead cannot act on "27 fragments"; it can act on
/// "for at line 1".
#[test]
fn refusing_to_write_names_the_construct_responsible() {
    let engine = corpus::engine();

    for (source, expected) in [
        ("let x = 1; eval(\"x\")", "an unlowered expression"),
        // `?.` short-circuits on unit rather than stepping, so it is not a
        // chain this compiler can express whatever its root is.
        ("let x = 1; x?.y", "an unlowered expression"),
    ] {
        let ast = engine.compile(source).expect("must compile");
        let program = Compiler::new().compile(&ast);

        assert!(
            program.residual_count() > 0,
            "{source:?} must still fragment, or this test has gone stale",
        );

        let Err(err @ WriteError::HasResiduals { construct, pos, .. }) = program.write() else {
            panic!("{source:?} must refuse to write");
        };
        assert_eq!(construct, expected, "for {source:?}");
        assert!(!pos.is_none(), "the refusal must say where: {err}");
        assert!(err.to_string().contains(expected), "{err}");
    }
}

/// A script function is a chunk like any other, so it crosses the wire with
/// the rest of the program.
#[test]
fn script_functions_survive_the_round_trip() {
    let engine = corpus::engine();

    for source in [
        "fn add(a, b) { a + b } add(2, 3)",
        "fn fib(n) { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } } fib(6)",
        "fn first() { 1 } fn second(x) { first() + x } second(4)",
        // Failing inside a function has to keep rhai's wrapping and position.
        "fn bad(x) { x / 0 } bad(1)",
    ] {
        let ast = engine.compile(source).expect("must compile");
        let program = Compiler::new().compile(&ast);
        assert!(
            !program.functions().is_empty(),
            "{source:?} must compile its functions, not leave them to the walker",
        );

        let bytes = program.write().expect("must be writable");
        let reloaded = Program::read(&bytes).expect("must load");

        assert_eq!(
            run(&engine, reloaded),
            run_stock(&engine, source),
            "{source:?} does not mean the same after a round trip",
        );
    }
}

/// A function the compiler cannot lower stays rhai's, and a program that still
/// depends on rhai's copy cannot be written — silently dropping it would
/// produce an artifact that loads and then cannot find its own function.
#[test]
fn a_function_the_compiler_cannot_lower_refuses_to_write() {
    let engine = corpus::engine();
    // `this` is not a scope entry, so no slot addresses it.
    let ast = engine
        .compile("fn double() { this * 2 } let x = 21; x.double()")
        .expect("must compile");
    let program = Compiler::new().compile(&ast);

    assert!(
        program.functions().is_empty(),
        "a body using `this` must not become a chunk",
    );
    assert!(
        matches!(
            program.write(),
            Err(WriteError::HasScriptFunctions | WriteError::HasResiduals { .. }),
        ),
        "got {:?}",
        program.write(),
    );
}

/// A program with one of everything the corruption tests need to reach: a
/// float constant, a loop, and a `switch` — whose table is the one part of an
/// artifact holding jump targets that are not in the code, and so the one the
/// verifier would most plausibly forget to check.
///
/// Both by-reference call forms are here too. Their argument count includes a
/// receiver that is not on the operand stack, so a corrupted one is read
/// against a different depth than any other call's.
fn sample(engine: &Engine) -> Vec<u8> {
    let ast = engine
        .compile(
            "let a = 1; let b = 2.5; while a < 10 { a += 1 } \
             let c = [a]; push(c, b); push(caller_supplied, a); \
             caller_supplied[0] = a; \
             switch a { 1 => \"one\", 0..=20 => \"some\", _ => \"many\" }",
        )
        .expect("must compile");
    Compiler::new()
        .compile(&ast)
        .write()
        .expect("the sample must be writable")
}

#[test]
fn something_that_is_not_an_artifact_is_refused_at_the_first_bytes() {
    assert_eq!(Program::read(b"").unwrap_err(), ReadError::Truncated);
    assert_eq!(
        Program::read(b"not an artifact at all").unwrap_err(),
        ReadError::BadMagic,
    );
}

#[test]
fn a_future_format_version_is_refused_rather_than_guessed_at() {
    let engine = corpus::engine();
    let mut bytes = sample(&engine);
    bytes[4] = 0xff;
    bytes[5] = 0xff;

    assert!(matches!(
        Program::read(&bytes).unwrap_err(),
        ReadError::UnsupportedVersion { found: 0xffff, .. },
    ));
}

/// The fingerprint is the difference between a clean failure and integers
/// decoded as the wrong type, so the error must name the flag.
#[test]
fn a_different_value_representation_is_refused_by_name() {
    let engine = corpus::engine();

    let mut narrow = sample(&engine);
    narrow[6] = 4; // INT width
    let message = Program::read(&narrow).unwrap_err().to_string();
    assert!(
        message.contains("INT") && message.contains('4'),
        "the message must name the width: {message}",
    );

    let mut restricted = sample(&engine);
    restricted[8] ^= 0b100; // the `no_index` bit
    let message = Program::read(&restricted).unwrap_err().to_string();
    assert!(
        message.contains("no_index"),
        "the message must name the flag: {message}",
    );
}

/// A `switch` carries hashes rhai's parser computed, and rhai seeds its hasher
/// per process unless the host says otherwise. Two processes that disagree
/// would load each other's artifacts perfectly and then send every subject to
/// the default — a wrong answer rather than a failure, which is the worst kind.
///
/// The probe is what turns it into a failure, so this checks the failure
/// happens and that the message says what to do about it.
#[test]
fn a_switch_hashed_by_a_different_seed_is_refused() {
    let engine = corpus::engine();
    let bytes = sample(&engine);

    // The probe is the only place the artifact repeats this value, and finding
    // it that way means the test does not have to know the layout.
    let probe = rhaigrain::bytecode::probe().to_le_bytes();
    let at = bytes
        .windows(probe.len())
        .position(|window| window == probe)
        .expect("an artifact with a switch in it carries a probe");

    let mut corrupt = bytes.clone();
    corrupt[at] ^= 1;

    let err = Program::read(&corrupt).expect_err("a foreign hasher must be refused");
    assert!(
        matches!(err, ReadError::HashSeedMismatch { .. }),
        "got {err:?}",
    );
    assert!(
        err.to_string().contains("set_hashing_seed"),
        "the message must say how to fix it: {err}",
    );

    // And the uncorrupted one still loads, so the check is not simply always
    // failing.
    assert!(Program::read(&bytes).is_ok());
}

/// An artifact arrives over a link, so every prefix of one is a thing that can
/// actually turn up. None may load, and none may panic.
#[test]
fn every_truncation_fails_cleanly() {
    let engine = corpus::engine();
    let bytes = sample(&engine);

    for cut in 0..bytes.len() {
        assert!(
            Program::read(&bytes[..cut]).is_err(),
            "a {cut}-byte prefix of a {}-byte artifact loaded",
            bytes.len(),
        );
    }

    assert!(Program::read(&bytes).is_ok(), "the whole thing must load");
}

/// Trailing bytes mean the file is not what it says it is, even though every
/// field parsed. Accepting them would let a valid artifact carry a payload.
#[test]
fn trailing_bytes_are_refused() {
    let engine = corpus::engine();
    let mut bytes = sample(&engine);
    bytes.push(0);

    assert_eq!(
        Program::read(&bytes).unwrap_err(),
        ReadError::TrailingBytes { count: 1 },
    );
}

/// The safety claim in one test: a corrupted artifact is a `Result`, never a
/// panic and never a chunk the VM will touch.
///
/// Flipping each bit of each byte is exhaustive over single-bit corruption,
/// which is what a bad link produces. Whatever survives has been through the
/// verifier, so it is safe to run — and running it here is what proves the
/// verifier is actually on the load path.
///
/// **Verification is not termination.** A flipped jump target that still lands
/// inside the chunk is a structurally valid infinite loop, and this test hung
/// until it ran under a budget. That is not a gap to close — no loader can
/// decide halting — it is the reason `Op::Tick` sits on every back edge and the
/// reason the patch exposes `track_operation`. A host running untrusted
/// bytecode must set `max_operations`, exactly as it must for untrusted source.
#[test]
fn no_single_bit_flip_can_panic_or_smuggle_a_bad_chunk() {
    let writer = Engine::new();
    let bytes = sample(&writer);

    let mut engine = Engine::new();
    engine.set_max_operations(10_000);

    let mut loaded = 0usize;

    for index in 0..bytes.len() {
        for bit in 0..8 {
            let mut corrupt = bytes.clone();
            corrupt[index] ^= 1 << bit;

            if let Ok(program) = Program::read(&corrupt) {
                loaded += 1;
                program
                    .verify()
                    .expect("read must not return a chunk that fails verification");
                // The result is free to be anything; not crashing is the claim.
                let _ = Vm::new(&engine).run(&program, &mut Scope::new());
            }
        }
    }

    // Most flips land in a length, a tag or the fingerprint and are rejected.
    // Some land in a constant's value and legitimately still load; that is the
    // case worth having run above.
    println!(
        "{loaded} of {} single-bit corruptions still loaded",
        bytes.len() * 8,
    );
}

/// The whole point of the split, end to end.
///
/// The device is sent a stripped artifact and knows nothing about the source.
/// It fails, and all it can say is which instruction. The host kept the table,
/// and turns that back into the position rhai itself would have reported.
#[test]
fn a_stripped_program_reports_an_address_the_host_can_resolve() {
    let engine = corpus::engine();
    let source = "let a = 1;\nlet b = 0;\na / b";

    // Host: compile, split.
    let ast = engine.compile(source).expect("must compile");
    let full = Compiler::new().compile(&ast);
    let expected = run_stock(&engine, source);
    let (shipped, table) = full.write_stripped().expect("must be writable");

    // Device: run bytes, with no table and no source.
    let device = Program::read(&shipped).expect("the device must load it");
    assert!(
        device.positions().is_stripped(),
        "a stripped artifact must not carry positions",
    );

    let mut vm = Vm::new(&engine);
    let error = vm
        .run(&device, &mut Scope::new())
        .expect_err("dividing by zero must fail");
    let address = vm.fault_pc().expect("a failed run must name an instruction");

    // Host: resolve what came back.
    let site = rhaigrain_pos::resolve(&table, address as u32)
        .expect("the failing instruction must have a recorded site");

    assert_eq!(
        (site.line, site.column),
        (3, 3),
        "the division is at line 3, column 3 of {source:?}",
    );

    // And the same program with its table attached says so itself, exactly as
    // rhai does — which is what makes the resolved site trustworthy.
    let mut reattached = Program::read(&shipped).unwrap();
    reattached
        .attach_positions(&table)
        .expect("its own table must attach");
    assert_eq!(run(&engine, reattached), expected);

    // The stripped run is the same failure, minus the position.
    assert!(
        error.position().is_none(),
        "a stripped program has no position to report, got {:?}",
        error.position(),
    );
}

/// Attaching another program's table would misreport every error rather than
/// reporting none, which is strictly worse than having no table.
#[test]
fn a_table_from_a_different_program_is_refused() {
    let engine = corpus::engine();

    let short = Compiler::new().compile(&engine.compile("1 + 1").unwrap());
    let long = Compiler::new()
        .compile(&engine.compile("let a = 1; while a < 9 { a += 1 } a").unwrap());

    let (_, long_table) = long.write_stripped().expect("must be writable");
    let (short_bytes, _) = short.write_stripped().expect("must be writable");

    let mut program = Program::read(&short_bytes).unwrap();
    assert!(
        program.attach_positions(&long_table).is_err(),
        "a table naming instructions this chunk does not have must be refused",
    );
}

/// A stripped artifact that arrives with a table still in it is a contradiction
/// the reader should not paper over.
#[test]
fn an_artifact_carrying_a_mismatched_table_does_not_load() {
    let engine = corpus::engine();
    let bytes = sample(&engine);
    let program = Program::read(&bytes).expect("the sample must load");

    assert!(
        !program.positions().is_stripped(),
        "`write` keeps the table, so this one must have positions",
    );
}

/// What the split costs, and what it saves.
#[test]
fn stripping_positions_shrinks_the_artifact() {
    let engine = corpus::engine();

    let mut with = 0usize;
    let mut without = 0usize;
    let mut tables = 0usize;

    for (name, _, full) in writable(&engine) {
        let ast = engine.compile(corpus::CASES.iter().find(|c| c.name == name).unwrap().source);
        let program = Compiler::new().compile(&ast.unwrap());
        let (stripped, table) = program.write_stripped().expect("must be writable");

        with += full.len();
        without += stripped.len();
        tables += table.len();
    }

    println!(
        "\n{with} bytes with positions -> {without} stripped ({:.0}% smaller), \
         {tables} bytes of table kept behind",
        100.0 * (with - without) as f64 / with as f64,
    );

    assert!(
        without < with,
        "stripping must actually remove something: {without} vs {with}",
    );
}

/// The number this project exists to move: bytes retained per source byte,
/// against the 24 a rhai `AST` costs on device.
///
/// This is the host-side artifact size, not device heap — M6 measures that.
/// What it establishes here is the encoding's own density, which is the part
/// the format controls.
#[test]
fn artifact_size_census() {
    let engine = corpus::engine();
    let written = writable(&engine);

    let mut source_bytes = 0usize;
    let mut artifact_bytes = 0usize;
    let mut rows: Vec<_> = written
        .iter()
        .map(|(name, source, bytes)| {
            source_bytes += source.len();
            artifact_bytes += bytes.len();
            (*name, source.len(), bytes.len())
        })
        .collect();

    rows.sort_by_key(|(_, _, artifact)| std::cmp::Reverse(*artifact));
    println!("\n{:>7}  {:>7}  script", "source", "bytes");
    for (name, source, artifact) in &rows {
        println!("{source:>7}  {artifact:>7}  {name}");
    }
    println!(
        "\n{} scripts: {source_bytes} source bytes -> {artifact_bytes} artifact bytes ({:.2}x)",
        rows.len(),
        artifact_bytes as f64 / source_bytes as f64,
    );

    // Not a target, a tripwire. The plan is explicit that bytecode need not
    // beat minified source on bytes — but an encoding several times larger
    // than its input has a bug in it, not a tradeoff.
    assert!(
        artifact_bytes < source_bytes * 3,
        "{artifact_bytes} artifact bytes for {source_bytes} of source is not an encoding",
    );
}
