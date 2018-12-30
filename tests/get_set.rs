extern crate rhai;

use rhai::{
    Engine,
    RegisterFn
};

#[test]
fn test_get_set() {
    #[derive(Clone)]
    struct TestStruct {
        x: i64,
    }

    impl TestStruct {
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
    engine.register_fn("new_ts", TestStruct::new);

    if let Ok(result) = engine.eval::<i64>("let a = new_ts(); a.x = 500; a.x") {
        assert_eq!(result, 500);
    } else {
        assert!(false);
    }
}

#[test]
fn test_big_get_set() {
    #[derive(Clone)]
    struct TestChild {
        x: i64,
    }

    impl TestChild {
        fn get_x(&mut self) -> i64 {
            self.x
        }

        fn set_x(&mut self, new_x: i64) {
            self.x = new_x;
        }

        fn new() -> TestChild {
            TestChild { x: 1 }
        }
    }

    #[derive(Clone)]
    struct TestParent {
        child: TestChild,
    }

    impl TestParent {
        fn get_child(&mut self) -> TestChild {
            self.child.clone()
        }

        fn set_child(&mut self, new_child: TestChild) {
            self.child = new_child;
        }

        fn new() -> TestParent {
            TestParent { child: TestChild::new() }
        }
    }

    let mut engine = Engine::new();

    engine.register_type::<TestChild>();
    engine.register_type::<TestParent>();

    engine.register_get_set("x", TestChild::get_x, TestChild::set_x);
    engine.register_get_set("child", TestParent::get_child, TestParent::set_child);

    engine.register_fn("new_tp", TestParent::new);

    assert_eq!(engine.eval::<i64>("let a = new_tp(); a.child.x = 500; a.child.x"), Ok(500));
}
