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

    #[cfg(not(feature = "no_object"))]
    #[cfg(not(feature = "no_optimize"))]
    assert!(matches!(
        *engine.run("const x = #{}; x.p = 42;").expect_err("expects error"),
        EvalAltResult::ErrorAssignmentToConstant(x, ..) if x == ""
    ));
}

#[test]
fn test_constant_scope() {
    let engine = Engine::new();

    let mut scope = Scope::new();
    scope.push_constant("x", 42 as INT);

    assert!(matches!(
        *engine.eval_with_scope::<()>(&mut scope, "x = 1").expect_err("expects error"),
        EvalAltResult::ErrorAssignmentToConstant(x, ..) if x == "x"
    ));
}

#[cfg(not(feature = "no_index"))]
#[test]
fn test_constant_scope_index() {
    let engine = Engine::new();

    let mut scope = Scope::new();
    scope.push_constant("s", "hello");
    scope.push_constant("n", 42 as INT);
    scope.push_constant("b", vec![1_u8, 2, 3]);

    for script in ["s[0] = 'j'", "s[1..3] = \"xy\"", "n[0] = true", "n[0..4] = 15", "b[0] = 9", "b[0] += 1"] {
        assert!(matches!(*engine.run_with_scope(&mut scope, script).expect_err("expects error"), EvalAltResult::ErrorAssignmentToConstant(..)), "{script}");
    }

    #[cfg(not(feature = "no_object"))]
    {
        let mut map = rhai::Map::new();
        map.insert("s".into(), "abc".into());
        scope.push_constant("m", map);

        for script in ["m.s[0] = 'z'", "m.t = 1", "m[\"t\"] = 1"] {
            assert!(matches!(*engine.run_with_scope(&mut scope, script).expect_err("expects error"), EvalAltResult::ErrorAssignmentToConstant(..)), "{script}");
        }

        for script in ["s[0].make_upper()", "s[0..2].clear()", "n[0..4].set_bit(0, true)", "b[0].set_bit(7, true)"] {
            assert!(matches!(*engine.run_with_scope(&mut scope, script).expect_err("expects error"), EvalAltResult::ErrorNonPureMethodCallOnConstant(..)), "{script}");
        }

        assert_eq!(format!("{:?}", scope.get_value::<rhai::Map>("m").unwrap()), r#"{"s": "abc"}"#);
    }

    assert_eq!(scope.get_value::<rhai::ImmutableString>("s").unwrap(), "hello");
    assert_eq!(scope.get_value::<INT>("n").unwrap(), 42);
    assert_eq!(scope.get_value::<rhai::Blob>("b").unwrap(), vec![1, 2, 3]);
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

    assert!(matches!(*engine.eval_with_scope::<()>(&mut scope, "MY_NUMBER.value = 42;").unwrap_err(), EvalAltResult::ErrorNonPureMethodCallOnConstant(..)));
}
