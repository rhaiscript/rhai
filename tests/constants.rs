use rhai::{Engine, EvalAltResult, ParseErrorType, Scope, INT};

#[test]
fn test_constant() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("const x = 123; x").unwrap(), 123);

    assert!(matches!(
        *engine.eval::<INT>("const x = 123; x = 42;").expect_err("expects error"),
        EvalAltResult::ErrorParsing(ParseErrorType::AssignmentToConstant(x), ..) if x == "x"
    ));

    #[cfg(not(feature = "no_index"))]
    assert!(matches!(
        *engine.run("const x = [1, 2, 3, 4, 5]; x[2] = 42;").expect_err("expects error"),
        EvalAltResult::ErrorAssignmentToConstant(x, ..) if x == "x"
    ));
}

#[test]
fn test_constant_scope() {
    let engine = Engine::new();

    let mut scope = Scope::new();
    scope.push_constant("x", 42 as INT);

    assert!(matches!(
        *engine.run_with_scope(&mut scope, "x = 1").expect_err("expects error"),
        EvalAltResult::ErrorAssignmentToConstant(x, ..) if x == "x"
    ));
}

#[cfg(not(feature = "no_object"))]
#[test]
fn test_constant_mut() {
    #[derive(Debug, Clone)]
    struct TestStruct(INT); // custom type

    let mut engine = Engine::new();

    fn set_value(obj: &mut TestStruct, value: INT) {
        obj.0 = value;
    }

    engine
        .register_type_with_name::<TestStruct>("TestStruct")
        .register_fn("new_ts", || TestStruct(123))
        .register_get("value", |obj: &mut TestStruct| obj.0)
        .register_set("value", set_value)
        .register_fn("update_value", set_value);

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    const MY_NUMBER = new_ts();
                    MY_NUMBER.update_value(42);
                    MY_NUMBER.value
                ",
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    const MY_NUMBER = new_ts();
                    update_value(MY_NUMBER, 42);
                    MY_NUMBER.value
                ",
            )
            .unwrap(),
        123
    );

    assert!(matches!(
        *engine
            .run(
                "
                    const MY_NUMBER = new_ts();
                    MY_NUMBER.value = 42;
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorNonPureMethodCallOnConstant(..)
    ));

    let mut scope = Scope::new();

    scope.push_constant("MY_NUMBER", TestStruct(123));

    assert_eq!(
        engine
            .eval_with_scope::<INT>(
                &mut scope,
                "
                    update_value(MY_NUMBER, 42);
                    MY_NUMBER.value
                ",
            )
            .unwrap(),
        123
    );

    assert_eq!(
        engine
            .eval_with_scope::<INT>(
                &mut scope,
                "
                    MY_NUMBER.update_value(42);
                    MY_NUMBER.value
                ",
            )
            .unwrap(),
        42
    );

    assert!(matches!(*engine.run_with_scope(&mut scope, "MY_NUMBER.value = 42;").unwrap_err(), EvalAltResult::ErrorNonPureMethodCallOnConstant(..)));
}
