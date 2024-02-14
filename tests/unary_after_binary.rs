use rhai::{Engine, INT};

#[test]
// TODO also add test case for unary after compound
// Hah, turns out unary + has a good use after all!
fn test_unary_after_binary() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("10 % +4").unwrap(), 2);
    assert_eq!(engine.eval::<INT>("10 << +4").unwrap(), 160);
    assert_eq!(engine.eval::<INT>("10 >> +4").unwrap(), 0);
    assert_eq!(engine.eval::<INT>("10 & +4").unwrap(), 0);
    assert_eq!(engine.eval::<INT>("10 | +4").unwrap(), 14);
    assert_eq!(engine.eval::<INT>("10 ^ +4").unwrap(), 14);
}
