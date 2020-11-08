Use the Low-Level API to Register a Rust Function
================================================

{{#include ../links.md}}

When a native Rust function is registered with an `Engine` using the `Engine::register_XXX` API,
Rhai transparently converts all function arguments from [`Dynamic`] into the correct types before
calling the function.

For more power and flexibility, there is a _low-level_ API to work directly with [`Dynamic`] values
without the conversions.


Raw Function Registration
-------------------------

The `Engine::register_raw_fn` method is marked _volatile_, meaning that it may be changed without warning.

If this is acceptable, then using this method to register a Rust function opens up more opportunities.

In particular, a the current _native call context_ (in form of the `NativeCallContext` type) is passed as an argument.
`NativeCallContext` exposes the current [`Engine`], among others, so the Rust function can also use [`Engine`] facilities
(such as evaluating a script).

```rust
engine.register_raw_fn(
    "increment_by",                                         // function name
    &[                                                      // a slice containing parameter types
        std::any::TypeId::of::<i64>(),                      // type of first parameter
        std::any::TypeId::of::<i64>()                       // type of second parameter
    ],
    |context, args| {                                       // fixed function signature
        // Arguments are guaranteed to be correct in number and of the correct types.

        // But remember this is Rust, so you can keep only one mutable reference at any one time!
        // Therefore, get a '&mut' reference to the first argument _last_.
        // Alternatively, use `args.split_first_mut()` etc. to split the slice first.

        let y = *args[1].read_lock::<i64>().unwrap();       // get a reference to the second argument
                                                            // then copy it because it is a primary type

        let y = std::mem::take(args[1]).cast::<i64>();      // alternatively, directly 'consume' it

        let x = args[0].write_lock::<i64>().unwrap();       // get a '&mut' reference to the first argument

        *x += y;                                            // perform the action

        Ok(().into())                                       // must be 'Result<Dynamic, Box<EvalAltResult>>'
    }
);

// The above is the same as (in fact, internally they are equivalent):

engine.register_fn("increment_by", |x: &mut i64, y: i64| *x += y);
```


Function Signature
------------------

The function signature passed to `Engine::register_raw_fn` takes the following form:

> `Fn(context: NativeCallContext, args: &mut [&mut Dynamic])`  
> `-> Result<T, Box<EvalAltResult>> + 'static`

where:

| Parameter                     |              Type               | Description                                                                                                                                                                                                                                |
| ----------------------------- | :-----------------------------: | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `T`                           |          `impl Clone`           | return type of the function                                                                                                                                                                                                                |
| `context`                     |       `NativeCallContext`       | the current _native call context_                                                                                                                                                                                                          |
| - `context.engine()`          |            `&Engine`            | the current [`Engine`], with all configurations and settings.<br/>This is sometimes useful for calling a script-defined function within the same evaluation context using [`Engine::call_fn`][`call_fn`], or calling a [function pointer]. |
| - `context.imports()`         |       `Option<&Imports>`        | reference to the current stack of [modules] imported via `import` statements (if any)                                                                                                                                                      |
| - `context.iter_namespaces()` | `impl Iterator<Item = &Module>` | iterator of the namespaces (as [modules]) containing all script-defined functions                                                                                                                                                          |
| `args`                        |      `&mut [&mut Dynamic]`      | a slice containing `&mut` references to [`Dynamic`] values.<br/>The slice is guaranteed to contain enough arguments _of the correct types_.                                                                                                |

### Return value

The return value is the result of the function call.

Remember, in Rhai, all arguments _except_ the _first_ one are always passed by _value_ (i.e. cloned).
Therefore, it is unnecessary to ever mutate any argument except the first one, as all mutations
will be on the cloned copy.


Extract Arguments
-----------------

To extract an argument from the `args` parameter (`&mut [&mut Dynamic]`), use the following:

| Argument type                  | Access (`n` = argument position)      | Result                                                |
| ------------------------------ | ------------------------------------- | ----------------------------------------------------- |
| [Primary type][standard types] | `args[n].clone().cast::<T>()`         | copy of value                                         |
| [Custom type]                  | `args[n].read_lock::<T>().unwrap()`   | immutable reference to value                          |
| [Custom type] (consumed)       | `std::mem::take(args[n]).cast::<T>()` | the _consumed_ value; the original value becomes `()` |
| `this` object                  | `args[0].write_lock::<T>().unwrap()`  | mutable reference to value                            |

When there is a mutable reference to the `this` object (i.e. the first argument),
there can be no other immutable references to `args`, otherwise the Rust borrow checker will complain.


Example - Passing a Callback to a Rust Function
----------------------------------------------

The low-level API is useful when there is a need to interact with the scripting [`Engine`]
within a function.

The following example registers a function that takes a [function pointer] as an argument,
then calls it within the same [`Engine`].  This way, a _callback_ function can be provided
to a native Rust function.

```rust
use rhai::{Engine, FnPtr};

let mut engine = Engine::new();

// Register a Rust function
engine.register_raw_fn(
    "bar",
    &[
        std::any::TypeId::of::<i64>(),                      // parameter types
        std::any::TypeId::of::<FnPtr>(),
        std::any::TypeId::of::<i64>(),
    ],
    |context, args| {
        // 'args' is guaranteed to contain enough arguments of the correct types

        let fp = std::mem::take(args[1]).cast::<FnPtr>();   // 2nd argument - function pointer
        let value = args[2].clone();                        // 3rd argument - function argument
        let this_ptr = args.get_mut(0).unwrap();            // 1st argument - this pointer

        // Use 'FnPtr::call_dynamic' to call the function pointer.
        // Beware, private script-defined functions will not be found.
        fp.call_dynamic(context, Some(this_ptr), [value])
    },
);

let result = engine.eval::<i64>(
          r#"
                fn foo(x) { this += x; }    // script-defined function 'foo'

                let x = 41;                 // object
                x.bar(Fn("foo"), 1);        // pass 'foo' as function pointer
                x
          "#)?;
```


TL;DR - Why `read_lock` and `write_lock`
---------------------------------------

The `Dynamic` API that casts it to a reference to a particular data type  is `read_lock`
(for an immutable reference) and `write_lock` (for a mutable reference).

As the naming shows, something is _locked_ in order to allow this access, and that something
is a _shared value_ created by [capturing][automatic currying] variables from [closures].

Shared values are implemented as `Rc<RefCell<Dynamic>>` (`Arc<RwLock<Dynamic>>` under [`sync`]).

If the value is _not_ a shared value, or if running under [`no_closure`] where there is
no [capturing][automatic currying], this API de-sugars to a simple `Dynamic::downcast_ref` and
`Dynamic::downcast_mut`.  In other words, there is no locking and reference counting overhead
for the vast majority of non-shared values.

If the value is a shared value, then it is first locked and the returned lock guard
then allows access to the underlying value in the specified type.


Hold Multiple References
------------------------

In order to access a value argument that is expensive to clone _while_ holding a mutable reference
to the first argument, either _consume_ that argument via `mem::take` as above, or use `args.split_first`
to partition the slice:

```rust
// Partition the slice
let (first, rest) = args.split_first_mut().unwrap();

// Mutable reference to the first parameter
let this_ptr = &mut *first.write_lock::<A>().unwrap();

// Immutable reference to the second value parameter
// This can be mutable but there is no point because the parameter is passed by value
let value_ref = &*rest[0].read_lock::<B>().unwrap();
```
