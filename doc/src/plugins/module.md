Export a Rust Module to Rhai
============================

{{#include ../links.md}}


Prelude
-------

When using the plugins system, the entire `rhai::plugin` module must be imported as a prelude
because code generated will need these imports.

```rust
use rhai::plugin::*;
```


`#[export_module]`
------------------

When applied to a Rust module, the `#[export_module]` attribute generates the necessary
code and metadata to allow Rhai access to its public (i.e. marked `pub`) functions, constants
and sub-modules.

This code is exactly what would need to be written by hand to achieve the same goal,
and is custom fit to each exported item.

All `pub` functions become registered functions, all `pub` constants become [module] constant variables,
and all sub-modules become Rhai sub-modules.

This Rust module can then either be loaded into an [`Engine`] as a normal [module] or
registered as a [package]. This is done by using the `exported_module!` macro.

The macro `combine_with_exported_module!` can also be used to _combine_ all the functions
and variables into an existing module, _flattening_ the namespace - i.e. all sub-modules
are eliminated and their contents promoted to the top level.  This is typical for
developing [custom packages].

```rust
use rhai::plugin::*;        // a "prelude" import for macros

#[export_module]
mod my_module {
    // This constant will be registered as the constant variable 'MY_NUMBER'.
    // Ignored when loaded as a package.
    pub const MY_NUMBER: i64 = 42;

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
    // This function is not 'pub', so NOT registered.
    fn mystic_number() -> i64 {
        42
    }

    // Sub-modules are ignored when the Module is loaded as a package.
    pub mod my_sub_module {
        // This function is ignored when loaded as a package.
        // Otherwise it is a valid registered function under a sub-module.
        pub fn get_info() -> String {
            "hello".to_string()
        }
    }

    // Sub-modules are commonly used to put feature gates on a group of
    // functions because feature gates cannot be put on function definitions.
    // This is currently a limitation of the plugin procedural macros.
    #[cfg(feature = "advanced_functions")]
    pub mod advanced {
        // This function is ignored when loaded as a package.
        // Otherwise it is a valid registered function under a sub-module
        // which only exists when the 'advanced_functions' feature is used.
        pub fn advanced_calc(input: i64) -> i64 {
            input * 2
        }
    }
}
```

### Use `Engine::load_package`

The simplest way to load this into an [`Engine`] is to first use the `exported_module!` macro
to turn it into a normal Rhai [module], then use the `Engine::load_package` method on it:

```rust
fn main() {
    let mut engine = Engine::new();

    // The macro call creates a Rhai module from the plugin module.
    let module = exported_module!(my_module);

    // A module can simply be loaded, registering all public functions.
    engine.load_package(module);
}
```

The functions contained within the module definition (i.e. `greet`, `get_num` and `increment`)
are automatically registered into the [`Engine`] when `Engine::load_package` is called.

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

Notice that, when using a [module] as a [package], only functions registered at the _top level_
can be accessed.  Variables as well as sub-modules are ignored.

### Use as loadable `Module`

Using this directly as a dynamically-loadable Rhai [module] is almost the same, except that a
[module resolver] must be used to serve the module, and the module is loaded via `import` statements.

See the [module] section for more information.

### Use as custom package

Finally the plugin module can also be used to develop a [custom package],
using `combine_with_exported_module!`:

```rust
def_package!(rhai:MyPackage:"My own personal super package", module, {
    combine_with_exported_module!(module, "my_module_ID", my_module));
});
```

`combine_with_exported_module!` automatically _flattens_ the module namespace so that all
functions in sub-modules are promoted to the top level.  This is convenient for [custom packages].


Sub-Modules and Feature Gates
----------------------------

Sub-modules in a plugin module definition are turned into valid sub-modules in the resultant
Rhai `Module`.

They are also commonly used to put _feature gates_ or _compile-time gates_ on a group of functions,
because currently attributes do not work on individual function definitions due to a limitation of
the procedural macros system.

This is especially convenient when using the `combine_with_exported_module!` macro to develop
[custom packages] because selected groups of functions can easily be included or excluded based on
different combinations of feature flags instead of having to manually include/exclude every
single function.

```rust
#[export_module]
mod my_module {
    // Always available
    pub fn func0() {}

    // The following sub-module is only available under 'feature1'
    #[cfg(feature = "feature1")]
    pub mod feature1 {
        fn func1() {}
        fn func2() {}
        fn func3() {}
    }

    // The following sub-module is only available under 'feature2'
    #[cfg(feature = "feature2")]
    pub mod feature2 {
        fn func4() {}
        fn func5() {}
        fn func6() {}
    }
}

// Registered functions:
//   func0 - always available
//   func1 - available under 'feature1'
//   func2 - available under 'feature1'
//   func3 - available under 'feature1'
//   func4 - available under 'feature2'
//   func5 - available under 'feature2'
//   func6 - available under 'feature2'
combine_with_exported_module!(module, "my_module_ID", my_module);
```


Function Overloading and Operators
---------------------------------

Operators and overloaded functions can be specified via applying the `#[rhai_fn(name = "...")]`
attribute to individual functions.

The text string given as the `name` parameter to `#[rhai_fn]` is used to register the function with
the [`Engine`], disregarding the actual name of the function.

With `#[rhai_fn(name = "...")]`, multiple functions may be registered under the same name in Rhai,
so long as they have different parameters.

Operators (which require function names that are not valid for Rust) can also be registered this way.

Registering the same function name with the same parameter types will cause a parsing error.

```rust
use rhai::plugin::*;        // a "prelude" import for macros

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


Getters, Setters and Indexers
-----------------------------

Functions can be marked as [getters/setters] and [indexers] for [custom types] via the `#[rhai_fn]`
attribute, which is applied on a function level.

```rust
use rhai::plugin::*;        // a "prelude" import for macros

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


Multiple Registrations
----------------------

Parameters to the `#[rhai_fn(...)]` attribute can be applied multiple times.

This is especially useful for the `name = "..."`, `get = "..."` and `set = "..."` parameters
to give multiple alternative names to the same function.

```rust
use rhai::plugin::*;        // a "prelude" import for macros

#[export_module]
mod my_module {
    // This function can be called in five ways
    #[rhai_fn(name = "get_prop_value", name = "prop", name = "+", set = "prop", index_get)]
    pub fn prop_function(obj: &mut MyType, index: i64) -> i64 {
        obj.prop[index]
    }
}
```

The above function can be called in five ways:

| Parameter for `#[rhai_fn(...)]` |      Type       | Call style                                    |
| ------------------------------- | :-------------: | --------------------------------------------- |
| `name = "get_prop_value"`       | method function | `get_prop_value(x, 0)`, `x.get_prop_value(0)` |
| `name = "prop"`                 | method function | `prop(x, 0)`, `x.prop(0)`                     |
| `name = "+"`                    |    operator     | `x + 42`                                      |
| `set = "prop"`                  |     setter      | `x.prop = 42`                                 |
| `index_get`                     |  index getter   | `x[0]`                                        |


Fallible Functions
------------------

To register [fallible functions] (i.e. functions that may return errors), apply the
`#[rhai_fn(return_raw)]` attribute on functions that return `Result<Dynamic, Box<EvalAltResult>>`.

A syntax error is generated if the function with `#[rhai_fn(return_raw)]` does not
have the appropriate return type.

```rust
use rhai::plugin::*;        // a "prelude" import for macros

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


`NativeCallContext` Parameter
----------------------------

If the _first_ parameter of a function is of type `rhai::NativeCallContext`, then it is treated
specially by the plugins system.

`NativeCallContext` is a type that encapsulates the current _native call context_ and exposes the following:

| Field               |              Type               | Description                                                                                                                                                                                                                                |
| ------------------- | :-----------------------------: | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `engine()`          |            `&Engine`            | the current [`Engine`], with all configurations and settings.<br/>This is sometimes useful for calling a script-defined function within the same evaluation context using [`Engine::call_fn`][`call_fn`], or calling a [function pointer]. |
| `imports()`         |       `Option<&Imports>`        | reference to the current stack of [modules] imported via `import` statements (if any)                                                                                                                                                      |
| `iter_namespaces()` | `impl Iterator<Item = &Module>` | iterator of the namespaces (as [modules]) containing all script-defined functions                                                                                                                                                          |

This first parameter, if exists, will be stripped before all other processing.  It is _virtual_.
Most importantly, it does _not_ count as a parameter to the function and there is no need to provide
this argument when calling the function in Rhai.

The native call context can be used to call a [function pointer] or [closure] that has been passed
as a parameter to the function, thereby implementing a _callback_:

```rust
use rhai::{Dynamic, FnPtr, NativeCallContext, EvalAltResult};
use rhai::plugin::*;        // a "prelude" import for macros

#[export_module]
mod my_module {
    #[rhai_fn(return_raw)]
    pub fn greet(context: NativeCallContext, callback: FnPtr)
                                -> Result<Dynamic, Box<EvalAltResult>>
    {
        // Call the callback closure with the current context
        // to obtain the name to greet!
        let name = callback.call_dynamic(context, None, [])?;
        Ok(format!("hello, {}!", name).into())
    }
}
```

The native call context is also useful in another scenario: protecting a function from malicious scripts.

```rust
use rhai::{Dynamic, Array, NativeCallContext, EvalAltResult, NO_POS};
use rhai::plugin::*;        // a "prelude" import for macros

#[export_module]
mod my_module {
    // This function builds an array of arbitrary size, but is protected
    // against attacks by first checking with the allowed limit set
    // into the 'Engine'.
    #[rhai_fn(return_raw)]
    pub fn grow(context: NativeCallContext, size: i64)
                                -> Result<Dynamic, Box<EvalAltResult>>
    {
        // Make sure the function does not generate a
        // data structure larger than the allowed limit
        // for the Engine!
        if size as usize > context.engine().max_array_size()
        {
            return EvalAltResult::ErrorDataTooLarge(
                "Size to grow".to_string(),
                context.engine().max_array_size(),
                size as usize,
                NO_POS,
            ).into();
        }

        let array = Array::new();

        for x in 0..size {
            array.push(x.into());
        }

        OK(array.into())
    }
}
```


`#[export_module]` Parameters
----------------------------

Parameters can be applied to the `#[export_module]` attribute to override its default behavior.

| Parameter               | Description                                                                                                                    |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| _none_                  | exports only public (i.e. `pub`) functions                                                                                     |
| `export_all`            | exports all functions (including private, non-`pub` functions); use `#[rhai_fn(skip)]` on individual functions to avoid export |
| `export_prefix = "..."` | exports functions (including private, non-`pub` functions) with names starting with a specific prefix                          |


Inner Attributes
----------------

Inner attributes can be applied to the inner items of a module to tweak the export process.

`#[rhai_fn]` is applied to functions, while `#[rhai_mod]` is applied to sub-modules.

Parameters should be set on inner attributes to specify the desired behavior.

| Attribute Parameter | Use with                    | Apply to                                              | Description                                            |
| ------------------- | --------------------------- | ----------------------------------------------------- | ------------------------------------------------------ |
| `skip`              | `#[rhai_fn]`, `#[rhai_mod]` | function or sub-module                                | do not export this function/sub-module                 |
| `name = "..."`      | `#[rhai_fn]`, `#[rhai_mod]` | function or sub-module                                | registers function/sub-module under the specified name |
| `get = "..."`       | `#[rhai_fn]`                | `pub fn (&mut Type) -> Value`                         | registers a getter for the named property              |
| `set = "..."`       | `#[rhai_fn]`                | `pub fn (&mut Type, Value)`                           | registers a setter for the named property              |
| `index_get`         | `#[rhai_fn]`                | `pub fn (&mut Type, INT) -> Value`                    | registers an index getter                              |
| `index_set`         | `#[rhai_fn]`                | `pub fn (&mut Type, INT, Value)`                      | registers an index setter                              |
| `return_raw`        | `#[rhai_fn]`                | `pub fn (...) -> Result<Dynamic, Box<EvalAltResult>>` | marks this as a [fallible function]                    |
