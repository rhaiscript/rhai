//! This is a bare-bones `no-std` application that evaluates
//! a simple expression and uses the result as the return value.

#![no_main]
#![no_std]
#![feature(alloc_error_handler, abort_immediate)]

extern crate alloc;
extern crate wee_alloc;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// NB: Rust needs a CRT runtime on Windows MSVC.
#[cfg(all(windows, target_env = "msvc"))]
#[link(name = "msvcrt")]
#[link(name = "libcmt")]
extern "C" {}

use rhai::{Engine, INT};

#[no_mangle]
pub extern "C" fn main(_argc: isize, _argv: *const *const u8) -> isize {
    // Notice that this is a _raw_ engine.
    // To do anything useful, load a few packages from `rhai::packages`.
    let engine = Engine::new_raw();

    // Evaluate a simple expression: 40 + 2
    engine.eval_expression::<INT>("40 + 2").unwrap() as isize
}

#[alloc_error_handler]
fn foo(_: core::alloc::Layout) -> ! {
    core::process::abort_immediate()
}

#[panic_handler]
fn rust_begin_panic(_: &core::panic::PanicInfo) -> ! {
    core::process::abort_immediate()
}

#[no_mangle]
extern "C" fn _rust_eh_personality() {}

#[no_mangle]
extern "C" fn rust_eh_personality() {}

#[no_mangle]
extern "C" fn rust_eh_register_frames() {}

#[no_mangle]
extern "C" fn rust_eh_unregister_frames() {}

#[no_mangle]
extern "C" fn _Unwind_Resume() {}
