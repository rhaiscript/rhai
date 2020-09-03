Engine Configuration Options
===========================

{{#include ../links.md}}

A number of other configuration options are available from the `Engine` to fine-tune behavior and safeguards.

| Method                   | Not available under          | Description                                                                                                              |
| ------------------------ | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `set_optimization_level` | [`no_optimize`]              | Set the amount of script _optimizations_ performed. See [script optimization].                                           |
| `set_max_expr_depths`    | [`unchecked`]                | Set the maximum nesting levels of an expression/statement. See [maximum statement depth].                                |
| `set_max_call_levels`    | [`unchecked`]                | Set the maximum number of function call levels (default 50) to avoid infinite recursion. See [maximum call stack depth]. |
| `set_max_operations`     | [`unchecked`]                | Set the maximum number of _operations_ that a script is allowed to consume. See [maximum number of operations].          |
| `set_max_modules`        | [`unchecked`]                | Set the maximum number of [modules] that a script is allowed to load. See [maximum number of modules].                   |
| `set_max_string_size`    | [`unchecked`]                | Set the maximum length (in UTF-8 bytes) for [strings]. See [maximum length of strings].                                  |
| `set_max_array_size`     | [`unchecked`], [`no_index`]  | Set the maximum size for [arrays]. See [maximum size of arrays].                                                         |
| `set_max_map_size`       | [`unchecked`], [`no_object`] | Set the maximum number of properties for [object maps]. See [maximum size of object maps].                               |
| `disable_symbol`         |                              | Disable a certain keyword or operator. See [disable keywords and operators].                                             |
