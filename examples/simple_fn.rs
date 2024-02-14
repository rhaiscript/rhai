//! An example showing how to register a simple Rust function.

use rhai::{Engine, EvalAltResult};

fn add(x: i64, y: i64) -> i64 {
    x + y
}

fn main() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();

    engine.register_fn("add", add);

    let result = engine.eval::<i64>("add(40, 2)")?;

    println!("Answer: {result}"); // prints 42

    Ok(())
}
