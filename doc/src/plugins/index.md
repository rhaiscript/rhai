Plugins
=======

{{#include ../links.md}}

Rhai contains a robust _plugin_ system that greatly simplifies registration of custom functions.

Instead of the large `Engine::register_XXX` API, and the parallel `Module::set_fn_XXX` API,
a _plugin_ simplifies the work of creating and registering multiple functions into an [`Engine`].

Plugins are processed via a set of procedural macros under the `rhai::plugins` module.
