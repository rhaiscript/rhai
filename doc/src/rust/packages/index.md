Packages
========

{{#include ../../links.md}}

Standard built-in Rhai features are provided in various _packages_ that can be loaded via a call to `Engine::load_package`.

Packages reside under `rhai::packages::*` and the trait `rhai::packages::Package` must be loaded in order for
packages to be used.

Packages typically contain Rust functions that are callable within a Rhai script.
All functions registered in a package is loaded under the _global namespace_ (i.e. they're available without module qualifiers).

Once a package is created (e.g. via `Package::new`), it can be _shared_ (via `Package::get`) among multiple instances of [`Engine`],
even across threads (under [`sync`]). Therefore, a package only has to be created _once_.

```rust
use rhai::Engine;
use rhai::packages::Package         // load the 'Package' trait to use packages
use rhai::packages::CorePackage;    // the 'core' package contains basic functionalities (e.g. arithmetic)

let mut engine = Engine::new_raw(); // create a 'raw' Engine
let package = CorePackage::new();   // create a package - can be shared among multiple `Engine` instances

engine.load_package(package.get()); // load the package manually. 'get' returns a reference to the shared package
```


Difference Between a Package and a Module
----------------------------------------

Packages are actually implemented as [modules], so they share a lot of behavior and characteristics.

The main difference is that a package loads under the _global_ namespace, while a module loads under its own
namespace alias specified in an [`import`] statement (see also [modules]).

A package is _static_ (i.e. pre-loaded into an [`Engine`]), while a module is _dynamic_ (i.e. loaded with
the `import` statement).


Load a Module as a Package
--------------------------

Stand-alone [modules] can be loaded directly into an [`Engine`] as a package via the same `Engine::load_package` API.

```rust
let mut module = Module::new();
        :
    // add functions into module
        :

engine.load_package(module);
```

[Modules], however, are not _shared_, so use a [custom package] if it must be shared among multiple
instances of [`Engine`].
