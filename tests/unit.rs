use rhai::Engine;

#[test]
fn test_unit() {
    let engine = Engine::new();
    engine.run("let x = (); x").unwrap();
}

#[test]
fn test_unit_eq() {
    let engine = Engine::new();
    assert!(engine.eval::<bool>("let x = (); let y = (); x == y").unwrap());
}

#[test]
fn test_unit_with_spaces() {
    let engine = Engine::new();
    let _ = engine.run("let x = ( ); x").unwrap_err();
}
