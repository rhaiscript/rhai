use rhai::{Engine, EvalAltResult, INT};

fn main() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    let result = engine.eval::<INT>("1e3")?;

    println!("Answer: {}", result); // prints 42

    Ok(())
}
