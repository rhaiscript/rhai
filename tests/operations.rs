#![cfg(not(feature = "unchecked"))]
use rhai::{Engine, EvalAltResult, INT};

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

    #[cfg(not(feature = "no_object"))]
    engine.run("#{a:1, b:2, c:3, d:4, e:5, f:6, g:7}").unwrap();

    #[cfg(not(feature = "no_object"))]
    assert!(matches!(*engine.run("#{a:1, b:2, c:3, d:4, e:5, f:6, g:7, h:8, i:9}").unwrap_err(), EvalAltResult::ErrorTooManyOperations(..)));
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

#[cfg(operation_meter)]
mod operation_deadline {
    use rhai::{Dynamic, Engine, EvalAltResult, OperationMeter, Shared, INT};

    #[test]
    fn the_callback_is_called_only_at_the_deadline() {
        let meter = Shared::new(OperationMeter::new());
        let calls = Shared::new(std::sync::atomic::AtomicU64::new(0));
        let mut engine = Engine::new();
        let (m, c) = (meter.clone(), calls.clone());
        engine.on_operation_deadline(meter.clone(), move |count| {
            c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            m.set_deadline(count + 100);
            None
        });
        meter.set_deadline(100);

        engine.run("let s = 0; for x in 0..300 { s += x; }").unwrap();

        let count = meter.count();
        assert!(count >= 300, "{count}");
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), count / 100);
    }

    #[test]
    fn the_callback_terminates_at_the_same_operation_as_on_progress() {
        let script = "let x = 0; while x < 1000 { x += 1; }";

        let mut progress = Engine::new();
        progress.on_progress(|count| (count >= 250).then(|| Dynamic::from(count as INT)));
        let expected = progress.run(script).unwrap_err();

        let meter = Shared::new(OperationMeter::new());
        meter.set_deadline(250);
        let mut deadline = Engine::new();
        deadline.on_operation_deadline(meter.clone(), |count| Some(Dynamic::from(count as INT)));
        let got = deadline.run(script).unwrap_err();

        match (*expected, *got) {
            (EvalAltResult::ErrorTerminated(a, ..), EvalAltResult::ErrorTerminated(b, ..)) => {
                assert_eq!(a.as_int().unwrap(), 250);
                assert_eq!(b.as_int().unwrap(), 250);
            }
            (a, b) => panic!("{a:?} {b:?}"),
        }
        assert_eq!(meter.count(), 250);
    }

    #[test]
    fn a_native_function_can_charge_its_own_work() {
        let meter = Shared::new(OperationMeter::new());
        meter.set_deadline(1000);
        let mut engine = Engine::new();
        engine.on_operation_deadline(meter.clone(), |_| Some(Dynamic::UNIT));
        let m = meter.clone();
        engine.register_fn("work", move |steps: INT| m.charge(steps as u64));

        // Without the work the script runs well under 1000 operations.
        let err = engine.run("work(990); let x = 1; x + 1; x + 2; x + 3;").unwrap_err();

        assert!(matches!(*err, EvalAltResult::ErrorTerminated(..)), "{err:?}");
        assert!(meter.count() <= 10, "{}", meter.count());
    }

    #[test]
    fn the_count_is_kept_after_an_evaluation() {
        let meter = Shared::new(OperationMeter::new());
        let mut engine = Engine::new();
        engine.on_operation_deadline(meter.clone(), |_| None);

        engine.run("let x = 1;").unwrap();
        let short = meter.count();
        engine.run("let x = 1; let y = 2; let z = x + y;").unwrap();

        assert!(short > 0);
        assert!(meter.count() > short);

        // A call of an empty function counts its own operations, not the last evaluation's, as
        // `on_progress` reports them.
        let last = Shared::new(std::sync::atomic::AtomicU64::new(0));
        let mut progress = Engine::new();
        let l = last.clone();
        progress.on_progress(move |count| {
            l.store(count, std::sync::atomic::Ordering::Relaxed);
            None
        });
        let ast = engine.compile("fn nothing() {}").unwrap();
        engine.call_fn::<()>(&mut rhai::Scope::new(), &ast, "nothing", ()).unwrap();
        progress.call_fn::<()>(&mut rhai::Scope::new(), &ast, "nothing", ()).unwrap();

        assert!(meter.count() < short);
        assert_eq!(meter.count(), last.load(std::sync::atomic::Ordering::Relaxed));
    }
}
