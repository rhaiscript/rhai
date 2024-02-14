use rhai::{Engine, EvalAltResult, Scope, INT};

#[test]
fn test_ops() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("60 + 5").unwrap(), 65);
    assert_eq!(engine.eval::<INT>("(1 + 2) * (6 - 4) / 2").unwrap(), 3);
    assert_eq!(engine.eval::<INT>("let x = 41; x = x + 1; x").unwrap(), 42);
    assert_eq!(engine.eval::<String>(r#"let s = "hello"; s = s + 42; s"#).unwrap(), "hello42");
}

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[test]
fn test_ops_other_number_types() {
    let engine = Engine::new();

    let mut scope = Scope::new();

    scope.push("x", 42_u16);

    assert!(matches!(
        *engine.eval_with_scope::<bool>(&mut scope, "x == 42").unwrap_err(),
        EvalAltResult::ErrorFunctionNotFound(f, ..) if f.starts_with("== (u16,")
    ));
    #[cfg(not(feature = "no_float"))]
    assert!(matches!(
        *engine.eval_with_scope::<bool>(&mut scope, "x == 42.0").unwrap_err(),
        EvalAltResult::ErrorFunctionNotFound(f, ..) if f.starts_with("== (u16,")
    ));

    assert!(!engine.eval_with_scope::<bool>(&mut scope, r#"x == "hello""#).unwrap());
}

#[test]
fn test_ops_strings() {
    let engine = Engine::new();

    assert!(engine.eval::<bool>(r#""hello" > 'c'"#).unwrap());
    assert!(engine.eval::<bool>(r#""" < 'c'"#).unwrap());
    assert!(engine.eval::<bool>(r#"'x' > "hello""#).unwrap());
    assert!(engine.eval::<bool>(r#""hello" > "foo""#).unwrap());
}

#[test]
fn test_ops_precedence() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 0; if x == 10 || true { x = 1} x").unwrap(), 1);
}

#[test]
fn test_ops_custom_types() {
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Test1;
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Test2;

    let mut engine = Engine::new();

    engine
        .register_type_with_name::<Test1>("Test1")
        .register_type_with_name::<Test2>("Test2")
        .register_fn("new_ts1", || Test1)
        .register_fn("new_ts2", || Test2)
        .register_fn("==", |_: Test1, _: Test2| true);

    assert!(engine.eval::<bool>("let x = new_ts1(); let y = new_ts2(); x == y").unwrap());
    assert!(engine.eval::<bool>("let x = new_ts1(); let y = new_ts2(); x != y").unwrap());
    assert!(!engine.eval::<bool>("let x = new_ts1(); x == ()").unwrap());
    assert!(engine.eval::<bool>("let x = new_ts1(); x != ()").unwrap());
}
