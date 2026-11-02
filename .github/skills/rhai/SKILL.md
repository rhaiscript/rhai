---
name: rhai
description: Write, review, or debug Rhai scripts (.rhai files, embedded Rhai script strings, Engine::eval/run usage). Use when the task involves Rhai language syntax or semantics.
---

# Rhai scripting

Rhai is JavaScript-like with Rust-like semantics. Write JavaScript-style code, then apply the
deltas in [`RHAI-CHEATSHEET.md`](../../../RHAI-CHEATSHEET.md) (repository root). Read it before writing
or reviewing any script.

Top pitfalls:

- Object maps are `#{a: 1}`; there is no `null`/`undefined`, only `()`.
- Values are cloned on assignment and function calls; mutate through the variable path.
- Conditions must be `bool`; `1` and `1.0` are different types.
- `fn` cannot see outer variables; closures (`|x| ...`) capture shared variables.
- Use `.len()`, `.contains()`, `print()`; do not guess JavaScript names.

Full reference: https://rhai.rs/book/ref
