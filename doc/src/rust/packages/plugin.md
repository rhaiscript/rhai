Load a Plugin Module as a Package
================================

{{#include ../../links.md}}

[Plugin modules] can be loaded as a package just like a normal [module].

```rust
use rhai::Engine;
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

fn main() {
    let mut engine = Engine::new();

    // Create plugin module.
    let module = exported_module!(my_module);

    // Make the 'greet' and 'get_num' functions available to scripts.
    engine.load_package(module);
}
```


Share a Package Among `Engine`s
------------------------------

Loading a [module] via `Engine::load_package` consumes the module and it cannot be used again.

If the functions are needed for multiple instances of [`Engine`], create a [custom package] from the
[plugin module], which can then be shared with `Package::get`.
