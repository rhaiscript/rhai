//! Lazy versus eager standard library: heap, construction time and script timings.
//!
//! The eager baseline is the standard library as it was before lazy resolution: the same package
//! initialized into a non-lazy module, so every function is registered up-front.
//!
//! Indicative, not criterion: timings are the fastest of several runs. Run with `--release`
//! (add `--features grain` to include the Grain VM).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::{Duration, Instant};

use rhai::packages::{Package, StandardPackage};
use rhai::{Dynamic, Engine, Module, Scope, AST};

static LIVE: AtomicIsize = AtomicIsize::new(0);
static COUNT: AtomicIsize = AtomicIsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size() as isize, Ordering::Relaxed);
        COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size() as isize, Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new_size: usize) -> *mut u8 {
        LIVE.fetch_add(new_size as isize - l.size() as isize, Ordering::Relaxed);
        COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const RUNS: usize = 15;

/// A short script calling many different functions once or a few times each: dominated by
/// resolving functions, since every evaluation starts with empty caches.
const SHORT: &str = r#"
    let s = "Hello, World";
    let a = [];
    for i in 0..100 { a.push(i * 2); }
    let m = #{};
    for i in 0..50 { m[`k${i}`] = i.to_string(); }
    let t = 0;
    for x in a { t += abs(x - 50) + max(x, 10); }
    let u = s.to_upper() + s.sub_string(1, 3) + s.len().to_string();
    t + u.len() + m.len() + a.filter(|x| x % 3 == 0).len()
"#;

/// A long-running loop calling a few functions many times: dominated by executing functions.
const LONG: &str = r#"
    let t = 0;
    let s = "abc";
    for i in 0..10000 { t += abs(i - 5000) + max(i, 3) + s.len() + sign(i); }
    t
"#;

/// Heap retained, and allocations made, while running `f`.
fn measure<T>(f: impl FnOnce() -> T) -> (T, isize, isize) {
    let live = LIVE.load(Ordering::Relaxed);
    let count = COUNT.load(Ordering::Relaxed);
    let value = f();
    (
        value,
        LIVE.load(Ordering::Relaxed) - live,
        COUNT.load(Ordering::Relaxed) - count,
    )
}

fn fastest(mut f: impl FnMut()) -> Duration {
    (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .min()
        .unwrap()
}

fn lazy_std() -> Module {
    let mut module = Module::new();
    module.set_lazy(true);
    <StandardPackage as Package>::init(&mut module);
    module.build_index();
    module
}

fn eager_std() -> Module {
    let mut module = Module::new();
    <StandardPackage as Package>::init(&mut module);
    module.build_index();
    module
}

fn engine_with(std: Module) -> Engine {
    let mut engine = Engine::new_raw();
    engine.register_global_module(std.into());
    engine
}

fn main() {
    println!("Standard library module");
    println!(
        "{:<8} {:>10} {:>12} {:>10} {:>12}",
        "", "functions", "heap bytes", "allocs", "build time"
    );
    for (name, build) in [("eager", eager_std as fn() -> Module), ("lazy", lazy_std)] {
        let (module, bytes, allocs) = measure(build);
        let time = fastest(|| drop(build()));
        println!(
            "{name:<8} {:>10} {bytes:>12} {allocs:>10} {:>10.1}µs",
            module.count().1,
            time.as_secs_f64() * 1e6
        );
    }

    for (title, script) in [
        ("Short script (resolution-bound)", SHORT),
        ("Long loop (execution-bound)", LONG),
    ] {
        println!();
        println!("{title}");

        for (name, build) in [("eager", eager_std as fn() -> Module), ("lazy", lazy_std)] {
            let engine = engine_with(build());
            let ast: AST = engine.compile(script).unwrap();
            let expected = engine.eval_ast::<Dynamic>(&ast).unwrap().to_string();

            let (_, _, allocs) = measure(|| engine.eval_ast::<Dynamic>(&ast).unwrap());
            let walker = fastest(|| {
                let _ = engine.eval_ast::<Dynamic>(&ast).unwrap();
            });
            println!(
                "{name:<8} walker {:>9.1}µs  ({allocs} allocs per run, result {expected})",
                walker.as_secs_f64() * 1e6
            );

            #[cfg(feature = "grain")]
            {
                use rhai::grain::{Compiler, Vm};

                let program = Compiler::new().compile(&ast).into_shared();
                let run = || {
                    Vm::new(&engine)
                        .eval_with_callbacks(&mut Scope::new(), &program)
                        .unwrap()
                };
                assert_eq!(run().to_string(), expected);
                let (_, _, allocs) = measure(run);
                let vm = fastest(|| drop(run()));
                println!(
                    "{name:<8} VM     {:>9.1}µs  ({allocs} allocs per run)",
                    vm.as_secs_f64() * 1e6
                );
            }
            #[cfg(not(feature = "grain"))]
            let _ = Scope::new();
        }
    }
}
