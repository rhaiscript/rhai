Rhai Release Notes
==================

Version 1.9.1
=============

This is a bug-fix version that fixes a bug.

Accessing properties in _Strict Variables Mode_ no longer generates a _variable not found_ error.


Version 1.9.0
=============

The minimum Rust version is now `1.60.0` in order to use the `dep:` syntax for dependencies.

Bug fixes
---------

* `switch` cases with conditions that evaluate to constant `()` no longer optimize to `false` (should raise a type error during runtime).
* Fixes concatenation of BLOB's and strings, where the BLOB's should be interpreted as UTF-8 encoded strings.
* Capturing an unknown variable in a closure no longer panics.
* Fixes panic in interpolated strings with constant expressions.
* Using `call_fn_raw` on a function without evaluating the AST no longer panics on namespace-qualified function calls due to `import` statements not run.
* Some reserved tokens (such as "?", "++") cannot be used in custom syntax; this is now fixed.

Breaking changes
----------------

* The first closure passed to `Engine::register_debugger` now takes a single parameter which is a reference to the current `Engine`.

New features
------------

### New feature flags

* A new feature flag, `std`, which is enabled by default, is added due to requirements from dependency crates.
* A new feature flag, `no_custom_syntax`, is added to remove custom syntax support from Rhai for applications that do not require it (which should be most).

### Module documentation

* Comment lines beginning with `//!` (requires the `metadata` feature) are now collected as the script file's _module documentation_.
* `AST` and `Module` have methods to access and manipulate documentation.

### Output definition files

* An API is added to automatically generate definition files from a fully-configured `Engine`, for use with the Rhai Language Server.

### Short-hand to function pointers

* Using a script-defined function's name (in place of a variable) implicitly creates a function pointer to the function.

### Top-level functions

* Crate-level functions `rhai::eval`, `rhai::run`, `rhai::eval_file`, `rhai::run_file` are added as convenient wrappers.

### CustomType trait and TypeBuilder

* A new volatile API, `Engine::build_type`, enables registration of the entire API of a custom type in one go, provided that the custom type implements the `CustomType` trait (which uses `TypeBuilder` to register the API functions).

### Simpler Package API

* It is now easier to register packages via the `Package::register_into_engine` and `Package::register_into_engine_as` API.
* Defining a custom package with base packages is also much easier with a new syntax - put the new base packages after a colon.

Enhancements
------------

### `switch` statement

* `switch` cases can now include multiple values separated by `|`.
* Duplicated `switch` cases are now allowed.
* The error `ParseErrorType::DuplicatedSwitchCase` is deprecated.
* Ranges in `switch` statements that are small (currently no more than 16 items) are unrolled if possible.

### Others

* `EvalContext::eval_expression_tree_raw` and `Expression::eval_with_context_raw` are added to allow for not rewinding the `Scope` at the end of a statements block.
* A new `range` function variant that takes an exclusive range with a step.
* `as_string` is added to BLOB's to convert it into a string by interpreting it as a UTF-8 byte stream.
* `FnAccess::is_private`, `FnAccess::is_public`, `FnNamespace::is_module_namespace` and `FnNameSpace::is_global_namespace` are added for convenience.
* `Iterator<Item=T>` type for functions metadata is simplified to `Iterator<T>`.
* `Scope::remove` is added to remove a variable from a `Scope`, returning its value.
* The code base is cleaner by running it through Clippy.
* `ParseError::err_type` and `ParseError::position` are added for convenience.
* The source of an `AST` compiled from a script file is set to the file's path.
* `|>` and `<|` are now reserved symbols.


Version 1.8.0
=============

Bug fixes
---------

* Self-contained `AST` now works properly with `Engine::call_fn`.
* Missing `to_int` from `Decimal` is added.
* Parsing of index expressions is relaxed and many cases no longer result in an index-type error to allow for custom indexers.
* Merging or combining a self-contained `AST` into another `AST` now works properly.
* Plugin modules/functions no longer generate errors under `#![deny(missing_docs)]`.
* Calling a property on a function call that returns a shared value no longer causes an error.
* _Strict Variables Mode_ now checks for module namespaces within functions as well.
* Module defined via `Engine::register_static_module` are now checked in _Strict Variables Mode_.

Reserved Symbols
----------------

* `?`, `??`, `?.`, `?[` and `!.` are now reserved symbols.

Deprecated API's
----------------

* `FnPtr::num_curried` is deprecated in favor of `FnPtr::curry().len()`.

New features
------------

* The _Elvis operators_ (`?.` and `?[`) are now supported for property access, method calls and indexing.
* The _null-coalescing operator_ (`??`) is now supported to short-circuit `()` values.

Enhancements
------------

* Indexing and property access are now faster.
* `EvalAltResult::IndexNotFound` is added to aid in raising errors for indexers.
* `Engine::def_tag`, `Engine::def_tag_mut` and `Engine::set_tag` are added to manage a default value for the custom evaluation state, accessible via `EvalState::tag()` (which is the same as `NativeCallContext::tag()`).
* Originally, the debugger's custom state uses the same state as `EvalState::tag()` (which is the same as `NativeCallContext::tag()`).  It is now split into its own variable accessible under `Debugger::state()`.
* Non-borrowed string keys can now be deserialized for object maps via `serde`.
* `Scope::get` is added to get a reference to a variable's value.
* Variable resolvers can now return a _shared_ value which can be mutated.


Version 1.7.0
=============

Bug fixes
---------

* Compound assignments now work properly with indexers.
* Cloning a `Scope` no longer turns all constants to mutable.

Script-breaking changes
-----------------------

* _Strict Variables Mode_ no longer returns an error when an undeclared variable matches a variable/constant in the provided external `Scope`.

Potentially breaking API changes
--------------------------------

* The `Engine::on_var` and `Engine::on_parse_token` API's are now marked unstable/volatile.
* The closures passed to `Engine::on_var`, `Engine::on_def_var` and `Engine::register_debugger` take `EvalContext` instead of `&EvalContext` or `&mut EvalContext`.
* The following enum's are marked `non_exhaustive`: `AccessMode`, `FnAccess`, `FnNamespace`, `FnMetadata`, `OptimizationLevel`

New API
-------

* `Module::eval_ast_as_new_raw` is made public as a low-level API.
* `format_map_as_json` is provided globally, which is the same as `to_json` for object maps.
* `Engine::call_fn_raw_raw` is added to add speed to repeated function calls.
* `Engine::eval_statements_raw` is added to evaluate a sequence of statements.

New features
------------

* A custom state is provided that is persistent during the entire evaluation run. This custom state is a `Dynamic`, which can hold any data, and can be accessed by the host via `EvalContext::tag`, `EvalContext::tag_mut`, `NativeCallContext::tag` and `GlobalRuntimeState.tag`.

Enhancements
------------

* Improper `switch` case condition syntax is now caught at parse time.
* `Engine::parse_json` now natively handles nested JSON inputs (using a token remap filter) without needing to replace `{` with `#{`.
* `to_json` is added to object maps to cheaply convert it to JSON format (`()` is mapped to `null`, all other data types must be supported by JSON)
* `FileModuleResolver` now accepts a custom `Scope` to provide constants for optimization.
* New variants, `Start` and `End`, are added to `DebuggerEvent` triggered at the start/end of script evaluation.


Version 1.6.1
=============

Bug fixes
---------

* Functions with `Dynamic` parameters now work in qualified calls from `import`ed modules.
* `rhai-repl` now compiles with the new patch version of `rustyline`.
* `rhai_codegen` dependency is now explicitly `1.4` or higher.

Script-breaking changes
-----------------------

* `split` now splits a string by whitespaces instead of splitting it into individual characters. This is more in line with common practices.
* A new function `to_chars` for strings is added to split the string into individual characters.

Enhancements
------------

* Strings are now directly iterable (via `for .. in`) yielding individual characters.


Version 1.6.0
=============

This version, in particular, fixes a plugin macro hygiene error for the nightly compiler:

```text
error[E0425]: cannot find value `args` in this scope
```

Compiler version
----------------

* Minimum compiler version is now `1.57` due to [`smartstring`](https://crates.io/crates/smartstring) dependency.

Bug fixes
---------

* Fixed macro hygiene error with nightly compiler.
* Invalid property or method access such as `a.b::c.d` or `a.b::func()` no longer panics but properly returns a syntax error.
* `Scope::is_constant` now returns the correct value.
* Exporting a variable that contains a local function pointer (including anonymous function or closure) now raises a runtime error.
* Full optimization is now skipped for method calls.

New features
------------

* [Type aliases](https://doc.rust-lang.org/reference/items/type-aliases.html) in plugin modules are now used as friendly names for custom types. This makes plugin modules more self-contained when they are used to define a custom type's API.

Enhancements
------------

* Variable definitions are optimized so that shadowed variables are reused as much as possible to reduce memory consumption.
* `FnAccess` and `FnNamespace` now implement `Ord` and `PartialOrd`.
* The `event_handler_map` example is enhanced to prevent shadowing of the state object map.
* Separation of constants in function calls is removed as its performance benefit is dubious.
* A function `sleep` is added to block the current thread by a specified number of seconds.
* `Scope::set_alias` is added to export a variable under a particular alias name.
* `starts_with` and `ends_with` are added for strings.
* Variables in modules registered via `register_global_module` can now be accessed in the global namespace.
* `Dynamic::into_read_only` is added to convert a `Dynamic` value into constant.
* `Module` now holds a collection of custom types with an API.


Version 1.5.0
=============

This version adds a debugging interface, which can be used to integrate a debugger.

Based on popular demand, an option is added to throw exceptions when invalid properties are accessed on object maps (default is to return `()`).

Also based on popular demand, the `REPL` tool now uses a slightly-enhanced version of [`rustyline`](https://crates.io/crates/rustyline) for line editing and history.

Bug fixes
---------

* In `Scope::clone_visible`, constants are now properly cloned as constants.
* Variables introduced inside `try` blocks are now properly cleaned up upon an exception.
* Off-by-one error in character positions after a comment line is now fixed.
* Globally-defined constants are now encapsulated correctly inside a loaded module and no longer spill across call boundaries.
* Type names display is fixed.
* Exceptions thrown inside function calls now unwrap correctly when `catch`-ed.
* Error messages for certain invalid property accesses are fixed.

Script-breaking changes
-----------------------

* For consistency with the `import` statement, the `export` statement no longer exports multiple variables.
* Appending a BLOB to a string (via `+`, `+=`, `append` or string interpolation) now treats the BLOB as a UTF-8 encoded string.
* Appending a string/character to a BLOB (via `+=` or `append`) now adds the string/character as a UTF-8 encoded byte stream.

New features
------------

* A debugging interface is added.
* A new bin tool, `rhai-dbg` (aka _The Rhai Debugger_), is added to showcase the debugging interface.
* A new package, `DebuggingPackage`, is added which contains the `back_trace` function to get the current call stack anywhere in a script.
* `Engine::set_fail_on_invalid_map_property` is added to control whether to raise an error (new `EvalAltResult::ErrorPropertyNotFound`) when invalid properties are accessed on object maps.
* `Engine::set_allow_shadowing` is added to allow/disallow variables _shadowing_, with new errors `EvalAltResult::ErrorVariableExists` and `ParseErrorType::VariableExists`.
* `Engine::on_def_var` allows registering a closure which can decide whether a variable definition is allow to continue, during compilation or runtime, or should fail with an error (`ParseErrorType::ForbiddenVariable` or `EvalAltResult::ErrorForbiddenVariable`).
* A new syntax for defining custom packages is introduced that removes the need to specify the Rhai crate name (internally uses the `$crate` meta variable).

Enhancements
------------

* Default features for dependencies (such as `ahash/std` and `num-traits/std`) are no longer required.
* The `no_module` feature now eliminates large sections of code via feature gates.
* Debug display of `AST` is improved.
* `NativeCallContext::call_level()` is added to give the current nesting level of function calls.
* A new feature, `bin-features`, pulls in all the required features for `bin` tools.
* `AST` position display is improved:
  * `Expr::start_position` is added to give the beginning of the expression (not the operator's position).
  * `StmtBlock` and `Stmt::Block` now keep the position of the closing `}` as well.
* `EvalAltResult::unwrap_inner` is added to access the base error inside multiple layers of wrappings (e.g. `EvalAltResult::ErrorInFunction`).
* Yet another new syntax is introduced for `def_package!` that further simplifies the old syntax.
* A new method `to_blob` is added to convert a string into a BLOB as UTF-8 encoded bytes.
* A new method `to_array` is added to convert a BLOB into array of integers.

REPL tool changes
-----------------

The REPL bin tool, `rhai-rpl`, has been enhanced.

### Build changes

* The `rustyline` feature is now required in order to build `rhai-repl`.
* Therefore, `rhai-repl` is no longer automatically built when using a simple `cargo build` with default features.

### Line editor

* `rhai-repl` now uses a modified version of [`rustyline`](https://crates.io/crates/rustyline) as a line editor with history.
* Ctrl-Enter can now be used to enter multiple lines without having to attach the `\` continuation character the end of each line.
* Bracketed paste is supported, even on Windows (version 10 or above), so pasting code directly into `rhai-repl` is made much more convenient.

### New commands

* `strict` to turn on/off _Strict Variables Mode_.
* `optimize` to turn on/off script optimization.
* `history` to print lines history.
* `!!`, `!`_num_, `!`_text_ and `!?`_text_ to recall a history line.
* `keys` to print all key bindings.


Version 1.4.1
=============

This is primarily a bug-fix version which fixes a large number of bugs.

Bug fixes
---------

* Expressions such as `x = x + 1` no longer panics.
* Padding arrays with another array via `pad` no longer loops indefinitely.
* `chop` for arrays and BLOB's now works properly.
* `set_bit` for bit-flags with negative index now works correctly.
* Misnamed `params` field `name` in the JSON output of `Engine::gen_fn_metadata_to_json` is fixed (was incorrectly named `type`).
* Fixes a potential `unsafe` violation in `for` loop.
* Missing `to_hex`, `to_octal` and `to_binary` for `i128` and `u128` are added.
* `remove` for arrays and BLOB's now treat negative index correctly.
* `parse_int` now works properly for negative numbers.
* `Engine::gen_fn_signatures` now generates signatures for external packages registered via `Engine::register_global_module`.
* `\r\n` pairs are now recognized correctly for doc-comments.

Enhancements
------------

* Formatting of return types in functions metadata info is improved.
* Use `SmartString` for `Scope` variable names and remove `unsafe` lifetime casting.
* Functions in the standard library now have doc-comments (which can be obtained via `Engine::gen_fn_metadata_to_json`).
* `get` and `set` methods are added to arrays, BLOB's, object maps and strings.


Version 1.4.0
=============

This version adds support for integer _ranges_ via the `..` and `..=` operators.
Many standard API's are extended with range parameters where appropriate.

Script-breaking changes
-----------------------

* `is` is (pun intended) now a reserved keyword to prepare for possible future type checking expressions (e.g. `x is "string"`).

Breaking changes
----------------

* `LogicPackage` is removed from `CorePackage`.
* Bit-field functions are moved into a new `BitFieldPackage` (used to be in `LogicPackage`) which makes more sense.

Bug fixes
---------

* Custom syntax now works properly inside binary expressions and with method calls.
* Hex numbers with the high-bit set now parse correctly into negative integer numbers.
* Constructing a literal array or object map now checks for size limits for each item instead of at the very end when it is already too late.
* Non-`INT` integer types are now treated exactly as custom types under `only_i64` and `only_i32`.
* Calling `pad` on an array now checks for total size over limit after each item added.

New features
------------

* Added support for integer _ranges_ via the `..` and `..=` operators.
* Added `EvalAltResult::ErrorCustomSyntax` to catch errors in custom syntax, which should not happen unless an `AST` is compiled on one `Engine` but evaluated on another unrelated `Engine`.

Enhancements
------------

* `BLOB`'s are refined to display in a more compact hex format.
* A new syntax is introduced for `def_package!` that will replace the old syntax in future versions.
* Added `NativeCallContext::call_fn` to easily call a function.
* Doc-comments on plugin module functions are extracted into the functions' metadata.

Deprecated API's
----------------

* `Expression::get_variable_name` is deprecated in favor of the new `Expression::get_string_value`.
* The old syntax of `def_package!` is deprecated in favor of the new syntax.


Version 1.3.0
=============

This version adds native support for `BLOB`'s (byte arrays), as well as a number of configuration
settings to fine-tun language features.

Compiler requirement
--------------------

* Minimum compiler version is now 1.51.

Bug fixes
---------

* `from_dynamic` now supports deserializing `Option`.

New features
------------

* `BLOB` (essentially a byte array) is added as a supported primitive value type parallel to arrays.
* New options for `Engine` which allows disabling `if`-expressions, `switch`-expressions, statement expressions, anonymous functions and/or looping (i.e. `while`, `loop`, `do` and `for` statements):
  * `Engine::set_allow_if_expression`
  * `Engine::set_allow_switch_expression`
  * `Engine::set_allow_statement_expression`
  * `Engine::set_allow_anonymous_fn`
  * `Engine::set_allow_looping`
* New _strict variables_ mode for `Engine` (enabled via `Engine::set_strict_variables`) to throw parse errors on undefined variable usage. Two new parse error variants, `ParseErrorType::VariableNotFound` and `ParseErrorType::ModuleNotFound`, are added.

Enhancements
------------

* Two double quotes (`""`) in a string literal now maps to `"`; two back-ticks (``` `` ```) in a literal string now maps to `` ` ``.
* Added `Engine::register_type_with_name_raw` to register a custom type based on a fully-qualified type path.
* Added `into_array` and `into_typed_array` for `Dynamic`.
* Added `FnPtr::call` and `FnPtr::call_within_context` to simplify calling a function pointer.
* A function's hashes are included in its JSON metadata to assist in debugging. Each function's `baseHash` field in the JSON object should map directly to the pre-calculated hash in the function call.
* `Expression` now derefs to `Expr`.

Deprecated and Gated API's
--------------------------

* `NativeCallContext::new` is deprecated because it is simpler to call a function pointer via `FnPtr::call`.
* `AST::merge_filtered` and `AST::combine_filtered` are no longer exported under `no_function`.
* `AST::new` and `AST::new_with_source` are moved under `internals`.
* `FnPtr::call_dynamic` is deprecated in favor of `FnPtr::call_raw`.


Version 1.2.1
=============

Bug fixes
---------

* Array methods (such as `map`) taking a closure with captures as argument now works properly.


Version 1.2.0
=============

Bug fixes (potentially script-breaking)
--------------------------------------

* As originally intended, function calls with a bang (`!`) now operates directly on the caller's scope, allowing variables inside the scope to be mutated.
* As originally intended, `Engine::XXX_with_scope` API's now properly propagate constants within the provided scope also to _functions_ in the script.
* Printing of integral floating-point numbers is fixed (used to only prints `0.0`).
* `func!()` calls now work properly under `no_closure`.
* Fixed parsing of unary negation such that expressions like `if foo { ... } -x` parses correctly.

New features
------------

* `#[cfg(...)]` attributes can now be put directly on plugin functions or function defined in a plugin module.
* A custom syntax parser can now return a symbol starting with `$$` to inform the implementation function which syntax variant was actually parsed.
* `AST::iter_literal_variables` is added to extract all top-level literal constant/variable definitions from a script without running it.
* `Engine::call_fn_dynamic` is deprecated and `Engine::call_fn_raw` is added which allows keeping new variables in the custom scope.

Enhancements
------------

* Array methods now avoid cloning as much as possible (although most predicates will involve cloning anyway if passed a closure).
* Array methods that take function pointers (e.g. closures) now optionally take the function name as a string.
* Array adds the `dedup` method.
* Array adds a `sort` method with no parameters which sorts homogeneous arrays of built-in comparable types (e.g. `INT`).
* Inlining is disabled for error-path functions because errors are exceptional and scripts usually fail completely when an error is encountered.
* The `optimize` module is completely eliminated under `no_optimize`, which should yield smaller code size.
* `NativeCallContext::position` is added to return the position of the function call.
* `Scope::clone_visible` is added that copies only the last instance of each variable, omitting all shadowed variables.

Deprecated API's
----------------

* `NativeCallContext::call_fn_dynamic_raw` is deprecated and `NativeCallContext::call_fn_raw` is added.
* `From<EvalAltResult>` for `Result<T, Box<EvalAltResult> >` is deprecated so it will no longer be possible to do `EvalAltResult::ErrorXXXXX.into()` to convert to a `Result`; instead, `Err(EvalAltResult:ErrorXXXXX.into())` must be used. Code is clearer if errors are explicitly wrapped in `Err`.


Version 1.1.2
=============

Bug fixes
---------

* `0.0` now prints correctly (used to print `0e0`).
* Unary operators are now properly recognized as an expression statement.
* Reverses a regression on string `+` operations.
* The global namespace is now searched before packages, which is the correct behavior.


Version 1.1.1
=============

Bug fixes
---------

* Assignment to indexing expression with dot expressions inside no longer cause a compilation error.
* The `no_module` and `internals` features now work together without a compilation error.
* String literal operations (such as `"hello" + ", world"`) now optimizes correctly.


Version 1.1.0
=============

Bug fixes
---------

* Custom syntax starting with a disabled standard keyword now works properly.
* When calling `Engine::call_fn`, new variables defined during evaluation of the body script are removed and no longer spill into the function call.
* `NamespaceRef::new` is fixed.

Enhancements
------------

### `Engine` API

* `Engine::consume_XXX` methods are renamed to `Engine::run_XXX` to make meanings clearer. The `consume_XXX` API is deprecated.
* `Engine::register_type_XXX` are now available even under `no_object`.
* Added `Engine::on_parse_token` to allow remapping certain tokens during parsing.
* Added `Engine::const_empty_string` to merge empty strings into a single instance.

### Custom Syntax

* `$symbol$` is supported in custom syntax to match any symbol.
* Custom syntax with `$block$`, `}` or `;` as the last symbol are now self-terminating (i.e. no need to attach a terminating `;`).

### `Dynamic` Values

* `Dynamic::as_string` and `Dynamic::as_immutable_string` are deprecated and replaced by `into_string` and `into_immutable_string` respectively.
* Added a number of constants to `Dynamic`.
* Added a number of constants and `fromXXX` constant methods to `Dynamic`.
* Added `sin`, `cos` and `tan` for `Decimal` values.

### `Decimal` Values

* `parse_float()`, `PI()` and `E()` now defer to `Decimal` under `no_float` if `decimal` is turned on.
* Added `log10()` for `Decimal`.
* `ln` for `Decimal` is now checked and won't panic.

### String Values

* `SmartString` now uses `LazyCompact` instead of `Compact` to minimize allocations.
* Added `pop` for strings.
* Added `ImmutableString::ptr_eq` to test if two strings point to the same allocation.
* The `serde` feature of `SmartString` is turned on under `metadata` to make `Map` serializable.

### `Scope` API

* `Scope::set_value` now takes anything that implements `Into<Cow<str> >`.
* Added `Scope::is_constant` to check if a variable is constant.
* Added `Scope::set_or_push` to add a new variable only if one doesn't already exist.

### `AST` API

* Added `ASTNode::position`.
* `ReturnType` is removed in favor of option flags for `Stmt::Return`.
* `Stmt::Break` and `Stmt::Continue` are merged into `Stmt::BreakLoop` via an option flag.
* `StaticVec` is changed to keep three items inline instead of four.


Version 1.0.6
=============

Bug fixes
---------

* Eliminate unnecessary property write-back when accessed via a getter since property getters are assumed to be _pure_.
* Writing to a property of an indexed valued obtained via an indexer now works properly by writing back the changed value via an index setter.

Enhancements
------------

* `MultiInputsStream`, `ParseState`, `TokenIterator`, `IdentifierBuilder` and `AccessMode` are exported under the `internals` feature.


Version 1.0.5
=============

Bug fixes
---------

* `FloatWrapper` is no longer erroneously exported under `no_float+internals`.
* The `sign` function now works properly for float values that are `NaN`.


Version 1.0.4
=============

* Fixed bug with `catch` variable used in `catch` block.


Version 1.0.2
=============

Bug fixes
---------

* Fixed bug in method call followed by an array indexing.


Version 1.0.1
=============

Bug fixes
---------

* Fixed bug in using indexing/dotting inside index bracket.
* `while` and `loop` statements are no longer considered _pure_ (since a loop can go on forever and this is a side effect).


Version 1.0.0
=============

The official version `1.0`.

Almost the same version as `0.20.3` but with deprecated API's removed.

Bug fixes
---------

* Fixed infinite loop in certain script optimizations.
* Building for `no-std` no longer requires patching `smartstring`.
* Parsing a lone `return` or `throw` without a semicolon at the end of a block no longer raises an error.

Breaking changes
----------------

* All deprecated API's (e.g. the `RegisterFn` and `RegisterResultFn` traits) are removed.
* `Module::set_id` is split into `Module::set_id` and `Module::clear_id` pair.
* `begin`, `end`, `each`, `then`, `unless` are no longer reserved keywords.

Enhancements
------------

* New methods `is_odd`, `is_even` for integers, and `is_zero` for all numbers.
* `From<BTreeSet>` and `From<HashSet>` are added for `Dynamic`, which create object maps with `()` values.


Version 0.20.3
==============

This version adds support to index into an integer number, treating it as a bit-field.

Bug fixes
---------

* Fixed incorrect optimization regarding chain-indexing with non-numeric index.
* Variable values are checked for over-sized violations after assignments and setters.

Breaking changes
----------------

* To keep the API consistent, strings are no longer iterable by default.  Use the `chars` method to iterate through the characters in a string.
* `Dynamic::take_string` and `Dynamic::take_immutable_string` are renamed to `Dynamic::as_string` and `Dynamic::as_immutable_string` respectively.

New features
------------

* New syntax for `for` statement to include counter variable.
* An integer value can now be indexed to get/set a single bit.
* The `bits` method of an integer can be used to iterate through its bits.
* New `$bool$`, `$int$`, `$float$` and `$string$` expression types for custom syntax.
* New methods `to_hex`, `to_octal` and `to_binary` for integer numbers.
* New methods `to_upper`, `to_lower`, `make_upper`, `make_lower` for strings/characters.


Version 0.20.2
==============

This version adds a number of convenience features:

* Ability for a `Dynamic` to hold an `i32` _tag_ of arbitrary data

* Simplifies dynamic properties access by falling back to an indexer (passing the name of the property as a string) when a property is not found.

Bug fixes
---------

* Propagation of constants held in a custom scope now works properly instead of always replacing by `()`.

Breaking changes
----------------

* `Engine::disable_doc_comments` is removed because doc-comments are now placed under the `metadata` feature flag.
* Registering a custom syntax now only requires specifying whether the `Scope` is adjusted (i.e. whether variables are added or removed). There is no need to specify the number of variables added/removed.
* Assigning to a property of a constant is now allowed and no longer raise an `EvalAltResult::ErrorAssignmentToConstant` error. This is to facilitate the Singleton pattern. Registered setter functions are automatically guarded against setters calling on constants and will continue to raise errors unless the `pure` attribute is present (for plugins).
* If a property getter/setter is not found, an indexer with string index, if any, is tried.
* The indexers API (`Engine::register_indexer_XXX` and `Module::set_indexer_XXX`) are now also exposed under `no_index`.

New features
------------

* Each `Dynamic` value can now contain arbitrary data (type `i32`) in the form of a _tag_. This is to use up otherwise wasted space in the `Dynamic` type.
* A new internal feature `no_smartstring` to turn off `SmartString` for those rare cases that it is needed.
* `DynamicReadLock` and `DynamicWriteLoc` are exposed under `internals`.
* `From< Shared< Locked<Dynamic> > >` is added for `Dynamic` mapping directly to a shared value, together with support for `Dynamic::from`.
* An indexer with string index acts as a _fallback_ to a property getter/setter.

Enhancements
------------

* Registering a custom syntax now only requires specifying whether the `Scope` is adjusted (i.e. whether variables are added or removed). This allows more flexibility for cases where the number of new variables declared depends on internal logic.
* Putting a `pure` attribute on a plugin property/index setter now enables it to be used on constants.


Version 0.20.1
==============

This version enables functions to access constants declared at global level via the special `global` module.

Bug fixes
---------

* Fixed bug when position is zero in `insert` and `split_at` methods for arrays.
* Indexing operations with pure index values are no longer considered pure due to the possibility of indexers.

Breaking changes
----------------

* `Dynamic::is_shared` and `Dynamic::is_locked` are removed under the `no_closure` feature. They used to always return `false`.
* `Engine::call_fn` now evaluates the `AST` before calling the function.
* `Engine::on_progress` is disabled with `unchecked`.

Enhancements
------------

* The crate [`no-std-compat`](https://crates.io/crates/no_std_compat) is used to compile for `no-std`. This removes the need to use a special `crate::stdlib` namespace for `std` imports.

New features
------------

* A module called `global` is automatically created to hold global-level constants, which can then be accessed from functions.
* A new feature `no_position` is added to turn off position tracking during parsing to squeeze out the last drop of performance.


Version 0.20.0
==============

This version adds string interpolation with `` `... ${`` ... ``} ...` `` syntax.

`switch` statement cases can now have conditions.

Negative indices for arrays and strings are allowed and now count from the end (-1 = last item/character).

Bug fixes
---------

* Property setter op-assignments now work properly.
* Off-by-one bug in `Array::drain` method with range is fixed.

Breaking changes
----------------

* Negative index to an array or string yields the appropriate element/character counting from the _end_.
* The default `_` case of a `switch` statement now must be the last case, together with two new error variants: `EvalAltResult::WrongSwitchDefaultCase` and `EvalAltResult::WrongSwitchCaseCondition`.
* `ModuleResolver` trait methods take an additional parameter `source_path` that contains the path of the current environment. This is to facilitate loading other script files always from the current directory.
* `FileModuleResolver` now resolves relative paths under the source path if there is no base path set.
* `FileModuleResolver::base_path` now returns `Option<&str>` which is `None` if there is no base path set.
* Doc-comments now require the `metadata` feature.

Enhancements
------------

* `Array::drain` and `Array::retain` methods with predicate now scan the array in forward order instead of in reverse.

New features
------------

* String interpolation support is added via the `` `... ${`` ... ``} ...` `` syntax.
* `FileModuleResolver` resolves relative paths under the parent path (i.e. the path holding the script that does the loading). This allows seamless cross-loading of scripts from a directory hierarchy instead of having all relative paths load from the current working directory.
* Negative index to an array or string yields the appropriate element/character counting from the _end_.
* `switch` statement cases can now have an optional `if` clause.


Version 0.19.15
===============

This version replaces all internal usage of `HashMap` with `BTreeMap`, which should result
in some speed improvement because a `BTreeMap` is leaner when the number of items held is small.
Most, if not all, collections in Rhai hold very few data items, so this is a typical scenario of
many tiny-sized collections.

The Rhai object map type, `Map`, used to be an alias to `HashMap` and is now aliased to `BTreeMap`
instead. This is also because, in the vast majority of usage cases, the number of properties held by
an object map is small.

`HashMap` and `BTreeMap` have almost identical public API's so this change is unlikely to break
existing code.

[`SmartString`](https://crates.io/crates/smartstring) is used to store identifiers (which tend to
be short, fewer than 23 characters, and ASCII-based) because they can usually be stored inline.
`Map` keys now also use [`SmartString`](https://crates.io/crates/smartstring).

In addition, there is now support for line continuation in strings (put `\` at the end of line) as
well as multi-line literal strings (wrapped by back-ticks: `` `...` ``).

Finally, all function signature/metadata methods are now grouped under the umbrella `metadata` feature.
This avoids spending precious resources maintaining metadata for functions for the vast majority of
use cases where such information is not required.


Bug fixes
---------

* The feature flags `no_index + no_object` now compile without errors.

Breaking changes
----------------

* The traits `RegisterFn` and `RegisterResultFn` are removed.  `Engine::register_fn` and `Engine::register_result_fn` are now implemented directly on `Engine`.
* `FnPtr::call_dynamic` now takes `&NativeCallContext` instead of consuming it.
* All `Module::set_fn_XXX` methods are removed, in favor of `Module::set_native_fn`.
* `Array::reduce` and `Array::reduce_rev` now take a `Dynamic` as initial value instead of a function pointer.
* `protected`, `super` are now reserved keywords.
* The `Module::set_fn_XXX` API now take `&str` as the function name instead of `Into<String>`.
* The _reflections_ API such as `Engine::gen_fn_signatures`, `Module::update_fn_metadata` etc. are put under the `metadata` feature gate.
* The shebang `#!` is now a reserved symbol.
* Shebangs at the very beginning of script files are skipped when loading them.
* [`SmartString`](https://crates.io/crates/smartstring) is used for identifiers by default. Currently, a PR branch is pulled for `no-std` builds. The official crate will be used once `SmartString` is fixed to support `no-std`.
* `Map` is now an alias to `BTreeMap<SmartString, Dynamic>` instead of `HashMap` because most object maps hold few properties.
* `EvalAltResult::FnWrongDefinition` is renamed `WrongFnDefinition` for consistency.

New features
------------

* Line continuation (via `\`) and multi-line literal strings (wrapped with `` ` ``) support are added.
* Rhai scripts can now start with a shebang `#!` which is ignored.

Enhancements
------------

* Replaced all `HashMap` usage with `BTreeMap` for better performance because collections in Rhai are tiny.
* `Engine::register_result_fn` no longer requires the successful return type to be `Dynamic`.  It can now be any clonable type.
* `#[rhai_fn(return_raw)]` can now return `Result<T, Box<EvalAltResult> >` where `T` is any clonable
  type instead of `Result<Dynamic, Box<EvalAltResult> >`.
* `Dynamic::clone_cast` is added to simplify casting from a `&Dynamic`.


Version 0.19.14
===============

This version runs faster due to optimizations done on AST node structures. It also fixes a number of
panic bugs related to passing shared values as function call arguments.

Bug fixes
---------

* Panic when passing a shared string into a registered function as `&str` argument is fixed.
* Panic when calling `switch` statements on custom types is fixed.
* Potential overflow panics in `range(from, to, step)` is fixed.
* `&mut String` parameters in registered functions no longer panic when passed a string.
* Some expressions involving shared variables now work properly, for example `x in shared_value`, `return shared_value`, `obj.field = shared_value` etc. Previously, the resultant value is still shared which is counter-intuitive.
* Errors in native Rust functions now contain the correct function call positions.
* Fixed error types in `EvalAltResult::ErrorMismatchDataType` which were swapped.

Breaking changes
----------------

* `Dynamic::as_str` is removed because it does not properly handle shared values.
* Zero step in the `range` function now raises an error instead of creating an infinite stream.
* Error variable captured by `catch` is now an _object map_ containing error fields.
* `EvalAltResult::clear_position` is renamed `EvalAltResult::take_position` and returns the position taken.
* `private` functions in an `AST` can now be called with `call_fn` etc.
* `NativeCallContext::call_fn_dynamic_raw` no longer has the `pub_only` parameter.
* `Module::update_fn_metadata` input parameter is changed.
* Function keywords (e.g. `type_of`, `eval`, `Fn`) can no longer be overloaded. It is more trouble than worth. To disable these keywords, use `Engine::disable_symbol`.
* `is_def_var` and `is_def_fn` are now reserved keywords.
* `Engine::id` field is removed because it is never used.
* `num-traits` is now a required dependency.
* The `in` operator is now implemented on top of the `contains` function and is no longer restricted to a few specific types.
* `EvalAltResult::ErrorInExpr` is removed because the `in` operator now calls `contains`.
* The methods `AST::walk`, `Expr::walk`, `Stmt::walk` and `ASTNode::walk` and the callbacks they take now return `bool` to optionally terminate the recursive walk.

Enhancements
------------

* Layout of AST nodes is optimized to reduce redirections, so speed is improved.
* Function calls are more optimized and should now run faster.
* `range` function now supports negative step and decreasing streams (i.e. to < from).
* More information is provided to the error variable captured by the `catch` statement in an _object map_.
* Previously, `private` functions in an `AST` cannot be called with `call_fn` etc. This is inconvenient when trying to call a function inside a script which also serves as a loadable module exporting part (but not all) of the functions. Now, all functions (`private` or not) can be called in an `AST`. The `private` keyword is relegated to preventing a function from being exported.
* `Dynamic::as_unit` just for completeness sake.
* `bytes` method added for strings to get length quickly (if the string is ASCII-only).
* `FileModuleResolver` can now enable/disable caching.
* Recursively walking an `AST` can now be terminated in the middle.


Version 0.19.13
===============

This version introduces functions with `Dynamic` parameters acting as wildcards.

Bug fixes
---------

* Bug in `Position::is_beginning_of_line` is fixed.

Breaking changes
----------------

* For plugin functions, constants passed to methods (i.e. `&mut` parameter) now raise an error unless the functions are marked with `#[rhai_fn(pure)]`.
* Visibility (i.e. `pub` or not) for generated _plugin_ modules now follow the visibility of the underlying module.
* Comparison operators between the sames types or different _numeric_ types now throw errors when they're not defined instead of returning the default. Only comparing between _different_ types will return the default.
* Default stack-overflow and top-level expression nesting limits for release builds are lowered to 64 from 128.
* `Engine::call_fn_dynamic` takes an additional parameter to optionally evaluate the given `AST` before calling the function.

New features
------------

* Functions are now allowed to have `Dynamic` arguments.
* `#[rhai_fn(pure)]` attribute to mark a plugin function with `&mut` parameter as _pure_ so constants can be passed to it. Without it, passing a constant value into the `&mut` parameter will now raise an error.

Enhancements
------------

* Built-in operators between `FLOAT`/[`Decimal`](https://crates.io/crates/rust_decimal) and `INT` are now implemented for more speed under those cases.
* Error position in `eval` statements is now wrapped in an `EvalAltResult::ErrorInFunctionCall`.
* `Position` now implements `Add` and `AddAssign`.
* `Scope` now implements `IntoIterator`.
* Strings now have the `-`/`-=` operators and the `remove` method to delete a sub-string/character.
* Strings now have the `split_rev` method and variations of `split` with maximum number of segments.
* Arrays now have the `split` method.
* Comparisons between `FLOAT`/[`Decimal`](https://crates.io/crates/rust_decimal) and `INT` are now built in.
* Comparisons between string and `char` are now built in.
* `Engine::call_fn_dynamic` can now optionally evaluate the given `AST` before calling the function.


Version 0.19.12
===============

This version is an incremental release with a number of enhancements and bug fixes.

Notice that there are a number of breaking changes, especially with regards to replacing the `~`
exponential  operator with `**`, and the addition of the `decimal` feature that turns on
[`Decimal`](https://crates.io/crates/rust_decimal) support.

Bug fixes
---------

* Empty statements (i.e. statements with only one `;`) now parse correctly and no longer hang.
* `continue`, `break` and `return` statements no longer panic inside a `try .. catch` block.
* `round` function for `f64` is now implemented correctly.

Breaking changes
----------------

* In order to be consistent with other scripting languages:
  * the power/exponentiation operator is changed from `~` to `**`; `~` is now a reserved symbol
  * the power/exponentiation operator now binds to the right
  * trigonometry functions now take radians and return radians instead of degrees
* `Dynamic::into_shared` is no longer available under `no_closure`. It used to panic.
* `Token::is_operator` is renamed to `Token::is_symbol`.
* `AST::clone_functions_only_filtered`, `AST::merge_filtered`, `AST::combine_filtered` and `AST::retain_functions` now take `Fn` instead of `FnMut` as the filter predicate.

New features
------------

* Scientific notation is supported for floating-point number literals.
* A new feature, `decimal`, enables the [`Decimal`](https://crates.io/crates/rust_decimal) data type. When both `no_float` and `decimal` features are enabled, floating-point literals parse to `Decimal`.

Enhancements
------------

* Functions resolution cache is used in more cases, making repeated function calls faster.
* Added `atan(x, y)` and `hypot(x, y)` to `BasicMathPackage`.
* Added standard arithmetic operators between `FLOAT`/[`Decimal`](https://crates.io/crates/rust_decimal) and `INT`.


Version 0.19.11
===============

This version streamlines compiling for WASM.

Rust compiler minimum version is raised to 1.49.

Bug fixes
---------

* Parameters passed to plugin module functions were sometimes erroneously consumed. This is now fixed.
* Fixes compilation errors in `metadata` feature build.
* Stacking `!` operators now work properly.
* Off-by-one error in `insert` method for arrays is fixed.
* Invalid property access now throws the appropriate error instead of panics.

Breaking changes
----------------

* Rust compiler requirement raised to 1.49.
* `NativeCallContext::new` taker an additional parameter containing the name of the function called.
* `Engine::set_doc_comments` is renamed `Engine::enable_doc_comments`.

New features
------------

* Two new features, `wasm-bindgen` and `stdweb`, to specify the JS interop layer for WASM builds. `wasm-bindgen` used to be required.

Enhancements
------------

* `ahash` is used to hash function call parameters. This should yield speed improvements.
* `Dynamic` and `ImmutableString` now implement `serde::Serialize` and `serde::Deserialize`.
* `NativeCallContext` has a new field containing the name of the function called, useful when the same Rust function is registered under multiple names in Rhai.
* New functions `PI()` and `E()` to return mathematical constants, and `to_radians` and `to_degrees` to convert between radians and degrees.


Version 0.19.10
===============

Bug fixes
---------

* `no_std` feature now compiles correctly (bug introduced in `0.19.9`).
* Bug in `FileModuleResolver::clear_cache_for_path` path mapping fixed.
* Some optimizer fringe cases are fixed - related to constants propagation when the evil `eval` is present.

Breaking changes
----------------

* The error variant `EvalAltResult::ErrorInFunctionCall` has a new parameter holding the _source_ of the function.
* `ParseErrorType::WrongFnDefinition` is renamed `FnWrongDefinition`.
* Redefining an existing function within the same script now throws a new `ParseErrorType::FnDuplicatedDefinition`. This is to prevent accidental overwriting an earlier function definition.
* `AST::set_source` is now split into `AST::set_source` and `AST::clear_source`.

New features
------------

* `Engine::compile_into_self_contained` compiles a script into an `AST` and _eagerly_ resolves all `import` statements with string literal paths. The resolved modules are directly embedded into the `AST`. When the `AST` is later evaluated, `import` statements directly yield the pre-resolved modules without going through the resolution process once again.
* `AST::walk`, `Stmt::walk` and `Expr::walk` internal API's to recursively walk an `AST`.

Enhancements
------------

* Source information is provided when there is an error within a call to a function defined in another module.
* Source information is provided to the `NativeCallContext` for native Rust functions.
* `EvalAltResult::clear_position` to clear the position information of an error - useful when only the message is needed and the position doesn't need to be printed out.
* A new optional function `resolve_ast` is added to the `ModuleResolver` trait for advanced usage.


Version 0.19.9
==============

This version fixes a bug introduced in `0.19.8` which breaks property access
within closures.

It also removes the confusing differences between _packages_ and _modules_
by unifying the terminology and API under the global umbrella of _modules_.

Bug fixes
---------

* Fix bug when accessing properties in closures.
* Fix bug when accessing a deep index with a function call.
* Fix bug that sometimes allow assigning to an invalid l-value.
* Fix off-by-one error with `Engine::set_max_call_levels`.

Breaking changes
----------------

* `Engine::load_package` is renamed `Engine::register_global_module` and now must explicitly pass a shared [`Module`].
* `Engine::register_module` is renamed `Engine::register_static_module` and now must explicitly pass a shared [`Module`].
* `Package::get` is renamed `Package::as_shared_module`.
* `Engine::set_module_resolver` now takes a straight module resolver instead of an `Option`. To disable module resolving, use the new `DummyModuleResolver`.

Enhancements
------------

* `Scope` is now `Clone + Hash`.
* `Engine::register_static_module` now supports sub-module paths (e.g. `foo::bar::baz`).
* `Engine::register_custom_operator` now accepts reserved symbols.
* `Engine::register_custom_operator` now returns an error if given a precedence of zero.
* The examples `repl` and `rhai_runner` are moved into `bin` and renamed `rhai-repl` and `rhai-run` respectively.


Version 0.19.8
==============

This version makes it easier to generate documentation for a Rhai code base.

Each function defined in an `AST` can optionally attach _doc-comments_ (which, as in Rust,
are comments prefixed by either `///` or `/**`).  Doc-comments allow third-party tools to
automatically generate documentation for functions defined in a Rhai script.

A new API, `Engine::gen_fn_metadata_to_json` and `Engine::gen_fn_metadata_with_ast_to_json`,
paired with the new `metadata` feature, exports the full list of functions metadata
(including those in an `AST`) as a JSON document.

There are also a sizable number of bug fixes.

Bug fixes
---------

* Unary prefix operators `-`, `+` and `!` now bind correctly when applied to an expression. Previously, `-x.len` is parsed as `(-x).len` which is obviously counter-intuitive.
* Indexing of namespace-qualified variables now work properly, such as `path::to::var[x]`.
* Constants are no longer propagated by the optimizer if shadowed by a non-constant variable.
* A constant passed as the `this` parameter to Rhai functions now throws an error if assigned to.
* Generic type parameter of `Engine::register_iterator` is `IntoIterator` instead of `Iterator`.
* Fixes parsing of block comments ending with `**/` or inner blocks starting with `//*`.

Breaking changes
----------------

* `Engine::on_progress` now takes `u64` instead of `&u64`.
* The closure for `Engine::on_debug` now takes two additional parameters: `source: Option<&str>` and `pos: Position`.
* `AST::iter_functions` now returns `ScriptFnMetadata`.
* The parser function passed to `Engine::register_custom_syntax_raw` now takes an additional parameter containing the _look-ahead_ symbol.

New features
------------

* `AST::iter_functions` now returns `ScriptFnMetadata` which includes, among others, _doc-comments_ for functions prefixed by `///` or `/**`.
* _Doc-comments_ can be enabled/disabled with the new `Engine::set_doc_comments` method.
* A new feature `metadata` is added that pulls in `serde_json` and enables `Engine::gen_fn_metadata_to_json` and `Engine::gen_fn_metadata_with_ast_to_json` which exports the full list of functions metadata (including those inside an `AST`) in JSON format.
* `Engine::on_debug` provides two additional parameters: `source: Option<&str>` and `pos: Position`, containing the current source (if any) and position of the `debug` statement.
* `NativeCallContext` and `EvalContext` both expose `source()` which returns the current source, if any.

Enhancements
------------

* A functions lookup cache is added to make function call resolution faster.
* Capturing a constant variable in a closure is now supported, with no cloning.
* A _look-ahead_ symbol is provided to custom syntax parsers, which can be used to parse variable-length symbol streams.


Version 0.19.7
==============

Bug fixes
---------

* Fixes compilation errors with certain feature flag combinations.

Enhancements
------------

* Property getters/setters and indexers defined in a plugin module are by default `#[rhai_fn(global)]`.
* `to_debug` is a new standard function for converting a value into debug format.
* Arrays and object maps now print values using `to_debug` (if available).


Version 0.19.6
==============

This version adds the `switch` statement.

It also allows exposing selected module functions (usually methods) to the global namespace.
This is very convenient when encapsulating the API of a custom Rust type into a module while having methods
and iterators registered on the custom type work normally.

A new `gen_fn_signatures` API enables enumerating the registered functions of an `Engine` for documentation purposes.
It also prepares the way for a future reflection API.

Bug fixes
---------

* Custom syntax that introduces a shadowing variable now works properly.

Breaking changes
----------------

* `Module::set_fn`, `Module::set_raw_fn` and `Module::set_fn_XXX_mut` all take an additional parameter of `FnNamespace`.
* `Module::set_fn` takes a further parameter with a list of parameter names/types plus the function return type, if any.
* `Module::get_sub_module_mut` is removed.
* `begin`, `end`, `unless` are now reserved keywords.
* `EvalPackage` is removed in favor of `Engine::disable_symbol`.

New features
------------

* New `switch` statement.
* New `do ... while` and `do ... until` statements.
* New `Engine::gen_fn_signatures`, `Module::gen_fn_signatures` and `PackagesCollection::gen_fn_signatures` to generate a list of signatures for functions registered.
* New `Engine::register_static_module` to register a module as a sub-module in the global namespace.
* New `set_exported_global_fn!` macro to register a plugin function and expose it to the global namespace.
* `Module::set_fn_XXX_mut` can expose a module function to the global namespace. This is convenient when registering an API for a custom type.
* `Module::set_getter_fn`, `Module::set_setter_fn`, `Module::set_indexer_get_fn`, `Module::set_indexer_set_fn` all expose the function to the global namespace by default. This is convenient when registering an API for a custom type.
* New `Module::update_fn_metadata` to update a module function's parameter names and types.
* New `#[rhai_fn(global)]` and `#[rhai_fn(internal)]` attributes to determine whether a function defined in a plugin module should be exposed to the global namespace. This is convenient when defining an API for a custom type.
* New `get_fn_metadata_list` to get the metadata of all script-defined functions in scope.

Enhancements
------------

* New constants under `Dynamic` including `UNIT`, `TRUE`, `FALSE`, `ZERO`, `ONE` etc.
* Floating-point numbers ending with a decimal point without a trailing `0` are supported.


Version 0.19.5
==============

This version fixes a bug that prevents compilation with the `internals` feature.
It also speeds up importing modules.

Bug fixes
---------

* Fixes compilation error when using the `internals` feature.  Bug introduced in `0.19.4`.
* Importing script files recursively no longer panics.

Breaking changes
----------------

* Modules imported at global level can now be accessed in functions.
* `ModuleResolver::resolve` now returns `Shared<Module>` for better resources sharing when loading modules.
* `ParseErrorType::DuplicatedExport` is removed as multiple `export`'s are now allowed.

Enhancements
------------

* Modules imported via `import` statements at global level can now be used in functions. There is no longer any need to re-`import` the modules at the beginning of each function block.
* Modules imported via `import` statements are encapsulated into the `AST` when loading a module from a script file.
* `export` keyword can now be tagged onto `let` and `const` statements as a short-hand, e.g.: `export let x = 42;`
* Variables can now be `export`-ed multiple times under different names.
* `index_of`, `==` and `!=` are defined for arrays.
* `==` and `!=` are defined for object maps.


Version 0.19.4
==============

This version basically cleans up the code structure in preparation for a potential `1.0` release in the future.
Most scripts should see a material speed increase.

This version also adds a low-level API for more flexibility when defining custom syntax.

Bug fixes
---------

* Fixes `Send + Sync` for `EvalAltResult` under the `sync` feature. Bug introduced with `0.19.3`.

Breaking changes
----------------

* Custom syntax can no longer start with a keyword (even a _reserved_ one), even if it has been disabled. That is to avoid breaking scripts later when the keyword is no longer disabled.

Changes to Error Handling
------------------------

* `EvalAltResult::ErrorAssignmentToUnknownLHS` is moved to `ParseError::AssignmentToInvalidLHS`. `ParseError::AssignmentToCopy` is removed.
* `EvalAltResult::ErrorDataTooLarge` is simplified.
* `Engine::on_progress` closure signature now returns `Option<Dynamic>` with the termination value passed on to `EvalAltResult::ErrorTerminated`.
* `ParseErrorType::BadInput` now wraps a `LexError` instead of a text string.

New features
------------

* `f32_float` feature to set `FLOAT` to `f32`.
* Low-level API for custom syntax allowing more flexibility in designing the syntax.
* `Module::fill_with` to poly-fill a module with another.
* Scripts terminated via `Engine::on_progress` can now pass on a value as a termination token.

Enhancements
------------

* Essential AST structures like `Expr` and `Stmt` are packed into smaller sizes (16 bytes and 32 bytes on 64-bit), stored inline for more cache friendliness, and de-`Box`ed as much as possible.
* `Scope` is optimized for cache friendliness.


Version 0.19.3
==============

This version streamlines some of the advanced API's, and adds the `try` ... `catch` statement
to catch exceptions.

Breaking changes
----------------

* `EvalAltResult::ErrorReadingScriptFile` is removed in favor of the new `EvalAltResult::ErrorSystem`.
* `EvalAltResult::ErrorLoopBreak` is renamed to `EvalAltResult::LoopBreak`.
* `Engine::register_raw_fn` and `FnPtr::call_dynamic` function signatures have changed.
* Callback signatures to `Engine::on_var` and `Engine::register_custom_syntax` have changed.
* `EvalAltResult::ErrorRuntime` now wraps a `Dynamic` instead of a string.
* Default call stack depth for `debug` builds is reduced to 8 (from 12) because it keeps overflowing the stack in GitHub CI!
* Keyword `thread` is reserved.

New features
------------

* The plugins system is enhanced to support functions taking a `NativeCallContext` as the first parameter.
* `throw` statement can now throw any value instead of just text strings.
* New `try` ... `catch` statement to catch exceptions.

Enhancements
------------

* Calling `eval` or `Fn` in method-call style, which is an error, is now caught during parsing.
* `func!()` call style is valid even under `no_closure` feature.


Version 0.19.2
==============

Bug fix on call module functions.


Version 0.19.1
==============

This version adds a variable resolver with the ability to short-circuit variable access,
plus a whole bunch of array methods.

Breaking changes
----------------

* `AST::iter_functions` now returns an iterator instead of taking a closure.
* `Module::get_script_function_by_signature` renamed to `Module::get_script_fn` and returns `&<Shared<ScriptFnDef> >`.
* `Module::num_fn`, `Module::num_var` and `Module::num_iter` are removed and merged into `Module::count`.
* The `merge_namespaces` parameter to `Module::eval_ast_as_new` is removed and now defaults to `true`.
* `GlobalFileModuleResolver` is removed because its performance gain over the `FileModuleResolver` is no longer very significant.
* The following `EvalAltResult` variants are removed and merged into `EvalAltResult::ErrorMismatchDataType`: `ErrorCharMismatch`, `ErrorNumericIndexExpr`, `ErrorStringIndexExpr`, `ErrorImportExpr`, `ErrorLogicGuard`, `ErrorBooleanArgMismatch`
* `Scope::iter_raw` returns an iterator with an additional field indicating whether the variable is constant or not.
* `rhai::ser` and `rhai::de` namespaces are merged into `rhai::serde`.
* New reserved symbols: `++`, `--`, `..`, `...`.
* Callback signature for custom syntax implementation function is changed to allow for more flexibility.
* Default call stack depth for `debug` builds is reduced to 12 (from 16).
* Precedence for `~` is raised, while `in` is moved below logic comparison operators.

New features
------------

* New `Engine::on_var` to register a _variable resolver_.
* `const` statements can now take any expression (or none at all) instead of only constant values.
* `OptimizationLevel::Simple` now eagerly evaluates built-in binary operators of primary types (if not overloaded).
* `is_def_var()` to detect if variable is defined, and `is_def_fn()` to detect if script function is defined.
* `Dynamic::from(&str)` now constructs a `Dynamic` with a copy of the string as value.
* `AST::combine` and `AST::combine_filtered` allows combining two `AST`'s without creating a new one.
* `map`, `filter`, `reduce`, `reduce_rev`, `some`, `all`, `extract`, `splice`, `chop` and `sort` functions for arrays.
* New `Module::set_iterable` and `Module::set_iterator` to define type iterators more easily. `Engine::register_iterator` is changed to use the simpler version.

Enhancements
------------

* Many one-liners and few-liners are now marked `#[inline]` or `[inline(always)]`, just in case it helps when LTO is not turned on.


Version 0.19.0
==============

The major new feature for this version is _Plugins_ support, powered by procedural macros.
Plugins make it extremely easy to develop and register Rust functions with an `Engine`.

Bug fixes
---------

* `if` statement with an empty `true` block would not evaluate the `false` block.  This is now fixed.
* Fixes a bug in `Module::set_fn_4_mut`.
* Module API's now properly handle `&str` and `String` parameters.
* Indexers are available under `no_object`.
* Registered operator-assignment functions (e.g. `+=`) now work correctly.

Breaking changes
----------------

* `Engine::register_set_result` and `Engine::register_indexer_set_result` now take a function that returns `Result<(), Box<EvalAltResult> >`.
* `Engine::register_indexer_XXX` and `Module::set_indexer_XXX` panic when the type is `Array`, `Map` or `String`.
* `EvalAltResult` has a new variant `ErrorInModule` which holds errors when loading an external module.
* `Module::eval_ast_as_new` now takes an extra boolean parameter, indicating whether to encapsulate the entire module into a separate namespace.
* Functions in `FileModuleResolver` loaded modules now can cross-call each other in addition to functions in the global namespace. For the old behavior, use `MergingFileModuleResolver` instead.
* New `EvalAltResult::ErrorInModule` variant capturing errors when loading a module from a script file.

New features
------------

* Plugins support via procedural macros.
* Scripted functions are allowed in packages.
* `parse_int` and `parse_float` functions for parsing numbers; `split` function for splitting strings.
* `AST::iter_functions` and `Module::iter_script_fn_info` to iterate functions.
* Functions iteration functions for `AST` and `Module` now take `FnMut` instead of `Fn`.
* New `FileModuleResolver` that encapsulates the entire `AST` of the module script, allowing function cross-calling. The old version is renamed `MergingFileModuleResolver`.
* `+` and `-` operators for timestamps to increment/decrement by seconds.


Version 0.18.3
==============

Bug fixes
---------

* `Engine::compile_expression`, `Engine::eval_expression` etc. no longer parse anonymous functions and closures.
* Imported modules now work inside closures.
* Closures that capture now work under `no_object`.

New features
------------

* Adds `Module::combine_flatten` to combine two modules while flattening to the root level.


Version 0.18.2
==============

Bug fixes
---------

* Fixes bug that prevents calling functions in closures.
* Fixes bug that erroneously consumes the first argument to a namespace-qualified function call.

Breaking changes
----------------

* `Module::contains_fn` and `Module::get_script_fn` no longer take the `public_only` parameter.

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
* `Module::set_indexer_get_set_fn` is added as a short-hand of both `Module::set_indexer_get_fn` and `Module::set_indexer_set_fn`.
* New `unicode-xid-ident` feature to allow [Unicode Standard Annex #31](http://www.unicode.org/reports/tr31/) for identifiers.
* `Scope::iter_raw` returns an iterator with a reference to the underlying `Dynamic` value (which may be shared).

Breaking changes
----------------

* Language keywords are now _reserved_ (even when disabled) and they can no longer be used as variable names.
* Function signature for defining custom syntax is simplified.
* `Engine::register_raw_fn_XXX` API shortcuts are removed.
* `PackagesCollection::get_fn`, `PackagesCollection::contains_fn`, `Module::get_fn` and `Module::contains_fn` now take an additional `public_only` parameter indicating whether only public functions are accepted.
* The iterator returned by `Scope::iter` now contains a clone of the `Dynamic` value (unshared).
* `Engine::register_global_module` takes any type that is `Into<PackageLibrary>`.
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
* Precedence of the `%` (modulo) operator is lowered to below bit shifts. This is to handle the case of `x < < 3 % 10`.

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
* It is now possible to mutate the first argument of a namespace-qualified function call when the argument is a simple variable (but not a module constant).
* Many configuration/setting API's now returns `&mut Self` so that the calls can be chained.
* `String` parameters in functions are supported (but inefficiently).


Version 0.16.1
==============

Bug fix release to fix errors when compiling with features.


Version 0.16.0
==============

The major new feature in this version is OOP - well, poor man's OOP, that is.

The `README` is officially transferred to [The Rhai Book](https://rhai.rs/book).

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

* [The Rhai Book](https://rhai.rs/book) is online.  Most content in the original `README` was transferred to the Book.
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

* Indexers are now split into getters and setters (which now support updates).  The API is split into `Engine::register_indexer_get` and `Engine::register_indexer_set` with `Engine::register_indexer_get_set` being a short-hand.  Similarly, `Module::set_indexer_get_fn` and `Module::set_indexer_set_fn` are added.
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
* The `RegisterDynamicFn` trait is merged into the `RegisterResultFn` trait which now always returns `RhaiResult`.
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
