#![cfg(not(feature = "no_object"))]
use rhai::{Engine, EvalAltResult, INT};

#[derive(Debug, Clone, Eq, PartialEq)]
struct TestStruct {
    x: INT,
}

impl TestStruct {
    fn update(&mut self, n: INT) {
        self.x += n;
    }

    fn new() -> Self {
        Self { x: 1 }
    }
}

#[test]
fn test_method_call() {
    let mut engine = Engine::new();

    engine.register_type::<TestStruct>().register_fn("update", TestStruct::update).register_fn("new_ts", TestStruct::new);

    assert_eq!(engine.eval::<TestStruct>("let x = new_ts(); x.update(1000); x").unwrap(), TestStruct { x: 1001 });
    assert_eq!(engine.eval::<TestStruct>("let x = new_ts(); update(x, 1000); x").unwrap(), TestStruct { x: 1001 });
}

#[test]
fn test_method_call_style() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = -123; x.abs(); x").unwrap(), -123);
}

#[cfg(not(feature = "no_optimize"))]
#[test]
fn test_method_call_with_full_optimization() {
    let mut engine = Engine::new();

    engine.set_optimization_level(rhai::OptimizationLevel::Full);

    engine
        .register_fn("new_ts", TestStruct::new)
        .register_fn("ymd", |_: INT, _: INT, _: INT| 42 as INT)
        .register_fn("range", |_: &mut TestStruct, _: INT, _: INT| TestStruct::new());

    assert_eq!(
        engine
            .eval::<TestStruct>(
                "
                    let xs = new_ts();
                    let ys = xs.range(ymd(2022, 2, 1), ymd(2022, 2, 2));
                    ys
                "
            )
            .unwrap(),
        TestStruct::new()
    );
}

#[cfg(not(feature = "no_function"))]
#[test]
fn test_method_call_typed() {
    let mut engine = Engine::new();

    engine
        .register_type_with_name::<TestStruct>("Test-Struct#ABC")
        .register_fn("update", TestStruct::update)
        .register_fn("new_ts", TestStruct::new);

    assert_eq!(
        engine
            .eval::<TestStruct>(
                r#"
                    fn "Test-Struct#ABC".foo(x) {
                        this.update(x);
                    }
                    fn foo(x) {
                        this += x;
                    }
                    
                    let z = 1000;
                    z.foo(1);

                    let x = new_ts();
                    x.foo(z);

                    x
                "#
            )
            .unwrap(),
        TestStruct { x: 1002 }
    );

    assert!(engine
        .eval::<bool>(
            r#"
                fn "Test-Struct#ABC".foo(x) {
                    this.update(x);
                }
                is_def_fn("Test-Struct#ABC", "foo", 1)
            "#
        )
        .unwrap());

    assert!(matches!(
        *engine
            .run(
                r#"
                    fn "Test-Struct#ABC".foo(x) {
                        this.update(x);
                    }
                    foo(1000);
                "#
            )
            .unwrap_err(),
        EvalAltResult::ErrorFunctionNotFound(f, ..) if f.starts_with("foo")
    ));

    assert!(matches!(
        *engine
            .run(
                r#"
                    fn "Test-Struct#ABC".foo(x) {
                        this.update(x);
                    }
                    let x = 42;
                    x.foo(1000);
                "#
            )
            .unwrap_err(),
        EvalAltResult::ErrorFunctionNotFound(f, ..) if f.starts_with("foo")
    ));
}
