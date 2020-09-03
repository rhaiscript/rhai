Rhai Release Notes
==================

Version 0.19.0
==============

New features
------------

* Plugins support via procedural macros.


Version 0.18.3
==============

Bug fixes
---------

* `Engine::compile_expression`, `Engine::eval_expression` etc. no longer parse anonymous functions and closures.
* Imported modules now work inside closures.
* Closures that capture now work under `no_object`.


Version 0.18.2
==============

New features
------------

* Adds `Module::combine_flatten` to combine two modules while flattening to the root level.


Version 0.18.2
==============

Bug fixes
---------

* Fixes bug that prevents calling functions in closures.
* Fixes bug that erroneously consumes the first argument to a module-qualified function call.

New features
------------

* Adds `Engine::register_get_result`, `Engine::register_set_result`, `Engine::register_indexer_get_result`, `Engine::register_indexer_set_result` API.
* Adds `Module::combine` to combine two modules.
* `Engine::parse_json` now also accepts a JSON object starting with `#{`.


Version 0.18.1
==============

This version adds:

* Anonymous functions (in Rust closure syntax).  Simplifies creation of single-use ad-hoc functions.
* Currying of function pointers.
* Closures - auto-currying of anonymous functions to capture shared variables from the external scope. Use the `no_closure` feature to disable sharing values and capturing.
* Binding the `this` pointer in a function pointer `call`.
* Capturing call scope via `func!(...)` syntax.

New features
------------

* `call` can now be called function-call style for function pointers - this is to handle builds with `no_object`.
* Reserve language keywords, such as `print`, `eval`, `call`, `this` etc.
* `x.call(f, ...)` allows binding `x` to `this` for the function referenced by the function pointer `f`.
* Anonymous functions are supported in the syntax of a Rust closure, e.g. `|x, y, z| x + y - z`.
* Custom syntax now works even without the `internals` feature.
* Currying of function pointers is supported via the new `curry` keyword.
* Automatic currying of anonymous functions to capture shared variables from the external scope.
* Capturing of the calling scope for function call via the `func!(...)` syntax.
* `Module::set_indexer_get_set_fn` is added as a shorthand of both `Module::set_indexer_get_fn` and `Module::set_indexer_set_fn`.
* New `unicode-xid-ident` feature to allow [Unicode Standard Annex #31](http://www.unicode.org/reports/tr31/) for identifiers.
* `Scope::iter_raw` returns an iterator with a reference to the underlying `Dynamic` value (which may be shared).

Breaking changes
----------------

* Language keywords are now _reserved_ (even when disabled) and they can no longer be used as variable names.
* Function signature for defining custom syntax is simplified.
* `Engine::register_raw_fn_XXX` API shortcuts are removed.
* `PackagesCollection::get_fn`, `PackagesCollection::contains_fn`, `Module::get_fn` and `Module::contains_fn` now take an additional `public_only` parameter indicating whether only public functions are accepted.
* The iterator returned by `Scope::iter` now contains a clone of the `Dynamic` value (unshared).
* `Engine::load_package` takes any type that is `Into<PackageLibrary>`.
* Error in `Engine::register_custom_syntax` is no longer `Box`-ed.

Housekeeping
------------

* Most compilation warnings are eliminated via feature gates.


Version 0.17.0
==============

This version adds:

* [`serde`](https://crates.io/crates/serde) support for working with `Dynamic` values (particularly _object maps_).
* Low-level API to register functions.
* Surgically disable keywords and/or operators in the language.
* Define custom operators.
* Extend the language via custom syntax.

Bug fixes
---------

* Fixed method calls in the middle of a dot chain.

Breaking changes
----------------

* `EvalAltResult::ErrorMismatchOutputType` has an extra argument containing the name of the requested type.
* `Engine::call_fn_dynamic` take an extra argument, allowing a `Dynamic` value to be bound to the `this` pointer.
* Precedence of the `%` (modulo) operator is lowered to below `<<` ad `>>`. This is to handle the case of `x << 3 % 10`.

New features
------------

* New `serde` feature to allow serializing/deserializing to/from `Dynamic` values using [`serde`](https://crates.io/crates/serde).
  This is particularly useful when converting a Rust `struct` to a `Dynamic` _object map_ and back.
* `Engine::disable_symbol` to surgically disable keywords and/or operators.
* `Engine::register_custom_operator` to define a custom operator.
* `Engine::register_custom_syntax` to define a custom syntax.
* New low-level API `Engine::register_raw_fn`.
* New low-level API `Module::set_raw_fn` mirroring `Engine::register_raw_fn`.
* `AST::clone_functions_only`, `AST::clone_functions_only_filtered` and `AST::clone_statements_only` to clone only part of an `AST`.
* The boolean `^` (XOR) operator is added.
* `FnPtr` is exposed as the function pointer type.
* `rhai::module_resolvers::ModuleResolversCollection` added to try a list of module resolvers.
* It is now possible to mutate the first argument of a module-qualified function call when the argument is a simple variable (but not a module constant).
* Many configuration/setting API's now returns `&mut Self` so that the calls can be chained.
* `String` parameters in functions are supported (but inefficiently).


Version 0.16.1
==============

Bug fix release to fix errors when compiling with features.


Version 0.16.0
==============

The major new feature in this version is OOP - well, poor man's OOP, that is.

The `README` is officially transferred to [The Rhai Book](https://schungx.github.io/rhai).

An online [Playground](https://alvinhochun.github.io/rhai-demo/) is available.

Breaking changes
----------------

* The trait function `ModuleResolver::resolve` no longer takes a `Scope` as argument.
* Functions defined in script now differentiates between using method-call style and normal function-call style.
  The method-call style will bind the object to the `this` parameter instead of consuming the first parameter.
* Imported modules are no longer stored in the `Scope`.  `Scope::push_module` is removed.
  Therefore, cannot rely on module imports to persist across invocations using a `Scope`.
* `AST::retain_functions` is used for another purpose. The old `AST::retain_functions` is renamed to `AST::clear_statements`.

New features
------------

* Support for _function pointers_ via `Fn(name)` and `Fn.call(...)` syntax - a poor man's first-class function.
* Support for calling script-defined functions in method-call style with `this` binding to the object.
* Special support in object maps for OOP.
* Expanded the `AST` API for fine-tuned manipulation of functions.

Enhancements
------------

* [The Rhai Book](https://schungx.github.io/rhai) is online.  Most content in the original `README` was transferred to the Book.
* New feature `internals` to expose internal data structures (e.g. the AST nodes).


Version 0.15.1
==============

This is a minor release which enables updating indexers (via registered indexer setters) and supports functions
with `&str` parameters (maps transparently to `ImmutableString`). WASM is also a tested target.

Bug fix
-------

* `let s="abc"; s[1].change_to('X');` now correctly sets the character '`X`' into '`s`' yielding `"aXc"`.

Breaking changes
----------------

* Callback closure passed to `Engine::on_progress` now takes `&u64` instead of `u64` to be consistent with other callback signatures.
* `Engine::register_indexer` is renamed to `Engine::register_indexer_get`.
* `Module::set_indexer_fn` is renamed to `Module::set_indexer_get_fn`.
* The tuple `ParseError` now exposes the internal fields and the `ParseError::error_type` and `ParseError::position` methods are removed.  The first tuple field is the `ParseErrorType` and the second tuple field is the `Position`.
* `Engine::call_fn_dynamic` now takes any type that implements `IntoIterator<Item = Dynamic>`.

New features
------------

* Indexers are now split into getters and setters (which now support updates).  The API is split into `Engine::register_indexer_get` and `Engine::register_indexer_set` with `Engine::register_indexer_get_set` being a shorthand.  Similarly, `Module::set_indexer_get_fn` and `Module::set_indexer_set_fn` are added.
* `Engine:register_fn` and `Engine:register_result_fn` accepts functions that take parameters of type `&str` (immutable string slice), which maps directly to `ImmutableString`. This is to avoid needing wrappers for functions taking string parameters.
* Set maximum limit on data sizes: `Engine::set_max_string_size`, `Engine::set_max_array_size` and `Engine::set_max_map_size`.
* Supports trailing commas on array literals, object map literals, function definitions and function calls.
* Enhances support for compiling to WASM.


Version 0.15.0
==============

This version uses immutable strings (`ImmutableString` type) and built-in operator functions (e.g. `+`, `>`, `+=`) to improve speed, plus some bug fixes.

Regression fix
--------------

* Do not optimize script with `eval_expression` - it is assumed to be one-off and short.

Bug fixes
---------

* Indexing with an index or dot expression now works property (it compiled wrongly before).
  For example, `let s = "hello"; s[s.len-1] = 'x';` now works property instead of causing a runtime error.
* `if` expressions are not supposed to be allowed when compiling for expressions only. This is fixed.

Breaking changes
----------------

* `Engine::compile_XXX` functions now return `ParseError` instead of `Box<ParseError>`.
* The `RegisterDynamicFn` trait is merged into the `RegisterResultFn` trait which now always returns `Result<Dynamic, Box<EvalAltResult>>`.
* Default maximum limit on levels of nested function calls is fine-tuned and set to a different value.
* Some operator functions are now built in (see _Speed enhancements_ below), so they are available even under `Engine::new_raw`.
* Strings are now immutable. The type `rhai::ImmutableString` is used instead of `std::string::String`. This is to avoid excessive cloning of strings.  All native-Rust functions taking string parameters should switch to `rhai::ImmutableString` (which is either `Rc<String>` or `Arc<String>` depending on whether the `sync` feature is used).
* Native Rust functions registered with the `Engine` also mutates the first argument when called in normal function-call style (previously the first argument will be passed by _value_ if not called in method-call style).  Of course, if the first argument is a calculated value (e.g. result of an expression), then mutating it has no effect, but at least it is not cloned.
* Some built-in methods (e.g. `len` for string, `floor` for `FLOAT`) now have _property_ versions in addition to methods to simplify coding.

New features
------------

* Set limit on maximum level of nesting expressions and statements to avoid panics during parsing.
* New `EvalPackage` to disable `eval`.
* `Module::set_getter_fn`, `Module::set_setter_fn` and `Module:set_indexer_fn` to register getter/setter/indexer functions.
* `Engine::call_fn_dynamic` for more control in calling script functions.

Speed enhancements
------------------

* Common operators (e.g. `+`, `>`, `==`) now call into highly efficient built-in implementations for standard types (i.e. `INT`, `FLOAT`, `bool`, `char`, `()` and `ImmutableString`) if not overridden by a registered function. This yields a 5-10% speed benefit depending on script operator usage. Scripts running tight loops will see significant speed-up.
* Common assignment operators (e.g. `+=`, `%=`) now call into highly efficient built-in implementations for standard types (i.e. `INT`, `FLOAT`, `bool`, `char`, `()` and `ImmutableString`) if not overridden by a registered function.
* Implementations of common operators for standard types are removed from the `ArithmeticPackage` and `LogicPackage` (and therefore the `CorePackage`) because they are now always available, even under `Engine::new_raw`.
* Operator-assignment statements (e.g. `+=`) are now handled directly and much faster.
* Strings are now _immutable_ and use the `rhai::ImmutableString` type, eliminating large amounts of cloning.
* For Native Rust functions taking a first `&mut` parameter, the first argument is passed by reference instead of by value, even if not called in method-call style.  This allows many functions declared with `&mut` parameter to avoid excessive cloning. For example, if `a` is a large array, getting its length in this manner: `len(a)` used to result in a full clone of `a` before taking the length and throwing the copy away. Now, `a` is simply passed by reference, avoiding the cloning altogether.
* A custom hasher simply passes through `u64` keys without hashing to avoid function call hash keys (which are by themselves `u64`) being hashed twice.


Version 0.14.1
==============

The major features for this release is modules, script resource limits, and speed improvements
(mainly due to avoiding allocations).

New features
------------

* Modules and _module resolvers_ allow loading external scripts under a module namespace. A module can contain constant variables, Rust functions and Rhai functions.
* `export` variables and `private` functions.
* _Indexers_ for Rust types.
* Track script evaluation progress and terminate script run.
* Set limit on maximum number of operations allowed per script run.
* Set limit on maximum number of modules loaded per script run.
* A new API, `Engine::compile_scripts_with_scope`, can compile a list of script segments without needing to first concatenate them together into one large string.
* Stepped `range` function with a custom step.

Speed improvements
------------------

### `StaticVec`

A script contains many lists - statements in a block, arguments to a function call etc.
In a typical script, most of these lists tend to be short - e.g. the vast majority of function calls contain
fewer than 4 arguments, while most statement blocks have fewer than 4-5 statements, with one or two being
the most common. Before, dynamic `Vec`'s are used to hold these short lists for very brief periods of time,
causing allocations churn.

In this version, large amounts of allocations are avoided by converting to a `StaticVec` -
a list type based on a static array for a small number of items (currently four) -
wherever possible plus other tricks. Most real-life scripts should see material speed increases.

### Pre-computed variable lookups

Almost all variable lookups, as well as lookups in loaded modules, are now pre-computed.
A variable's name is almost never used to search for the variable in the current scope.

_Getters_ and _setter_ function names are also pre-computed and cached, so no string allocations are
performed during a property get/set call.

### Pre-computed function call hashes

Lookup of all function calls, including Rust and Rhai ones, are now through pre-computed hashes.
The function name is no longer used to search for a function, making function call dispatches
much faster.

### Large Boxes for expressions and statements

The expression (`Expr`) and statement (`Stmt`) types are modified so that all of the variants contain only
one single `Box` to a large allocated structure containing _all_ the fields.  This makes the `Expr` and
`Stmt` types very small (only one single pointer) and improves evaluation speed due to cache efficiency.

Error handling
--------------

Previously, when an error occurs inside a function call, the error position reported is the function
call site. This makes it difficult to diagnose the actual location of the error within the function.

A new error variant `EvalAltResult::ErrorInFunctionCall` is added in this version.
It wraps the internal error returned by the called function, including the error position within the function.
