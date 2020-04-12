use criterion::{black_box, criterion_group, criterion_main, Criterion};

pub fn compile_benchmark(c: &mut Criterion) {
    use rhai::Engine;
    let mut engine = Engine::new();
    c.bench_function("compile.rhai", |b|b.iter(|| engine.compile(black_box(PRIMES_RHAI))));
}

criterion_group!{compile, compile_benchmark}

criterion_main!(compile);

const PRIMES_RHAI: &str = r#"// This script uses the Sieve of Eratosthenes to calculate prime numbers.

const MAX_NUMBER_TO_CHECK = 10_000;     // 1229 primes <= 10000

let prime_mask = [];
prime_mask.pad(MAX_NUMBER_TO_CHECK, true);

prime_mask[0] = prime_mask[1] = false;

let total_primes_found = 0;

for p in range(2, MAX_NUMBER_TO_CHECK) {
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

print("Total " + total_primes_found + " primes.");"#;