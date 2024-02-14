Rhai - Embedded Scripting for Rust
==================================

![GitHub last commit](https://img.shields.io/github/last-commit/rhaiscript/rhai?logo=github)
[![Build Status](https://github.com/rhaiscript/rhai/workflows/Build/badge.svg)](https://github.com/rhaiscript/rhai/actions)
[![Stars](https://img.shields.io/github/stars/rhaiscript/rhai?style=flat&logo=github)](https://github.com/rhaiscript/rhai)
[![License](https://img.shields.io/crates/l/rhai)](https://github.com/license/rhaiscript/rhai)
[![crates.io](https://img.shields.io/crates/v/rhai?logo=rust)](https://crates.io/crates/rhai/)
[![crates.io](https://img.shields.io/crates/d/rhai?logo=rust)](https://crates.io/crates/rhai/)
[![API Docs](https://docs.rs/rhai/badge.svg?logo=docs-rs)](https://docs.rs/rhai/)
[![VS Code plugin installs](https://img.shields.io/visual-studio-marketplace/i/rhaiscript.vscode-rhai?logo=visual-studio-code&label=vs%20code)](https://marketplace.visualstudio.com/items?itemName=rhaiscript.vscode-rhai)
[![Sublime Text package downloads](https://img.shields.io/packagecontrol/dt/Rhai.svg?logo=sublime-text&label=sublime%20text)](https://packagecontrol.io/packages/Rhai)
[![Discord Chat](https://img.shields.io/discord/767611025456889857.svg?logo=discord&label=discord)](https://discord.gg/HquqbYFcZ9)
[![Zulip Chat](https://img.shields.io/badge/zulip-join_chat-brightgreen.svg?logo=zulip)](https://rhaiscript.zulipchat.com)
[![Reddit Channel](https://img.shields.io/reddit/subreddit-subscribers/Rhai?logo=reddit&label=reddit)](https://www.reddit.com/r/Rhai)

[![Rhai logo](https://rhai.rs/book/images/logo/rhai-banner-transparent-colour.svg)](https://rhai.rs)

Rhai is an embedded scripting language and evaluation engine for Rust that gives a safe and easy way
to add scripting to any application.


Targets and builds
------------------

* All CPU and O/S targets supported by Rust, including:
  * WebAssembly (WASM)
  * `no-std`
* Minimum Rust version 1.66.0


Standard features
-----------------

* Simple language similar to JavaScript+Rust with [dynamic](https://rhai.rs/book/language/dynamic.html) typing.
* Fairly efficient evaluation (1 million iterations in 0.14 sec on a single-core 2.6 GHz Linux VM).
* Tight integration with native Rust [functions](https://rhai.rs/book/rust/functions.html) and [types](https://rhai.rs/book/rust/custom-types.html), including [getters/setters](https://rhai.rs/book/rust/getters-setters.html), [methods](https://rhai.rs/book/rust/methods.html) and [indexers](https://rhai.rs/book/rust/indexers.html).
* Freely pass Rust values into a script as [variables](https://rhai.rs/book/language/variables.html)/[constants](https://rhai.rs/book/language/constants.html) via an external [`Scope`](https://rhai.rs/book/engine/scope.html) - all clonable Rust types are supported; no need to implement any special trait. Or tap directly into the [variable resolution process](https://rhai.rs/book/engine/var.html).
* Built-in support for most common [data types](https://rhai.rs/book/language/values-and-types.html) including booleans, [integers](https://rhai.rs/book/language/numbers.html), [floating-point numbers](https://rhai.rs/book/language/numbers.html) (including [`Decimal`](https://crates.io/crates/rust_decimal)), [strings](https://rhai.rs/book/language/strings-chars.html), [Unicode characters](https://rhai.rs/book/language/strings-chars.html), [arrays](https://rhai.rs/book/language/arrays.html) (including packed [byte arrays](https://rhai.rs/book/language/blobs.html)) and [object maps](https://rhai.rs/book/language/object-maps.html).
* Easily [call a script-defined function](https://rhai.rs/book/engine/call-fn.html) from Rust.
* Relatively little `unsafe` code (yes there are some for performance reasons).
* Few dependencies - currently only [`smallvec`](https://crates.io/crates/smallvec), [`thin-vec`](https://crates.io/crates/thin-vec), [`num-traits`](https://crates.io/crates/num-traits), [`once_cell`](https://crates.io/crates/once_cell), [`ahash`](https://crates.io/crates/ahash), [`bitflags`](https://crates.io/crates/bitflags) and [`smartstring`](https://crates.io/crates/smartstring).
* Re-entrant scripting engine can be made `Send + Sync` (via the `sync` feature).
* Compile once to [AST](https://rhai.rs/book/engine/compile.html) form for repeated evaluations.
* Scripts are [optimized](https://rhai.rs/book/engine/optimize) (useful for template-based machine-generated scripts).
* Easy custom API development via [plugins](https://rhai.rs/book/plugins) system powered by procedural macros.
* [Function overloading](https://rhai.rs/book/language/overload.html) and [operator overloading](https://rhai.rs/book/rust/operators.html).
* Dynamic dispatch via [function pointers](https://rhai.rs/book/language/fn-ptr.html) with additional support for [currying](https://rhai.rs/book/language/fn-curry.html).
* [Closures](https://rhai.rs/book/language/fn-closure.html) (anonymous functions) that can capture shared values.
* Some syntactic support for [object-oriented programming (OOP)](https://rhai.rs/book/patterns/oop.html).
* Organize code base with dynamically-loadable [modules](https://rhai.rs/book/language/modules), optionally [overriding the resolution process](https://rhai.rs/book/rust/modules/resolvers.html).
* Serialization/deserialization support via [serde](https://crates.io/crates/serde) (requires the `serde` feature).
* Support for [minimal builds](https://rhai.rs/book/start/builds/minimal.html) by excluding unneeded language [features](https://rhai.rs/book/start/features.html).
* A [debugging](https://rhai.rs/book/engine/debugging) interface.


Protected against attacks
-------------------------

* [_Don't Panic_](https://rhai.rs/book/safety/index.html#dont-panic-guarantee--any-panic-is-a-bug) guarantee - Any panic is a bug. Rhai subscribes to the motto that a library should never panic the host system, and is coded with this in mind.
* [Sand-boxed](https://rhai.rs/book/safety/sandbox.html) - the scripting engine, if declared immutable, cannot mutate the containing environment unless [explicitly permitted](https://rhai.rs/book/patterns/control.html).
* Rugged - protected against malicious attacks (such as [stack-overflow](https://rhai.rs/book/safety/max-call-stack.html), [over-sized data](https://rhai.rs/book/safety/max-string-size.html), and [runaway scripts](https://rhai.rs/book/safety/max-operations.html) etc.) that may come from untrusted third-party user-land scripts.
* Track script evaluation [progress](https://rhai.rs/book/safety/progress.html) and manually terminate a script run.
* Passes Miri.


For those who actually want their own language
----------------------------------------------

* Use as a [DSL](https://rhai.rs/book/engine/dsl.html).
* Disable certain [language features](https://rhai.rs/book/engine/options.html#language-features) such as [looping](https://rhai.rs/book/engine/disable-looping.html).
* Further restrict the language by surgically [disabling keywords and operators](https://rhai.rs/book/engine/disable-keywords.html).
* Define [custom operators](https://rhai.rs/book/engine/custom-op.html).
* Extend the language with [custom syntax](https://rhai.rs/book/engine/custom-syntax.html).


Example
-------

The [`scripts`](https://github.com/rhaiscript/rhai/tree/master/scripts) subdirectory contains sample Rhai scripts.

Below is the standard _Fibonacci_ example for scripting languages:

```ts
// This Rhai script calculates the n-th Fibonacci number using a
// really dumb algorithm to test the speed of the scripting engine.

const TARGET = 28;
const REPEAT = 5;
const ANSWER = 317_811;

fn fib(n) {
    if n < 2 {
        n
    } else {
        fib(n-1) + fib(n-2)
    }
}

print(`Running Fibonacci(${TARGET}) x ${REPEAT} times...`);
print("Ready... Go!");

let result;
let now = timestamp();

for n in 0..REPEAT {
    result = fib(TARGET);
}

print(`Finished. Run time = ${now.elapsed} seconds.`);

print(`Fibonacci number #${TARGET} = ${result}`);

if result != ANSWER {
    print(`The answer is WRONG! Should be ${ANSWER}!`);
}
```

Project Site
------------

[`rhai.rs`](https://rhai.rs)


Documentation
-------------

See [_The Rhai Book_](https://rhai.rs/book) for details on the Rhai scripting engine and language.


Playground
----------

An [_Online Playground_](https://rhai.rs/playground) is available with syntax-highlighting editor,
powered by WebAssembly.

Scripts can be evaluated directly from the editor.


License
-------

Licensed under either of the following, at your choice:

* [Apache License, Version 2.0](https://github.com/rhaiscript/rhai/blob/master/LICENSE-APACHE.txt), or
* [MIT license](https://github.com/rhaiscript/rhai/blob/master/LICENSE-MIT.txt)

Unless explicitly stated otherwise, any contribution intentionally submitted
for inclusion in this crate, as defined in the Apache-2.0 license, shall
be dual-licensed as above, without any additional terms or conditions.
