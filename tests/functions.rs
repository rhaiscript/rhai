#![cfg(not(feature = "no_function"))]
use rhai::{Dynamic, Engine, EvalAltResult, FuncRegistration, Module, NativeCallContext, ParseErrorType, Shared, INT};

#[test]
fn test_functions() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("fn add_me(a, b) { a+b } add_me(3, 4)").unwrap(), 7);
    assert_eq!(engine.eval::<INT>("fn add_me(a, b,) { a+b } add_me(3, 4,)").unwrap(), 7);
    assert_eq!(engine.eval::<INT>("fn bob() { return 4; 5 } bob()").unwrap(), 4);
    assert_eq!(engine.eval::<INT>("fn add(x, n) { x + n } add(40, 2)").unwrap(), 42);
    assert_eq!(engine.eval::<INT>("fn add(x, n,) { x + n } add(40, 2,)").unwrap(), 42);
    assert_eq!(engine.eval::<INT>("fn add(x, n) { x + n } let a = 40; add(a, 2); a").unwrap(), 40);
    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>("fn add(n) { this + n } let x = 40; x.add(2)").unwrap(), 42);
    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>("fn add(n) { this += n; } let x = 40; x.add(2); x").unwrap(), 42);
    assert_eq!(engine.eval::<INT>("fn mul2(x) { x * 2 } mul2(21)").unwrap(), 42);
    assert_eq!(engine.eval::<INT>("fn mul2(x) { x *= 2 } let a = 21; mul2(a); a").unwrap(), 21);
    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>("fn mul2() { this * 2 } let x = 21; x.mul2()").unwrap(), 42);
    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>("fn mul2() { this *= 2; } let x = 21; x.mul2(); x").unwrap(), 42);

    let _ = engine.eval::<INT>("fn/*\0„").unwrap_err();
}

#[test]
fn test_functions_dynamic() {
    let mut engine = Engine::new();

    engine.register_fn(
        "foo",
        |a: INT, b: Dynamic, c: INT, d: INT, e: INT, f: INT, g: INT, h: INT, i: INT, j: INT, k: INT, l: INT, m: INT, n: INT, o: INT, p: INT, q: INT, r: INT, s: INT, t: INT| match b.try_cast::<bool>() {
            Some(true) => a + c + d + e + f + g + h + i + j + k + l + m + n + o + p + q + r + s + t,
            Some(false) => 0,
            None => 42,
        },
    );

    assert_eq!(engine.eval::<INT>("foo(1, true, 3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20)").unwrap(), 208);
    assert_eq!(engine.eval::<INT>("foo(1, false, 3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20)").unwrap(), 0);
    assert_eq!(engine.eval::<INT>("foo(1, 2, 3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20)").unwrap(), 42);
}

#[cfg(not(feature = "no_object"))]
#[test]
fn test_functions_trait_object() {
    trait TestTrait {
        fn greet(&self) -> INT;
    }

    #[allow(clippy::upper_case_acronyms)]
    #[derive(Debug, Clone)]
    struct ABC(INT);

    impl TestTrait for ABC {
        fn greet(&self) -> INT {
            self.0
        }
    }

    #[cfg(not(feature = "sync"))]
    type MySharedTestTrait = Shared<dyn TestTrait>;

    #[cfg(feature = "sync")]
    type MySharedTestTrait = Shared<dyn TestTrait + Send + Sync>;

    let mut engine = Engine::new();

    engine
        .register_type_with_name::<MySharedTestTrait>("MySharedTestTrait")
        .register_fn("new_ts", || Shared::new(ABC(42)) as MySharedTestTrait)
        .register_fn("greet", |x: MySharedTestTrait| x.greet());

    assert_eq!(engine.eval::<String>("type_of(new_ts())").unwrap(), "MySharedTestTrait");
    assert_eq!(engine.eval::<INT>("let x = new_ts(); greet(x)").unwrap(), 42);
}

#[test]
fn test_functions_namespaces() {
    let mut engine = Engine::new();

    #[cfg(not(feature = "no_module"))]
    {
        let mut m = Module::new();

        let f = || 999 as INT;
        FuncRegistration::new("test").in_global_namespace().set_into_module(&mut m, f);

        engine.register_static_module("hello", m.into());

        let mut m = Module::new();
        m.set_var("ANSWER", 123 as INT);

        assert_eq!(engine.eval::<INT>("test()").unwrap(), 999);

        assert_eq!(engine.eval::<INT>("fn test() { 123 } test()").unwrap(), 123);
    }

    engine.register_fn("test", || 42 as INT);

    assert_eq!(engine.eval::<INT>("fn test() { 123 } test()").unwrap(), 123);
    assert_eq!(engine.eval::<INT>("test()").unwrap(), 42);
}

#[cfg(not(feature = "no_module"))]
#[test]
fn test_functions_global_module() {
    let mut engine = Engine::new();

    // assert_eq!(
    //     engine
    //         .eval::<INT>(
    //             "
    //                 const ANSWER = 42;
    //                 fn foo() { global::ANSWER }
    //                 foo()
    //             "
    //         )
    //         .unwrap(),
    //     42
    // );

    assert!(matches!(*engine.run(
        "
            fn foo() { global::ANSWER }

            {
                const ANSWER = 42;
                foo()
            }
        ").unwrap_err(),
        EvalAltResult::ErrorInFunctionCall(.., err, _)
            if matches!(&*err, EvalAltResult::ErrorVariableNotFound(v, ..) if v == "global::ANSWER")
    ));

    engine.register_fn("do_stuff", |context: NativeCallContext, callback: rhai::FnPtr| -> Result<INT, _> { callback.call_within_context(&context, ()) });

    #[cfg(not(feature = "no_closure"))]
    assert!(matches!(*engine.run(
        "
            do_stuff(|| {
                const LOCAL_VALUE = 42;
                global::LOCAL_VALUE
            });
        ").unwrap_err(),
        EvalAltResult::ErrorInFunctionCall(.., err, _)
            if matches!(&*err, EvalAltResult::ErrorVariableNotFound(v, ..) if v == "global::LOCAL_VALUE")
    ));

    // #[cfg(not(feature = "no_closure"))]
    // assert_eq!(
    //     engine
    //         .eval::<INT>(
    //             "
    //                 const GLOBAL_VALUE = 42;
    //                 do_stuff(|| global::GLOBAL_VALUE);
    //             "
    //         )
    //         .unwrap(),
    //     42
    // );

    // Override global
    let mut module = Module::new();
    module.set_var("ANSWER", 123 as INT);
    engine.register_static_module("global", module.into());

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    const ANSWER = 42;
                    fn foo() { global::ANSWER }
                    foo()
                "
            )
            .unwrap(),
        123
    );

    // Other globals
    let mut module = Module::new();
    module.set_var("ANSWER", 123 as INT);
    engine.register_global_module(module.into());

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    fn foo() { global::ANSWER }
                    foo()
                "
            )
            .unwrap(),
        123
    );
}

#[test]
fn test_functions_bang() {
    let engine = Engine::new();

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    fn foo() {
                        hello + bar
                    }

                    let hello = 42;
                    let bar = 123;

                    foo!()
                ",
            )
            .unwrap(),
        165
    );

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    fn foo() {
                        hello = 0;
                        hello + bar
                    }

                    let hello = 42;
                    let bar = 123;

                    foo!()
                ",
            )
            .unwrap(),
        123
    );

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    fn foo() {
                        let hello = bar + 42;
                    }

                    let bar = 999;
                    let hello = 123;

                    foo!();

                    hello
                ",
            )
            .unwrap(),
        123
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    fn foo(x) {
                        let hello = bar + 42 + x;
                    }

                    let bar = 999;
                    let hello = 123;

                    let f = Fn("foo");

                    call!(f, 1);

                    hello
                "#,
            )
            .unwrap(),
        123
    );
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    fn foo(y) { x += y; x }

                    let x = 41;
                    let y = 999;

                    foo!(1) + x
                "
            )
            .unwrap(),
        84
    );

    let _ = engine
        .eval::<INT>(
            "
                fn foo(y) { x += y; x }

                let x = 41;
                let y = 999;

                foo(1) + x
            ",
        )
        .unwrap_err();

    #[cfg(not(feature = "no_object"))]
    assert!(matches!(
        engine
            .compile(
                "
                    fn foo() { this += x; }

                    let x = 41;
                    let y = 999;

                    y.foo!();
                "
            )
            .unwrap_err()
            .err_type(),
        ParseErrorType::MalformedCapture(..)
    ));
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
struct TestStruct(INT);

impl Clone for TestStruct {
    fn clone(&self) -> Self {
        Self(self.0 + 1)
    }
}

#[test]
fn test_functions_take() {
    let mut engine = Engine::new();

    engine.register_type_with_name::<TestStruct>("TestStruct").register_fn("new_ts", |x: INT| TestStruct(x));

    assert_eq!(
        engine
            .eval::<TestStruct>(
                "
                    let x = new_ts(0);
                    for n in 0..41 { x = x }
                    x
                ",
            )
            .unwrap(),
        TestStruct(42)
    );

    assert_eq!(
        engine
            .eval::<TestStruct>(
                "
                    let x = new_ts(0);
                    for n in 0..41 { x = take(x) }
                    take(x)
                ",
            )
            .unwrap(),
        TestStruct(0)
    );
}

#[test]
fn test_functions_big() {
    let engine = Engine::new();

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    fn math_me(a, b, c, d, e, f) {
                        a - b * c + d * e - f
                    }
                    math_me(100, 5, 2, 9, 6, 32)
                ",
            )
            .unwrap(),
        112
    );
}

#[test]
fn test_functions_overloading() {
    let engine = Engine::new();

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    fn abc(x,y,z) { 2*x + 3*y + 4*z + 888 }
                    fn abc(x,y) { x + 2*y + 88 }
                    fn abc() { 42 }
                    fn abc(x) { x - 42 }

                    abc() + abc(1) + abc(1,2) + abc(1,2,3)
                "
            )
            .unwrap(),
        1002
    );

    assert_eq!(
        *engine
            .compile(
                "
                    fn abc(x) { x + 42 }
                    fn abc(x) { x - 42 }
                "
            )
            .unwrap_err()
            .err_type(),
        ParseErrorType::FnDuplicatedDefinition("abc".to_string(), 1)
    );
}

#[test]
fn test_functions_params() {
    let engine = Engine::new();

    // Expect duplicated parameters error
    assert!(matches!(
        engine.compile("fn hello(x, x) { x }").unwrap_err().err_type(),
        ParseErrorType::FnDuplicatedParam(a, b) if a == "hello" && b == "x"));
}

#[test]
fn test_function_pointers() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<String>(r#"type_of(Fn("abc"))"#).unwrap(), "Fn");

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    fn foo(x) { 40 + x }

                    let f = Fn("foo");
                    call(f, 2)
                "#
            )
            .unwrap(),
        42
    );

    #[cfg(not(feature = "no_object"))]
    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    fn foo(x) { 40 + x }

                    let fn_name = "f";
                    fn_name += "oo";

                    let f = Fn(fn_name);
                    f.call(2)
                "#
            )
            .unwrap(),
        42
    );

    #[cfg(not(feature = "no_object"))]
    assert!(matches!(
        *engine.eval::<INT>(r#"let f = Fn("abc"); f.call(0)"#).unwrap_err(),
        EvalAltResult::ErrorFunctionNotFound(f, ..) if f.starts_with("abc (")
    ));

    #[cfg(not(feature = "no_object"))]
    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    fn foo(x) { 40 + x }

                    let x = #{ action: Fn("foo") };
                    x.action.call(2)
                "#
            )
            .unwrap(),
        42
    );

    // #[cfg(not(feature = "no_object"))]
    // assert_eq!(
    //     engine
    //         .eval::<INT>(
    //             r#"
    //                 fn foo(x) { this.data += x; }

    //                 let x = #{ data: 40, action: Fn("foo") };
    //                 x.action(2);
    //                 x.data
    //             "#
    //         )
    //         .unwrap(),
    //     42
    // );
}

#[test]
fn test_functions_is_def() {
    let engine = Engine::new();

    assert!(engine
        .eval::<bool>(
            r#"
                fn foo(x) { x + 1 }
                is_def_fn("foo", 1)
            "#
        )
        .unwrap());
    assert!(!engine
        .eval::<bool>(
            r#"
                fn foo(x) { x + 1 }
                is_def_fn("bar", 1)
            "#
        )
        .unwrap());
    assert!(!engine
        .eval::<bool>(
            r#"
                fn foo(x) { x + 1 }
                is_def_fn("foo", 0)
            "#
        )
        .unwrap());
}

#[test]
fn test_functions_is_def_registration() {
    let mut engine = Engine::new();

    engine.register_fn("is_def_fn", |name: &str, _: INT| name.to_string());

    assert!(engine
        .eval::<bool>(
            r#"
                fn foo(x) { x + 1 }
                is_def_fn("foo", 1)
            "#
        )
        .unwrap());

    engine.register_fn("is_def_fn", |name: &str| name.to_string());

    assert!(engine
        .eval::<String>(
            r#"
                fn foo() { 1 }
                is_def_fn("foo")
            "#
        )
        .is_err());
}

#[test]
#[cfg(not(feature = "unchecked"))]
fn test_functions_max() {
    let mut engine = Engine::new();
    engine.set_max_functions(5);

    engine
        .compile(
            "
            fn foo1() {}
            fn foo2() {}
            fn foo3() {}
            fn foo4() {}
            fn foo5() {}
        ",
        )
        .unwrap();

    assert!(matches!(
        engine
            .compile(
                "
                fn foo1() {}
                fn foo2() {}
                fn foo3() {}
                fn foo4() {}
                fn foo5() {}
                fn foo6() {}
            "
            )
            .expect_err("should err")
            .err_type(),
        ParseErrorType::TooManyFunctions
    ))
}

/// on_missing_function callback tests

#[test]
#[cfg(feature = "internals")]
#[cfg(not(feature = "no_object"))]
fn test_missing_function_basic() {
    let mut engine = Engine::new();
    #[allow(deprecated)]
    engine.on_missing_function(|name, _args, _is_method_call, _ctx| if name == "greet" { Ok(Some(Dynamic::from("hello"))) } else { Ok(None) });

    let result: String = engine.eval(r#"let x = 42; x.greet()"#).unwrap();
    assert_eq!(result, "hello");
}

#[test]
#[cfg(feature = "internals")]
#[cfg(not(feature = "no_object"))]
fn test_missing_function_fallthrough() {
    let mut engine = Engine::new();
    #[allow(deprecated)]
    engine.on_missing_function(|_name, _args, _is_method_call, _ctx| Ok(None));

    let result = engine.eval::<Dynamic>("let x = 42; x.nope()");
    assert!(result.is_err());
}

#[test]
#[cfg(feature = "internals")]
#[cfg(not(feature = "no_object"))]
fn test_missing_function_receives_args() {
    let mut engine = Engine::new();
    #[allow(deprecated)]
    engine.on_missing_function(|name, args, _is_method_call, _ctx| {
        if name == "add" && args.len() == 3 {
            let a = args[1].as_int().unwrap();
            let b = args[2].as_int().unwrap();
            Ok(Some(Dynamic::from(a + b)))
        } else {
            Ok(None)
        }
    });

    let result: INT = engine.eval("let x = 0; x.add(3, 4)").unwrap();
    assert_eq!(result, 7);
}

#[test]
#[cfg(feature = "internals")]
#[cfg(not(feature = "no_object"))]
fn test_missing_function_not_called_for_existing() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let called = Shared::new(AtomicBool::new(false));
    let called_clone = called.clone();

    let mut engine = Engine::new();
    engine.register_fn("my_existing", |x: INT| x * 2);
    #[allow(deprecated)]
    engine.on_missing_function(move |_name, _args, _is_method_call, _ctx| {
        called_clone.store(true, Ordering::SeqCst);
        Ok(None)
    });

    let result: INT = engine.eval("let x = 21; x.my_existing()").unwrap();
    assert_eq!(result, 42);
    assert!(!called.load(Ordering::SeqCst));
}

#[test]
#[cfg(feature = "internals")]
#[cfg(not(feature = "no_object"))]
fn test_missing_function_error_propagation() {
    let mut engine = Engine::new();
    #[allow(deprecated)]
    engine.on_missing_function(|_name, _args, _is_method_call, _ctx| Err("custom error".into()));

    let result = engine.eval::<Dynamic>("let x = 42; x.test()");
    assert!(result.is_err());
}

#[test]
#[cfg(feature = "internals")]
#[cfg(not(feature = "no_object"))]
fn test_missing_function_custom_type() {
    #[derive(Debug, Clone)]
    struct MyType(INT);

    let mut engine = Engine::new();
    engine.register_type_with_name::<MyType>("MyType");
    engine.register_fn("new_my", || MyType(10));
    #[allow(deprecated)]
    engine.on_missing_function(|name, args, _is_method_call, _ctx| {
        if name == "value" {
            if let Some(obj) = args[0].read_lock::<MyType>() {
                return Ok(Some(Dynamic::from(obj.0)));
            }
        }
        Ok(None)
    });

    let result: INT = engine.eval("let x = new_my(); x.value()").unwrap();
    assert_eq!(result, 10);
}

#[test]
#[cfg(feature = "internals")]
#[cfg(not(feature = "no_object"))]
fn test_missing_function_multiple_arities() {
    let mut engine = Engine::new();
    #[allow(deprecated)]
    engine.on_missing_function(|name, args, _is_method_call, _ctx| {
        if name == "count" {
            // args[0] is self, remaining are the actual arguments
            Ok(Some(Dynamic::from((args.len() - 1) as INT)))
        } else {
            Ok(None)
        }
    });

    let r1: INT = engine.eval("let x = 0; x.count()").unwrap();
    assert_eq!(r1, 0);
    let r2: INT = engine.eval("let x = 0; x.count(1)").unwrap();
    assert_eq!(r2, 1);
    let r3: INT = engine.eval("let x = 0; x.count(1, 2, 3)").unwrap();
    assert_eq!(r3, 3);
}

#[test]
#[cfg(feature = "internals")]
#[cfg(not(feature = "no_object"))]
fn test_missing_function_is_method_call_flag() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let saw_method = Shared::new(AtomicBool::new(false));
    let saw_method_clone = saw_method.clone();

    let mut engine = Engine::new();
    #[allow(deprecated)]
    engine.on_missing_function(move |name, _args, is_method_call, _ctx| {
        if name == "greet" {
            saw_method_clone.store(is_method_call, Ordering::SeqCst);
            Ok(Some(Dynamic::from("hello")))
        } else {
            Ok(None)
        }
    });

    // Method-style call: is_method_call should be true
    let _: String = engine.eval(r#"let x = 42; x.greet()"#).unwrap();
    assert!(saw_method.load(Ordering::SeqCst), "method-style call should set is_method_call=true");
}

#[test]
#[cfg(feature = "internals")]
#[cfg(not(feature = "no_index"))]
#[cfg(not(feature = "no_object"))]
fn on_missing_function_isolates_nested_cache_frame() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Two modules, each registering `step(target, n) -> INT` under the same
    // name and argument types. Same `(name, arg_types)` produces the same
    // fn-resolution cache hash.
    let mut module_a = Module::new();
    module_a.set_native_fn("step", |_target: &mut INT, n: INT| Ok(n * 2));
    let module_a = Shared::new(module_a);

    let mut module_b = Module::new();
    module_b.set_native_fn("step", |_target: &mut INT, n: INT| Ok(n * 10));
    let module_b = Shared::new(module_b);

    // Counter picks module_a for invocations 0-1, module_b for invocation 2+.
    // Three invocations are needed to exercise the cache lifecycle: call 1
    // sets the bloom filter bit (no dict entry yet), call 2 populates the
    // dict against module_a, call 3 (with module_b pushed) must hit a FRESH
    // resolution in its isolated frame — if isolation is missing it would
    // hit the stale dict entry from call 2 and return module_a's result.
    let call_idx = Shared::new(AtomicUsize::new(0));

    let a = module_a.clone();
    let b = module_b.clone();
    let call_idx_clone = call_idx.clone();

    let mut engine = Engine::new();

    #[allow(deprecated)]
    engine.on_missing_function(move |name, args, _is_method_call, mut ctx| {
        if name != "step" {
            return Ok(None);
        }

        let idx = call_idx_clone.fetch_add(1, Ordering::SeqCst);
        let module = if idx < 2 { a.clone() } else { b.clone() };

        // Isolate the nested dispatch in a fresh cache frame. Without this,
        // invocation 2 would cache the resolution against module_a, and
        // invocation 3 would hit that stale cache entry after module_b had
        // been pushed.
        ctx.new_frame().with_new_caching_layer().with_namespace(module).call_fn_raw(name, true, false, args).map(Some)
    });

    // Three method-style calls with identical name and argument types so
    // they all hash to the same fn-resolution cache key.
    let script = r#"
        let x = 0;
        [x.step(3), x.step(3), x.step(3)]
    "#;

    let results: rhai::Array = engine.eval(script).expect("eval must succeed");
    let results: Vec<INT> = results.into_iter().map(|v| v.as_int().expect("array element must be INT")).collect();

    assert_eq!(
        results,
        vec![6, 6, 30],
        "third invocation must dispatch to module_b (3*10=30), not a stale \
         cached entry from module_a (3*2=6); without push/rewind_fn_resolution_cache \
         the third element would be 6."
    );

    assert_eq!(call_idx.load(Ordering::SeqCst), 3, "on_missing_function must fire exactly three times");
}
