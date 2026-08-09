//! Rough VM-versus-walker timings, on the same AST.
//!
//! Indicative, not criterion: repeated runs, reporting the fastest of each and
//! the spread around it so a single scheduling hiccup does not read as a
//! result. Run with `--release`; a debug build measures bounds checks more than
//! anything else.
//!
//! # Catching a regression
//!
//! `cargo run --release --example bench -- --check` exits non-zero if any case
//! has fallen below the floor recorded beside it.
//!
//! What is compared is the **ratio**, not the time. Absolute milliseconds say
//! as much about the machine as about the code, and there is no useful way to
//! commit one; the walker and the VM run back to back on the same machine in
//! the same process, so their ratio mostly divides the machine out. Mostly, not
//! entirely — cache size and core count still move it — so a floor sits about
//! 15% under the observed figure. That is wide enough not to cry wolf on a
//! slower runner and tight enough that losing a fast path shows up.

use std::time::{Duration, Instant};

use rhai::grain::{Compiler, Program, Vm};
use rhai::{Dynamic, Engine, Scope};

const RUNS: usize = 9;

struct Case {
    name: &'static str,
    source: &'static str,
    iterations: usize,
    /// Whether the run needs the callback wrappers installed, which costs an
    /// owned program and a module built per run.
    callbacks: bool,
    /// The speedup this case must not drop below.
    ///
    /// Beside the source rather than in a table of its own, so changing one
    /// without the other is visible in the diff.
    floor: f64,
}

/// The fastest sample, and how much slower the middle one was.
///
/// Noise on a timing is one-sided — nothing makes a run finish sooner than it
/// can — so the fastest sample is the least contaminated estimate, and the
/// median is here only to say how contaminated the rest were. A wide spread
/// means the number below it should not be read closely.
struct Timing {
    fastest: Duration,
    median: Duration,
}

impl Timing {
    fn secs(&self) -> f64 {
        self.fastest.as_secs_f64()
    }

    /// How far the median sits above the fastest, as a fraction.
    fn spread(&self) -> f64 {
        self.median.as_secs_f64() / self.fastest.as_secs_f64() - 1.0
    }
}

const CASES: &[Case] = &[
    Case {
        name: "tight integer loop",
        source: "let s = 0; let i = 0; while i < 20000 { s += i; i += 1; } s",
        iterations: 20,
        callbacks: false,
        floor: 1.30,
    },
    Case {
        name: "float arithmetic",
        source: "let x = 0.0; let i = 0; while i < 20000 { x += (i.to_float() * 1.5) / 2.5; i += 1; } x",
        iterations: 20,
        callbacks: false,
        floor: 1.10,
    },
    Case {
        name: "script fn calls",
        source: "fn add(a, b) { a + b } let s = 0; let i = 0; while i < 5000 { s = add(s, i); i += 1; } s",
        iterations: 20,
        callbacks: false,
        floor: 1.55,
    },
    // The VM scans its case hashes; rhai probes a hash map. Two sizes,
    // because which of those wins is a question about how many arms there
    // are, and a `switch` nobody would write is the only place the scan can
    // lose.
    Case {
        name: "switch, 4 arms",
        source: "let s = 0; let i = 0; while i < 20000 { \
                 switch i % 4 { 0 => s += 1, 1 => s += 2, 2 => s += 3, _ => s += 4 } \
                 i += 1; } s",
        iterations: 20,
        callbacks: false,
        floor: 1.40,
    },
    Case {
        name: "switch, 16 arms",
        source: "let s = 0; let i = 0; while i < 20000 { \
                 switch i % 16 { \
                 0 => s += 1, 1 => s += 2, 2 => s += 3, 3 => s += 4, \
                 4 => s += 5, 5 => s += 6, 6 => s += 7, 7 => s += 8, \
                 8 => s += 9, 9 => s += 10, 10 => s += 11, 11 => s += 12, \
                 12 => s += 13, 13 => s += 14, 14 => s += 15, _ => s += 16 } \
                 i += 1; } s",
        iterations: 20,
        callbacks: false,
        floor: 1.35,
    },
    Case {
        name: "branch heavy",
        source: "let s = 0; let i = 0; while i < 20000 { if i % 3 == 0 { s += 1; } else if i % 3 == 1 { s += 2; } else { s -= 1; } i += 1; } s",
        iterations: 20,
        callbacks: false,
        floor: 1.45,
    },
    // The one case the VM is expected to lose. Every element is a boundary out
    // of the VM, through rhai's dispatch and back into a second `Vm` with an
    // empty resolution cache — where the walker stays inside itself and reaches
    // the closure body directly. 1000 crossings per iteration.
    //
    // Also the only case here that indexes: the `a.push(i)` loop is a chain
    // rooted at a local, so it is what says the root is still being walked
    // where it lives rather than copied out and put back.
    Case {
        name: "native callbacks",
        source: "let a = []; let i = 0; while i < 500 { a.push(i); i += 1; } \
                 let b = a.map(|x| x * 2); b.filter(|x| x % 3 == 0).len()",
        iterations: 20,
        callbacks: true,
        floor: 0.64,
    },
];

fn time(mut run: impl FnMut()) -> Timing {
    let mut samples: Vec<Duration> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            run();
            start.elapsed()
        })
        .collect();
    samples.sort_unstable();
    Timing {
        fastest: samples[0],
        median: samples[RUNS / 2],
    }
}

fn main() {
    let check = std::env::args().any(|arg| arg == "--check");
    let engine = Engine::new();

    // Rhai's default options include FAST_OPS, which makes the walker
    // short-circuit binary operators and op-assignments straight to builtin
    // function pointers — no hash, no resolution cache
    // (`func/call.rs:1775-1799`, `eval/stmt.rs:131-148`). Turning it off
    // measures how much of the walker's speed comes from that, and therefore
    // how much of the VM's planned "typed fast opcodes" win is already taken.
    let mut slow_engine = Engine::new();
    slow_engine.set_fast_operators(false);

    println!(
        "{:<22} {:>11} {:>11} {:>9} {:>7} {:>8} {:>11} {:>10}",
        "", "walker", "vm", "speedup", "floor", "spread", "walker-slow", "fragments"
    );

    let mut below_floor = Vec::new();

    for case in CASES {
        let ast = engine.compile(case.source).expect("must compile");
        let program: Program = Compiler::new().compile(&ast);
        // Owned and shared only where a pointer can escape to a native, so the
        // ordinary cases keep measuring the ordinary path.
        let shared = case
            .callbacks
            .then(|| Compiler::new().compile(&ast).into_shared());
        let run_vm = || match &shared {
            Some(shared) => Vm::new(&engine).eval_with_callbacks(&mut Scope::new(), shared),
            None => Vm::new(&engine).eval_with_scope(&mut Scope::new(), &program),
        };

        // Same result, or the comparison is meaningless.
        let expected = engine
            .eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast)
            .expect("walker must succeed");
        let actual = run_vm().expect("vm must succeed");
        assert_eq!(
            format!("{expected:?}"),
            format!("{actual:?}"),
            "{} disagreed, so its timing means nothing",
            case.name,
        );

        let walker = time(|| {
            for _ in 0..case.iterations {
                let _ = engine
                    .eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &ast)
                    .unwrap();
            }
        });

        let vm = time(|| {
            for _ in 0..case.iterations {
                let _ = run_vm().unwrap();
            }
        });

        let slow_ast = slow_engine.compile(case.source).expect("must compile");
        let walker_slow = time(|| {
            for _ in 0..case.iterations {
                let _ = slow_engine
                    .eval_ast_with_scope::<Dynamic>(&mut Scope::new(), &slow_ast)
                    .unwrap();
            }
        });

        // The spread reported is the VM's, because that is the number the
        // floor is about. A walker sample knocked sideways shows up in the
        // speedup anyway.
        let speedup = walker.secs() / vm.secs();
        println!(
            "{:<22} {:>9.1}ms {:>9.1}ms {:>8.2}x {:>6.2}x {:>7.0}% {:>9.1}ms {:>10}",
            case.name,
            walker.secs() * 1000.0,
            vm.secs() * 1000.0,
            speedup,
            case.floor,
            vm.spread() * 100.0,
            walker_slow.secs() * 1000.0,
            program.residual_nodes(),
        );

        if speedup < case.floor {
            below_floor.push(format!(
                "\n  {}: {speedup:.2}x, floor {:.2}x (VM samples spread {:.0}%)",
                case.name,
                case.floor,
                vm.spread() * 100.0,
            ));
        }
    }

    if below_floor.is_empty() {
        return;
    }

    // Printed whether or not this is a gated run: a regression is worth seeing
    // even when nobody asked for an exit code.
    eprintln!(
        "\n{} case(s) below their floor:{}",
        below_floor.len(),
        below_floor.join(""),
    );
    eprintln!(
        "\nA wide spread means the machine was busy — rerun before believing it. \
         If the loss is real, either find it or move the floor in the same commit \
         that causes it.",
    );
    if check {
        std::process::exit(1);
    }
}
