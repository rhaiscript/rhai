//! Arbitrary bytes into `Program::read`, then run whatever survives.
//!
//! This is the whole untrusted surface: a device is handed an artifact over a
//! link and executes it in place. The claim is total — any byte string either
//! fails to load, or loads into a chunk that runs without panicking, reading
//! outside itself, or running away.
//!
//! Running the survivors is the point rather than a bonus. A loader that
//! accepts a chunk it should not have has done nothing observable until
//! something executes it, so a target that only called `read` would miss the
//! failures that matter most.
//!
//! `cargo fuzz run load`

#![no_main]

use libfuzzer_sys::fuzz_target;
use rhai::grain::{Program, Vm};
use rhai::{Engine, Scope};

fuzz_target!(|data: &[u8]| {
    let Ok(program) = Program::read(data) else {
        return;
    };

    // `read` verifies before returning, and the VM's missing bounds checks
    // rest on that. If it ever hands back something unverified, the contract
    // is broken whether or not running it happens to work.
    assert!(
        program.verify().is_ok(),
        "read returned a chunk that does not verify",
    );

    // Verification proves structure, not termination: a jump target that is
    // in range is a valid infinite loop. That is not a gap a loader can close
    // — no one can decide halting — which is why `Op::Tick` sits on every back
    // edge and why a host running untrusted bytecode must set this.
    let mut engine = Engine::new();
    engine.set_max_operations(10_000);
    engine.set_max_string_size(4096);
    engine.set_max_array_size(1024);
    engine.set_max_map_size(64);
    engine.set_max_call_levels(16);

    let _ = Vm::new(&engine).eval_with_scope(&mut Scope::new(), &program);
});
