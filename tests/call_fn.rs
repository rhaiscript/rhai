#![cfg(not(feature = "no_function"))]
use rhai::{CallFnOptions, Dynamic, Engine, EvalAltResult, FnPtr, Func, FuncArgs, Scope, AST, INT};
use std::any::TypeId;

#[test]
fn test_call_fn() {
    let engine = Engine::new();
    let mut scope = Scope::new();

    scope.push("foo", 42 as INT);

    let ast = engine
        .compile(
            "
                fn hello(x, y) {
                    x + y
                }
                fn hello(x) {
                    x *= foo;
                    foo = 1;
                    x
                }
                fn hello() {
                    41 + foo
                }
                fn define_var(scale) {
                    let bar = 21;
                    bar * scale
                }
            ",
        )
        .unwrap();

    let r = engine.call_fn::<INT>(&mut scope, &ast, "hello", (42 as INT, 123 as INT)).unwrap();
    assert_eq!(r, 165);

    let r = engine.call_fn::<INT>(&mut scope, &ast, "hello", (123 as INT,)).unwrap();
    assert_eq!(r, 5166);

    let r = engine.call_fn::<INT>(&mut scope, &ast, "hello", ()).unwrap();
    assert_eq!(r, 42);

    assert_eq!(scope.get_value::<INT>("foo").expect("variable foo should exist"), 1);

    let r = engine.call_fn::<INT>(&mut scope, &ast, "define_var", (2 as INT,)).unwrap();
    assert_eq!(r, 42);

    assert!(!scope.contains("bar"));

    let options = CallFnOptions::new().eval_ast(false).rewind_scope(false);

    let r = engine.call_fn_with_options::<INT>(options, &mut scope, &ast, "define_var", (2 as INT,)).unwrap();
    assert_eq!(r, 42);

    assert_eq!(scope.get_value::<INT>("bar").expect("variable bar should exist"), 21);

    assert!(!scope.contains("scale"));
}

#[test]
fn test_call_fn_scope() {
    let engine = Engine::new();
    let mut scope = Scope::new();

    let ast = engine
        .compile(
            "
                fn foo(x) {
                    let hello = 42;
                    bar + hello + x
                }

                let bar = 123;
            ",
        )
        .unwrap();

    for _ in 0..50 {
        assert_eq!(
            engine
                .call_fn_with_options::<INT>(CallFnOptions::new().rewind_scope(false), &mut scope, &ast, "foo", [Dynamic::THREE],)
                .unwrap(),
            168
        );
    }

    assert_eq!(scope.len(), 100);
}

struct Options {
    pub foo: bool,
    pub bar: String,
    pub baz: INT,
}

impl FuncArgs for Options {
    fn parse<C: Extend<Dynamic>>(self, container: &mut C) {
        container.extend(Some(self.foo.into()));
        container.extend(Some(self.bar.into()));
        container.extend(Some(self.baz.into()));
    }
}

#[test]
fn test_call_fn_args() {
    let options = Options { foo: false, bar: "world".to_string(), baz: 42 };

    let engine = Engine::new();
    let mut scope = Scope::new();

    let ast = engine
        .compile(
            "
                fn hello(x, y, z) {
                    if x { `hello ${y}` } else { y + z }
                }
            ",
        )
        .unwrap();

    let result = engine.call_fn::<String>(&mut scope, &ast, "hello", options).unwrap();

    assert_eq!(result, "world42");
}

#[test]
fn test_call_fn_private() {
    let engine = Engine::new();
    let mut scope = Scope::new();

    let ast = engine.compile("fn add(x, n) { x + n }").unwrap();

    let r = engine.call_fn::<INT>(&mut scope, &ast, "add", (40 as INT, 2 as INT)).unwrap();
    assert_eq!(r, 42);

    let ast = engine.compile("private fn add(x, n, ) { x + n }").unwrap();

    let r = engine.call_fn::<INT>(&mut scope, &ast, "add", (40 as INT, 2 as INT)).unwrap();
    assert_eq!(r, 42);
}

#[test]
#[cfg(not(feature = "no_object"))]
fn test_fn_ptr_raw() {
    let mut engine = Engine::new();

    engine
        .register_fn("mul", |x: &mut INT, y: INT| *x *= y)
        .register_raw_fn("bar", [TypeId::of::<INT>(), TypeId::of::<FnPtr>(), TypeId::of::<INT>()], move |context, args| {
            let fp = args[1].take().cast::<FnPtr>();
            let value = args[2].clone();
            let this_ptr = args.get_mut(0).unwrap();

            fp.call_raw(&context, Some(this_ptr), [value])
        });

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    fn foo(x) { this += x; }

                    let x = 41;
                    x.bar(foo, 1);
                    x
                "#
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    fn foo(x, y) { this += x + y; }

                    let x = 40;
                    let v = 1;
                    x.bar(Fn("foo").curry(v), 1);
                    x
                "#
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    private fn foo(x) { this += x; }

                    let x = 41;
                    x.bar(Fn("foo"), 1);
                    x
                "#
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let x = 21;
                    x.bar(Fn("mul"), 2);
                    x
                "#
            )
            .unwrap(),
        42
    );
}

#[test]
fn test_anonymous_fn() {
    let calc_func = Func::<(INT, INT, INT), INT>::create_from_script(Engine::new(), "fn calc(x, y, z,) { (x + y) * z }", "calc").unwrap();
    assert_eq!(calc_func(42, 123, 9).unwrap(), 1485);

    let calc_func = Func::<(INT, String, INT), INT>::create_from_script(Engine::new(), "fn calc(x, y, z) { (x + len(y)) * z }", "calc").unwrap();
    assert_eq!(calc_func(42, "hello".to_string(), 9).unwrap(), 423);

    let calc_func = Func::<(INT, String, INT), INT>::create_from_script(Engine::new(), "private fn calc(x, y, z) { (x + len(y)) * z }", "calc").unwrap();
    assert_eq!(calc_func(42, "hello".to_string(), 9).unwrap(), 423);

    let calc_func = Func::<(INT, &str, INT), INT>::create_from_script(Engine::new(), "fn calc(x, y, z) { (x + len(y)) * z }", "calc").unwrap();
    assert_eq!(calc_func(42, "hello", 9).unwrap(), 423);
}

#[test]
fn test_call_fn_events() {
    // Event handler
    struct Handler {
        // Scripting engine
        pub engine: Engine,
        // Use a custom 'Scope' to keep stored state
        pub scope: Scope<'static>,
        // Program script
        pub ast: AST,
    }

    const SCRIPT: &str = r#"
        fn start(data) { 42 + data }
        fn end(data) { 0 }
    "#;

    impl Handler {
        pub fn new() -> Self {
            let engine = Engine::new();

            // Create a custom 'Scope' to hold state
            let mut scope = Scope::new();

            // Add initialized state into the custom 'Scope'
            scope.push("state", false);

            // Compile the handler script.
            let ast = engine.compile(SCRIPT).unwrap();

            // Evaluate the script to initialize it and other state variables.
            // In a real application you'd again be handling errors...
            engine.run_ast_with_scope(&mut scope, &ast).unwrap();

            // The event handler is essentially these three items:
            Handler { engine, scope, ast }
        }

        // Say there are three events: 'start', 'end', 'update'.
        // In a real application you'd be handling errors...
        pub fn on_event(&mut self, event_name: &str, event_data: INT) -> Dynamic {
            let engine = &self.engine;
            let scope = &mut self.scope;
            let ast = &self.ast;

            match event_name {
                // The 'start' event maps to function 'start'.
                // In a real application you'd be handling errors...
                "start" => engine.call_fn(scope, ast, "start", (event_data,)).unwrap(),

                // The 'end' event maps to function 'end'.
                // In a real application you'd be handling errors...
                "end" => engine.call_fn(scope, ast, "end", (event_data,)).unwrap(),

                // The 'update' event maps to function 'update'.
                // This event provides a default implementation when the script-defined function is not found.
                "update" => engine
                    .call_fn(scope, ast, "update", (event_data,))
                    .or_else(|err| match *err {
                        EvalAltResult::ErrorFunctionNotFound(fn_name, ..) if fn_name.starts_with("update") => {
                            // Default implementation of 'update' event handler
                            self.scope.set_value("state", true);
                            // Turn function-not-found into a success
                            Ok(Dynamic::UNIT)
                        }
                        _ => Err(err),
                    })
                    .unwrap(),
                // In a real application you'd be handling unknown events...
                _ => panic!("unknown event: {}", event_name),
            }
        }
    }

    let mut handler = Handler::new();
    assert!(!handler.scope.get_value::<bool>("state").unwrap());
    let _ = handler.on_event("update", 999);
    assert!(handler.scope.get_value::<bool>("state").unwrap());
    assert_eq!(handler.on_event("start", 999).as_int().unwrap(), 1041);
}
