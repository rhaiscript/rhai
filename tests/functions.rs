#![cfg(not(feature = "no_function"))]
use rhai::{Dynamic, Engine, EvalAltResult, FnNamespace, FuncRegistration, Module, NativeCallContext, ParseErrorType, Shared, INT};

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

    assert!(engine.eval::<INT>("fn/*\0„").is_err());
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
        FuncRegistration::new("test").with_namespace(FnNamespace::Global).set_into_module(&mut m, f);

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
        42
    );

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

    #[cfg(not(feature = "no_closure"))]
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    const GLOBAL_VALUE = 42;
                    do_stuff(|| global::GLOBAL_VALUE);
                "
            )
            .unwrap(),
        42
    );

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

    assert!(engine
        .eval::<INT>(
            "
                fn foo(y) { x += y; x }

                let x = 41;
                let y = 999;

                foo(1) + x
            "
        )
        .is_err());

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

    #[cfg(not(feature = "no_object"))]
    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    fn foo(x) { this.data += x; }

                    let x = #{ data: 40, action: Fn("foo") };
                    x.action(2);
                    x.data
                "#
            )
            .unwrap(),
        42
    );
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
