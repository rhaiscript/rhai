`String` Parameters in Rust Functions
====================================

{{#include ../links.md}}


Avoid `String`
--------------

As must as possible, avoid using `String` parameters in functions.

Each `String` argument is cloned during every single call to that function - and the copy
immediately thrown away right after the call.

Needless to say, it is _extremely_ inefficient to use `String` parameters.


`&str` Maps to `ImmutableString`
-------------------------------

Rust functions accepting parameters of `String` should use `&str` instead because it maps directly to
[`ImmutableString`][string] which is the type that Rhai uses to represent [strings] internally.

The parameter type `String` involves always converting an [`ImmutableString`][string] into a `String`
which mandates cloning it.

Using `ImmutableString` or `&str` is much more efficient.
A common mistake made by novice Rhai users is to register functions with `String` parameters.

```rust
fn get_len1(s: String) -> i64 { s.len() as i64 }            // <- Very inefficient!!!
fn get_len2(s: &str) -> i64 { s.len() as i64 }              // <- This is better
fn get_len3(s: ImmutableString) -> i64 { s.len() as i64 }   // <- the above is equivalent to this

engine
    .register_fn("len1", get_len1)
    .register_fn("len2", get_len2)
    .register_fn("len3", get_len3);

let len = engine.eval::<i64>("x.len1()")?;                  // 'x' is cloned, very inefficient!
let len = engine.eval::<i64>("x.len2()")?;                  // 'x' is shared
let len = engine.eval::<i64>("x.len3()")?;                  // 'x' is shared
```
