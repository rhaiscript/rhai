//! An example that evaluates two pieces of code in separate runs, but using a common `Scope`.

use rhai::{Engine, EvalAltResult, Scope};

fn main() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();
    let mut scope = Scope::new();

    engine.run_with_scope(&mut scope, "let x = 4 + 5")?;

    println!("x = {}", scope.get_value::<i64>("x").unwrap());

    for _ in 0..10 {
        let result = engine.eval_with_scope::<i64>(&mut scope, "x += 1; x")?;

        println!("result: {result}");
    }

    println!("x = {}", scope.get_value::<i64>("x").unwrap());

    Ok(())
}
