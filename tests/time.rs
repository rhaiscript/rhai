#![cfg(not(feature = "no_time"))]
use rhai::Engine;
#[cfg(not(feature = "no_float"))]
use rhai::FLOAT;
#[cfg(feature = "no_float")]
use rhai::INT;

#[test]
fn test_timestamp() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<String>("type_of(timestamp())").unwrap(), "timestamp");

    #[cfg(not(feature = "no_float"))]
    assert!(
        engine
            .eval::<FLOAT>(
                r#"
                    let time = timestamp();
                    let x = 10_000;
                    while x > 0 { x -= 1; }
                    elapsed(time)
                "#
            )
            .unwrap()
            < 10.0
    );

    #[cfg(feature = "no_float")]
    assert!(
        engine
            .eval::<INT>(
                r#"
                    let time = timestamp();
                    let x = 10_000;
                    while x > 0 { x -= 1; }
                    elapsed(time)
                "#
            )
            .unwrap()
            < 10
    );

    assert!(engine
        .eval::<bool>(
            "
                let time1 = timestamp();
                for x in 0..10000 {}
                let time2 = timestamp();
                time1 <= time2
            "
        )
        .unwrap());
}

#[test]
fn test_timestamp_op() {
    let engine = Engine::new();

    #[cfg(not(feature = "no_float"))]
    assert!(
        (engine
            .eval::<FLOAT>(
                r#"
                    let time1 = timestamp();
                    let time2 = time1 + 123.45;
                    time2 - time1
                "#
            )
            .unwrap()
            - 123.45)
            .abs()
            < 0.001
    );

    #[cfg(not(feature = "no_float"))]
    assert!(
        (engine
            .eval::<FLOAT>(
                r#"
                    let time1 = timestamp();
                    let time2 = time1 - 123.45;
                    time1 - time2
                "#
            )
            .unwrap()
            - 123.45)
            .abs()
            < 0.001
    );

    #[cfg(feature = "no_float")]
    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let time1 = timestamp();
                    let time2 = time1 + 42;
                    time2 - time1
                "#
            )
            .unwrap(),
        42
    );

    #[cfg(feature = "no_float")]
    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let time1 = timestamp();
                    let time2 = time1 - 42;
                    time1 - time2
                "#
            )
            .unwrap(),
        42
    );

    // Check edge case for crashes
    #[cfg(not(feature = "unchecked"))]
    let _ = engine.run("timestamp()-24>>-60");
}
