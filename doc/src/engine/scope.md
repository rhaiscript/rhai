`Scope` - Initializing and Maintaining State
===========================================

{{#include ../links.md}}

By default, Rhai treats each [`Engine`] invocation as a fresh one, persisting only the functions that have been defined
but no global state. This gives each evaluation a clean starting slate.

In order to continue using the same global state from one invocation to the next,
such a state must be manually created and passed in.

All `Scope` variables are [`Dynamic`], meaning they can store values of any type.

Under [`sync`], however, only types that are `Send + Sync` are supported, and the entire `Scope` itself
will also be `Send + Sync`. This is extremely useful in multi-threaded applications.

In this example, a global state object (a `Scope`) is created with a few initialized variables,
then the same state is threaded through multiple invocations:

```rust
use rhai::{Engine, Scope, EvalAltResult};

let engine = Engine::new();

// First create the state
let mut scope = Scope::new();

// Then push (i.e. add) some initialized variables into the state.
// Remember the system number types in Rhai are i64 (i32 if 'only_i32') ond f64.
// Better stick to them or it gets hard working with the script.
scope
    .push("y", 42_i64)
    .push("z", 999_i64)
    .set_value("s", "hello, world!".to_string());   //'set_value' adds a variable when one doesn't exist
                                                    // remember to use 'String', not '&str'

// First invocation
engine.eval_with_scope::<()>(&mut scope, r"
    let x = 4 + 5 - y + z + s.len;
    y = 1;
")?;

// Second invocation using the same state
let result = engine.eval_with_scope::<i64>(&mut scope, "x")?;

println!("result: {}", result);                     // prints 979

// Variable y is changed in the script - read it with 'get_value'
assert_eq!(scope.get_value::<i64>("y").expect("variable y should exist"), 1);

// We can modify scope variables directly with 'set_value'
scope.set_value("y", 42_i64);
assert_eq!(scope.get_value::<i64>("y").expect("variable y should exist"), 42);
```
