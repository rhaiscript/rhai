Function Pointers
=================

{{#include ../links.md}}

It is possible to store a _function pointer_ in a variable just like a normal value.
In fact, internally a function pointer simply stores the _name_ of the function as a string.

A function pointer is created via the `Fn` function, which takes a [string] parameter.

Call a function pointer using the `call` method.


Built-in methods
----------------

The following standard methods (mostly defined in the [`BasicFnPackage`][packages] but excluded if
using a [raw `Engine`]) operate on function pointers:

| Function                           | Parameter(s) | Description                                                                                      |
| ---------------------------------- | ------------ | ------------------------------------------------------------------------------------------------ |
| `name` method and property         | _none_       | returns the name of the function encapsulated by the function pointer                            |
| `is_anonymous` method and property | _none_       | does the function pointer refer to an [anonymous function]? Not available under [`no_function`]. |
| `call`                             | _arguments_  | calls the function matching the function pointer's name with the _arguments_                     |


Examples
--------

```rust
fn foo(x) { 41 + x }

let func = Fn("foo");       // use the 'Fn' function to create a function pointer

print(func);                // prints 'Fn(foo)'

let func = fn_name.Fn();    // <- error: 'Fn' cannot be called in method-call style

func.type_of() == "Fn";     // type_of() as function pointer is 'Fn'

func.name == "foo";

func.call(1) == 42;         // call a function pointer with the 'call' method

foo(1) == 42;               // <- the above de-sugars to this

call(func, 1);              // normal function call style also works for 'call'

let len = Fn("len");        // 'Fn' also works with registered native Rust functions

len.call("hello") == 5;

let add = Fn("+");          // 'Fn' works with built-in operators also

add.call(40, 2) == 42;

let fn_name = "hello";      // the function name does not have to exist yet

let hello = Fn(fn_name + "_world");

hello.call(0);              // error: function not found - 'hello_world (i64)'
```


Global Namespace Only
--------------------

Because of their dynamic nature, function pointers cannot refer to functions in [`import`]-ed [modules].
They can only refer to functions within the global [namespace][function namespace].
See _[Function Namespaces]_ for more details.

```rust
import "foo" as f;          // assume there is 'f::do_work()'

f::do_work();               // works!

let p = Fn("f::do_work");   // error: invalid function name

fn do_work_now() {          // call it from a local function
    f::do_work();
}

let p = Fn("do_work_now");

p.call();                   // works!
```


Dynamic Dispatch
----------------

The purpose of function pointers is to enable rudimentary _dynamic dispatch_, meaning to determine,
at runtime, which function to call among a group.

Although it is possible to simulate dynamic dispatch via a number and a large `if-then-else-if` statement,
using function pointers significantly simplifies the code.

```rust
let x = some_calculation();

// These are the functions to call depending on the value of 'x'
fn method1(x) { ... }
fn method2(x) { ... }
fn method3(x) { ... }

// Traditional - using decision variable
let func = sign(x);

// Dispatch with if-statement
if func == -1 {
    method1(42);
} else if func == 0 {
    method2(42);
} else if func == 1 {
    method3(42);
}

// Using pure function pointer
let func = if x < 0 {
    Fn("method1")
} else if x == 0 {
    Fn("method2")
} else if x > 0 {
    Fn("method3")
}

// Dynamic dispatch
func.call(42);

// Using functions map
let map = [ Fn("method1"), Fn("method2"), Fn("method3") ];

let func = sign(x) + 1;

// Dynamic dispatch
map[func].call(42);
```


Bind the `this` Pointer
----------------------

When `call` is called as a _method_ but not on a function pointer, it is possible to dynamically dispatch
to a function call while binding the object in the method call to the `this` pointer of the function.

To achieve this, pass the function pointer as the _first_ argument to `call`:

```rust
fn add(x) {                 // define function which uses 'this'
    this += x;
}

let func = Fn("add");       // function pointer to 'add'

func.call(1);               // error: 'this' pointer is not bound

let x = 41;

func.call(x, 1);            // error: function 'add (i64, i64)' not found

call(func, x, 1);           // error: function 'add (i64, i64)' not found

x.call(func, 1);            // 'this' is bound to 'x', dispatched to 'func'

x == 42;
```

Beware that this only works for _method-call_ style.  Normal function-call style cannot bind
the `this` pointer (for syntactic reasons).

Therefore, obviously, binding the `this` pointer is unsupported under [`no_object`].


Call a Function Pointer in Rust
------------------------------

It is completely normal to register a Rust function with an [`Engine`] that takes parameters
whose types are function pointers.  The Rust type in question is `rhai::FnPtr`.

A function pointer in Rhai is essentially syntactic sugar wrapping the _name_ of a function
to call in script.  Therefore, the script's [`AST`] is required to call a function pointer,
as well as the entire _execution context_ that the script is running in.

For a rust function taking a function pointer as parameter, the [Low-Level API](../rust/register-raw.md)
must be used to register the function.

Essentially, use the low-level `Engine::register_raw_fn` method to register the function.
`FnPtr::call_dynamic` is used to actually call the function pointer, passing to it the
current _native call context_, the `this` pointer, and other necessary arguments.

```rust
use rhai::{Engine, Module, Dynamic, FnPtr, NativeCallContext};

let mut engine = Engine::new();

// Define Rust function in required low-level API signature
fn call_fn_ptr_with_value(context: NativeCallContext, args: &mut [&mut Dynamic])
    -> Result<Dynamic, Box<EvalAltResult>>
{
    // 'args' is guaranteed to contain enough arguments of the correct types
    let fp = std::mem::take(args[1]).cast::<FnPtr>();   // 2nd argument - function pointer
    let value = args[2].clone();                        // 3rd argument - function argument
    let this_ptr = args.get_mut(0).unwrap();            // 1st argument - this pointer

    // Use 'FnPtr::call_dynamic' to call the function pointer.
    // Beware, private script-defined functions will not be found.
    fp.call_dynamic(context, Some(this_ptr), [value])
}

// Register a Rust function using the low-level API
engine.register_raw_fn("super_call",
    &[ // parameter types
        std::any::TypeId::of::<i64>(),
        std::any::TypeId::of::<FnPtr>(),
        std::any::TypeId::of::<i64>()
    ],
    call_fn_ptr_with_value
);
```


`NativeCallContext`
------------------

`FnPtr::call_dynamic` takes a parameter of type `NativeCallContext` which holds the _native call context_
of the particular call to a registered Rust function. It is a type that exposes the following:

| Field               |              Type               | Description                                                                                                                                                                                                                                |
| ------------------- | :-----------------------------: | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `engine()`          |            `&Engine`            | the current [`Engine`], with all configurations and settings.<br/>This is sometimes useful for calling a script-defined function within the same evaluation context using [`Engine::call_fn`][`call_fn`], or calling a [function pointer]. |
| `imports()`         |       `Option<&Imports>`        | reference to the current stack of [modules] imported via `import` statements (if any)                                                                                                                                                      |
| `iter_namespaces()` | `impl Iterator<Item = &Module>` | iterator of the namespaces (as [modules]) containing all script-defined functions                                                                                                                                                          |


This type is normally provided by the [`Engine`] (e.g. when using [`Engine::register_fn_raw`](../rust/register-raw.md)).
However, it may also be manually constructed from a tuple:

```rust
use rhai::{Engine, FnPtr};

let engine = Engine::new();

// Compile script to AST
let mut ast = engine.compile(
    r#"
        let test = "hello";
        |x| test + x            // this creates an closure
    "#,
)?;

// Save the closure together with captured variables
let fn_ptr = engine.eval_ast::<FnPtr>(&ast)?;

// Get rid of the script, retaining only functions
ast.retain_functions(|_, _, _| true);

// Create native call context via a tuple
let context =
        (
            &engine,            // the 'Engine'
            &[ast.as_ref()]     // function namespace from the 'AST'
                                // as a one-element slice
        ).into();

// 'f' captures: the engine, the AST, and the closure
let f = move |x: i64| fn_ptr.call_dynamic(context, None, [x.into()]);

// 'f' can be called like a normal function
let result = f(42)?;
```
