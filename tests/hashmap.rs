#![cfg(all(feature = "experimental_hashmap", not(feature = "no_index"), not(feature = "no_float")))]
use std::collections::HashMap;
use std::iter::FromIterator;

use rhai::{AnyExt, Dynamic, Engine, EvalAltResult, RegisterFn, INT, FLOAT, MAP};

#[test]
fn test_hashmaps() -> Result<(), EvalAltResult> {
    // TODO: indexing is not supported yet
    /*
    let mut engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = [a: 1, b: 2, c: 3]; x[1]")?, 2);
    assert_eq!(engine.eval::<INT>("let y = [a: 1, b: 2, c: 3]; y[1] = 5; y[1]")?, 5);
    assert_eq!(
        engine.eval::<char>(r#"let y = [d: 1, e: [ 42, 88, "93" ], 3]; y["d"]["e"][1]"#)?,
        '3'
    );
    */

    Ok(())
}

#[test]
fn test_hashmap_assign() -> Result<(), EvalAltResult> {
    let mut engine = Engine::new();

    let mut x = engine.eval::<MAP>("let x = [: a: 1, b: 2.0, c: \"3\"]; x")?;
    let box_a = x.remove("a").unwrap();
    let box_b = x.remove("b").unwrap();
    let box_c = x.remove("c").unwrap();

    assert_eq!(box_a.downcast::<INT>().unwrap(), Box::new(1));
    assert_eq!(box_b.downcast::<FLOAT>().unwrap(), Box::new(2.0));
    assert_eq!(box_c.downcast::<String>().unwrap(), Box::new("3".to_string()));
    Ok(())
}

#[test]
fn test_hashmap_return() -> Result<(), EvalAltResult> {
    let mut engine = Engine::new();

    let mut x = engine.eval::<MAP>("[: a: 1, b: 2.0, c: \"3\"]")?;
    let box_a = x.remove("a").unwrap();
    let box_b = x.remove("b").unwrap();
    let box_c = x.remove("c").unwrap();

    assert_eq!(box_a.downcast::<INT>().unwrap(), Box::new(1));
    assert_eq!(box_b.downcast::<FLOAT>().unwrap(), Box::new(2.0));
    assert_eq!(box_c.downcast::<String>().unwrap(), Box::new("3".to_string()));
    Ok(())
}


#[test]
fn test_hashmap_with_structs() -> Result<(), EvalAltResult> {
    #[derive(Clone)]
    struct TestStruct {
        x: INT,
    }

    impl TestStruct {
        fn update(&mut self) {
            self.x += 1000;
        }

        fn get_x(&mut self) -> INT {
            self.x
        }

        fn set_x(&mut self, new_x: INT) {
            self.x = new_x;
        }

        fn new() -> Self {
            TestStruct { x: 1 }
        }
    }

    let mut engine = Engine::new();

    engine.register_type::<TestStruct>();

    engine.register_get_set("x", TestStruct::get_x, TestStruct::set_x);
    engine.register_fn("update", TestStruct::update);
    engine.register_fn("new_ts", TestStruct::new);

    let mut struct_a = engine.eval::<MAP>("let a = [: item: new_ts()]; a")?;
    let box_item = struct_a.remove("item").unwrap();
    assert_eq!(box_item.downcast::<TestStruct>().unwrap().as_mut().get_x(),
               TestStruct::new().get_x());


    // TODO: indexing is not supported yet
    /*
    assert_eq!(
        engine.eval::<INT>(
            r"
                let a = [item: new_ts()];
                a[0].x = 100;
                a[0].update();
                a[0].x
            "
        )?,
        1100
    );
    */

    Ok(())
}
