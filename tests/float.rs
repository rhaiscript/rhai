extern crate rhai;

use rhai::{
    Engine,
    RegisterFn
};

#[test]
fn test_float() {
    let mut engine = Engine::new();

    if let Ok(result) = engine.eval::<bool>("let x = 0.0; let y = 1.0; x < y") {
        assert!(result);
    } else {
        assert!(false);
    }

    if let Ok(result) = engine.eval::<bool>("let x = 0.0; let y = 1.0; x > y") {
        assert!(!result);
    } else {
        assert!(false);
    }

    if let Ok(result) = engine.eval::<f64>("let x = 9.9999; x") {
        assert_eq!(result, 9.9999);
    } else {
        assert!(false);
    }
}

#[test]
fn struct_with_float() {
    #[derive(Clone)]
    struct TestStruct {
        x: f64,
    }

    impl TestStruct {
        fn update(&mut self) {
            self.x += 5.789_f64;
        }

        fn get_x(&mut self) -> f64 {
            self.x
        }

        fn set_x(&mut self, new_x: f64) {
            self.x = new_x;
        }

        fn new() -> TestStruct {
            TestStruct { x: 1.0 }
        }
    }

    let mut engine = Engine::new();

    engine.register_type::<TestStruct>();

    engine.register_get_set("x", TestStruct::get_x, TestStruct::set_x);
    engine.register_fn("update", TestStruct::update);
    engine.register_fn("new_ts", TestStruct::new);

    if let Ok(result) = engine.eval::<f64>("let ts = new_ts(); ts.update(); ts.x") {
        assert_eq!(result, 6.789);
    } else {
        assert!(false);
    }

    if let Ok(result) = engine.eval::<f64>("let ts = new_ts(); ts.x = 10.1001; ts.x") {
        assert_eq!(result, 10.1001);
    } else {
        assert!(false);
    }
}
