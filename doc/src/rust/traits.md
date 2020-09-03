Traits
======

{{#include ../links.md}}

A number of traits, under the `rhai::` module namespace, provide additional functionalities.

| Trait              | Description                                                                              | Methods                                 |
| ------------------ | ---------------------------------------------------------------------------------------- | --------------------------------------- |
| `RegisterFn`       | Trait for registering functions                                                          | `register_fn`                           |
| `RegisterResultFn` | Trait for registering fallible functions returning `Result<Dynamic, Box<EvalAltResult>>` | `register_result_fn`                    |
| `Func`             | Trait for creating Rust closures from script                                             | `create_from_ast`, `create_from_script` |
| `ModuleResolver`   | Trait implemented by module resolution services                                          | `resolve`                               |
