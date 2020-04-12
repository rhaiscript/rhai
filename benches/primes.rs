use criterion::{criterion_group, criterion_main, Criterion};

const PRIMES_RHAI: &str = "
const MAX_NUMBER_TO_CHECK = 10_000;
let prime_mask = [];
prime_mask.pad(MAX_NUMBER_TO_CHECK, true);

prime_mask[0] = prime_mask[1] = false;

let total_primes_found = 0;

for p in range(2, MAX_NUMBER_TO_CHECK) {
    if prime_mask[p] {
        total_primes_found += 1;
        let i = 2 * p;

        while i < MAX_NUMBER_TO_CHECK {
            prime_mask[i] = false;
            i += p;
        }
    }
}";

fn primes_benchmark(c: &mut Criterion) {
    use rhai::Engine;
    let mut engine = Engine::new();
    let ast = engine.compile(PRIMES_RHAI).expect("compile PRIMES_RHAI");
    c.bench_function("primes.rhai", |b|b.iter(|| engine.eval_ast::<()>(&ast)));
}

criterion_group!{
    name = primes;
    config = Criterion::default().sample_size(10);
    targets = primes_benchmark
}

criterion_main!(primes);