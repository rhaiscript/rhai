#![cfg(not(feature = "unchecked"))]
use rhai::{Engine, EvalAltResult, INT};

#[cfg(all(feature = "internals", not(feature = "sync")))]
#[test]
fn test_operations_shared_state() {
    let engine = Engine::new();
    let global = engine.new_global_runtime_state();
    let shared = &global;
    assert_eq!(shared.num_operations(), 0);
    shared.num_operations.set(42);
    assert_eq!(shared.num_operations(), 42);

    let cloned = shared.clone();
    assert_eq!(cloned.num_operations(), 42);
    cloned.num_operations.set(100);
    assert_eq!(shared.num_operations(), 42);
    assert_eq!(cloned.num_operations(), 100);
}

#[cfg(all(feature = "internals", not(feature = "no_function"), any(not(feature = "sync"), atomic_ops_count)))]
#[test]
fn test_operations_native_reentry() {
    let mut engine = Engine::new();
    engine.set_max_operations(1000);
    engine.register_fn("reenter", |context: rhai::NativeCallContext| -> Result<(), Box<EvalAltResult>> {
        let before = context.global_runtime_state().num_operations();
        assert_eq!(context.call_fn::<INT>("work", ())?, 45);
        assert!(context.global_runtime_state().num_operations() > before);
        Ok(())
    });

    engine.run("fn work() { let sum = 0; for x in 0..10 { sum += x; } sum } reenter();").unwrap();
}

#[test]
fn test_max_operations() {
    let mut engine = Engine::new();
    #[cfg(not(feature = "no_optimize"))]
    engine.set_optimization_level(rhai::OptimizationLevel::None);
    engine.set_max_operations(500);

    engine.on_progress(|count| {
        if count % 100 == 0 {
            println!("{count}");
        }
        None
    });

    engine.run("let x = 0; while x < 20 { x += 1; }").unwrap();

    assert!(matches!(*engine.run("for x in 0..500 {}").unwrap_err(), EvalAltResult::ErrorTooManyOperations(..)));

    engine.set_max_operations(0);

    engine.run("for x in 0..10000 {}").unwrap();
}

#[test]
fn test_max_operations_literal() {
    let mut engine = Engine::new();
    #[cfg(not(feature = "no_optimize"))]
    engine.set_optimization_level(rhai::OptimizationLevel::None);
    engine.set_max_operations(10);

    #[cfg(not(feature = "no_index"))]
    engine.run("[1, 2, 3, 4, 5, 6, 7]").unwrap();

    #[cfg(not(feature = "no_index"))]
    assert!(matches!(*engine.run("[1, 2, 3, 4, 5, 6, 7, 8, 9]").unwrap_err(), EvalAltResult::ErrorTooManyOperations(..)));

    engine.set_max_operations(25);

    #[cfg(not(feature = "no_object"))]
    engine.run("let x = 0; #{a:x+1, b:x+2, c:x+3, d:x+4, e:x+5, f:x+6, g:x+7}").unwrap();

    #[cfg(not(feature = "no_object"))]
    assert!(matches!(*engine.run("let x = 0; #{a:x+1, b:x+2, c:x+3, d:x+4, e:x+5, f:x+6, g:x+7, h:x+8, i:x+9}").unwrap_err(), EvalAltResult::ErrorTooManyOperations(..)));
}

#[test]
fn test_max_operations_functions() {
    let mut engine = Engine::new();
    engine.set_max_operations(500);

    engine.on_progress(|count| {
        if count % 100 == 0 {
            println!("{count}");
        }
        None
    });

    engine
        .run(
            r#"
                print("Test1");
                let x = 0;

                while x < 28 {
                    print(x);
                    x += 1;
                }
            "#,
        )
        .unwrap();

    #[cfg(not(feature = "no_function"))]
    engine
        .run(
            r#"
                print("Test2");
                fn inc(x) { x + 1 }
                let x = 0;
                while x < 20 { x = inc(x); }
            "#,
        )
        .unwrap();

    #[cfg(not(feature = "no_function"))]
    assert!(matches!(
        *engine
            .run(
                r#"
                    print("Test3");
                    fn inc(x) { x + 1 }
                    let x = 0;

                    while x < 36 {
                        print(x);
                        x = inc(x);
                    }
                "#,
            )
            .unwrap_err(),
        EvalAltResult::ErrorTooManyOperations(..)
    ));
}

#[test]
fn test_max_operations_eval() {
    let mut engine = Engine::new();
    engine.set_max_operations(500);

    engine.on_progress(|count| {
        if count % 100 == 0 {
            println!("{count}");
        }
        None
    });

    assert!(matches!(
        *engine
            .run(
                r#"
                    let script = "for x in 0..500 {}";
                    eval(script);
                "#
            )
            .unwrap_err(),
        EvalAltResult::ErrorInFunctionCall(.., err, _) if matches!(*err, EvalAltResult::ErrorTooManyOperations(..))
    ));
}

#[test]
fn test_max_operations_progress() {
    let mut engine = Engine::new();
    #[cfg(not(feature = "no_optimize"))]
    engine.set_optimization_level(rhai::OptimizationLevel::None);
    engine.set_max_operations(500);

    engine.on_progress(|count| if count < 100 { None } else { Some((42 as INT).into()) });

    assert!(matches!(
        *engine.run("for x in 0..500 {}").unwrap_err(),
        EvalAltResult::ErrorTerminated(x, ..) if x.as_int().unwrap() == 42));
}

// Without 64-bit atomics, `sync` builds cannot hand the count back from a callback.
#[cfg(any(not(feature = "sync"), atomic_ops_count))]
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_index"))]
#[cfg(not(feature = "no_object"))]
#[test]
fn test_max_operations_closures_in_callbacks() {
    let mut engine = Engine::new();
    engine.set_max_operations(10_000);

    // Each call of the closure takes about 1,000 operations, so 100 calls must hit the limit.
    for script in [
        "for k in 0..100 { [1].map(|x| { let s = 0; for i in 0..300 { s += i; } x + s }); }",
        "for k in 0..100 { [1].filter(|x| { let s = 0; for i in 0..300 { s += i; } s > 0 }); }",
        "for k in 0..100 { [1, 2].sort(|x, y| { let s = 0; for i in 0..300 { s += i; } x - y }); }",
    ] {
        let err = engine.run(script).unwrap_err();
        assert!(matches!(err.unwrap_inner(), EvalAltResult::ErrorTooManyOperations(..)), "{script}: {err}");
    }
}
