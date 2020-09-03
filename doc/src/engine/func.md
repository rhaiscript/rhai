Create a Rust Closure from a Rhai Function
=========================================

{{#include ../links.md}}

It is possible to further encapsulate a script in Rust such that it becomes a normal Rust function.

Such a _closure_ is very useful as call-back functions.

Creating them is accomplished via the `Func` trait which contains `create_from_script`
(as well as its companion method `create_from_ast`):

```rust
use rhai::{Engine, Func};                       // use 'Func' for 'create_from_script'

let engine = Engine::new();                     // create a new 'Engine' just for this

let script = "fn calc(x, y) { x + y.len < 42 }";

// Func takes two type parameters:
//   1) a tuple made up of the types of the script function's parameters
//   2) the return type of the script function
//
// 'func' will have type Box<dyn Fn(i64, String) -> Result<bool, Box<EvalAltResult>>> and is callable!
let func = Func::<(i64, String), bool>::create_from_script(
//                ^^^^^^^^^^^^^ function parameter types in tuple

                engine,                         // the 'Engine' is consumed into the closure
                script,                         // the script, notice number of parameters must match
                "calc"                          // the entry-point function name
)?;

func(123, "hello".to_string())? == false;       // call the closure

schedule_callback(func);                        // pass it as a callback to another function

// Although there is nothing you can't do by manually writing out the closure yourself...
let engine = Engine::new();
let ast = engine.compile(script)?;
schedule_callback(Box::new(move |x: i64, y: String| -> Result<bool, Box<EvalAltResult>> {
    engine.call_fn(&mut Scope::new(), &ast, "calc", (x, y))
}));
```
