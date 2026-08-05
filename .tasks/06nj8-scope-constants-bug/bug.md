Title: Global constants are unavailable from helper functions called through `Engine::call_fn`

## Summary

A script can define a top-level `const`, and the host can evaluate the script with `run_ast_with_scope`. The constant is available to a host-invoked callback in some cases, but the same constant is reported as missing when referenced by a helper function called from that callback.

This is unexpected because the helper is part of the same AST and the top-level constant has already been evaluated into the same scope.

## Reproduction

Using Rhai 1.25.1:

```rust
use rhai::{Engine, Scope};

let engine = Engine::new();
let ast = engine.compile(
    r#"
        const VALUE = 42;

        fn direct() {
            VALUE
        }

        fn helper() {
            VALUE
        }

        fn indirect() {
            helper()
        }
    "#,
)?;

let mut scope = Scope::new();
engine.run_ast_with_scope(&mut scope, &ast)?;

assert_eq!(engine.call_fn::<i64>(&mut scope, &ast, "direct", ())?, 42);
// Unexpected: this reports Variable not found: VALUE.
assert_eq!(engine.call_fn::<i64>(&mut scope, &ast, "indirect", ())?, 42);
```

The production case that exposed this uses a callback invoked with `call_fn`; the callback calls a script helper, and that helper references a top-level constant. The helper fails with:

```text
Variable not found: VALUE
```

## Expected behavior

Top-level constants should be resolvable from all script functions in the AST, including helper functions called transitively from a function invoked through `Engine::call_fn`.

## Actual behavior

The constant may resolve in the directly invoked function but fails in the nested helper function at runtime.

## Relevant host pattern

The issue occurs with this sequence:

1. `Engine::compile(source)`
2. `Engine::run_ast_with_scope(&mut scope, &ast)`
3. `Engine::call_fn(&mut scope, &ast, callback, args)`

Compiling with a populated scope, or passing the constant as a function argument, may avoid the failure, but passing arguments is only a workaround and changes normal global-constant semantics.

## Environment

- Rhai: 1.25.1
- Rust host uses `Engine::compile`, `run_ast_with_scope`, and `call_fn`
