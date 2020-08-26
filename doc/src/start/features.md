Optional Features
================

{{#include ../links.md}}

By default, Rhai includes all the standard functionalities in a small, tight package.

Most features are here to opt-**out** of certain functionalities that are not needed.
Notice that this deviates from Rust norm where features are _additive_.

Excluding unneeded functionalities can result in smaller, faster builds as well as
more control over what a script can (or cannot) do.

| Feature             | Description                                                                                                                                                                                                |
| ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `unchecked`         | Disable arithmetic checking (such as over-flows and division by zero), call stack depth limit, operations count limit and modules loading limit.<br/>Beware that a bad script may panic the entire system! |
| `sync`              | Restrict all values types to those that are `Send + Sync`. Under this feature, all Rhai types, including [`Engine`], [`Scope`] and [`AST`], are all `Send + Sync`.                                         |
| `no_optimize`       | Disable [script optimization].                                                                                                                                                                             |
| `no_float`          | Disable floating-point numbers and math.                                                                                                                                                                   |
| `decimal`           | Enable decimal number support (through the 'rust_decimal' crate). This will disable floating point support.
| `only_i32`          | Set the system integer type to `i32` and disable all other integer types. `INT` is set to `i32`.                                                                                                           |
| `only_i64`          | Set the system integer type to `i64` and disable all other integer types. `INT` is set to `i64`.                                                                                                           |
| `no_index`          | Disable [arrays] and indexing features.                                                                                                                                                                    |
| `no_object`         | Disable support for [custom types] and [object maps].                                                                                                                                                      |
| `no_function`       | Disable script-defined [functions].                                                                                                                                                                        |
| `no_module`         | Disable loading external [modules].                                                                                                                                                                        |
| `no_closure`        | Disable [capturing][automatic currying] external variables in [anonymous functions] to simulate _closures_, or [capturing the calling scope]({{rootUrl}}/language/fn-capture.md) in function calls.        |
| `no_std`            | Build for `no-std` (implies `no_closure`). Notice that additional dependencies will be pulled in to replace `std` features.                                                                                |
| `serde`             | Enable serialization/deserialization via `serde`. Notice that the [`serde`](https://crates.io/crates/serde) crate will be pulled in together with its dependencies.                                        |
| `internals`         | Expose internal data structures (e.g. [`AST`] nodes). Beware that Rhai internals are volatile and may change from version to version.                                                                      |
| `unicode-xid-ident` | Allow [Unicode Standard Annex #31](http://www.unicode.org/reports/tr31/) as identifiers.                                                                                                                   |


Example
-------

The `Cargo.toml` configuration below turns on these six features:

* `sync` (everything `Send + Sync`)
* `unchecked` (disable all checking - should not be used with untrusted user scripts)
* `only_i32` (only 32-bit signed integers)
* `no_float` (no floating point numbers)
* `no_module` (no loading external [modules])
* `no_function` (no defining [functions])

```toml
[dependencies]
rhai = { version = "{{version}}", features = [ "sync", "unchecked", "only_i32", "no_float", "no_module", "no_function" ] }
```

The resulting scripting engine supports only the `i32` integer numeral type (and no others like `u32`, `i16` or `i64`),
no floating-point, is `Send + Sync` (so it can be safely used across threads), and does not support defining [functions]
nor loading external [modules].

This configuration is perfect for an expression parser in a 32-bit embedded system without floating-point hardware.
