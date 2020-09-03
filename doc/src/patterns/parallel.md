One Engine Instance Per Call
===========================

{{#include ../links.md}}


Usage Scenario
--------------

* A system where scripts are called a _lot_, in tight loops or in parallel.

* Keeping a global [`Engine`] instance is sub-optimal due to contention and locking.

* Scripts need to be executed independently from each other, perhaps concurrently.

* Scripts are used to [create Rust closure][`Func`] that are stored and may be called at any time, perhaps concurrently.
  In this case, the [`Engine`] instance is usually moved into the closure itself.


Key Concepts
------------

* Create a single instance of each standard [package] required.  To duplicate `Engine::new`, create a [`StandardPackage`]({{rootUrl}}/rust/packages/builtin.md).

* Gather up all common custom functions into a [custom package].

* Store a global `AST` for use with all engines.

* Always use `Engine::new_raw` to create a [raw `Engine`], instead of `Engine::new` which is _much_ more expensive.
  A [raw `Engine`] is _extremely_ cheap to create.
  
  Loading the [`StandardPackage`]({{rootUrl}}/rust/packages/builtin.md) into a [raw `Engine`] via `Engine::load_package` is essentially the same as `Engine::new`.
  But because packages are shared, loading an existing package is _much cheaper_ than registering all the functions one by one.

* Load the required packages into the [raw `Engine`] via `Engine::load_package`, using `Package::get` to obtain a shared copy.


Examples
--------

```rust
use rhai::packages::{Package, StandardPackage};

let ast = /* ... some AST ... */;
let std_pkg = StandardPackage::new();
let custom_pkg = MyCustomPackage::new();

let make_call = |x: i64| -> Result<(), Box<EvalAltResult>> {
    // Create a raw Engine - extremely cheap.
    let mut engine = Engine::new_raw();

    // Load packages - cheap.
    engine.load_package(std_pkg.get());
    engine.load_package(custom_pkg.get());

    // Create custom scope - cheap.
    let mut scope = Scope::new();

    // Push variable into scope - relatively cheap.
    scope.push("x", x);

    // Evaluate script.
    engine.consume_ast_with_scope(&mut scope, &ast)
};

// The following loop creates 10,000 Engine instances!
for x in 0..10_000 {
    make_call(x)?;
}
```
