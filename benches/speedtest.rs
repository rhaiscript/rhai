use criterion::{criterion_group, criterion_main, Criterion};

const SPEEDTEST_RHAI: &str = "
let x = 1_000_000;
while x > 0 {
    x = x - 1;
}
;";

pub fn speedtest_benchmark(c: &mut Criterion) {
    use rhai::Engine;
    let mut engine = Engine::new();
    let ast = engine.compile(SPEEDTEST_RHAI).expect("compile SPEEDTEST_RHAI");
    c.bench_function("speedtest.rhai", |b|b.iter(|| engine.eval_ast::<()>(&ast)));
}

criterion_group!{
    name = speedtest;
    config = Criterion::default().sample_size(10);
    targets = speedtest_benchmark
}

criterion_main!(speedtest);