#![feature(test)]

///! Test evaluating expressions
extern crate test;

use rhai::{Engine, OptimizationLevel};
use test::Bencher;

#[bench]
fn bench_eval_array_small_get(bench: &mut Bencher) {
    let script = "let x = [1, 2, 3, 4, 5]; x[3]";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}

#[bench]
fn bench_eval_array_small_set(bench: &mut Bencher) {
    let script = "let x = [1, 2, 3, 4, 5]; x[3] = 42;";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}

#[bench]
fn bench_eval_array_large_get(bench: &mut Bencher) {
    let script = r#"let x = [ 1, 2.345, "hello", true,
                                [ 1, 2, 3, [ "hey", [ "deeply", "nested" ], "jude" ] ]
                            ];
                            x[4][3][1][1]
    "#;

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}

#[bench]
fn bench_eval_array_large_set(bench: &mut Bencher) {
    let script = r#"let x = [ 1, 2.345, "hello", true,
                                [ 1, 2, 3, [ "hey", [ "deeply", "nested" ], "jude" ] ]
                            ];
                            x[4][3][1][1] = 42
    "#;

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}

#[bench]
fn bench_eval_array_loop(bench: &mut Bencher) {
    let script = "
            let list = [];
            
            for i in 0..10_000 {
                list.push(i);
            }

            let sum = 0;

            for i in list {
                sum += i;
            }
        ";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}
