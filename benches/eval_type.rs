#![feature(test)]

///! Test evaluating expressions
extern crate test;

use rhai::{Engine, OptimizationLevel, Scope, INT};
use test::Bencher;

#[derive(Debug, Clone)]
struct Test {
    x: INT,
}

impl Test {
    pub fn get_x(&mut self) -> INT {
        self.x
    }
    pub fn action(&mut self) {
        self.x = 0;
    }
    pub fn update(&mut self, val: INT) {
        self.x = val;
    }
    pub fn get_nest(&mut self) -> Test {
        Test { x: 9 }
    }
}

#[bench]
fn bench_type_field(bench: &mut Bencher) {
    let script = "foo.field";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    engine.register_type_with_name::<Test>("Test");
    engine.register_get("field", Test::get_x);

    let ast = engine.compile_expression(script).unwrap();

    let mut scope = Scope::new();
    scope.push("foo", Test { x: 42 });

    bench.iter(|| engine.run_ast_with_scope(&mut scope, &ast).unwrap());
}

#[bench]
fn bench_type_method(bench: &mut Bencher) {
    let script = "foo.action()";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    engine.register_type_with_name::<Test>("Test");
    engine.register_fn("action", Test::action);

    let ast = engine.compile_expression(script).unwrap();

    let mut scope = Scope::new();
    scope.push("foo", Test { x: 42 });

    bench.iter(|| engine.run_ast_with_scope(&mut scope, &ast).unwrap());
}

#[bench]
fn bench_type_method_with_params(bench: &mut Bencher) {
    let script = "foo.update(1)";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    engine.register_type_with_name::<Test>("Test");
    engine.register_fn("update", Test::update);

    let ast = engine.compile_expression(script).unwrap();

    let mut scope = Scope::new();
    scope.push("foo", Test { x: 42 });

    bench.iter(|| engine.run_ast_with_scope(&mut scope, &ast).unwrap());
}

#[bench]
fn bench_type_method_nested(bench: &mut Bencher) {
    let script = "foo.nest.field";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    engine.register_type_with_name::<Test>("Test");
    engine.register_get("field", Test::get_x);
    engine.register_get("nest", Test::get_nest);

    let ast = engine.compile_expression(script).unwrap();

    let mut scope = Scope::new();
    scope.push("foo", Test { x: 42 });

    bench.iter(|| engine.run_ast_with_scope(&mut scope, &ast).unwrap());
}
