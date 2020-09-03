Create a Module from Rust
========================

{{#include ../../links.md}}


Create via Plugin
-----------------

By far the simplest way to create a [module] is via a [plugin module].


Create via `Module` API
-----------------------

Manually creating a [module] is possible via the `Module` API.

For the complete `Module` API, refer to the [documentation](https://docs.rs/rhai/{{version}}/rhai/struct.Module.html) online.


Make the `Module` Available to the `Engine`
------------------------------------------

In order to _use_ a custom module, there must be a [module resolver].

The easiest way is to use, for example, the [`StaticModuleResolver`][module resolver] to hold such
a custom module.

```rust
use rhai::{Engine, Scope, Module, i64};
use rhai::module_resolvers::StaticModuleResolver;

let mut engine = Engine::new();
let mut scope = Scope::new();

let mut module = Module::new();             // new module
module.set_var("answer", 41_i64);           // variable 'answer' under module
module.set_fn_1("inc", |x: i64| Ok(x+1));   // use the 'set_fn_XXX' API to add functions

// Create the module resolver
let mut resolver = StaticModuleResolver::new();

// Add the module into the module resolver under the name 'question'
// They module can then be accessed via: 'import "question" as q;'
resolver.insert("question", module);

// Set the module resolver into the 'Engine'
engine.set_module_resolver(Some(resolver));

// Use module-qualified variables
engine.eval::<i64>(&scope, r#"import "question" as q; q::answer + 1"#)? == 42;

// Call module-qualified functions
engine.eval::<i64>(&scope, r#"import "question" as q; q::inc(q::answer)"#)? == 42;
```
