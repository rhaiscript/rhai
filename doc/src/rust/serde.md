Serialization and Deserialization of `Dynamic` with `serde`
=========================================================

{{#include ../links.md}}

Rhai's [`Dynamic`] type supports serialization and deserialization by [`serde`](https://crates.io/crates/serde)
via the [`serde`][features] feature.

A [`Dynamic`] can be seamlessly converted to and from a type that implements
[`serde::Serialize`](https://docs.serde.rs/serde/trait.Serialize.html) and/or
[`serde::Deserialize`](https://docs.serde.rs/serde/trait.Deserialize.html).


Serialization
-------------

The function `rhai::ser::to_dynamic` automatically converts any Rust type that implements
[`serde::Serialize`](https://docs.serde.rs/serde/trait.Serialize.html) into a [`Dynamic`].

This is usually not necessary because using [`Dynamic::from`][`Dynamic`] is much easier and is essentially
the same thing.  The only difference is treatment for integer values.  `Dynamic::from` will keep the different
integer types intact, while `rhai::ser::to_dynamic` will convert them all into [`INT`][standard types]
(i.e. the system integer type which is `i64` or `i32` depending on the [`only_i32`] feature).

In particular, Rust `struct`'s (or any type that is marked as a `serde` map) are converted into [object maps]
while Rust `Vec`'s (or any type that is marked as a `serde` sequence) are converted into [arrays].

While it is also simple to serialize a Rust type to `JSON` via `serde`,
then use [`Engine::parse_json`]({{rootUrl}}/language/json.md) to convert it into an [object map],
`rhai::ser::to_dynamic` serializes it to [`Dynamic`] directly via `serde` without going through the `JSON` step.

```rust
use rhai::{Dynamic, Map};
use rhai::ser::to_dynamic;

#[derive(Debug, serde::Serialize)]
struct Point {
    x: f64,
    y: f64
}

#[derive(Debug, serde::Serialize)]
struct MyStruct {
    a: i64,
    b: Vec<String>,
    c: bool,
    d: Point
}

let x = MyStruct {
    a: 42,
    b: vec![ "hello".into(), "world".into() ],
    c: true,
    d: Point { x: 123.456, y: 999.0 }
};

// Convert the 'MyStruct' into a 'Dynamic'
let map: Dynamic = to_dynamic(x);

map.is::<Map>() == true;
```


Deserialization
---------------

The function `rhai::de::from_dynamic` automatically converts a [`Dynamic`] value into any Rust type
that implements [`serde::Deserialize`](https://docs.serde.rs/serde/trait.Deserialize.html).

In particular, [object maps] are converted into Rust `struct`'s (or any type that is marked as
a `serde` map) while [arrays] are converted into Rust `Vec`'s (or any type that is marked
as a `serde` sequence).

```rust
use rhai::{Engine, Dynamic};
use rhai::de::from_dynamic;

#[derive(Debug, serde::Deserialize)]
struct Point {
    x: f64,
    y: f64
}

#[derive(Debug, serde::Deserialize)]
struct MyStruct {
    a: i64,
    b: Vec<String>,
    c: bool,
    d: Point
}

let engine = Engine::new();

let result: Dynamic = engine.eval(r#"
            ##{
                a: 42,
                b: [ "hello", "world" ],
                c: true,
                d: #{ x: 123.456, y: 999.0 }
            }
        "#)?;

// Convert the 'Dynamic' object map into 'MyStruct'
let x: MyStruct = from_dynamic(&result)?;
```


Lighter Alternative
-------------------

The [`serde`](https://crates.io/crates/serde) crate is quite heavy.

If only _simple_ JSON parsing (i.e. only deserialization) of a hash object into a Rhai [object map] is required,
the [`Engine::parse_json`]({{rootUrl}}/language/json.md}}) method is available as a _cheap_ alternative,
but it does not provide the same level of correctness, nor are there any configurable options.
