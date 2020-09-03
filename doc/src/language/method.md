Call Method as Function
======================

{{#include ../links.md}}


First `&mut` Parameter
----------------------

Property [getters/setters] and [methods][custom types] in a Rust custom type registered with the [`Engine`] can be called
just like a regular function.  In fact, like Rust, property getters/setters and object methods
are registered as regular [functions] in Rhai that take a first `&mut` parameter.

Unlike functions defined in script (for which all arguments are passed by _value_),
native Rust functions may mutate the object (or the first argument if called in normal function call style).

However, sometimes it is not as straight-forward, and methods called in function-call style may end up
not muting the object - see the example below. Therefore, it is best to always use method-call style.

Custom types, properties and methods can be disabled via the [`no_object`] feature.

```rust
let a = new_ts();   // constructor function
a.field = 500;      // property setter
a.update();         // method call, 'a' can be modified

update(a);          // <- this de-sugars to 'a.update()' thus if 'a' is a simple variable
                    //    unlike scripted functions, 'a' can be modified and is not a copy

let array = [ a ];

update(array[0]);   // <- 'array[0]' is an expression returning a calculated value,
                    //    a transient (i.e. a copy), so this statement has no effect
                    //    except waste a lot of time cloning

array[0].update();  // <- call in method-call style will update 'a'
```

**IMPORTANT: Rhai does NOT support normal references (i.e. `&T`) as parameters.**


Number of Parameters
--------------------

Native Rust methods registered with an [`Engine`] take _one additional parameter_ more than
an equivalent method coded in script, where the object is accessed via the `this` pointer instead.

The following table illustrates the differences:

| Function type | Parameters |    Object reference    |                   Function signature                    |
| :-----------: | :--------: | :--------------------: | :-----------------------------------------------------: |
|  Native Rust  |  _n_ + 1   | First `&mut` parameter | `fn method<T, U, V>`<br/>`(obj: &mut T, x: U, y: V) {}` |
|  Rhai script  |    _n_     |         `this`         |                  `fn method(x, y) {}`                   |


`&mut` is Efficient, Except for `ImmutableString`
-----------------------------------------------

Using a `&mut` first parameter is highly encouraged when using types that are expensive to clone,
even when the intention is not to mutate that argument, because it avoids cloning that argument value.

For example, the `len` method of an [array] has the signature: `Fn(&mut Array) -> INT`.
The array itself is not modified in any way, but using a `&mut` parameter avoids a cloning that would
otherwise have happened if the signature were `Fn(Array) -> INT`.

For primary types that are cheap to clone (e.g. those that implement `Copy`), including `ImmutableString`,
this is not necessary.
