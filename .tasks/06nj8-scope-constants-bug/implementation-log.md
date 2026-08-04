# Implementation Log

## Reproduction

Added `test_call_fn_global_constant_in_nested_function` to `tests/call_fn.rs` using the reported `compile`, `run_ast_with_scope`, direct `call_fn`, and nested helper call sequence. Before the fix, the direct call passed and the indirect call failed with `ErrorVariableNotFound("VALUE")`.

## Cause

`call_fn` evaluated top-level constants into the caller scope, but nested script calls use a fresh scope. Unqualified constant references therefore could not resolve transitively. Encapsulated environments could also replace active runtime constants with an empty map.

## Fix

Collect read-only values from the evaluated scope into the runtime constant map, seed fresh script-call scopes with active constants, and merge encapsulated constants with active constants while restoring the original map afterward.

## Verification

- `cargo test --test call_fn test_call_fn_global_constant_in_nested_function -- --exact` passed.
- `cargo test --test call_fn` passed: 8 tests.
- `cargo fmt --all` passed.
- `git diff --check` passed.

The build continues to emit the pre-existing `unpredictable_function_pointer_comparisons` warning in `src/packages/iter_basic.rs`.
