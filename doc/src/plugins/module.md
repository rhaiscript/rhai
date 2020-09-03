Create a Plugin Module
======================

{{#include ../links.md}}


The core of creating a plugin [module] is the `#[export_module]` attribute.

When applied on a module definition, `#[export_module]` automatically generates Rhai-acceptable
functions from all `pub` functions defined within.

The resulting module can then be loaded into an [`Engine`] as a normal [module],
or as a [custom package].


Macros
------

| Macro                       | Apply to                                                                      | Behavior                                        |
| --------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------- |
| `#[export_module]`          | Rust module                                                                   | Export all `pub` functions                      |
| `#[rhai_fn(skip)]`          | Function in Rust module                                                       | Do not export this function                     |
| `#[rhai_fn(return_raw)]`    | `pub` function in Rust module returning `Result<Dynamic, Box<EvalAltResult>>` | Specify that this is a [fallible function]      |
| `#[rhai_fn(name = "...")]`  | `pub` function in Rust module                                                 | Register function under specific name           |
| `#[rhai_fn(get = "...")]`   | `pub` function in Rust module (first parameter must be `&mut`)                | Register a property getter under specific name  |
| `#[rhai_fn(set = "...")]`   | `pub` function in Rust module (first parameter must be `&mut`)                | Register a property setter under specific name  |
| `#[rhai_fn(index_get]`      | `pub` function in Rust module (first parameter must be `&mut`)                | Register an index getter                        |
| `#[rhai_fn(index_set)]`     | `pub` function in Rust module (first parameter must be `&mut`)                | Register an index setter                        |
| `#[rhai_mod(name = "...")]` | `pub` sub-module in Rust module                                               | Export the sub-module under specific name       |
| `exported_module!`          | Rust module name                                                              | Create a [module] containing exported functions |


`#[export_module]` and `exported_module!`
----------------------------------------

Apply `#[export_module]` onto a standard module to convert all `pub` functions
into Rhai plugin functions.

```rust
use rhai::plugins::*;       // import macros

#[export_module]
mod my_module {
    // This function will be registered as 'greet'.
    pub fn greet(name: &str) -> String {
        format!("hello, {}!", name)
    }
    // This function will be registered as 'get_num'.
    pub fn get_num() -> i64 {
        mystic_number()
    }
    // This function will be registered as 'increment'.
    pub fn increment(num: &mut i64) {
        *num += 1;
    }
    // This function is NOT registered.
    fn mystic_number() -> i64 {
        42
    }
}

fn main() {
    let mut engine = Engine::new();

    // 'exported_module!' creates the plugin module.
    let module = exported_module!(my_module);

    // A module can simply be loaded as a custom package.
    engine.load_package(module);
}
```

The above automatically defines a plugin module named `my_module` which can be converted into
a Rhai [module] via `exported_module!`.  The functions contained within the module definition
(i.e. `greet`, `get_num` and `increment`) are automatically registered into the [`Engine`] when
`Engine::load_package` is called.

```rust
let x = greet("world");
x == "hello, world!";

let x = greet(get_num().to_string());
x == "hello, 42!";

let x = get_num();
x == 42;

increment(x);
x == 43;
```


Getters, Setters and Indexers
-----------------------------

Functions can be marked as [getters/setters] and [indexers] for [custom types] via the `#[rhai_fn]`
attribute, which is applied on a function level.

```rust
use rhai::plugins::*;       // import macros

#[export_module]
mod my_module {
    // This is a normal function 'greet'.
    pub fn greet(name: &str) -> String {
        format!("hello, {}!", name)
    }
    // This is a getter for 'MyType::prop'.
    #[rhai_fn(get = "prop")]
    pub fn get_prop(obj: &mut MyType) -> i64 {
        obj.prop
    }
    // This is a setter for 'MyType::prop'.
    #[rhai_fn(set = "prop")]
    pub fn set_prop(obj: &mut MyType, value: i64) {
        obj.prop = value;
    }
    // This is an index getter for 'MyType'.
    #[rhai_fn(index_get)]
    pub fn get_index(obj: &mut MyType, index: i64) -> bool {
        obj.list[index]
    }
    // This is an index setter for 'MyType'.
    #[rhai_fn(index_set)]
    pub fn get_index(obj: &mut MyType, index: i64, state: bool) {
        obj.list[index] = state;
    }
}
```


Function Overloading and Operators
---------------------------------

Operators and overloaded functions can be specified via `#[rhai_fn(name = "...")]` applied upon
individual functions.

The text string given as the `name` parameter to `#[rhai_fn]` is used to register the function with
the [`Engine`], disregarding the actual name of the function.

With `#[rhai_fn(name = "...")]`, multiple functions may be registered under the same name in Rhai.

Operators (which require function names that are not valid for Rust) can also be registered this way.

```rust
use rhai::plugins::*;       // import macros

#[export_module]
mod my_module {
    // This is the '+' operator for 'MyType'.
    #[rhai_fn(name = "+")]
    pub fn add(obj: &mut MyType, value: i64) {
        obj.prop += value;
    }
    // This function is 'calc (i64)'.
    #[rhai_fn(name = "calc")]
    pub fn calc_with_default(num: i64) -> i64 {
        ...
    }
    // This function is 'calc (i64, bool)'.
    #[rhai_fn(name = "calc")]
    pub fn calc_with_option(num: i64, option: bool) -> i64 {
        ...
    }
}
```


Fallible Functions
------------------

To register [fallible functions] (i.e. functions that may return errors), apply the
`#[rhai_fn(return_raw)]` attribute on functions that return `Result<Dynamic, Box<EvalAltResult>>`.

A syntax error is generated if the function with `#[rhai_fn(return_raw)]` does not
have the appropriate return type.

```rust
use rhai::plugins::*;       // import macros

#[export_module]
mod my_module {
    // This overloads the '/' operator for i64.
    #[rhai_fn(name = "/", return_raw)]
    pub fn double_and_divide(x: i64, y: i64) -> Result<Dynamic, Box<EvalAltResult>> {
        if y == 0 {
            Err("Division by zero!".into())
        } else {
            let result = (x * 2) / y;
            Ok(result.into())
        }
    }
}
```
