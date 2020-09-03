Multiple Instantiation
======================

{{#include ../links.md}}


Background
----------

Rhai's [features] are not strictly additive.  This is easily deduced from the [`no_std`] feature
which prepares the crate for `no-std` builds.  Obviously, turning on this feature has a material
impact on how Rhai behaves.

Many crates resolve this by going the opposite direction: build for `no-std` in default,
but add a `std` feature, included by default, which builds for the `stdlib`.


Rhai Language Features Are Not Additive
--------------------------------------

Rhai, however, is more complex.  Language features cannot be easily made _additive_.

That is because the _lack_ of a language feature is a feature by itself.

For example, by including [`no_float`], a project sets the Rhai language to ignore floating-point math.
Floating-point numbers do not even parse under this case and will generate syntax errors.
Assume that the project expects this behavior (why? perhaps integers are all that make sense
within the project domain).

Now, assume that a dependent crate also depends on Rhai. Under such circumstances,
unless _exact_ versioning is used and the dependent crate depends on a _different_ version
of Rhai, Cargo automatically _merges_ both dependencies, with the [`no_float`] feature turned on
because Cargo features are _additive_.

This will break the dependent crate, which does not by itself specify [`no_float`]
and expects floating-point numbers and math to work normally.

There is no way out of this dilemma.  Reversing the [features] set with a `float` feature
causes the project to break because floating-point numbers are not rejected as expected.


Multiple Instantiations of Rhai Within The Same Project
------------------------------------------------------

The trick is to differentiate between multiple identical copies of Rhai, each having
a different [features] set, by their _sources_:

* Different versions from [`crates.io`](https://crates.io/crates/rhai/) - The official crate.

* Different releases from [`GitHub`](https://github.com/jonathandturner/rhai) - Crate source on GitHub.

* Forked copy of [https://github.com/jonathandturner/rhai](https://github.com/jonathandturner/rhai) on GitHub.

* Local copy of [https://github.com/jonathandturner/rhai](https://github.com/jonathandturner/rhai) downloaded form GitHub.

Use the following configuration in `Cargo.toml` to pull in multiple copies of Rhai within the same project:

```toml
[dependencies]
rhai = { version = "{{version}}", features = [ "no_float" ] }
rhai_github = { git = "https://github.com/jonathandturner/rhai", features = [ "unchecked" ] }
rhai_my_github = { git = "https://github.com/my_github/rhai", branch = "variation1", features = [ "serde", "no_closure" ] }
rhai_local = { path = "../rhai_copy" }
```

The example above creates four different modules: `rhai`, `rhai_github`, `rhai_my_github` and
`rhai_local`, each referring to a different Rhai copy with the appropriate [features] set.

Only one crate of any particular version can be used from each source, because Cargo merges
all candidate cases within the same source, adding all [features] together.

If more than four different instantiations of Rhai is necessary (why?), create more local repositories
or GitHub forks or branches.


Caveat - No Way To Avoid Dependency Conflicts
--------------------------------------------

Unfortunately, pulling in Rhai from different sources do not resolve the problem of
[features] conflict between dependencies.  Even overriding `crates.io` via the `[patch]` manifest
section doesn't work - all dependencies will eventually find the only one copy.

What is necessary - multiple copies of Rhai, one for each dependent crate that requires it,
together with their _unique_ [features] set intact.  In other words, turning off Cargo's
crate merging feature _just for Rhai_.

Unfortunately, as of this writing, there is no known method to achieve it.

Therefore, moral of the story: avoid pulling in multiple crates that depend on Rhai.
