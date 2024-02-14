//! This example stores a Rhai closure for later use as a callback.

use rhai::{Engine, EvalAltResult, FnPtr};

// To call a Rhai closure at a later time, you'd need three things:
// 1) an `Engine` (with all needed functions registered),
// 2) a compiled `AST`,
// 3) the closure (of type `FnPtr`).
fn main() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // This script creates a closure which captures a variable and returns it.
    let ast = engine.compile(
        "
            let x = 18;

            // The following closure captures 'x'
            return |a, b| {
                x += 1;         // x is incremented each time
                (x + a) * b
            };
        ",
    )?;

    let closure = engine.eval_ast::<FnPtr>(&ast)?;

    // Create a closure by encapsulating the `Engine`, `AST` and `FnPtr`.
    // In a real application, you'd be handling errors.
    let func = move |x: i64, y: i64| -> i64 { closure.call(&engine, &ast, (x, y)).unwrap() };

    // Now we can call `func` anywhere just like a normal function!
    let r1 = func(1, 2);

    // Notice that each call to `func` returns a different value
    // because the captured `x` is always changing!
    let r2 = func(1, 2);
    let r3 = func(1, 2);

    println!("The Answers: {r1}, {r2}, {r3}"); // prints 40, 42, 44

    Ok(())
}
