//! Regression test for the fn-resolution cache bug that motivates the
//! `EvalContext::push_fn_resolution_cache` / `fn_resolution_caches_len` /
//! `rewind_fn_resolution_caches` API exposed by this PR.
//!
//! The bug: when an `on_missing_function` callback pushes a module onto
//! `global.lib` and then calls `ctx.call_fn_raw`, the nested resolution is
//! cached against whatever `global.lib` configuration is in effect at the
//! time. Subsequent callback invocations that push a *different* module
//! under the same `(name, arg_types)` hash will hit the cached (stale)
//! entry and dispatch to the wrong module's function. Without a way to
//! isolate the nested cache frame, external callers of `on_missing_function`
//! cannot implement correct class-hierarchy dispatch, `parent_view()`/super
//! walks, or any pattern that relies on swapping `global.lib` contents
//! between calls of the same name.
//!
//! The fix: bracket `call_fn_raw` with `push_fn_resolution_cache` /
//! `rewind_fn_resolution_caches` so each nested dispatch resolves in a
//! fresh cache frame and discards its entries on exit. The primitives
//! already exist on `Caches` internally (see `src/eval/stmt.rs:63-95` and
//! `src/func/script.rs:94-206`); this PR just forwards them through
//! `EvalContext` so `on_missing_function` callbacks can use them too.

#![cfg(feature = "internals")]
#![cfg(not(feature = "no_object"))]
#![cfg(not(feature = "no_function"))]

use rhai::{Array, Engine, Module, INT};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn on_missing_function_isolates_nested_cache_frame() {
    // Two modules, each registering `step(target, n) -> INT` under the same
    // name and argument types. Same `(name, arg_types)` produces the same
    // fn-resolution cache hash.
    let mut module_a = Module::new();
    module_a.set_native_fn(
        "step",
        |_target: &mut INT, n: INT| -> Result<INT, Box<rhai::EvalAltResult>> { Ok(n * 2) },
    );
    let module_a: Arc<Module> = Arc::new(module_a);

    let mut module_b = Module::new();
    module_b.set_native_fn(
        "step",
        |_target: &mut INT, n: INT| -> Result<INT, Box<rhai::EvalAltResult>> { Ok(n * 10) },
    );
    let module_b: Arc<Module> = Arc::new(module_b);

    // Counter picks module_a for invocations 0-1, module_b for invocation 2+.
    // Three invocations are needed to exercise the cache lifecycle: call 1
    // sets the bloom filter bit (no dict entry yet), call 2 populates the
    // dict against module_a, call 3 (with module_b pushed) must hit a FRESH
    // resolution in its isolated frame — if isolation is missing it would
    // hit the stale dict entry from call 2 and return module_a's result.
    let call_idx = Arc::new(AtomicUsize::new(0));

    let a = module_a.clone();
    let b = module_b.clone();
    let call_idx_clone = call_idx.clone();

    let mut engine = Engine::new();

    #[allow(deprecated)]
    engine.on_missing_function(move |name, args, _is_method_call, mut ctx| {
        if name != "step" {
            return Ok(None);
        }

        let idx = call_idx_clone.fetch_add(1, Ordering::SeqCst);
        let module = if idx < 2 { a.clone() } else { b.clone() };

        ctx.global_runtime_state_mut().lib.push(module);

        // Isolate the nested dispatch in a fresh cache frame. Without this,
        // invocation 2 would cache the resolution against module_a, and
        // invocation 3 would hit that stale cache entry after module_b had
        // been pushed.
        let orig_cache_len = ctx.fn_resolution_caches_len();
        ctx.push_fn_resolution_cache();

        let result = ctx.call_fn_raw(name, true, false, args);

        ctx.rewind_fn_resolution_caches(orig_cache_len);
        ctx.global_runtime_state_mut().lib.pop();

        result.map(Some)
    });

    // Three method-style calls with identical name and argument types so
    // they all hash to the same fn-resolution cache key.
    let script = r#"
        let x = 0;
        [x.step(3), x.step(3), x.step(3)]
    "#;

    let results: Array = engine.eval(script).expect("eval must succeed");
    let results: Vec<INT> = results
        .into_iter()
        .map(|v| v.as_int().expect("array element must be INT"))
        .collect();

    assert_eq!(
        results,
        vec![6, 6, 30],
        "third invocation must dispatch to module_b (3*10=30), not a stale \
         cached entry from module_a (3*2=6); without push/rewind_fn_resolution_cache \
         the third element would be 6."
    );

    assert_eq!(
        call_idx.load(Ordering::SeqCst),
        3,
        "on_missing_function must fire exactly three times"
    );
}
