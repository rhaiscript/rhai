#![feature(test)]

///! Test 1,000 iterations
extern crate test;

use rhai::{Engine, OptimizationLevel, INT};
use test::Bencher;

#[bench]
fn bench_iterations_1000(bench: &mut Bencher) {
    let script = "
            let x = 1_000;
            
            while x > 0 {
                x -= 1;
            }
        ";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}

#[bench]
fn bench_iterations_fibonacci(bench: &mut Bencher) {
    let script = "
        fn fibonacci(n) {
            if n < 2 {
                n
            } else {
                fibonacci(n-1) + fibonacci(n-2)
            }
        }

        fibonacci(20)
    ";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.eval_ast::<INT>(&ast).unwrap());
}

#[bench]
fn bench_iterations_array(bench: &mut Bencher) {
    let script = "
            let x = [];
            x.pad(1000, 0);
            for i in 0..1000 { x[i] = i % 256; }
        ";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}

#[bench]
fn bench_iterations_blob(bench: &mut Bencher) {
    let script = "
            let x = blob(1000, 0);
            for i in 0..1000 { x[i] = i % 256; }
        ";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}
