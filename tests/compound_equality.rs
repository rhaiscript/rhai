use rhai::{Engine, INT};

#[test]
fn test_or_equals() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 16; x |= 74; x").unwrap(), 90);
    assert!(engine.eval::<bool>("let x = true; x |= false; x").unwrap());
    assert!(engine.eval::<bool>("let x = false; x |= true; x").unwrap());
}

#[test]
fn test_and_equals() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 16; x &= 31; x").unwrap(), 16);
    assert!(!engine.eval::<bool>("let x = true; x &= false; x").unwrap());
    assert!(!engine.eval::<bool>("let x = false; x &= true; x").unwrap());
    assert!(engine.eval::<bool>("let x = true; x &= true; x").unwrap());
}

#[test]
fn test_xor_equals() {
    let engine = Engine::new();
    assert_eq!(engine.eval::<INT>("let x = 90; x ^= 12; x").unwrap(), 86);
}

#[test]
fn test_multiply_equals() {
    let engine = Engine::new();
    assert_eq!(engine.eval::<INT>("let x = 2; x *= 3; x").unwrap(), 6);
}

#[test]
fn test_divide_equals() {
    let engine = Engine::new();
    assert_eq!(engine.eval::<INT>("let x = 6; x /= 2; x").unwrap(), 3);
}

#[test]
fn test_right_shift_equals() {
    let engine = Engine::new();
    assert_eq!(engine.eval::<INT>("let x = 9; x >>=1; x").unwrap(), 4);
}

#[test]
fn test_left_shift_equals() {
    let engine = Engine::new();
    assert_eq!(engine.eval::<INT>("let x = 4; x <<= 2; x").unwrap(), 16);
}

#[test]
fn test_modulo_equals() {
    let engine = Engine::new();
    assert_eq!(engine.eval::<INT>("let x = 10; x %= 4; x").unwrap(), 2);
}
