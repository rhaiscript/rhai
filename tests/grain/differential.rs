//! The VM must mean exactly what rhai means.
//!
//! Every corpus script is evaluated twice against the same `Engine` — once
//! through `eval_ast_with_scope`, once through the VM — and the two runs must
//! agree on the result, on the error (variant *and* position), and on the scope
//! they leave behind.
//!
//! The scope check is not incidental. Rhai evaluates a program's top-level
//! statements without rewinding, so `let` at the top level outlives the run and
//! is observable by the caller. A VM that manages its own frames could return
//! the right value and still get that wrong.

use super::corpus;

use rhai::grain::{Compiler, Vm};
use rhai::{Dynamic, Engine, Scope};

/// What a run produced, in a form two runs can be compared on.
///
/// `Dynamic` and `EvalAltResult` have no `PartialEq`, so this compares their
/// `Debug` rendering. That is stricter than value equality, not looser: it
/// distinguishes `1` from `1.0`, and it includes error positions.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    result: Result<String, String>,
    scope: Vec<(String, String)>,
}

fn snapshot_scope(scope: &Scope) -> Vec<(String, String)> {
    scope.iter_raw().map(|(name, _, value)| (name.to_string(), format!("{value:?}"))).collect()
}

fn run_stock(engine: &Engine, source: &str) -> Outcome {
    let mut scope = Scope::new();
    let result = engine.compile(source).map_err(|err| format!("{err:?}")).and_then(|ast| {
        engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &ast)
            .map(|value| format!("{value:?}"))
            .map_err(|err| format!("{err:?}"))
    });

    Outcome { result, scope: snapshot_scope(&scope) }
}

fn run_vm(engine: &Engine, source: &str) -> Outcome {
    let mut scope = Scope::new();
    let result = engine.compile(source).map_err(|err| format!("{err:?}")).and_then(|ast| {
        let program = Compiler::new().compile(&ast);
        // A program that can hand a pointer to a native has to be run the
        // way such a program is meant to be run, or the comparison is
        // against a configuration nobody would ship.
        if program.makes_fn_pointers() {
            let program = program.into_shared();
            Vm::new(engine).run_with_callbacks(&program, &mut scope)
        } else {
            Vm::new(engine).run(&program, &mut scope)
        }
        .map(|value| format!("{value:?}"))
        .map_err(|err| format!("{err:?}"))
    });

    Outcome { result, scope: snapshot_scope(&scope) }
}

#[test]
fn vm_agrees_with_rhai() {
    let engine = corpus::engine();

    let mut failures = Vec::new();

    for case in corpus::CASES {
        let stock = run_stock(&engine, case.source);
        let vm = run_vm(&engine, case.source);

        if stock != vm {
            failures.push(format!(
                "\n=== {} ===\n  source: {}\n  rhai:   {:?}\n  vm:     {:?}\n  \
                 rhai scope: {:?}\n  vm scope:   {:?}",
                case.name, case.source, stock.result, vm.result, stock.scope, vm.scope,
            ));
        }
    }

    assert!(failures.is_empty(), "{} of {} corpus scripts diverged:{}", failures.len(), corpus::CASES.len(), failures.join(""),);
}

/// The corpus is only worth anything if the comparison can actually fail.
///
/// Guards against the harness silently degrading into a tautology — comparing
/// two identical code paths, or stringifying everything into the same value.
#[test]
fn harness_detects_a_real_difference() {
    let engine = corpus::engine();

    assert_ne!(run_stock(&engine, "1 + 1"), run_stock(&engine, "1 + 2"), "differing results must compare unequal",);
    assert_ne!(run_stock(&engine, "1"), run_stock(&engine, "1.0"), "int and float must not compare equal",);
    assert_ne!(run_stock(&engine, "let a = 1; a"), run_stock(&engine, "1"), "differing leftover scope must compare unequal",);
    assert_ne!(run_stock(&engine, "let a = [1]; a[9]"), run_stock(&engine, "let a = [1];  a[9]"), "the same error at a different position must compare unequal",);
}

/// A script that does not parse compares equal on both sides for the wrong
/// reason: two identical parse errors. Such a case tests nothing, and would sit
/// in the corpus looking like coverage.
#[test]
fn every_corpus_script_parses() {
    let engine = corpus::engine();

    let broken: Vec<_> = corpus::CASES
        .iter()
        .filter_map(|case| engine.compile(case.source).err().map(|err| format!("\n  {}: {err}", case.name)))
        .collect();

    assert!(broken.is_empty(), "{} corpus scripts do not parse, so they assert nothing:{}", broken.len(), broken.join(""),);
}

/// Whether a corpus case exercises anything on this build.
///
/// Distinct from [`MAY_FRAGMENT`], which is a tolerance. A restriction feature
/// removes the syntax outright — rhai will not parse a capturing closure under
/// `no_closure` — so the case tests nothing here, and both sides agreeing on
/// the parse failure would be an empty agreement rather than a passing one.
fn applies_to_this_build(name: &str) -> bool {
    #[cfg(feature = "no_closure")]
    if name.starts_with("closure_") || name.starts_with("is_shared") {
        return false;
    }
    #[cfg(feature = "no_module")]
    if name.starts_with("import_") || name.starts_with("export_") {
        return false;
    }
    let _ = name;
    true
}

/// A case that errors unintentionally is nearly as weak as one that does not
/// parse: both sides agree on the failure, and the machinery the case was
/// written to exercise never runs. Cases that mean to fail say so in the name.
#[test]
fn only_error_cases_error() {
    let engine = corpus::engine();

    let surprises: Vec<_> = corpus::CASES
        .iter()
        .filter(|case| !case.name.starts_with("error_") && !case.name.starts_with("throw_"))
        .filter(|case| applies_to_this_build(case.name))
        .filter_map(|case| match run_stock(&engine, case.source).result {
            Err(err) => Some(format!("\n  {}: {err}", case.name)),
            Ok(_) => None,
        })
        .collect();

    assert!(surprises.is_empty(), "{} cases fail without meaning to, so they exercise nothing:{}", surprises.len(), surprises.join(""),);
}

/// Corpus scripts allowed to leave a fragment behind.
///
/// Empty, and that is the claim: every script in the corpus lowers with nothing
/// left over. A case that fragments therefore fails on arrival rather than
/// quietly joining a majority, which is the point of stating it this way round.
///
/// What legitimately belongs here: `eval`, `import`/`export`, custom syntax,
/// and `?.`. All four are the escape hatch working as intended rather than a
/// gap, and none is in the corpus.
const MAY_FRAGMENT: &[&str] = &[];

/// Every chunk the compiler emits must pass its own verifier.
///
/// The check that matters is depth agreement at merge points: one branch of a
/// conditional leaving a value where the other does not is invisible until a
/// program happens to take the unlucky path, and the differential corpus only
/// covers the paths it happens to exercise.
#[test]
fn every_compiled_chunk_verifies() {
    let engine = corpus::engine();

    let broken: Vec<_> = corpus::CASES
        .iter()
        .filter_map(|case| {
            let ast = engine.compile(case.source).ok()?;
            Compiler::new().compile(&ast).verify().err().map(|err| format!("\n  {}: {err:?}", case.name))
        })
        .collect();

    assert!(broken.is_empty(), "{} chunks failed verification:{}", broken.len(), broken.join(""),);
}

/// A chunk must declare the stack it uses, not the stack it might use.
///
/// The lowering's own estimate is one slot per instruction, which is safe and
/// wildly loose — and it is what the VM reserves from, and what the artifact
/// records. On a device with ~12KB to spend, reserving 25 `Dynamic` slots for a
/// chunk that stacks three is the difference worth closing.
#[test]
fn every_compiled_chunk_declares_the_stack_it_uses() {
    let engine = corpus::engine();

    let mut loose = Vec::new();
    let mut total_declared = 0usize;
    let mut total_ops = 0usize;

    for case in corpus::CASES {
        let Ok(ast) = engine.compile(case.source) else {
            continue;
        };
        let program = Compiler::new().compile(&ast);
        let Ok(high_water) = program.verify() else {
            continue;
        };

        let declared: Vec<u16> = std::iter::once(program.main().max_stack()).chain(program.functions().iter().map(|f| f.chunk.max_stack())).collect();

        total_declared += high_water.iter().map(|n| *n as usize).sum::<usize>();
        total_ops += program.code().len();

        if declared != high_water {
            loose.push(format!("\n  {}: declares {declared:?}, uses {high_water:?}", case.name,));
        }
    }

    println!("\n{total_declared} stack slots declared across {total_ops} bytes of code");

    assert!(loose.is_empty(), "{} chunks declare a stack they do not use:{}", loose.len(), loose.join(""),);
}

/// Residuals are the work left to do, so the count is the progress metric.
///
/// Prints the whole census so a milestone's effect is visible, and pins the
/// cases that should already be at zero.
#[test]
fn residual_census() {
    let engine = corpus::engine();

    let mut total_nodes = 0usize;
    let mut at_zero = Vec::new();
    let mut remaining = Vec::new();
    let mut regressions = Vec::new();

    // Cases the build removed are counted on neither side, or the completeness
    // check below would read their absence as a corpus that stopped compiling.
    let applicable = corpus::CASES.iter().filter(|case| applies_to_this_build(case.name)).count();

    for case in corpus::CASES.iter().filter(|c| applies_to_this_build(c.name)) {
        let Ok(ast) = engine.compile(case.source) else {
            continue;
        };
        let program = Compiler::new().compile(&ast);
        let count = program.residual_count();
        let nodes = program.residual_nodes();
        total_nodes += nodes;

        if count == 0 {
            at_zero.push(case.name);
        } else {
            remaining.push((case.name, nodes));
            if !MAY_FRAGMENT.contains(&case.name) && applies_to_this_build(case.name) {
                regressions.push(format!("\n  {} leaves {count}", case.name));
            }
        }
    }

    println!("\n{} of {} scripts fully lowered, {total_nodes} AST nodes still in fragments", at_zero.len(), corpus::CASES.len(),);
    println!("\nfully lowered: {}", at_zero.join(", "));
    println!("\nremaining:");
    remaining.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (name, count) in &remaining {
        println!("  {count:>3}  {name}");
    }

    assert!(
        regressions.is_empty(),
        "{} scripts fragment that are not on `MAY_FRAGMENT`. Either the \
         construct regressed, or the case needs one of the four things the \
         escape hatch is for — say which, in the list:{}",
        regressions.len(),
        regressions.join(""),
    );

    // The other direction, which the check above cannot see: a corpus that
    // stopped compiling at all would have nothing to fragment and would pass.
    assert_eq!(at_zero.len(), applicable - MAY_FRAGMENT.len(), "some scripts did not compile, so they were counted as neither",);
}
