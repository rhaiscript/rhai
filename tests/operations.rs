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
