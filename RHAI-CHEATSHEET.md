# Rhai Language Cheat Sheet

For AI agents writing or reviewing Rhai scripts. Rhai is JavaScript-like plus certain Rust-inspired syntax.
**Start from JavaScript, then apply the deltas below.**
Full reference: [_The Rhai Book_](https://rhai.rs/book/ref).

## JS → Rhai deltas (read first)

| JavaScript habit | Rhai reality |
|---|---|
| `{a: 1}` object literal | `#{a: 1}` (object map) |
| `null` / `undefined` | single value `()`; absent map key reads as `()` |
| truthiness (`if (x)`) | conditions must be `bool` |
| `===` / `==` coercion | only `==`; no coercion; `1` (`i64`) and `1.0` (`f64`) are distinct types |
| objects/arrays are references | **no references exist**: values are cloned on assign and on call; mutate via the variable path |
| `function f(){}` sees globals/outer scope | `fn` cannot see outer variables; closures `\|x\| ...` capture (shared) |
| `arr.length`, `.forEach`, `.includes` | `arr.len()`, `for` loop, `.contains()` |
| `console.log` | `print(x)` / `debug(x)` |
| `Math.floor(x)` | `x.floor()`; `PI()`; `x.sin()` etc. |
| `for (let i=0;i<n;i++)` | `for i in 0..n` (exclusive) / `0..=n` (inclusive) |
| `if (c) stmt` | `if c { ... }` braces mandatory, no parentheses |
| `return` required | last statement is always the value of the block (not just functions), with or without trailing `;` (different from Rust) |
| Blocks <> expressions | every block is an expression (the last statement is the value) |
| `throw`/`try`/`catch (e)` | same; `throw` takes any value |
| `class`, `async`, `await`, `new` | none; use maps + closures, or host-registered types |
| `import x from` | `import "name" as x; x::f()` |
| `'a'` is a string | `'a'` is a `char`; strings use `"..."` or `` `...${x}...` `` |
| `s.length`, UTF-16 indexing | `s.len()` counts chars; `s[i]` is a `char` (different from Rust where strings are UTF-8 encoded bytes) |
| `x = a ? b : c` | `x = if a { b } else { c }` |

When unsure whether a method exists, check the host's registered functions instead of guessing JS names.

## Values

```rhai
let count = 0;                  // mutable, dynamically typed
const LIMIT = 10;
let u;                          // ()
let items = [10, 20, 30,];      // trailing comma ok; items[-1] == 30
let opts = #{mode: "fast", "odd-key": true};
opts.mode = "safe";
opts["odd-key"];                // brackets for non-identifier keys
"mode" in opts;                 // key test
opts.timeout ?? 30;             // ?? defaults when left is ()
opts?.a?.b;                     // ?. short-circuits on ()
x = if a { b } else { c };      // if statement used as expression, similar to x = a ? b : c;
```

- Integer literals: decimal, `0b`, `0o`, `0x`, `_` separators. Default int is `i64`.
- Out-of-range array reads are errors. Unsupported operations are errors, not coercions.
- Blocks, `if`, `switch`, `while`, `loop`, `for` are expressions; `break value` yields from a loop.
- Mismatched non-numeric `==` is `false`; mixed numeric types may fail: convert with `to_int()`/`to_float()`.
- `&&`/`||` short-circuit; single `&`/`|` on booleans evaluate both sides.

## IMPORTANT: value semantics

```rhai
let a = [1, 2];
let b = a;                      // clone
b[0] = 99;                      // a unchanged
fn f(x) { x[0] = 99; }
f(a);                           // a unchanged

a[0] = 7;                       // edits a
let user = #{s: #{theme: "light"}};
user.s.theme = "dark";          // edits nested property in place
let s = user.s;                 // clones
```

- Rhai has no reference type: scripts cannot hold, pass, or return a reference to a value. The only
  in-place mutation is through a variable-rooted path or a method call on a receiver (`this`).
  To refer to data indirectly, pass an index, key, or ID and resolve it at the point of mutation.
- Property/index paths rooted at a variable mutate in place (built-in arrays, BLOBs, maps only).
  Custom-type getters and function results do not.
- **Prefer built-in types in scripts.** Arrays, maps and BLOBs edit in place; custom types are
  cloned and set back through getters/setters. Keep script data in arrays/maps and convert
  to/from custom types at the edge (host functions or `serde`).
- `take(x)` moves a value out (source becomes `()`); needs the standard package.
- Strings are immutable values; string operations return new strings.

## Control flow

```rhai
for item in items { if item == 20 { continue; } if item > 30 { break; } }
for (item, i) in items { }      // with index
let r = if ok { "go" } else { "wait" };   // omitted else yields ()
let n = 0; while n < 3 { n += 1; }
loop { break; }
```

`switch` exists (`switch x { 1 => ..., 2 | 3 => ..., _ => ... }`); see the reference for ranges and guards.

## Functions, closures, pointers

```rhai
fn add(a, b) { a + b }                  // global-level only, no nesting, no capture
let offset = 2;
let f = |n| n + offset;                 // closure: captures SHARED variables
f.call(3);                              // call closures/pointers with .call()
let g = add;                            // fn pointer (same as Fn("add"))
g.call(1, 2);
```

- Function pointers store only a name; they cannot name functions inside modules.
- Loop closures all see the same shared captured variable.
- Do not use a captured variable as both capture and `this` receiver (data-race error; deadlock under `sync`).

## `this` and methods

```rhai
fn increase(amount) { this += amount; }
let score = 10;
score.increase(5);                      // score == 15; plain increase(5) errors
fn int.double() { this * 2 }            // restrict receiver type (match type_of())
let c = #{v: 0, inc: || this.v += 1};   // closures in map props get `this`
c.inc();
```

## Strings

`"escaped"`, `` `interpolated ${x}, no escapes` ``, `#"raw \ string"#`, `'c'` (char).

## Modules and host dependence

`import "m" as m; m::f();` `export` marks items from a module script.

Built-ins depend on the host's registered packages and features (`no_index`, `no_object`,
`no_function`, `no_closure`, `no_module`, ...). If a built-in seems missing, check the embedding.

## Performance style

Calls and module lookups are not inlined or flattened: avoid deep module trees and trivial
wrapper functions; inline simple logic.
