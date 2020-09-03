use rhai::plugin::*;
use rhai::{Engine, EvalAltResult, INT};

#[export_fn]
pub fn add_together(x: INT, y: INT) -> INT {
    x + y
}

#[test]
fn test_exported_fn_register() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    register_exported_fn!(engine, "add_two", add_together);
    assert_eq!(engine.eval::<INT>("let a = 1; add_two(a, 41)")?, 42);

    Ok(())
}
