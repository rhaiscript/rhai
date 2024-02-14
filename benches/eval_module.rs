#![feature(test)]

///! Test evaluating with scope
extern crate test;

use rhai::{Engine, Module, OptimizationLevel, Scope};
use test::Bencher;

#[bench]
fn bench_eval_module(bench: &mut Bencher) {
    let script = "
        fn foo(x) { x + 1 }
        fn bar(x) { foo(x) }
    ";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(script).unwrap();

    let module = Module::eval_ast_as_new(Scope::new(), &ast, &engine).unwrap();

    engine.register_static_module("testing", module.into());

    let ast = engine
        .compile(
            "
                fn foo(x) { x - 1 }
                testing::bar(41)
            ",
        )
        .unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}

#[bench]
fn bench_eval_function_call(bench: &mut Bencher) {
    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine
        .compile(
            "
                fn foo(x) { x - 1 }
                fn bar(x) { foo(x) }
                bar(41)
            ",
        )
        .unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}
