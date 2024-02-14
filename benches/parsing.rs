#![feature(test)]

///! Test parsing expressions
extern crate test;

use rhai::{Engine, OptimizationLevel};
use test::Bencher;

#[bench]
fn bench_parse_single(bench: &mut Bencher) {
    let script = "1";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    bench.iter(|| engine.compile_expression(script).unwrap());
}

#[bench]
fn bench_parse_simple(bench: &mut Bencher) {
    let script = "(requests_made * requests_succeeded / 100) >= 90";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    bench.iter(|| engine.compile_expression(script).unwrap());
}

#[bench]
fn bench_parse_full(bench: &mut Bencher) {
    let script = r#"
            2 > 1 &&
            "something" != "nothing" ||
            "2014-01-20" < "Wed Jul  8 23:07:35 MDT 2015" &&
            [array, has, spaces].len <= #{prop:name}.len &&
            modifierTest + 1000 / 2 > (80 * 100 % 2)
        "#;

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    bench.iter(|| engine.compile_expression(script).unwrap());
}

#[bench]
fn bench_parse_array(bench: &mut Bencher) {
    let script = r#"[1, 234.789, "hello", false, [ 9, 8, 7] ]"#;

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    bench.iter(|| engine.compile_expression(script).unwrap());
}

#[bench]
fn bench_parse_map(bench: &mut Bencher) {
    let script = r#"#{a: 1, b: 42, c: "hi", "dc%$& ": "strange", x: true, y: 123.456 }"#;

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    bench.iter(|| engine.compile_expression(script).unwrap());
}

#[bench]
fn bench_parse_primes(bench: &mut Bencher) {
    let script = "
            // This script uses the Sieve of Eratosthenes to calculate prime numbers.

            let now = timestamp();
            
            const MAX_NUMBER_TO_CHECK = 10_000;     // 1229 primes <= 10000
            
            let prime_mask = [];
            prime_mask.pad(MAX_NUMBER_TO_CHECK, true);
            
            prime_mask[0] = false;
            prime_mask[1] = false;
            
            let total_primes_found = 0;
            
            for p in 2..MAX_NUMBER_TO_CHECK {
                if prime_mask[p] {
                    print(p);
            
                    total_primes_found += 1;
                    let i = 2 * p;
            
                    while i < MAX_NUMBER_TO_CHECK {
                        prime_mask[i] = false;
                        i += p;
                    }
                }
            }
            
            print(`Total ${total_primes_found} primes.`);
            print(`Run time = ${now.elapsed} seconds.`);
        ";

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    bench.iter(|| engine.compile(script).unwrap());
}

#[bench]
fn bench_parse_optimize_simple(bench: &mut Bencher) {
    let script = r#"
            2 > 1 &&
            "something" != "nothing" ||
            "2014-01-20" < "Wed Jul  8 23:07:35 MDT 2015" &&
            [array, has, spaces].len <= #{prop:name}.len &&
            modifierTest + 1000 / 2 > (80 * 100 % 2)
        "#;

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::Simple);

    bench.iter(|| engine.compile_expression(script).unwrap());
}

#[bench]
fn bench_parse_optimize_full(bench: &mut Bencher) {
    let script = r#"
            2 > 1 &&
            "something" != "nothing" ||
            "2014-01-20" < "Wed Jul  8 23:07:35 MDT 2015" &&
            [array, has, spaces].len <= #{prop:name}.len &&
            modifierTest + 1000 / 2 > (80 * 100 % 2)
        "#;

    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::Full);

    bench.iter(|| engine.compile_expression(script).unwrap());
}
