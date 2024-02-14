#![cfg(not(feature = "no_module"))]
use rhai::{
    module_resolvers::{DummyModuleResolver, StaticModuleResolver},
    Dynamic, Engine, EvalAltResult, FnNamespace, FuncRegistration, ImmutableString, Module, ParseError, ParseErrorType, Scope, INT,
};
//
#[cfg(all(not(feature = "no_function"), feature = "internals"))]
use rhai::{FnPtr, NativeCallContext};

#[test]
fn test_module() {
    let mut module = Module::new();
    module.set_var("answer", 42 as INT);

    assert!(module.contains_var("answer"));
    assert_eq!(module.get_var_value::<INT>("answer").unwrap(), 42);
}

#[test]
fn test_module_syntax() {
    let engine = Engine::new();
    assert!(engine.compile("abc.def::xyz").is_err());
    assert!(engine.compile("abc.def::xyz()").is_err());
}

#[test]
fn test_module_sub_module() {
    let mut module = Module::new();

    let mut sub_module = Module::new();

    let mut sub_module2 = Module::new();
    sub_module2.set_var("answer", 41 as INT);

    let hash_inc = sub_module2.set_native_fn("inc", |x: &mut INT| Ok(*x + 1));
    sub_module2.build_index();
    assert!(!sub_module2.contains_indexed_global_functions());

    let f = |x: &mut INT| *x + 1;
    FuncRegistration::new("super_inc").with_namespace(FnNamespace::Global).set_into_module(&mut sub_module2, f);

    sub_module2.build_index();
    assert!(sub_module2.contains_indexed_global_functions());

    #[cfg(not(feature = "no_object"))]
    sub_module2.set_getter_fn("doubled", |x: &mut INT| Ok(*x * 2));

    sub_module.set_sub_module("universe", sub_module2);
    module.set_sub_module("life", sub_module);
    module.set_var("MYSTIC_NUMBER", Dynamic::from(42 as INT));
    module.build_index();

    assert!(module.contains_indexed_global_functions());

    assert!(module.contains_sub_module("life"));
    let m = module.get_sub_module("life").unwrap();

    assert!(m.contains_sub_module("universe"));
    let m2 = m.get_sub_module("universe").unwrap();

    assert!(m2.contains_var("answer"));
    assert!(m2.contains_fn(hash_inc));

    assert_eq!(m2.get_var_value::<INT>("answer").unwrap(), 41);

    module.set_custom_type::<()>("Don't Panic");

    let mut engine = Engine::new();
    engine.register_static_module("question", module.into());

    assert_eq!(engine.eval::<String>("type_of(())").unwrap(), "Don't Panic");

    assert_eq!(engine.eval::<INT>("question::MYSTIC_NUMBER").unwrap(), 42);
    assert!(engine.eval::<INT>("MYSTIC_NUMBER").is_err());
    assert_eq!(engine.eval::<INT>("question::life::universe::answer").unwrap(), 41);
    assert_eq!(engine.eval::<INT>("question::life::universe::answer + 1").unwrap(), 42);
    assert_eq!(engine.eval::<INT>("question::life::universe::inc(question::life::universe::answer)").unwrap(), 42);
    assert!(engine.eval::<INT>("inc(question::life::universe::answer)").is_err());
    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>("question::MYSTIC_NUMBER.doubled").unwrap(), 84);
    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>("question::life::universe::answer.doubled").unwrap(), 82);
    assert_eq!(engine.eval::<INT>("super_inc(question::life::universe::answer)").unwrap(), 42);
}

#[test]
fn test_module_resolver() {
    let mut resolver = StaticModuleResolver::new();

    let mut module = Module::new();

    module.set_var("answer", 42 as INT);
    module.set_native_fn("sum", |x: INT, y: INT, z: INT, w: INT| Ok(x + y + z + w));

    let f = |x: &mut INT| *x *= 2;
    FuncRegistration::new("double").with_namespace(FnNamespace::Global).set_into_module(&mut module, f);

    #[cfg(not(feature = "no_float"))]
    module.set_native_fn("sum_of_three_args", |target: &mut INT, a: INT, b: INT, c: rhai::FLOAT| {
        *target = a + b + c as INT;
        Ok(())
    });

    resolver.insert("hello", module);

    let mut engine = Engine::new();
    engine.set_module_resolver(resolver);

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                import "hello" as h1;
                import "hello" as h2;
                h1::sum(h2::answer, -10, 3, 7)
            "#
            )
            .unwrap(),
        42
    );

    assert!(engine
        .eval::<INT>(
            r#"
                import "hello" as h;
                sum(h::answer, -10, 3, 7)
            "#
        )
        .is_err());

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                import "hello" as h1;
                import "hello" as h2;
                let x = 42;
                h1::sum(x, -10, 3, 7)
            "#
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                import "hello" as h1;
                import "hello" as h2;
                let x = 42;
                h1::sum(x, 0, 0, 0);
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
                import "hello" as h;
                let x = 21;
                h::double(x);
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
                import "hello" as h;
                let x = 21;
                double(x);
                x
            "#
            )
            .unwrap(),
        42
    );
    #[cfg(not(feature = "no_float"))]
    {
        assert_eq!(
            engine
                .eval::<INT>(
                    r#"
                    import "hello" as h;
                    let x = 21;
                    h::sum_of_three_args(x, 14, 26, 2.0);
                    x
                "#
                )
                .unwrap(),
            42
        );
    }

    #[cfg(not(feature = "unchecked"))]
    {
        engine.set_max_modules(5);

        assert!(matches!(
            *engine
                .eval::<INT>(
                    r#"
                        let sum = 0;

                        for x in 0..10 {
                            import "hello" as h;
                            sum += h::answer;
                        }

                        sum
                    "#
                )
                .unwrap_err(),
            EvalAltResult::ErrorTooManyModules(..)
        ));

        #[cfg(not(feature = "no_function"))]
        assert!(matches!(
            *engine
                .eval::<INT>(
                    r#"
                        let sum = 0;

                        fn foo() {
                            import "hello" as h;
                            sum += h::answer;
                        }

                        for x in 0..10 {
                            foo();
                        }

                        sum
                    "#
                )
                .unwrap_err(),
            EvalAltResult::ErrorInFunctionCall(fn_name, ..) if fn_name == "foo"
        ));

        engine.set_max_modules(1000);

        #[cfg(not(feature = "no_function"))]
        engine
            .run(
                r#"
                fn foo() {
                    import "hello" as h;
                }

                for x in 0..10 {
                    foo();
                }
            "#,
            )
            .unwrap();
    }

    #[cfg(not(feature = "no_function"))]
    {
        let script = r#"
            fn foo(x) {
                import "hello" as h;
                h::answer * x
            }
            foo(1) + { import "hello" as h; h::answer }
        "#;
        let scope = Scope::new();

        let ast = engine.compile_into_self_contained(&scope, script).unwrap();

        engine.set_module_resolver(DummyModuleResolver::new());

        assert_eq!(engine.eval_ast::<INT>(&ast).unwrap(), 84);

        assert!(engine.eval::<INT>(script).is_err());

        let result = engine.call_fn::<INT>(&mut Scope::new(), &ast, "foo", (2 as INT,)).unwrap();

        assert_eq!(result, 84);

        let mut ast2 = engine.compile("fn foo(x) { 42 }").unwrap();

        #[cfg(feature = "internals")]
        let len = ast.resolver().unwrap().len();

        ast2 += ast;

        #[cfg(feature = "internals")]
        {
            assert!(ast2.resolver().is_some());
            assert_eq!(ast2.resolver().unwrap().len(), len);
        }

        let result = engine.call_fn::<INT>(&mut Scope::new(), &ast2, "foo", (2 as INT,)).unwrap();

        assert_eq!(result, 84);
    }
}

#[test]
#[cfg(not(feature = "no_function"))]
fn test_module_from_ast() {
    let mut engine = Engine::new();

    let mut resolver1 = StaticModuleResolver::new();
    let mut sub_module = Module::new();
    sub_module.set_var("foo", true);
    resolver1.insert("another module", sub_module);

    let ast = engine
        .compile(
            r#"
            // Functions become module functions
            fn calc(x) {
                x + 1
            }
            fn add_len(x, y) {
                x + len(y)
            }
            fn cross_call(x) {
                calc(x)
            }
            private fn hidden() {
                throw "you shouldn't see me!";
            }

            // Imported modules become sub-modules
            import "another module" as extra;

            // Variables defined at global level become module variables
            export const x = 123;
            let foo = 41;
            let hello;

            // Final variable values become constant module variable values
            foo = calc(foo);
            hello = `hello, ${foo} worlds!`;

            export x as abc;
            export x as xxx;
            export foo;
            export hello;
        "#,
        )
        .unwrap();

    engine.set_module_resolver(resolver1);

    let module = Module::eval_ast_as_new(Scope::new(), &ast, &engine).unwrap();

    let mut resolver2 = StaticModuleResolver::new();
    resolver2.insert("testing", module);
    engine.set_module_resolver(resolver2);

    assert_eq!(engine.eval::<INT>(r#"import "testing" as ttt; ttt::abc"#).unwrap(), 123);
    assert_eq!(engine.eval::<INT>(r#"import "testing" as ttt; ttt::x"#).unwrap(), 123);
    assert_eq!(engine.eval::<INT>(r#"import "testing" as ttt; ttt::xxx"#).unwrap(), 123);
    assert_eq!(engine.eval::<INT>(r#"import "testing" as ttt; ttt::foo"#).unwrap(), 42);
    assert!(engine.eval::<bool>(r#"import "testing" as ttt; ttt::extra::foo"#).unwrap());
    assert_eq!(engine.eval::<String>(r#"import "testing" as ttt; ttt::hello"#).unwrap(), "hello, 42 worlds!");
    assert_eq!(engine.eval::<INT>(r#"import "testing" as ttt; ttt::calc(999)"#).unwrap(), 1000);
    assert_eq!(engine.eval::<INT>(r#"import "testing" as ttt; ttt::cross_call(999)"#).unwrap(), 1000);
    assert_eq!(engine.eval::<INT>(r#"import "testing" as ttt; ttt::add_len(ttt::foo, ttt::hello)"#).unwrap(), 59);
    assert!(matches!(
        *engine
            .run(r#"import "testing" as ttt; ttt::hidden()"#)
            .unwrap_err(),
        EvalAltResult::ErrorFunctionNotFound(fn_name, ..) if fn_name == "ttt::hidden ()"
    ));
}

#[test]
fn test_module_export() {
    let engine = Engine::new();

    assert!(matches!(
        engine.compile("let x = 10; { export x; }").unwrap_err(),
        ParseError(x, ..) if *x == ParseErrorType::WrongExport
    ));

    #[cfg(not(feature = "no_function"))]
    assert!(matches!(
        engine.compile("fn abc(x) { export x; }").unwrap_err(),
        ParseError(x, ..) if *x == ParseErrorType::WrongExport
    ));
}

#[test]
fn test_module_str() {
    fn test_fn(input: ImmutableString) -> Result<INT, Box<EvalAltResult>> {
        Ok(input.len() as INT)
    }
    fn test_fn2(input: &str) -> Result<INT, Box<EvalAltResult>> {
        Ok(input.len() as INT)
    }
    fn test_fn3(input: String) -> Result<INT, Box<EvalAltResult>> {
        Ok(input.len() as INT)
    }

    let mut engine = rhai::Engine::new();
    let mut module = Module::new();
    module.set_native_fn("test", test_fn);
    module.set_native_fn("test2", test_fn2);
    module.set_native_fn("test3", test_fn3);

    let mut static_modules = rhai::module_resolvers::StaticModuleResolver::new();
    static_modules.insert("test", module);
    engine.set_module_resolver(static_modules);

    assert_eq!(engine.eval::<INT>(r#"import "test" as test; test::test("test");"#).unwrap(), 4);
    assert_eq!(engine.eval::<INT>(r#"import "test" as test; test::test2("test");"#).unwrap(), 4);
    assert_eq!(engine.eval::<INT>(r#"import "test" as test; test::test3("test");"#).unwrap(), 4);
}

#[cfg(not(feature = "no_function"))]
#[test]
fn test_module_ast_namespace() {
    let script = "
        fn foo(x) { x + 1 }
        fn bar(x) { foo(x) }
    ";

    let mut engine = Engine::new();

    let ast = engine.compile(script).unwrap();

    let module = Module::eval_ast_as_new(Scope::new(), &ast, &engine).unwrap();

    let mut resolver = StaticModuleResolver::new();
    resolver.insert("testing", module);
    engine.set_module_resolver(resolver);

    assert_eq!(engine.eval::<INT>(r#"import "testing" as t; t::foo(41)"#).unwrap(), 42);
    assert_eq!(engine.eval::<INT>(r#"import "testing" as t; t::bar(41)"#).unwrap(), 42);
    assert_eq!(engine.eval::<INT>(r#"fn foo(x) { x - 1 } import "testing" as t; t::foo(41)"#).unwrap(), 42);
    assert_eq!(engine.eval::<INT>(r#"fn foo(x) { x - 1 } import "testing" as t; t::bar(41)"#).unwrap(), 42);
}

#[cfg(not(feature = "no_function"))]
#[test]
fn test_module_ast_namespace2() {
    use rhai::{Engine, Module, Scope};

    const MODULE_TEXT: &str = "
        fn run_function(function) {
            call(function)
        }
    ";

    const SCRIPT: &str = r#"
        import "test_module" as test;

        fn foo() {
            print("foo");
        }

        test::run_function(Fn("foo"));
    "#;

    let mut engine = Engine::new();
    let module_ast = engine.compile(MODULE_TEXT).unwrap();
    let module = Module::eval_ast_as_new(Scope::new(), &module_ast, &engine).unwrap();
    let mut static_modules = rhai::module_resolvers::StaticModuleResolver::new();
    static_modules.insert("test_module", module);
    engine.set_module_resolver(static_modules);

    engine.run(SCRIPT).unwrap();
}

#[cfg(all(not(feature = "no_function"), feature = "internals"))]
#[test]
fn test_module_context() {
    let script = "fn bar() { calc(|x| x + 1) }";

    let mut engine = Engine::new();

    let ast = engine.compile(script).unwrap();

    let module = Module::eval_ast_as_new(Scope::new(), &ast, &engine).unwrap();

    let mut resolver = StaticModuleResolver::new();
    resolver.insert("testing", module);
    engine.set_module_resolver(resolver);

    engine.register_fn("calc", |context: NativeCallContext, fp: FnPtr| -> Result<INT, Box<EvalAltResult>> {
        let engine = context.engine();

        // Store context for later use - requires the 'internals' feature
        let context_data = context.store_data();

        // Recreate the 'NativeCallContext'
        let new_context = context_data.create_context(engine);

        fp.call_within_context(&new_context, (41 as INT,))
    });

    assert_eq!(engine.eval::<INT>(r#"import "testing" as t; t::bar()"#).unwrap(), 42);
}

#[test]
fn test_module_file() {
    let engine = Engine::new();
    let ast = engine
        .compile(
            r#"
                import "scripts/module";
                print("top");
            "#,
        )
        .unwrap();
    Module::eval_ast_as_new(Scope::new(), &ast, &engine).unwrap();
}

#[cfg(not(feature = "no_function"))]
#[test]
fn test_module_environ() {
    let mut engine = Engine::new();

    let ast = engine
        .compile(
            r#"
            const SECRET = 42;

            fn foo(x) {
                print(global::SECRET);
                global::SECRET + x
            }
        "#,
        )
        .unwrap();

    let mut m = Module::eval_ast_as_new(Scope::new(), &ast, &engine).unwrap();

    m.set_id("test");
    m.build_index();

    engine.register_static_module("test", m.into());

    assert_eq!(
        engine
            .eval::<String>(
                r#"
                const SECRET = "hello";

                fn foo(x) {
                    print(global::SECRET);
                    global::SECRET + x
                }

                let t = test::foo(0);

                foo(t)
            "#
            )
            .unwrap(),
        "hello42"
    );
}

#[test]
fn test_module_dynamic() {
    fn test_fn(input: Dynamic, x: INT) -> Result<INT, Box<EvalAltResult>> {
        let s = input.into_string().unwrap();
        Ok(s.len() as INT + x)
    }

    let mut engine = rhai::Engine::new();
    let mut module = Module::new();
    module.set_native_fn("test", test_fn);

    let mut static_modules = rhai::module_resolvers::StaticModuleResolver::new();
    static_modules.insert("test", module);
    engine.set_module_resolver(static_modules);
    engine.register_fn("test2", test_fn);

    assert_eq!(engine.eval::<INT>(r#"test2("test", 38);"#).unwrap(), 42);

    assert_eq!(engine.eval::<INT>(r#"import "test" as test; test::test("test", 38);"#).unwrap(), 42);
}
