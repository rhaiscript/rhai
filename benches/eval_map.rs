#![feature(test)]

///! Test evaluating expressions
extern crate test;

use rhai::{Engine, OptimizationLevel};
use test::Bencher;

#[bench]
fn bench_eval_map_small_get(bench: &mut Bencher) {
    let script = "let x = #{a:1}; x.a";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}

#[bench]
fn bench_eval_map_small_set(bench: &mut Bencher) {
    let script = "let x = #{a:1}; x.a = 42;";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}

#[bench]
fn bench_eval_map_large_get(bench: &mut Bencher) {
    let script = r#"let x = #{
                                a:1,
                                b:2.345,
                                c:"hello",
                                d: true,
                                e: #{ x: 42, "y$@#%": (), z: [ 1, 2, 3, #{}, #{ "hey": "jude" }]}
                            };
                            x["e"].z[4].hey
    "#;

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}

#[bench]
fn bench_eval_map_large_set(bench: &mut Bencher) {
    let script = r#"let x = #{
                                a:1,
                                b:2.345,
                                c:"hello",
                                d: true,
                                e: #{ x: 42, "y$@#%": (), z: [ 1, 2, 3, #{}, #{ "hey": "jude" }]}
                            };
                            x["e"].z[4].hey = 42;
    "#;

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}
