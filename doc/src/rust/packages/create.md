Manually Create a Custom Package
===============================

{{#include ../../links.md}}

Sometimes specific functionalities are needed, so custom packages can be created.

A custom package is a convenient means to gather up a number of functions for later use.
An [`Engine`] only needs to `Engine::load_package` the custom package once to gain access
to the entire set of functions within.

Loading a package into an [`Engine`] is functionally equivalent to calling `Engine::register_fn` etc.
on _each_ of the functions inside the package.  But because packages are _shared_, loading an existing
package is _much_ cheaper than registering all the functions one by one.

The macro `rhai::def_package!` can be used to create a new custom package.


Macro Parameters
---------------

`def_package!(root:package_name:description, variable, block)`

* `root` - root namespace, usually `"rhai"`.

* `package_name` - name of the package, usually ending in `Package`.

* `description` - doc comment for the package.

* `variable` - a variable name holding a reference to the [module] that is to form the package.

* `block` - a code block that initializes the package.

```rust
// Import necessary types and traits.
use rhai::{
    def_package,
    packages::Package,
    packages::{ArithmeticPackage, BasicArrayPackage, BasicMapPackage, LogicPackage}
};

// Define the package 'MyPackage'.
def_package!(rhai:MyPackage:"My own personal super package", module, {
    // Aggregate existing packages simply by calling 'init' on each.
    ArithmeticPackage::init(module);
    LogicPackage::init(module);
    BasicArrayPackage::init(module);
    BasicMapPackage::init(module);

    // Register additional Rust functions using the standard 'set_fn_XXX' module API.
    module.set_fn_1("foo", |s: ImmutableString| {
        Ok(foo(s.into_owned()))
    });
});
```


Create a Custom Package from a Plugin Module
-------------------------------------------

By far the easiest way to create a custom module is to call `Module::merge_flatten` from within
`rhai::def_package!` which simply merges in all the functions defined within a [plugin module].

In fact, this exactly is how Rhai's built-in packages, such as `BasicMathPackage`, are implemented.

`rhai::plugins::exported_module!` generates a module from the [plugins][plugin module] definition,
and `Module::merge_flatten` consumes its, adding all its registered functions into the package itself.

```rust
// Import necessary types and traits.
use rhai::{
    def_package,
    packages::Package,
    packages::{ArithmeticPackage, BasicArrayPackage, BasicMapPackage, LogicPackage}
};
use rhai::plugin::*;

// Define plugin module.
#[export_module]
mod my_module {
    pub fn greet(name: &str) -> String {
        format!("hello, {}!", name)
    }
    pub fn get_num() -> i64 {
        42
    }
}

// Define the package 'MyPackage'.
def_package!(rhai:MyPackage:"My own personal super package", module, {
    // Aggregate existing packages simply by calling 'init' on each.
    ArithmeticPackage::init(module);
    LogicPackage::init(module);
    BasicArrayPackage::init(module);
    BasicMapPackage::init(module);

    // Merge the plugin module into the custom package.
    module.merge_flatten(exported_module!(my_module));
});
```
