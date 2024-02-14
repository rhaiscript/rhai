#![feature(test)]

///! Test evaluating expressions
extern crate test;

use rhai::{Engine, OptimizationLevel};
use test::Bencher;

// This script uses the Sieve of Eratosthenes to calculate prime numbers.

const SCRIPT: &str = "
    const MAX_NUMBER_TO_CHECK = 1_000;     // 168 primes <= 1000

    let prime_mask = [];
    prime_mask.pad(MAX_NUMBER_TO_CHECK, true);

    prime_mask[0] = false;
    prime_mask[1] = false;

    let total_primes_found = 0;

    for p in 2..MAX_NUMBER_TO_CHECK {
        if prime_mask[p] {
            total_primes_found += 1;
            let i = 2 * p;

            while i < MAX_NUMBER_TO_CHECK {
                prime_mask[i] = false;
                i += p;
            }
        }
    }
";

#[bench]
fn bench_eval_primes(bench: &mut Bencher) {
    let mut engine = Engine::new();
    engine.set_optimization_level(OptimizationLevel::None);

    let ast = engine.compile(SCRIPT).unwrap();

    bench.iter(|| engine.run_ast(&ast).unwrap());
}
