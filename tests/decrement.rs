use rhai::{Engine, INT};

#[test]
fn test_decrement() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 10; x -= 7; x").unwrap(), 3);
    assert_eq!(engine.eval::<String>(r#"let s = "test"; s -= 's'; s"#).unwrap(), "tet");
}
