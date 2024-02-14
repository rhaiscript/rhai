use rhai::Engine;

#[test]
fn test_bool_op1() {
    let engine = Engine::new();

    assert!(engine.eval::<bool>("true && (false || true)").unwrap());
    assert!(engine.eval::<bool>("true & (false | true)").unwrap());
}

#[test]
fn test_bool_op2() {
    let engine = Engine::new();

    assert!(!engine.eval::<bool>("false && (false || true)").unwrap());
    assert!(!engine.eval::<bool>("false & (false | true)").unwrap());
}

#[test]
fn test_bool_op3() {
    let engine = Engine::new();

    assert!(engine.eval::<bool>("true && (false || 123)").is_err());
    assert!(engine.eval::<bool>("true && (true || { throw })").unwrap());
    assert!(engine.eval::<bool>("123 && (false || true)").is_err());
    assert!(!engine.eval::<bool>("false && (true || { throw })").unwrap());
}

#[test]
fn test_bool_op_short_circuit() {
    let engine = Engine::new();

    assert!(engine
        .eval::<bool>(
            "
                let x = true;
                x || { throw; };
            "
        )
        .unwrap());

    assert!(!engine
        .eval::<bool>(
            "
                let x = false;
                x && { throw; };
            "
        )
        .unwrap());
}

#[test]
fn test_bool_op_no_short_circuit1() {
    let engine = Engine::new();

    let _ = engine
        .eval::<bool>(
            "
                let x = true;
                x | { throw; }
            ",
        )
        .unwrap_err();
}

#[test]
fn test_bool_op_no_short_circuit2() {
    let engine = Engine::new();

    assert!(engine
        .eval::<bool>(
            "
                let x = false;
                x & { throw; }
            "
        )
        .is_err());
}
