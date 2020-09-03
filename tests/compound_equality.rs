use rhai::{Engine, EvalAltResult, INT};

#[test]
fn test_or_equals() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 16; x |= 74; x")?, 90);
    assert_eq!(engine.eval::<bool>("let x = true; x |= false; x")?, true);
    assert_eq!(engine.eval::<bool>("let x = false; x |= true; x")?, true);

    Ok(())
}

#[test]
fn test_and_equals() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 16; x &= 31; x")?, 16);
    assert_eq!(engine.eval::<bool>("let x = true; x &= false; x")?, false);
    assert_eq!(engine.eval::<bool>("let x = false; x &= true; x")?, false);
    assert_eq!(engine.eval::<bool>("let x = true; x &= true; x")?, true);

    Ok(())
}

#[test]
fn test_xor_equals() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();
    assert_eq!(engine.eval::<INT>("let x = 90; x ^= 12; x")?, 86);
    Ok(())
}

#[test]
fn test_multiply_equals() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();
    assert_eq!(engine.eval::<INT>("let x = 2; x *= 3; x")?, 6);
    Ok(())
}

#[test]
fn test_divide_equals() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();
    assert_eq!(engine.eval::<INT>("let x = 6; x /= 2; x")?, 3);
    Ok(())
}

#[test]
fn test_right_shift_equals() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();
    assert_eq!(engine.eval::<INT>("let x = 9; x >>=1; x")?, 4);
    Ok(())
}

#[test]
fn test_left_shift_equals() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();
    assert_eq!(engine.eval::<INT>("let x = 4; x <<= 2; x")?, 16);
    Ok(())
}

#[test]
fn test_modulo_equals() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();
    assert_eq!(engine.eval::<INT>("let x = 10; x %= 4; x")?, 2);
    Ok(())
}
