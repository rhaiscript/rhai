use rhai::{Engine, EvalAltResult, INT};

#[test]
fn test_mismatched_op() {
    let engine = Engine::new();

    assert!(matches!(
        *engine.eval::<INT>(r#""hello, " + "world!""#).expect_err("expects error"),
        EvalAltResult::ErrorMismatchOutputType(need, actual, ..) if need == std::any::type_name::<INT>() && actual == "string"
    ));
}

#[test]
fn test_mismatched_op_name() {
    let engine = Engine::new();

    assert!(matches!(
        *engine.eval::<String>("true").expect_err("expects error"),
        EvalAltResult::ErrorMismatchOutputType(need, actual, ..) if need == "string" && actual == "bool"
    ));

    assert!(matches!(
        *engine.eval::<&str>("true").expect_err("expects error"),
        EvalAltResult::ErrorMismatchOutputType(need, actual, ..) if need == "&str" && actual == "bool"
    ));
}

#[test]
#[cfg(not(feature = "no_object"))]
fn test_mismatched_op_custom_type() {
    #[allow(dead_code)] // used inside `register_type_with_name`
    #[derive(Debug, Clone)]
    struct TestStruct {
        x: INT,
    }

    impl TestStruct {
        fn new() -> Self {
            Self { x: 1 }
        }
    }

    let mut engine = Engine::new();

    engine.register_type_with_name::<TestStruct>("TestStruct").register_fn("new_ts", TestStruct::new);

    assert!(!engine.eval::<bool>("new_ts() == 42").unwrap());

    assert!(engine.eval::<bool>("new_ts() != ()").unwrap());

    assert!(matches!(*engine.eval::<bool>(
        "
            let x = new_ts();
            let y = new_ts();
            x == y
        ").unwrap_err(),
        EvalAltResult::ErrorFunctionNotFound(f, ..) if f == "== (TestStruct, TestStruct)"));

    assert!(matches!(
        *engine.eval::<INT>("60 + new_ts()").unwrap_err(),
        EvalAltResult::ErrorFunctionNotFound(f, ..) if f == format!("+ ({}, TestStruct)", std::any::type_name::<INT>())
    ));

    assert!(matches!(
        *engine.eval::<TestStruct>("42").unwrap_err(),
        EvalAltResult::ErrorMismatchOutputType(need, actual, ..)
            if need == "TestStruct" && actual == std::any::type_name::<INT>()
    ));
}
