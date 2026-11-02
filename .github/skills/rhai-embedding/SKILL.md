---
name: rhai-embedding
description: Embed Rhai in a Rust project - create an Engine, register functions and custom types, pass variables via Scope, call script functions, convert Dynamic values, and sandbox untrusted scripts. Use when editing Rust code that uses the `rhai` crate.
---

# Embedding Rhai in Rust

For script syntax see the `rhai` skill / `RHAI-CHEATSHEET.md`. API docs: https://docs.rs/rhai.
Working examples: `examples/` (`hello.rs`, `simple_fn.rs`, `custom_types.rs`, `reuse_scope.rs`,
`callback.rs`, `serde.rs`, `threading.rs`) and `tests/`.

```rust
use rhai::{Dynamic, Engine, EvalAltResult, Scope};

let mut engine = Engine::new();                       // Engine::new_raw() = no standard library
let n: i64 = engine.eval("40 + 2")?;                  // eval an expression
engine.run("print(1)")?;                              // run, ignore result
```

## Register native functions

```rust
engine.register_fn("add2", |a: i64, b: i64| a + b);
engine.register_fn("fallible", |a: i64| -> Result<i64, Box<EvalAltResult>> {
    if a < 0 { Err("negative".into()) } else { Ok(a) }  // becomes a script runtime error
});
```

Use script-visible types: `i64`/`f64` (not `i32`/`f32` unless `only_i32`/`f32_float`), `bool`, `char`,
`ImmutableString` (prefer it when holding or returning strings: it is cheap to clone and avoids
conversions; a registered function's *parameter* may also be `&str`, which Rhai converts transparently), `rhai::Array`, `rhai::Map`, `Dynamic`.
Taking `&mut T` as the first argument makes the function a method on the receiver.

## Custom types

```rust
use rhai::{CustomType, TypeBuilder};

#[derive(Clone, CustomType)]
#[rhai_type(extra = Point::build_extra)]
struct Point { pub x: i64 }                 // fields get get/set properties; `#[rhai_type(skip)]` / `readonly` to adjust
impl Point {
    fn new(x: i64) -> Self { Self { x } }
    fn bump(&mut self) { self.x += 1 }
    fn build_extra(b: &mut TypeBuilder<Self>) {
        b.with_name("Point").with_fn("new_point", Self::new).with_fn("bump", Self::bump);
    }
}
engine.build_type::<Point>();
let p: Point = engine.eval("let p = new_point(1); p.bump(); p")?;
```

Custom types must be `Clone` (and `Send + Sync` under `sync`).

## Plugin modules

```rust
use rhai::plugin::*;

#[export_module]
mod my_mod { pub fn double(x: i64) -> i64 { x * 2 } }

engine.register_static_module("m", exported_module!(my_mod).into());   // script: m::double(4)
```

Prefer this over many `register_fn` calls for groups of functions.

## Scope, AST, and calling script functions

```rust
let mut scope = Scope::new();
scope.push("x", 40_i64);                     // push_constant for read-only
let r: i64 = engine.eval_with_scope(&mut scope, "x + 2")?;

let ast = engine.compile("fn add(a, b) { a + b }")?;      // compile once, reuse
let r = engine.call_fn::<i64>(&mut scope, &ast, "add", (1_i64, 2_i64))?;
// one argument needs a one-tuple: ( "abc", )   no arguments: ()
```

Compile once and reuse the `AST` for repeated calls. `call_fn` is unavailable under `no_function`.
Script functions cannot see `scope` variables; pass them as arguments.

## Dynamic conversions

```rust
let d: Dynamic = engine.eval("#{a: 1}")?;
let m = d.try_cast::<rhai::Map>().unwrap();   // Option; .cast::<T>() panics on mismatch
let i = m["a"].as_int().unwrap();             // as_int/as_float/as_bool/as_string -> Result
```

With the `serde` feature: `rhai::serde::{to_dynamic, from_dynamic}` convert Rust structs to and from `Dynamic`.

## Sandbox untrusted scripts (do this by default)

```rust
engine
    .set_max_operations(100_000)
    .set_max_call_levels(32)
    .set_max_expr_depths(64, 32)               // (global, function)
    .set_max_string_size(10_000)
    .set_max_array_size(10_000)
    .set_max_map_size(10_000);
engine.on_progress(|ops| if ops > 50_000 { Some("too long".into()) } else { None }); // Some(_) aborts
```

Also consider `set_max_variables`, `set_max_functions`, `set_max_modules`, `disable_symbol("eval")`,
and `Engine::new_raw()` plus only the packages you need. None of these apply under `unchecked`.
Do not register filesystem/network/process functions for untrusted scripts.

## API design: avoid hidden cloning

Rhai values are cloned freely, and the engine hides it. Large types (`Vec`, buffers, big structs) get
expensive when scripts chain calls.

- **Property getters clone.** `obj.items.push(1)` on a custom-type property calls the getter (which
  returns an owned clone), mutates the clone, then calls the setter to write it back
  ("auto set-back"). The script looks like an in-place edit, but each step copies the whole value.
  Built-in arrays/maps/BLOBs reached through a variable path are the exception: they are mutated in place.
- **Indexers behave the same way.** Indexer getters return owned values; indexer setters are called to write back.
- **Rhai has no references except the first `&mut` argument.** Native functions cannot return
  references or take `&T`/`&mut T` for any parameter other than the first (the receiver). Everything
  else is passed and returned by value, hence cloned.
- **By-value arguments clone.** A native function taking `Vec<T>` or a large struct by value clones the
  caller's value on every call. Take `&mut T` as the first parameter (a method) to operate in place.
- **Fluent chaining:** `fn push(&mut self, x) -> Self` forces a clone of `self` per call. Prefer
  `&mut self` methods returning `()`, and let scripts call them as separate statements. Rhai cannot
  return references (`&mut Self`), so true builder-style chaining cannot avoid a copy.
- **Pass an index/key/ID, resolve the mutation site last.** Usually the best design: scripts hold a
  cheap handle (index, key, ID) and call `engine_fn(&mut Host, id, args)` or `host.update(id, ...)`,
  which locates the data and mutates it in place at the final step. Avoid handing out intermediate
  collections or sub-objects that must be cloned and set back.
- **Share large data instead of copying it:** if handles aren't enough, wrap it in `Rc<RefCell<_>>`/`Arc<Mutex<_>>` (or
  `rhai::Shared` + `Locked`) inside a small custom type, so cloning the handle is cheap and
  mutation is visible everywhere. Or expose lookup functions (`get_item(handle, i)`) rather than the
  whole collection through a getter.
- **Avoid getters that return big collections.** Provide `len()`, `get(i)`, and iteration/`drain`-style methods instead.
- **Prefer built-in types inside scripts.** Arrays, maps and BLOBs mutate in place when reached through
  a variable path; custom types do not. Keep script data in `Array`/`Map` and convert to/from
  custom types at the edge (`serde` `to_dynamic`/`from_dynamic`, or host functions taking/returning them once).
- Script side: use `take(value)` to move rather than clone, and mutate through the variable path.

## Errors

`eval`/`run`/`call_fn` return `Result<_, Box<EvalAltResult>>`; `compile` returns `ParseError`
(convert with `?` into `Box<EvalAltResult>`). `EvalAltResult` implements `Display` with position info.

## Cargo features that matter

`sync` (Send+Sync everywhere), `serde`, `metadata` (function signatures/doc-comments),
`no_std`, `no_index`/`no_object`/`no_function`/`no_closure`/`no_module`/`no_float` (remove language
features), `only_i32`/`only_i64`/`f32_float`/`decimal` (numeric types), `internals` (volatile AST access),
`unchecked` (disables all limits).
Check feature combinations that touch your change; CI workflows list the matrix.
