use rhai::{Engine, INT};

#[test]
fn test_if() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("if true { 55 }").unwrap(), 55);
    assert_eq!(engine.eval::<INT>("if false { 55 } else { 44 }").unwrap(), 44);
    assert_eq!(engine.eval::<INT>("if true { 55 } else { 44 }").unwrap(), 55);
    assert_eq!(engine.eval::<INT>("if false { 55 } else if true { 33 } else { 44 }").unwrap(), 33);
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    if false { 55 }
                    else if false { 33 }
                    else if false { 66 }
                    else if false { 77 }
                    else if false { 88 }
                    else { 44 }
                "
            )
            .unwrap(),
        44
    );
}

#[test]
fn test_if_expr() {
    let engine = Engine::new();

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = 42;
                    let y = 1 + if x > 40 { 100 } else { 0 } / x;
                    y
                "
            )
            .unwrap(),
        3
    );
}
