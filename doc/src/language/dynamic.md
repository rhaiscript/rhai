Dynamic Values
==============

{{#include ../links.md}}

A `Dynamic` value can be _any_ type. However, under [`sync`], all types must be `Send + Sync`.


Use [`type_of()`] to Get Value Type
----------------------------------

Because [`type_of()`] a `Dynamic` value returns the type of the actual value,
it is usually used to perform type-specific actions based on the actual value's type.

```rust
let mystery = get_some_dynamic_value();

if type_of(mystery) == "i64" {
    print("Hey, I got an integer here!");
} else if type_of(mystery) == "f64" {
    print("Hey, I got a float here!");
} else if type_of(mystery) == "string" {
    print("Hey, I got a string here!");
} else if type_of(mystery) == "bool" {
    print("Hey, I got a boolean here!");
} else if type_of(mystery) == "array" {
    print("Hey, I got an array here!");
} else if type_of(mystery) == "map" {
    print("Hey, I got an object map here!");
} else if type_of(mystery) == "Fn" {
    print("Hey, I got a function pointer here!");
} else if type_of(mystery) == "TestStruct" {
    print("Hey, I got the TestStruct custom type here!");
} else {
    print("I don't know what this is: " + type_of(mystery));
}
```


Functions Returning `Dynamic`
----------------------------

In Rust, sometimes a `Dynamic` forms part of a returned value - a good example is an [array]
which contains `Dynamic` elements, or an [object map] which contains `Dynamic` property values.

To get the _real_ values, the actual value types _must_ be known in advance.
There is no easy way for Rust to decide, at run-time, what type the `Dynamic` value is
(short of using the `type_name` function and match against the name).


Type Checking and Casting
------------------------

A `Dynamic` value's actual type can be checked via the `is` method.

The `cast` method then converts the value into a specific, known type.

Alternatively, use the `try_cast` method which does not panic but returns `None` when the cast fails.

```rust
let list: Array = engine.eval("...")?;          // return type is 'Array'
let item = list[0];                             // an element in an 'Array' is 'Dynamic'

item.is::<i64>() == true;                       // 'is' returns whether a 'Dynamic' value is of a particular type

let value = item.cast::<i64>();                 // if the element is 'i64', this succeeds; otherwise it panics
let value: i64 = item.cast();                   // type can also be inferred

let value = item.try_cast::<i64>()?;            // 'try_cast' does not panic when the cast fails, but returns 'None'
```

Type Name
---------

The `type_name` method gets the name of the actual type as a static string slice,
which can be `match`-ed against.

```rust
let list: Array = engine.eval("...")?;          // return type is 'Array'
let item = list[0];                             // an element in an 'Array' is 'Dynamic'

match item.type_name() {                        // 'type_name' returns the name of the actual Rust type
    "i64" => ...
    "alloc::string::String" => ...
    "bool" => ...
    "path::to::module::TestStruct" => ...
}
```


Conversion Traits
----------------

The following conversion traits are implemented for `Dynamic`:

* `From<i64>` (`i32` if [`only_i32`])
* `From<f64>` (if not [`no_float`])
* `From<Decimal>` (if [`decimal`])
* `From<bool>`
* `From<rhai::ImmutableString>`
* `From<String>`
* `From<char>`
* `From<Vec<T>>` (into an [array])
* `From<HashMap<String, T>>` (into an [object map]).
