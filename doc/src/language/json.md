Parse an Object Map from JSON
============================

{{#include ../links.md}}

The syntax for an [object map] is extremely similar to the JSON representation of a object hash,
with the exception of `null` values which can technically be mapped to [`()`].

A valid JSON string does not start with a hash character `#` while a Rhai [object map] does - that's the major difference!

Use the `Engine::parse_json` method to parse a piece of JSON into an object map.
The JSON text must represent a single object hash (i.e. must be wrapped within "`{ .. }`")
otherwise it returns a syntax error.

```rust
// JSON string - notice that JSON property names are always quoted
//               notice also that comments are acceptable within the JSON string
let json = r#"{
                "a": 1,                 // <- this is an integer number
                "b": true,
                "c": 123.0,             // <- this is a floating-point (not(feature = 'no_float')) or decimal (feature = 'decimal') number
                "$d e f!": "hello",     // <- any text can be a property name
                "^^^!!!": [1,42,"999"], // <- value can be array or another hash
                "z": null               // <- JSON 'null' value
              }
"#;

// Parse the JSON expression as an object map
// Set the second boolean parameter to true in order to map 'null' to '()'
let map = engine.parse_json(json, true)?;

map.len() == 6;       // 'map' contains all properties in the JSON string

// Put the object map into a 'Scope'
let mut scope = Scope::new();
scope.push("map", map);

let result = engine.eval_with_scope::<INT>(r#"map["^^^!!!"].len()"#)?;

result == 3;          // the object map is successfully used in the script
```

Representation of Numbers
------------------------

JSON numbers are all floating-point while Rhai supports integers (`INT`) floating-point (`FLOAT`) if
the [`no_float`] feature is not used and decimal if the [`decimal`] feature is used.  
Most common generators of JSON data distinguish between
integer and floating-point values by always serializing a floating-point number with a decimal point
(i.e. `123.0` instead of `123` which is assumed to be an integer).  This style can be used successfully
with Rhai [object maps].


Parse JSON with Sub-Objects
--------------------------

`Engine::parse_json` depends on the fact that the [object map] literal syntax in Rhai is _almost_
the same as a JSON object.  However, it is _almost_ because the syntax for a sub-object in JSON
(i.e. "`{ ... }`") is different from a Rhai [object map] literal (i.e. "`#{ ... }`").

When `Engine::parse_json` encounters JSON with sub-objects, it fails with a syntax error.

If it is certain that no text string in the JSON will ever contain the character '`{`',
then it is possible to parse it by first replacing all occupance of '`{`' with "`#{`".

A JSON object hash starting with `#{` is handled transparently by `Engine::parse_json`.

```rust
// JSON with sub-object 'b'.
let json = r#"{"a":1, "b":{"x":true, "y":false}}"#;

let new_json = json.replace("{" "#{");

// The leading '{' will also be replaced to '#{', but parse_json can handle this.
let map = engine.parse_json(&new_json, false)?;

map.len() == 2;       // 'map' contains two properties: 'a' and 'b'
```


Use `serde` to Serialize/Deserialize to/from JSON
------------------------------------------------

Remember, `Engine::parse_json` is nothing more than a _cheap_ alternative to true JSON parsing.

If correctness is needed, or for more configuration possibilities, turn on the [`serde`][features]
feature to pull in the [`serde`](https://crates.io/crates/serde) crate which enables
serialization and deserialization to/from multiple formats, including JSON.

Beware, though... the [`serde`](https://crates.io/crates/serde) crate is quite heavy.

See _[Serialization/Deserialization of `Dynamic` with `serde`][`serde`]_ for more details.
