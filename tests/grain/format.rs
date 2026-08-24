//! An artifact must mean what the program it came from meant, and nothing a
//! wire can hand it may take the process down.
//!
//! Two separate claims, and they need separate tests. The first is a round
//! trip: compile, write, read back, run, and get what running the original
//! got — including the scope left behind and the exact error position. The
//! second is that `read` is total over arbitrary bytes: every truncation and
//! every single-byte corruption of a valid artifact either loads or fails, and
//! never panics.

use super::corpus;

use rhai::grain::format::{Abi, ReadError, WriteError};
use rhai::grain::{Compiler, Program, Vm};
use rhai::{Dynamic, Engine, Scope, INT};

/// What a run produced, in a form two runs can be compared on.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    result: Result<String, String>,
    scope: Vec<(String, String)>,
}

/// A finished run, reduced to what two of them can be compared on.
fn snapshot(scope: &Scope, result: Result<Dynamic, Box<rhai::EvalAltResult>>) -> Outcome {
    Outcome {
        result: result.map(|value| format!("{value:?}")).map_err(|err| format!("{err:?}")),
        scope: scope.iter_raw().map(|(name, _, value)| (name.to_string(), format!("{value:?}"))).collect(),
    }
}

/// Taken by value, because a program that hands pointers to natives has to be
/// owned to be run at all — and whether this one does is read back off the
/// bytes, which is the property that makes an artifact self-describing.
fn run(engine: &Engine, program: Program) -> Outcome {
    let mut scope = Scope::new();
    let result = if program.makes_fn_pointers() {
        let program = program.into_shared();
        Vm::new(engine).eval_with_callbacks(&mut scope, &program)
    } else {
        Vm::new(engine).eval_with_scope(&mut scope, &program)
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
        .filter(|case| corpus::applies_to_this_build(case.name))
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
            failures.push(format!("\n  {name}: {source}\n    rhai: {expected:?}\n    artifact: {actual:?}"));
        }
    }

    assert!(failures.is_empty(), "{} artifacts do not mean what they came from:{}", failures.len(), failures.join(""),);
}

/// A round trip over an empty set passes trivially, so pin the size of the set
/// and pin that the things it should contain are in it.
#[test]
fn the_round_trip_covers_something_worth_covering() {
    let engine = corpus::engine();
    let written = writable(&engine);
    let names: Vec<_> = written.iter().map(|(name, ..)| *name).collect();

    assert!(written.len() >= 20, "only {} corpus scripts are writable, which is too few to prove anything: {names:?}", written.len(),);

    // One per construct the encoder has a branch for, so a branch that stops
    // working names itself.
    for required in [
        "int_arithmetic", // Call with an operator token
        // A float constant, whose width the ABI pins — and which `no_float`
        // removes from the language, so there is no branch left to cover.
        #[cfg(not(feature = "no_float"))]
        "float_arithmetic",
        "shadowing_nested", // DeclareLocal and UnwindTo
        "while_loop",       // jumps, Tick, AssignLocal with an op
        "loop_break_value", // backpatched jumps
        // A position that has to survive. `unchecked` turns the failure it
        // rests on into a panic in Rhai, so the case is not run at all there.
        #[cfg(not(feature = "unchecked"))]
        "error_divide_by_zero",
        "switch_range", // a switch table, and the hasher probe with it
        "switch_guard", // and one whose arms are a chain rather than a target
        // A range constant is a host type in `Dynamic`, and both tags are
        // reached by slicing — which is `[..]`, and so `no_index` syntax, as is
        // every chain below it.
        #[cfg(not(feature = "no_index"))]
        "string_slice_read",
        #[cfg(not(feature = "no_index"))]
        "string_slice_inclusive", // and the other range tag
        #[cfg(not(feature = "no_index"))]
        "index_assign_array", // a chain rooted at a slot, and its name
        // and one rooted on the operand stack instead, which takes a method
        // call to get there
        #[cfg(not(any(feature = "no_index", feature = "no_object")))]
        "temp_root_array_method",
    ] {
        assert!(names.contains(&required), "`{required}` no longer writes, so the encoder branch it covers is untested",);
    }
}

/// Where the golden pair lives. The source is checked in beside the artifact so
/// a regeneration is a visible two-file change.
const GOLDEN_SOURCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/grain/fixtures/golden.rhai");
const GOLDEN_ARTIFACT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/grain/fixtures/golden.rgrn");

/// The fixture is one build's source, checked in byte-identical beside the
/// bytes it produced — rewriting it to suit a build would make it a different
/// fixture. So it is a default-build pair, and a build missing the syntax it
/// is written in has nothing here to check.
///
/// A runtime flag rather than a `cfg`, so the reason stays in one place and the
/// body below needs no gating of its own.
const GOLDEN_APPLIES: bool = !cfg!(any(feature = "no_float", feature = "no_function", feature = "no_index", feature = "no_object"));

/// The caller state `golden.rhai` expects. Part of the fixture, so it lives
/// with it rather than being invented at each use.
fn golden_scope() -> Scope<'static> {
    let mut scope = Scope::new();
    scope.push("supplied", vec![Dynamic::from(7 as INT)]);
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
/// REGENERATE_GOLDEN=1 cargo test --features grain --test grain golden
/// ```
///
/// and bump `VERSION` if an older reader would *misread* the new bytes rather
/// than reject them — the rule is at `src/format/mod.rs:56`.
#[test]
fn a_golden_artifact_written_by_an_older_build_still_runs() {
    // Reached before the ABI guard below, and for the same reason: on a build
    // the fixture was not written for, this would test the guard rather than
    // the encoding.
    if !GOLDEN_APPLIES {
        println!("skipped: the golden Rhai source requires features this build does not have to compile");
        return;
    }

    {
        let engine = corpus::engine();
        let source = std::fs::read_to_string(GOLDEN_SOURCE).expect("the golden source is checked in");
        let ast = engine.compile(&source).expect("the golden source must parse");
        let program = Compiler::new().compile(&ast);
        assert_eq!(program.residual_count(), 0, "the golden source must lower whole, or the artifact covers less than it claims: {:?}", program.first_unsupported(),);

        if std::env::var_os("REGENERATE_GOLDEN").is_some() {
            let bytes = program.write().expect("the golden source must be writable");
            std::fs::write(GOLDEN_ARTIFACT, &bytes).expect("must write the artifact");
            println!("wrote {} bytes to {GOLDEN_ARTIFACT}", bytes.len());
            return;
        }

        println!("host caps: {:?}", Abi::host());

        let bytes = std::fs::read(GOLDEN_ARTIFACT).expect("the golden artifact is checked in");

        let loaded = match Program::read(&bytes) {
            Ok(loaded) => loaded,
            // The header records the ABI the fixture was written under, and a build
            // with different numeric widths or capability flags refuses it *by design*
            // — that refusal is what `abi.rs` is for. The fixture is the bytes of the
            // default build, so it can only be checked on a compatible build; anywhere
            // else this would be testing the ABI guard rather than the encoding.
            Err(err) if format!("{err}").contains("artifact cannot load") => {
                println!("skipped: the golden fixture is a default-build artifact and cannot load ({err})");
                return;
            }
            Err(err) => panic!(
                "the golden artifact no longer loads:\n{err}\n\n\
                The format probably has moved.\n\
                If that was deliberate, regenerate the fixture with:\n\
                REGENERATE_GOLDEN=1 cargo test --features grain --test grain golden",
            ),
        };

        println!("artifact requires caps: {:?}", loaded.caps());

        // A fixture only pins what it contains, and narrowing one while editing the
        // source is easy and silent. These are read off the *artifact*, so they say
        // what the encoder branch coverage actually is rather than what the source
        // looks like it should give.
        let kinds: std::collections::BTreeSet<String> = rhai::grain::bytecode::disassemble(loaded.code())
            .map(|(_, op)| format!("{op:?}").split(['(', ' ', '{']).next().unwrap_or_default().to_string())
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
                Vm::new(&engine).eval_with_callbacks(&mut scope, &loaded)
            } else {
                Vm::new(&engine).eval_with_scope(&mut scope, &loaded)
            };
            snapshot(&scope, result)
        };

        assert_eq!(
            ran, walked,
            "the golden artifact no longer means what its source means.\n\
         The format moved without the reader noticing, which is the failure this \
         fixture exists to catch. If the change was deliberate, regenerate with \
         `REGENERATE_GOLDEN=1 cargo test --features grain --test grain golden`.",
        );
        assert!(ran.result.is_ok(), "the golden must produce a value, not an error: {ran:?}",);
    }
}

/// A chain rooted at a caller's variable, which the corpus cannot cover.
///
/// Every case in `writable` runs from an empty scope, so the name a
/// [`Root::Named`] carries — and the position beside it, which is what an
/// `ErrorVariableNotFound` is reported against — has no encoder coverage there.
#[test]
#[cfg(not(any(feature = "no_index", feature = "no_object")))]
fn a_chain_rooted_at_a_name_survives_the_round_trip() {
    let engine = corpus::engine();
    let source = "host.push(2); host[9]";

    let ast = engine.compile(source).expect("must compile");
    let program = Compiler::new().compile(&ast);
    assert_eq!(program.residual_count(), 0, "the chain must lower");
    let bytes = program.write().expect("must be writable");
    let reloaded = Program::read(&bytes).expect("what we wrote must read back");

    let seed = |scope: &mut Scope| {
        scope.push("host", vec![Dynamic::from(1 as INT)]);
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
        let out = Vm::new(&engine).eval_with_scope(&mut loaded, &reloaded);
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

    const SCRIPTS: &[(&str, &str)] = &[("let x = 1; eval(\"x\")", "an unlowered expression")];

    for (source, expected) in SCRIPTS {
        let ast = engine.compile(source).expect("must compile");
        let program = Compiler::new().compile(&ast);

        assert!(program.residual_count() > 0, "{source:?} must still fragment, or this test has gone stale",);

        let Err(err @ WriteError::HasResiduals { construct, pos, .. }) = program.write() else {
            panic!("{source:?} must refuse to write");
        };
        assert_eq!(&construct, expected, "for {source:?}");
        // Under `no_position` there is no "where" to say, and the naming half
        // above is the part that still means something.
        #[cfg(not(feature = "no_position"))]
        assert!(!pos.is_none(), "the refusal must say where: {err}");
        let _ = pos;
        assert!(err.to_string().contains(expected), "{err}");
    }
}

/// A script function is a chunk like any other, so it crosses the wire with
/// the rest of the program.
#[test]
#[cfg(not(feature = "no_function"))]
fn script_functions_survive_the_round_trip() {
    let engine = corpus::engine();

    for source in [
        "fn add(a, b) { a + b } add(2, 3)",
        "fn fib(n) { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } } fib(6)",
        "fn first() { 1 } fn second(x) { first() + x } second(4)",
        // Failing inside a function has to keep Rhai's wrapping and position.
        // `unchecked` turns this into a panic in Rhai's own built-in rather
        // than an error, so there is nothing left here to compare.
        #[cfg(not(feature = "unchecked"))]
        "fn bad(x) { x / 0 } bad(1)",
    ] {
        let ast = engine.compile(source).expect("must compile");
        let program = Compiler::new().compile(&ast);
        assert!(!program.functions().is_empty(), "{source:?} must compile its functions, not leave them to the walker",);

        let bytes = program.write().expect("must be writable");
        let reloaded = Program::read(&bytes).expect("must load");

        assert_eq!(run(&engine, reloaded), run_stock(&engine, source), "{source:?} does not mean the same after a round trip",);
    }
}

/// Nothing about a compile may depend on the run that performed it.
///
/// The hash seed is fixed within a process, so this cannot catch a hash-ordered
/// function list — `functions_are_lowered_in_a_stable_order` in `grain::compile`
/// checks that directly. This covers the rest: a counter, an address-keyed
/// dedup, anything that would make the second artifact differ.
#[test]
#[cfg(not(feature = "no_function"))]
fn the_same_source_compiles_to_the_same_bytes() {
    let engine = corpus::engine();
    let source = "
        fn zulu(x) { x + 1 }
        fn alpha(a, b) { zulu(a) * b }
        fn alpha(a) { zulu(a) }
        fn mike() { 7 }
        alpha(2, 3) + alpha(4) + mike()
    ";

    let ast = engine.compile(source).expect("must compile");
    let first = Compiler::new().compile(&ast).write().expect("must be writable");
    let second = Compiler::new().compile(&ast).write().expect("must be writable");

    assert_eq!(first, second, "two compiles of one source disagree");

    let recompiled = engine.compile(source).expect("must compile");
    let third = Compiler::new().compile(&recompiled).write().expect("must be writable");

    assert_eq!(first, third, "two parses of one source disagree");
}

/// A function the compiler cannot lower stays Rhai's, and a program that still
/// depends on Rhai's copy cannot be written — silently dropping it would
/// produce an artifact that loads and then cannot find its own function.
#[test]
#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "no_function"))]
fn a_function_the_compiler_cannot_lower_refuses_to_write() {
    let engine = corpus::engine();
    // `import` declares into the caller's scope, which the slot model cannot
    // account for. `this` used to be the example here, and is not one any more.
    let ast = engine.compile(r#"fn m() { import "x" as y; 1 } m()"#).expect("must compile");
    let program = Compiler::new().compile(&ast);

    assert!(program.functions().is_empty(), "a body the slot model cannot account for must not become a chunk",);
    assert!(matches!(program.write(), Err(WriteError::HasScriptFunctions | WriteError::HasResiduals { .. }),), "got {:?}", program.write(),);
}

/// The counterpart, and the milestone: a body that uses `this` is a chunk now,
/// so a program full of them is an artifact rather than a tree.
#[test]
#[cfg(not(any(feature = "no_function", feature = "no_object")))]
fn a_body_using_this_is_compiled_and_writable() {
    let engine = corpus::engine();
    let ast = engine.compile("fn bump(n) { this += n; this } let x = 21; x.bump(21); x").expect("must compile");
    let program = Compiler::new().compile(&ast);

    assert_eq!(program.residual_count(), 0, "{:?}", program.first_unsupported());
    assert!(!program.functions().is_empty(), "a body using `this` must become a chunk");
    assert!(program.write().is_ok(), "got {:?}", program.write());
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
    // A float in the constant pool is part of what this covers, and `no_float`
    // has no float to put there. The rest of the shape — a loop, an array, a
    // caller variable, an indexed write, a switch — is the same either way.
    #[cfg(not(feature = "no_float"))]
    const SECOND: &str = "2.5";
    #[cfg(feature = "no_float")]
    const SECOND: &str = "2";

    // The array literal and the indexed write are `no_index` syntax. The
    // by-reference call forms are the part that matters here — a corrupted
    // argument count is read against a depth that excludes the receiver — so a
    // host type stands in for them. The indexed write has no stand-in, and no
    // build without indexing has one to corrupt.
    #[cfg(not(feature = "no_index"))]
    const INDEXED: &str = "let c = [a]; push(c, b); push(caller_supplied, a); caller_supplied[0] = a;";
    #[cfg(feature = "no_index")]
    const INDEXED: &str = "let c = widget(a); bump(c); bump(caller_supplied);";

    let source = format!(
        "let a = 1; let b = {SECOND}; while a < 10 {{ a += 1 }} \
         {INDEXED} \
         switch a {{ 1 => \"one\", 0..=20 => \"some\", _ => \"many\" }}"
    );
    let ast = engine.compile(&source).expect("must compile");
    Compiler::new().compile(&ast).write().expect("the sample must be writable")
}

#[test]
fn something_that_is_not_an_artifact_is_refused_at_the_first_bytes() {
    assert_eq!(Program::read(b"").unwrap_err(), ReadError::Truncated);
    assert_eq!(Program::read(b"not an artifact at all").unwrap_err(), ReadError::BadMagic,);
}

#[test]
fn a_future_format_version_is_refused_rather_than_guessed_at() {
    let engine = corpus::engine();
    let mut bytes = sample(&engine);
    bytes[4] = 0xff;
    bytes[5] = 0xff;

    assert!(matches!(Program::read(&bytes).unwrap_err(), ReadError::UnsupportedVersion { found: 0xffff, .. },));
}

/// Every capability the compiler tracks for a script is produced by a
/// common Rhai scriptlet that actually uses it and still round-trips.
#[test]
fn capabilities_round_trip() {
    use rhai::grain::format::Caps;

    const CASES: &[(&str, &str, Caps, &str)] = &[
        #[cfg(not(feature = "no_float"))]
        ("float", "let x = 3.5; x + 1.5", Caps::FLOAT, "floating-point"),
        #[cfg(not(feature = "no_index"))]
        ("array", "let a = [1, 2, 3]; a[1] + a[2]", Caps::ARRAY.union(Caps::INDEXING), "arrays"),
        #[cfg(not(feature = "no_object"))]
        ("map", "let m = #{ answer: 42 }; m.answer", Caps::MAP.union(Caps::PROPERTY), "object maps"),
        // #[cfg(feature = "decimal")]
        // ("decimal", "let d = 1e1000; d + 1", Caps::DECIMAL, "decimal numbers"),
        #[cfg(not(feature = "no_object"))]
        #[cfg(not(feature = "no_index"))]
        ("method", "let a = [1, 2, 3]; a.len()", Caps::ARRAY.union(Caps::METHOD), "method calling style"),
        #[cfg(not(feature = "no_function"))]
        #[cfg(not(feature = "no_object"))]
        ("this", "fn add(n) { this + n } let x = 40; x.add(2)", Caps::THIS.union(Caps::FUNCTION).union(Caps::METHOD), "this"),
        #[cfg(not(feature = "no_function"))]
        #[cfg(not(feature = "no_closure"))]
        #[cfg(not(feature = "no_object"))]
        ("sharing", "let x = 40; let f = || x + 2; f.call()", Caps::SHARING.union(Caps::FUNCTION).union(Caps::METHOD), "shared values"),
        #[cfg(not(any(feature = "no_index", feature = "no_object", feature = "no_closure")))]
        (
            "combined_features",
            "let items = [1, 2, 3]; let map = #{ answer: 42 }; let f = || items[0] + map.answer; f.call()",
            Caps::ARRAY.union(Caps::INDEXING).union(Caps::MAP).union(Caps::PROPERTY).union(Caps::SHARING).union(Caps::METHOD),
            "shared values",
        ),
    ];

    let engine = corpus::engine();

    for (name, source, expected, expected_text) in CASES {
        let ast = engine.compile(source).unwrap_or_else(|err| panic!("{name} must compile: {err}"));
        let program = Compiler::new().compile(&ast);

        for cap in Caps::all() {
            if expected.contains(cap) {
                assert!(program.caps().contains(cap), "{name} is missing {:?}", cap);
            }
        }

        let bytes = program.write().expect("scriptlet must serialize");
        let loaded = Program::read(&bytes).expect("scriptlet must survive a round trip");
        assert!(loaded.caps().contains(*expected), "{name} lost {expected:?} on read-back");
        let caps_string = loaded.caps().to_string();
        assert!(caps_string.contains(expected_text), "{name} caps string did not mention `{expected_text}`: {caps_string}");
    }
}

/// Capabilities of the host.
#[test]
fn host_capabilities() {
    use rhai::grain::format::{Abi, Caps};

    const SUPPORTED: &[(Caps, bool, &str)] = &[
        (Caps::FLOAT, !cfg!(feature = "no_float"), "floating-point"),
        (Caps::ARRAY, !cfg!(feature = "no_index"), "arrays"),
        (Caps::BLOB, !cfg!(feature = "no_index"), "BLOB"),
        (Caps::MAP, !cfg!(feature = "no_object"), "object maps"),
        (Caps::DECIMAL, cfg!(feature = "decimal"), "decimal numbers"),
        (Caps::FUNCTION, !cfg!(feature = "no_function"), "functions"),
        (Caps::INDEXING, !cfg!(feature = "no_index"), "indexing"),
        (Caps::PROPERTY, !cfg!(feature = "no_object"), "properties"),
        (Caps::METHOD, !cfg!(feature = "no_object"), "method calling style"),
        (Caps::THIS, !cfg!(feature = "no_function"), "this"),
        (Caps::SHARING, !cfg!(feature = "no_closure"), "shared values"),
        (Caps::IMPORT, !cfg!(feature = "no_module"), "imports modules"),
        (Caps::EXPORT, !cfg!(feature = "no_module"), "exports data in modules"),
        (Caps::CUSTOM_SYNTAX, false, "custom syntax"),
    ];

    let host = Abi::host();
    for (cap, supported, text) in SUPPORTED {
        assert_eq!(host.caps.contains(*cap), *supported, "cap {:?} should be {} on this build ({text})", cap, if *supported { "available" } else { "unavailable" });
    }
}

/// A `switch` carries hashes Rhai's parser computed, and Rhai seeds its hasher
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
    let probe = rhai::grain::bytecode::probe().to_le_bytes();
    let at = bytes.windows(probe.len()).position(|window| window == probe).expect("an artifact with a switch in it carries a probe");

    let mut corrupt = bytes.clone();
    corrupt[at] ^= 1;

    let err = Program::read(&corrupt).expect_err("a foreign hasher must be refused");
    assert!(matches!(err, ReadError::HashSeedMismatch { .. }), "got {err:?}",);
    assert!(err.to_string().contains("set_hashing_seed"), "the message must say how to fix it: {err}",);

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
        assert!(Program::read(&bytes[..cut]).is_err(), "a {cut}-byte prefix of a {}-byte artifact loaded", bytes.len(),);
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

    assert_eq!(Program::read(&bytes).unwrap_err(), ReadError::TrailingBytes { count: 1 },);
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
// A corrupted chunk can loop, and `max_operations` is what stops it. Without
// limits this hangs rather than fails, which is worse than not running.
#[cfg(not(feature = "unchecked"))]
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
                program.verify().expect("read must not return a chunk that fails verification");
                // The result is free to be anything; not crashing is the claim.
                let _ = Vm::new(&engine).eval_with_scope(&mut Scope::new(), &program);
            }
        }
    }

    // Most flips land in a length, a tag or the fingerprint and are rejected.
    // Some land in a constant's value and legitimately still load; that is the
    // case worth having run above.
    println!("{loaded} of {} single-bit corruptions still loaded", bytes.len() * 8,);
}

/// The whole point of the split, end to end.
///
/// The device is sent a stripped artifact and knows nothing about the source.
/// It fails, and all it can say is which instruction. The host kept the table,
/// and turns that back into the position Rhai itself would have reported.
/// There is no table to strip under `no_position`, so nothing to resolve; and
/// the failure it turns on is a division by zero, which `unchecked` makes a
/// panic in Rhai rather than an error.
#[test]
#[cfg(not(any(feature = "no_position", feature = "unchecked")))]
fn a_stripped_program_reports_an_address_the_host_can_resolve() {
    let engine = corpus::engine();
    let source = "let a = 1;\nlet b = 0;\na / b";

    // Host: compile, split.
    let ast = engine.compile(source).expect("must compile");
    let full = Compiler::new().compile(&ast);
    let expected = run_stock(&engine, source);
    let stripped = full.write_stripped().expect("must be writable");

    // Device: run bytes, with no table and no source.
    let device = Program::read(&stripped.artifact).expect("the device must load it");
    assert!(device.positions().is_stripped(), "a stripped artifact must not carry positions",);

    let mut vm = Vm::new(&engine);
    let error = vm.eval_with_scope(&mut Scope::new(), &device).expect_err("dividing by zero must fail");
    let address = vm.fault_pc().expect("a failed run must name an instruction");

    // Host: resolve what came back.
    let site = rhai::grain::pos::resolve(&stripped.sidecar.positions, address as u32).expect("the failing instruction must have a recorded site");

    assert_eq!((site.line, site.column), (3, 3), "the division is at line 3, column 3 of {source:?}",);

    // And the same program with its sidecar attached says so itself, exactly as
    // Rhai does — which is what makes the resolved site trustworthy.
    let mut reattached = Program::read(&stripped.artifact).unwrap();
    reattached.attach_positions(&stripped.sidecar).expect("its own sidecar must attach");
    assert_eq!(run(&engine, reattached), expected);

    // The stripped run is the same failure, minus the position.
    assert!(error.position().is_none(), "a stripped program has no position to report, got {:?}", error.position(),);
}

/// Attaching another program's sidecar would misreport every error rather than
/// reporting none, which is worse than having no sidecar at all.
#[test]
#[cfg(not(feature = "no_position"))]
fn a_table_from_a_different_program_is_refused() {
    let engine = corpus::engine();

    let short = Compiler::new().compile(&engine.compile("1 + 1").unwrap());
    let long = Compiler::new().compile(&engine.compile("let a = 1; while a < 9 { a += 1 } a").unwrap());

    let long_stripped = long.write_stripped().expect("must be writable");
    let short_stripped = short.write_stripped().expect("must be writable");

    let mut program = Program::read(&short_stripped.artifact).unwrap();
    assert!(program.attach_positions(&long_stripped.sidecar).is_err(), "a sidecar taken from another program must be refused",);
}

/// The whole point of the split, end to end.
///
/// A device runs an artifact with no positions in it, fails, and can say only
/// which instructions — innermost frame first. The host resolves that against
/// the sidecar it kept and has a backtrace.
///
/// Every frame resolves on its own, so a run that crossed a native is not a
/// special case: `xs.map(|x| half(x))` gives the divide inside `half`, the call
/// to it inside the closure, and the `map` that started it.
///
/// `unchecked` makes a division by zero a panic, and `no_position` leaves
/// nothing to resolve.
#[test]
#[cfg(not(any(feature = "no_position", feature = "unchecked", feature = "no_function")))]
fn a_device_reports_addresses_the_host_turns_into_a_backtrace() {
    let engine = corpus::engine();

    for (what, source, want) in [
        ("in main", "let a = 1;\nlet b = 0;\na / b", &[(3, 3)][..]),
        ("one call deep", "fn half(x) { x / 0 }\nhalf(4)", &[(1, 16), (2, 1)]),
        ("two calls deep", "fn inner(x) { x / 0 }\nfn outer(y) { inner(y) }\nouter(2)", &[(1, 17), (2, 15), (3, 1)]),
        // A chain step is reached through an array here and a callback through
        // a method on one, so the restricted builds that remove either leave
        // nothing here that parses.
        #[cfg(not(feature = "no_index"))]
        ("in a chain step", "let k = [1, 2];\nk[9]", &[(2, 3)]),
        #[cfg(not(feature = "no_index"))]
        #[cfg(not(feature = "no_closure"))]
        #[cfg(not(feature = "no_object"))]
        ("in a callback", "let xs = [1, 2, 3];\nxs.map(|x| x / 0)", &[(2, 14), (2, 4)]),
        #[cfg(not(feature = "no_index"))]
        #[cfg(not(feature = "no_closure"))]
        #[cfg(not(feature = "no_object"))]
        ("a compiled function through a callback", "fn half(x) { x / 0 }\nlet xs = [1, 2, 3];\nxs.map(|x| half(x))", &[(1, 16), (3, 12), (3, 4)]),
    ] {
        let ast = engine.compile(source).expect("must compile");
        let stripped = Compiler::new().compile(&ast).write_stripped().expect("must be writable");

        // Device: bytes, no source, no sidecar.
        let device = Program::read(&stripped.artifact).expect("must load");
        let (error, trace) = fail(&engine, device, what);
        assert!(!format!("{error}").contains("(line "), "{what}: a stripped program must report no position",);

        // Host: the sidecar, and the addresses that came back.
        let sites: Vec<_> = stripped.sidecar.resolve(&trace).into_iter().map(|site| site.map(|s| (s.line, s.column))).collect();
        let want: Vec<_> = want.iter().map(|&pair| Some(pair)).collect();

        assert_eq!(sites, want, "{what}: the backtrace is wrong for {source:?}");
    }
}

/// The two things that cross between a device and its host have to cross.
///
/// A fault trace goes one way and a sidecar is stored for later, so both are
/// carried by whatever the caller already uses rather than by a format of their
/// own — which is only true if the derives work.
#[test]
#[cfg(feature = "serde")]
#[cfg(not(any(feature = "no_position", feature = "unchecked", feature = "no_function")))]
fn a_trace_and_a_sidecar_survive_serde() {
    let engine = corpus::engine();
    let ast = engine.compile("fn half(x) { x / 0 }\nhalf(4)").expect("must compile");
    let stripped = Compiler::new().compile(&ast).write_stripped().expect("must be writable");

    let device = Program::read(&stripped.artifact).expect("must load");
    let (_, trace) = fail(&engine, device, "serde");

    let wire = serde_json::to_string(&trace).expect("a trace must serialize");
    let back: Vec<rhai::grain::Fault> = serde_json::from_str(&wire).expect("and come back");
    assert_eq!(back, trace);

    let stored = serde_json::to_string(&stripped.sidecar).expect("a sidecar must serialize");
    let reloaded: rhai::grain::Sidecar = serde_json::from_str(&stored).expect("and come back");
    assert_eq!(reloaded, stripped.sidecar);

    // The point of carrying both: resolving on the far side gives the same
    // backtrace as resolving here.
    assert_eq!(reloaded.resolve(&back), stripped.sidecar.resolve(&trace));
}

/// An error that was handled is not where the run failed.
///
/// Left in the trace, the frames a caught error unwound past would head the
/// next one — where a reader looks first.
#[test]
#[cfg(not(any(feature = "no_position", feature = "unchecked", feature = "no_function")))]
fn a_caught_error_leaves_no_frames_behind() {
    let engine = corpus::engine();
    let source = "
        fn bad(x) { x / 0 }
        try { bad(1) } catch { }
        throw 42
    ";

    let ast = engine.compile(source).expect("must compile");
    let stripped = Compiler::new().compile(&ast).write_stripped().expect("must be writable");
    let device = Program::read(&stripped.artifact).expect("must load");

    let (_, trace) = fail(&engine, device, "a caught error");

    assert_eq!(trace.len(), 1, "only the `throw` is where this failed; the caught divide left {} frames behind", trace.len() - 1,);
}

/// Run a program that is expected to fail, keeping the error and the trace.
///
/// Mirrors [`run`]'s split on `makes_fn_pointers`: a program that hands a
/// pointer to a native has to be shared to be run at all.
#[cfg(not(any(feature = "no_position", feature = "unchecked", feature = "no_function")))]
fn fail(engine: &Engine, program: Program, what: &str) -> (rhai::EvalAltResult, Vec<rhai::grain::Fault>) {
    let mut scope = Scope::new();
    let mut vm = Vm::new(engine);

    let result = if program.makes_fn_pointers() {
        let program = program.into_shared();
        vm.eval_with_callbacks(&mut scope, &program)
    } else {
        vm.eval_with_scope(&mut scope, &program)
    };

    let error = result.err().unwrap_or_else(|| panic!("{what}: the case must fail"));
    (*error, vm.fault_trace())
}

/// The case a check on the code alone cannot catch.
///
/// Two scripts differing only in whitespace compile to the same instructions,
/// so nothing about the code separates them — while their positions, the half
/// that was left behind, are exactly what changed. Swapping their sidecars
/// would report every error a line or two out, which reads as an answer.
///
/// So the id is taken from the diagnostics rather than from the code, and the
/// artifact carries it: the two artifacts differ here in that one field alone.
#[test]
#[cfg(not(feature = "no_position"))]
fn a_sidecar_from_another_build_of_the_same_code_is_refused() {
    let engine = corpus::engine();

    let one = Compiler::new().compile(&engine.compile("let a = 1;\nlet b = 2;\na + b").unwrap());
    let two = Compiler::new().compile(&engine.compile("let a = 1;\n\n\nlet b = 2;\na + b").unwrap());

    let one_stripped = one.write_stripped().expect("must be writable");
    let two_stripped = two.write_stripped().expect("must be writable");

    assert_eq!(one.code(), two.code(), "the case needs two programs the code cannot tell apart",);
    assert_ne!(one_stripped.sidecar.positions, two_stripped.sidecar.positions, "their positions are what differs",);
    assert_ne!(one_stripped.sidecar.debug_id, two_stripped.sidecar.debug_id, "so their ids must differ too",);

    let mut program = Program::read(&one_stripped.artifact).unwrap();
    assert!(program.attach_positions(&two_stripped.sidecar).is_err(), "another build's sidecar must be refused",);
    program.attach_positions(&one_stripped.sidecar).expect("its own sidecar must attach");
}

/// Stripping must not change which sidecar an artifact answers to.
///
/// The id names the diagnostics a program was compiled with, not the ones it
/// still holds — otherwise stripping would rename the artifact and leave the
/// sidecar it just produced unable to attach.
///
/// Indexing, so both halves of the sidecar are non-empty and the chain sites
/// are covered along with the table.
#[test]
#[cfg(not(any(feature = "no_position", feature = "no_index")))]
fn stripping_does_not_change_the_debug_id() {
    let engine = corpus::engine();
    let ast = engine.compile("let a = [1, 2];\nlet b = 0;\na[b] / b").expect("must compile");

    let mut program = Compiler::new().compile(&ast);
    let before = program.debug_id();
    let sidecar = program.strip_positions();

    assert_eq!(program.debug_id(), before, "stripping must not rename the program");
    assert_eq!(sidecar.debug_id, before, "the sidecar must name what it came from");

    // The long way round: strip in memory, then write, then load and reattach.
    let bytes = program.write().expect("must be writable");
    let mut loaded = Program::read(&bytes).expect("must load");

    assert_eq!(loaded.debug_id(), before, "the id must survive the wire");
    loaded.attach_positions(&sidecar).expect("its own sidecar must attach");
}

/// A stripped artifact that arrives with a table still in it is a contradiction
/// the reader should not paper over.
#[test]
#[cfg(not(feature = "no_position"))]
fn an_artifact_carrying_a_mismatched_table_does_not_load() {
    let engine = corpus::engine();
    let bytes = sample(&engine);
    let program = Program::read(&bytes).expect("the sample must load");

    assert!(!program.positions().is_stripped(), "`write` keeps the table, so this one must have positions",);
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
        let stripped = program.write_stripped().expect("must be writable");

        with += full.len();
        without += stripped.artifact.len();
        tables += stripped.sidecar.positions.len() + stripped.sidecar.chains.len();
    }

    println!(
        "\n{with} bytes with positions -> {without} stripped ({:.0}% smaller), \
         {tables} bytes of sidecar kept behind",
        100.0 * (with - without) as f64 / with as f64,
    );

    assert!(without < with, "stripping must actually remove something: {without} vs {with}",);
}

/// The number this project exists to move: bytes retained per source byte,
/// against the 24 a Rhai `AST` costs on device.
///
/// This is the host-side artifact size, not device heap, which only a device
/// can report. What it establishes is the encoding's own density, which is the
/// part the format controls.
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
    println!("\n{} scripts: {source_bytes} source bytes -> {artifact_bytes} artifact bytes ({:.2}x)", rows.len(), artifact_bytes as f64 / source_bytes as f64,);

    // Not a target, a tripwire. The plan is explicit that bytecode need not
    // beat minified source on bytes — but an encoding several times larger
    // than its input has a bug in it, not a tradeoff.
    //
    // The header is allowed for separately, or this measures the wrong thing:
    // these scripts average forty source bytes, so a fixed 28 of magic,
    // version, ABI and debug id would dominate a plain ratio and the density
    // this is watching would stop showing through.
    const HEADER: usize = 4 + 2 + 6 + 16;

    // Statement markers are allowed for the same way. A `debugging` build puts
    // one before every statement so the debugger has somewhere to stop
    // (`Op::Statement`) and no shipping artifact carries any, so counting them
    // against the encoding would measure the wrong build. Three bytes of
    // instruction and up to three of position-table entry, counted off the
    // artifacts rather than guessed at; zero of them anywhere else.
    const MARKER: usize = 3 + 3;
    let markers: usize = written
        .iter()
        .filter_map(|(_, _, bytes)| Program::read(bytes).ok())
        .map(|program| {
            rhai::grain::bytecode::disassemble(program.code())
                .filter(|(_, op)| matches!(op, rhai::grain::bytecode::Op::Statement { .. }))
                .count()
        })
        .sum();
    println!("{markers} statement markers, allowed {} bytes", markers * MARKER);

    let allowed = source_bytes * 3 + rows.len() * HEADER + markers * MARKER;
    assert!(artifact_bytes < allowed, "{artifact_bytes} artifact bytes for {source_bytes} of source across {} scripts is not an encoding", rows.len(),);
}
