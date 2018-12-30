extern crate rhai;

use rhai::{
    Engine,
    RegisterFn
};

#[test]
fn test_arrays() {
    let mut engine = Engine::new();

    if let Ok(result) = engine.eval::<i64>("let x = [1, 2, 3]; x[1]") {
        assert_eq!(result, 2);
    } else {
        assert!(false);
    }

    if let Ok(result) = engine.eval::<i64>("let y = [1, 2, 3]; y[1] = 5; y[1]") {
        assert_eq!(result, 5);
    } else {
        assert!(false);
    }
}

#[test]
fn test_array_with_structs() {
    #[derive(Clone)]
    struct TestStruct {
        x: i64,
    }

    impl TestStruct {
        fn update(&mut self) {
            self.x += 1000;
        }

        fn get_x(&mut self) -> i64 {
            self.x
        }

        fn set_x(&mut self, new_x: i64) {
            self.x = new_x;
        }

        fn new() -> TestStruct {
            TestStruct { x: 1 }
        }
    }

    let mut engine = Engine::new();

    engine.register_type::<TestStruct>();

    engine.register_get_set("x", TestStruct::get_x, TestStruct::set_x);
    engine.register_fn("update", TestStruct::update);
    engine.register_fn("new_ts", TestStruct::new);

    if let Ok(result) = engine.eval::<i64>("let a = [new_ts()]; a[0].x") {
        assert_eq!(result, 1);
    } else {
        assert!(false);
    }

    if let Ok(result) = engine.eval::<i64>("let a = [new_ts()]; a[0].x = 100; a[0].update(); \
                                            a[0].x") {
        assert_eq!(result, 1100);
    } else {
        assert!(false);
    }
}
