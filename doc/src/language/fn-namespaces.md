Function Namespaces
==================

{{#include ../links.md}}

Each Function is a Separate Compilation Unit
-------------------------------------------

[Functions] in Rhai are _pure_ and they form individual _compilation units_.
This means that individual functions can be separated, exported, re-grouped, imported,
and generally mix-'n-match-ed with other completely unrelated scripts.

For example, the `AST::merge` and `AST::combine` methods (or the equivalent `+` and `+=` operators)
allow combining all functions in one [`AST`] into another, forming a new, unified, group of functions.

In general, there are two types of _namespaces_ where functions are looked up:

| Namespace | Source                                                                                | Lookup method                  | Sub-modules? | Variables? |
| --------- | ------------------------------------------------------------------------------------- | ------------------------------ | :----------: | :--------: |
| Global    | 1) `Engine::register_XXX` API<br/>2) [`AST`] being evaluated<br/>3) [packages] loaded | simple function name           |   ignored    |  ignored   |
| Module    | [`Module`]                                                                            | module-qualified function name |     yes      |    yes     |


Global Namespace
----------------

There is one _global_ namespace for every [`Engine`], which includes:

* All the native Rust functions registered via the `Engine::register_XXX` API.

* All the Rust functions defined in [packages] that are loaded into the [`Engine`].

* All the [modules] imported via [`import`] statements.

In addition, during evaluation of an [`AST`], all script-defined functions bundled together within
the [`AST`] are added to the global namespace and override any existing registered functions of
the same names and number of parameters.

Anywhere in a Rhai script, when a function call is made, it is searched within the global namespace.
Therefore, function calls in Rhai are _late_ bound - meaning that the function called cannot be
determined or guaranteed and there is no way to _lock down_ the function being called.
This aspect is very similar to JavaScript before ES6 modules.

```rust
// Compile a script into AST
let ast1 = engine.compile(
    r#"
        fn get_message() {
            "Hello!"                // greeting message
        }

        fn say_hello() {
            print(get_message());   // prints message
        }

        say_hello();
    "#
)?;

// Compile another script with an overriding function
let ast2 = engine.compile(r#"fn get_message() { "Boo!" }"#)?;

// Combine the two AST's
ast1 += ast2;                       // 'message' will be overwritten

engine.consume_ast(&ast1)?;         // prints 'Boo!'
```

Therefore, care must be taken when _cross-calling_ functions to make sure that the correct
functions are called.

The only practical way to ensure that a function is a correct one is to use [modules] -
i.e. define the function in a separate module and then [`import`] it:

```rust
----------------
| message.rhai |
----------------

fn get_message() { "Hello!" }


---------------
| script.rhai |
---------------

import "message" as msg;

fn say_hello() {
    print(msg::get_message());
}
say_hello();
```
