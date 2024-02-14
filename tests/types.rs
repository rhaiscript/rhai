use rhai::{Engine, INT};

#[test]
fn test_type_of() {
    #[allow(dead_code)]
    #[derive(Clone)]
    struct TestStruct {
        x: INT,
    }

    let mut engine = Engine::new();

    #[cfg(not(feature = "only_i32"))]
    assert_eq!(engine.eval::<String>("type_of(60 + 5)").unwrap(), "i64");

    #[cfg(feature = "only_i32")]
    assert_eq!(engine.eval::<String>("type_of(60 + 5)").unwrap(), "i32");

    #[cfg(not(feature = "no_float"))]
    #[cfg(not(feature = "f32_float"))]
    assert_eq!(engine.eval::<String>("type_of(1.0 + 2.0)").unwrap(), "f64");

    #[cfg(not(feature = "no_float"))]
    #[cfg(feature = "f32_float")]
    assert_eq!(engine.eval::<String>("type_of(1.0 + 2.0)").unwrap(), "f32");

    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<String>(r#"type_of([true, 2, "hello"])"#).unwrap(), "array");

    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<String>(r#"type_of(#{a:true, "":2, "z":"hello"})"#).unwrap(), "map");

    #[cfg(not(feature = "no_object"))]
    {
        engine.register_type_with_name::<TestStruct>("Hello").register_fn("new_ts", || TestStruct { x: 1 });
        assert_eq!(engine.eval::<String>("type_of(new_ts())").unwrap(), "Hello");
    }

    assert_eq!(engine.eval::<String>(r#"type_of("hello")"#).unwrap(), "string");

    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<String>(r#""hello".type_of()"#).unwrap(), "string");

    #[cfg(not(feature = "only_i32"))]
    assert_eq!(engine.eval::<String>("let x = 123; type_of(x)").unwrap(), "i64");

    #[cfg(feature = "only_i32")]
    assert_eq!(engine.eval::<String>("let x = 123; type_of(x)").unwrap(), "i32");
}
