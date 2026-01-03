#![cfg(not(feature = "no_function"))]
use rhai::{Engine, INT};

#[test]
fn test_pipeline_basic() {
    let engine = Engine::new();

    // Simple function chaining: value is passed as first argument to the function on the right
    let result = engine.eval::<INT>("fn inc(x) { x + 1 }; 1 |> inc |> inc").unwrap();

    assert_eq!(result, 3);
}

#[test]
fn test_pipeline_with_extra_arg() {
    let engine = Engine::new();

    // Pipeline into a function that takes additional arguments
    let result = engine.eval::<INT>("fn add(a, b) { a + b }; 1 |> add(2)").unwrap();

    assert_eq!(result, 3);
}

#[test]
fn test_pipeline_into_method_call_style() {
    let engine = Engine::new();

    // Pipeline into a method-call-style builtin (abs).
    // The pipeline passes the left-hand value as the first argument of the call on the right.
    let result = engine.eval::<INT>("let x = -123; x |> abs(); x").unwrap();

    // abs should not have mutated `x` here, so `x` remains -123
    assert_eq!(result, -123);
}

#[cfg(not(feature = "no_object"))]
mod pipeline_method_tests {
    use rhai::{Engine, INT};

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
    fn test_pipeline_into_registered_method() {
        let mut engine = Engine::new();

        engine.register_type::<TestStruct>().register_fn("update", TestStruct::update).register_fn("new_ts", TestStruct::new);

        // Pipeline into a registered method should forward the left-hand value as the first argument
        let result = engine.eval::<TestStruct>("let x = new_ts(); x |> update(1000); x").unwrap();

        assert_eq!(result, TestStruct { x: 1001 });
    }
}
