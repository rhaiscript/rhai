#![feature(test)]

///! Test evaluating expressions
extern crate test;

use rhai::{Array, Engine, Map, INT};
use test::Bencher;

#[bench]
fn bench_engine_new(bench: &mut Bencher) {
    bench.iter(|| Engine::new());
}

#[bench]
fn bench_engine_new_raw(bench: &mut Bencher) {
    bench.iter(|| Engine::new_raw());
}

#[bench]
fn bench_engine_new_raw_core(bench: &mut Bencher) {
    use rhai::packages::*;
    let package = CorePackage::new();

    bench.iter(|| {
        let mut engine = Engine::new_raw();
        package.register_into_engine(&mut engine);
    });
}

#[bench]
fn bench_engine_register_fn(bench: &mut Bencher) {
    fn hello(_a: INT, _b: Array, _c: Map) -> bool {
        true
    }

    bench.iter(|| {
        let mut engine = Engine::new_raw();
        engine.register_fn("hello", hello);
    });
}
