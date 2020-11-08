Module Resolvers
================

{{#include ../../links.md}}

When encountering an [`import`] statement, Rhai attempts to _resolve_ the module based on the path string.

See the section on [_Importing Modules_][`import`] for more details.

_Module Resolvers_ are service types that implement the [`ModuleResolver`][traits] trait.


Built-In Module Resolvers
------------------------

There are a number of standard resolvers built into Rhai, the default being the `FileModuleResolver`
which simply loads a script file based on the path (with `.rhai` extension attached)
and execute it to form a module.

Built-in module resolvers are grouped under the `rhai::module_resolvers` module namespace.


`FileModuleResolver` (default)
-----------------------------

The _default_ module resolution service, not available for [`no_std`] or [WASM] builds.
Loads a script file (based off the current directory) with `.rhai` extension.

All functions in the _global_ namespace, plus all those defined in the same module,
are _merged_ into a _unified_ namespace.

Modules are also _cached_ so a script file is only evaluated _once_, even when repeatedly imported.

```rust
------------------
| my_module.rhai |
------------------

// This function overrides any in the main script.
private fn inner_message() { "hello! from module!" }

fn greet() {
    print(inner_message());     // call function in module script
}

fn greet_main() {
    print(main_message());      // call function not in module script
}

-------------
| main.rhai |
-------------

// This function is overridden by the module script.
fn inner_message() { "hi! from main!" }

// This function is found by the module script.
fn main_message() { "main here!" }

import "my_module" as m;

m::greet();                     // prints "hello! from module!"

m::greet_main();                // prints "main here!"
```

### Simulating virtual functions

When calling a namespace-qualified function defined within a module, other functions defined within
the same module script override any similar-named functions (with the same number of parameters)
defined in the global namespace.  This is to ensure that a module acts as a self-contained unit and
functions defined in the calling script do not override module code.

In some situations, however, it is actually beneficial to do it in reverse: have module code call functions
defined in the calling script (i.e. in the global namespace) if they exist, and only call those defined
in the module script if none are found.

One such situation is the need to provide a _default implementation_ to a simulated _virtual_ function:

```rust
------------------
| my_module.rhai |
------------------

// Do not do this (it will override the main script):
// fn message() { "hello! from module!" }

// This function acts as the default implementation.
private fn default_message() { "hello! from module!" }

// This function depends on a 'virtual' function 'message'
// which is not defined in the module script.
fn greet() {
    if is_def_fn("message", 0) {    // 'is_def_fn' detects if 'message' is defined.
        print(message());
    } else {
        print(default_message());
    }
}

-------------
| main.rhai |
-------------

// The main script defines 'message' which is needed by the module script.
fn message() { "hi! from main!" }

import "my_module" as m;

m::greet();                         // prints "hi! from main!"

--------------
| main2.rhai |
--------------

// The main script does not define 'message' which is needed by the module script.

import "my_module" as m;

m::greet();                         // prints "hello! from module!"
```

### Changing the base directory

The base directory can be changed via the `FileModuleResolver::new_with_path` constructor function.


`StaticModuleResolver`
---------------------

Loads modules that are statically added. This can be used under [`no_std`].

Functions are searched in the _global_ namespace by default.

```rust
use rhai::{Module, module_resolvers::StaticModuleResolver};

let module: Module = create_a_module();

let mut resolver = StaticModuleResolver::new();
resolver.insert("my_module", module);
```


`ModuleResolversCollection`
--------------------------

A collection of module resolvers. Modules will be resolved from each resolver in sequential order.

This is useful when multiple types of modules are needed simultaneously.


Set into `Engine`
-----------------

An [`Engine`]'s module resolver is set via a call to `Engine::set_module_resolver`:

```rust
use rhai::module_resolvers::StaticModuleResolver;

// Create a module resolver
let resolver = StaticModuleResolver::new();

// Register functions into 'resolver'...

// Use the module resolver
engine.set_module_resolver(Some(resolver));

// Effectively disable 'import' statements by setting module resolver to 'None'
engine.set_module_resolver(None);
```
