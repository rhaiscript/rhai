use rhai::{Engine, INT};

#[test]
fn test_number_literal() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("42").unwrap(), 42);

    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<String>("42.type_of()").unwrap(), if cfg!(feature = "only_i32") { "i32" } else { "i64" });
}

#[test]
fn test_hex_literal() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 0xf; x").unwrap(), 15);
    assert_eq!(engine.eval::<INT>("let x = 0Xf; x").unwrap(), 15);
    assert_eq!(engine.eval::<INT>("let x = 0xff; x").unwrap(), 255);

    #[cfg(not(feature = "only_i32"))]
    assert_eq!(engine.eval::<INT>("let x = 0xffffffffffffffff; x").unwrap(), -1);
    #[cfg(feature = "only_i32")]
    assert_eq!(engine.eval::<INT>("let x = 0xffffffff; x").unwrap(), -1);

    #[cfg(not(feature = "no_float"))]
    assert!(engine.compile("0xabcd.123").is_err());
}

#[test]
fn test_octal_literal() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 0o77; x").unwrap(), 63);
    assert_eq!(engine.eval::<INT>("let x = 0O77; x").unwrap(), 63);
    assert_eq!(engine.eval::<INT>("let x = 0o1234; x").unwrap(), 668);

    #[cfg(not(feature = "no_float"))]
    assert!(engine.compile("0o77.123").is_err());
}

#[test]
fn test_binary_literal() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 0b1111; x").unwrap(), 15);
    assert_eq!(engine.eval::<INT>("let x = 0B1111; x").unwrap(), 15);
    assert_eq!(engine.eval::<INT>("let x = 0b0011_1100_1010_0101; x").unwrap(), 15525);

    #[cfg(not(feature = "only_i32"))]
    assert_eq!(engine.eval::<INT>("let x = 0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111111; x").unwrap(), -1);
    #[cfg(feature = "only_i32")]
    assert_eq!(engine.eval::<INT>("let x = 0b11111111_11111111_11111111_11111111; x").unwrap(), -1);

    #[cfg(not(feature = "no_float"))]
    assert!(engine.compile("0b101.101").is_err());
}
