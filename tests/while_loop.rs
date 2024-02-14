use rhai::{Engine, INT};

#[test]
fn test_while() {
    let engine = Engine::new();

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = 0;

                    while x < 10 {
                        x += 1;
                        if x > 5 { break; }
                        if x > 3 { continue; }
                        x += 3;
                    }
                    
                    x
                ",
            )
            .unwrap(),
        6
    );

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = 0;

                    while x < 10 {
                        x += 1;
                        if x > 5 { break x * 2; }
                        if x > 3 { continue; }
                        x += 3;
                    }
                ",
            )
            .unwrap(),
        12
    );
}

#[test]
fn test_do() {
    let engine = Engine::new();

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = 0;

                    do {
                        x += 1;
                        if x > 5 { break; }
                        if x > 3 { continue; }
                        x += 3;
                    } while x < 10;
                    
                    x
                ",
            )
            .unwrap(),
        6
    );
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = 0;

                    do {
                        x += 1;
                        if x > 5 { break x * 2; }
                        if x > 3 { continue; }
                        x += 3;
                    } while x < 10;
                ",
            )
            .unwrap(),
        12
    );

    engine.run("do {} while false").unwrap();

    assert_eq!(engine.eval::<INT>("do { break 42; } while false").unwrap(), 42);
}

#[cfg(not(feature = "unchecked"))]
#[test]
fn test_infinite_loops() {
    let mut engine = Engine::new();

    engine.set_max_operations(1024);

    assert!(engine.run("loop {}").is_err());
    assert!(engine.run("while true {}").is_err());
    assert!(engine.run("do {} while true").is_err());
}
