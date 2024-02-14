use rhai::{Engine, INT};

#[test]
fn test_unary_minus() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = -5; x").unwrap(), -5);

    #[cfg(not(feature = "no_function"))]
    assert_eq!(engine.eval::<INT>("fn neg(x) { -x } neg(5)").unwrap(), -5);

    assert_eq!(engine.eval::<INT>("5 - -+ + + - -+-5").unwrap(), 0);
}
