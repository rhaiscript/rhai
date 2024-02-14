use rhai::{Engine, LexError, ParseErrorType, Scope, INT};

#[test]
fn test_eval() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>(r#"eval("40 + 2")"#).unwrap(), 42);

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let foo = 42;

                    eval("let foo = 123");
                    eval("let xyz = 10");

                    foo + xyz
                "#
            )
            .unwrap(),
        133
    );
}

#[test]
fn test_eval_blocks() {
    let engine = Engine::new();

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let x = 999;

                    eval("let x = x - 1000");

                    let y = if x < 0 {
                        eval("let x = 42");
                        x
                    } else {
                        0
                    };

                    x + y
                "#
            )
            .unwrap(),
        41
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let foo = 42;

                    eval("{ let foo = 123; }");

                    foo
                "#
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let foo = 42;
                    { { {
                        eval("let foo = 123");
                    } } }
                    foo
                "#
            )
            .unwrap(),
        42
    );
}

#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_module"))]
#[test]
fn test_eval_globals() {
    let engine = Engine::new();

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    const XYZ = 123;

                    fn foo() { global::XYZ } 
                    {
                        eval("const XYZ = 42;");
                    }

                    foo()
                "#
            )
            .unwrap(),
        123
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    const XYZ = 123;

                    fn foo() { global::XYZ } 

                    eval("const XYZ = 42;");

                    foo()
                "#
            )
            .unwrap(),
        42
    );
}

#[test]
#[cfg(not(feature = "no_function"))]
fn test_eval_function() {
    let engine = Engine::new();
    let mut scope = Scope::new();

    assert_eq!(
        engine
            .eval_with_scope::<INT>(
                &mut scope,
                r#"
                    let x = 10;

                    fn foo(x) { x += 12; x }

                    let script = "let y = x;";      // build a script
                    script +=    "y += foo(y);";
                    script +=    "x + y";

                    eval(script) + x + y
                "#
            )
            .unwrap(),
        84
    );

    assert_eq!(scope.get_value::<INT>("x").expect("variable x should exist"), 10);
    assert_eq!(scope.get_value::<INT>("y").expect("variable y should exist"), 32);
    assert!(scope.contains("script"));
    assert_eq!(scope.len(), 3);
}

#[test]
fn test_eval_disabled() {
    let mut engine = Engine::new();

    engine.disable_symbol("eval");

    assert!(matches!(
        engine.compile(r#"eval("40 + 2")"#).unwrap_err().err_type(),
        ParseErrorType::BadInput(LexError::ImproperSymbol(err, ..)) if err == "eval"
    ));
}
