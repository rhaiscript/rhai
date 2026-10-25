use std::{
    env,
    fs::File,
    io::{Read, Write},
};

fn main() {
    // Tell Cargo that if the given environment variable changes, to rerun this build script.
    println!("cargo:rerun-if-changed=build.template");
    println!("cargo:rerun-if-env-changed=RHAI_AHASH_SEED");
    println!("cargo:rerun-if-env-changed=RHAI_HASHING_SEED");
    println!("cargo:rerun-if-env-changed=RHAI_NO_ATOMIC64");

    // The operations count is an `AtomicU64` where the target has 64-bit atomics,
    // and a plain `u64` otherwise. Setting `RHAI_NO_ATOMIC64` forces the plain
    // `u64` so that path can be built and tested on any host.
    println!("cargo:rustc-check-cfg=cfg(atomic_ops_count)");
    let has_atomic64 =
        env::var("CARGO_CFG_TARGET_HAS_ATOMIC").map_or(false, |v| v.split(',').any(|w| w == "64"));
    if has_atomic64 && env::var_os("RHAI_NO_ATOMIC64").is_none() {
        println!("cargo:rustc-cfg=atomic_ops_count");
    }

    // The operations meter counts in a `Cell` without `sync`, and in an `AtomicU64` with it,
    // which needs 64-bit atomics; it does not exist under `unchecked`.
    println!("cargo:rustc-check-cfg=cfg(operation_meter)");
    let sync = env::var_os("CARGO_FEATURE_SYNC").is_some();
    let unchecked = env::var_os("CARGO_FEATURE_UNCHECKED").is_some();
    if !unchecked && (!sync || has_atomic64) {
        println!("cargo:rustc-cfg=operation_meter");
    }

    let mut contents = String::new();

    File::open("build.template")
        .expect("cannot open `build.template`")
        .read_to_string(&mut contents)
        .expect("cannot read from `build.template`");

    let seed = env::var("RHAI_HASHING_SEED")
        .or_else(|_| env::var("RHAI_AHASH_SEED"))
        .map_or_else(|_| "None".into(), |s| format!("Some({s})"));

    contents = contents.replace("{{HASHING_SEED}}", &seed);

    File::create("src/config/hashing_env.rs")
        .expect("cannot create `hashing_env.rs`")
        .write_all(contents.as_bytes())
        .expect("cannot write to `config/hashing_env.rs`");
}
