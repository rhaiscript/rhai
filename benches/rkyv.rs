#![feature(test)]
#![cfg(all(feature = "rkyv", feature = "serde"))]

//! Benchmark comparing rkyv and serde serialization performance

extern crate test;

use rhai::Dynamic;
use test::Bencher;

// ============================================================================
// Integer Benchmarks
// ============================================================================

#[bench]
fn bench_rkyv_serialize_int(bench: &mut Bencher) {
    let value = Dynamic::from(42_i64);
    bench.iter(|| {
        let bytes = rhai::rkyv::to_bytes(&value).unwrap();
        test::black_box(bytes);
    });
}

#[bench]
fn bench_serde_json_serialize_int(bench: &mut Bencher) {
    let value = Dynamic::from(42_i64);
    bench.iter(|| {
        let json = serde_json::to_string(&value).unwrap();
        test::black_box(json);
    });
}

#[bench]
fn bench_rkyv_deserialize_int(bench: &mut Bencher) {
    let value = Dynamic::from(42_i64);
    let bytes = rhai::rkyv::to_bytes(&value).unwrap();
    
    bench.iter(|| {
        let restored: Dynamic = rhai::rkyv::from_bytes_owned(&bytes).unwrap();
        test::black_box(restored);
    });
}

#[bench]
fn bench_serde_json_deserialize_int(bench: &mut Bencher) {
    let value = Dynamic::from(42_i64);
    let json = serde_json::to_string(&value).unwrap();
    
    bench.iter(|| {
        let restored: Dynamic = serde_json::from_str(&json).unwrap();
        test::black_box(restored);
    });
}

#[bench]
fn bench_rkyv_roundtrip_int(bench: &mut Bencher) {
    let value = Dynamic::from(42_i64);
    bench.iter(|| {
        let bytes = rhai::rkyv::to_bytes(&value).unwrap();
        let restored: Dynamic = rhai::rkyv::from_bytes_owned(&bytes).unwrap();
        test::black_box(restored);
    });
}

#[bench]
fn bench_serde_json_roundtrip_int(bench: &mut Bencher) {
    let value = Dynamic::from(42_i64);
    bench.iter(|| {
        let json = serde_json::to_string(&value).unwrap();
        let restored: Dynamic = serde_json::from_str(&json).unwrap();
        test::black_box(restored);
    });
}

// ============================================================================
// String Benchmarks
// ============================================================================

#[bench]
fn bench_rkyv_serialize_string(bench: &mut Bencher) {
    let value = Dynamic::from("Hello, World! This is a benchmark string.");
    bench.iter(|| {
        let bytes = rhai::rkyv::to_bytes(&value).unwrap();
        test::black_box(bytes);
    });
}

#[bench]
fn bench_serde_json_serialize_string(bench: &mut Bencher) {
    let value = Dynamic::from("Hello, World! This is a benchmark string.");
    bench.iter(|| {
        let json = serde_json::to_string(&value).unwrap();
        test::black_box(json);
    });
}

#[bench]
fn bench_rkyv_deserialize_string(bench: &mut Bencher) {
    let value = Dynamic::from("Hello, World! This is a benchmark string.");
    let bytes = rhai::rkyv::to_bytes(&value).unwrap();
    
    bench.iter(|| {
        let restored: Dynamic = rhai::rkyv::from_bytes_owned(&bytes).unwrap();
        test::black_box(restored);
    });
}

#[bench]
fn bench_serde_json_deserialize_string(bench: &mut Bencher) {
    let value = Dynamic::from("Hello, World! This is a benchmark string.");
    let json = serde_json::to_string(&value).unwrap();
    
    bench.iter(|| {
        let restored: Dynamic = serde_json::from_str(&json).unwrap();
        test::black_box(restored);
    });
}

// ============================================================================
// Float Benchmarks
// ============================================================================

#[cfg(not(feature = "no_float"))]
#[bench]
fn bench_rkyv_roundtrip_float(bench: &mut Bencher) {
    let value = Dynamic::from(3.14159265358979_f64);
    bench.iter(|| {
        let bytes = rhai::rkyv::to_bytes(&value).unwrap();
        let restored: Dynamic = rhai::rkyv::from_bytes_owned(&bytes).unwrap();
        test::black_box(restored);
    });
}

#[cfg(not(feature = "no_float"))]
#[bench]
fn bench_serde_json_roundtrip_float(bench: &mut Bencher) {
    let value = Dynamic::from(3.14159265358979_f64);
    bench.iter(|| {
        let json = serde_json::to_string(&value).unwrap();
        let restored: Dynamic = serde_json::from_str(&json).unwrap();
        test::black_box(restored);
    });
}
