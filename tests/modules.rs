#![cfg(not(feature = "no_module"))]
use rhai::{
    module_resolvers::{DummyModuleResolver, StaticModuleResolver},
    Dynamic, Engine, EvalAltResult, FnNamespace, FnPtr, ImmutableString, Module, NativeCallContext,
    ParseError, ParseErrorType, Scope, Shared, INT,
};

#[test]
fn test_module() {
    let mut module = Module::new();
    module.set_var("answer", 42 as INT);

    assert!(module.contains_var("answer"));
    assert_eq!(module.get_var_value::<INT>("answer").unwrap(), 42);
}

#[test]
fn test_module_sub_module() -> Result<(), Box<EvalAltResult>> {
    let mut module = Module::new();

    let mut sub_module = Module::new();

    let mut sub_module2 = Module::new();
    sub_module2.set_var("answer", 41 as INT);

    let hash_inc = sub_module2.set_native_fn("inc", |x: &mut INT| Ok(*x + 1));
    sub_module2.build_index();
    assert!(!sub_module2.contains_indexed_global_functions());

    let super_hash = sub_module2.set_native_fn("super_inc", |x: &mut INT| Ok(*x + 1));
    sub_module2.update_fn_namespace(super_hash, FnNamespace::Global);
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

    assert_eq!(engine.eval::<String>("type_of(())")?, "Don't Panic");

    assert_eq!(engine.eval::<INT>("question::MYSTIC_NUMBER")?, 42);
    assert!(engine.eval::<INT>("MYSTIC_NUMBER").is_err());
    assert_eq!(engine.eval::<INT>("question::life::universe::answer")?, 41);
    assert_eq!(
        engine.eval::<INT>("question::life::universe::answer + 1")?,
        42
    );
    assert_eq!(
        engine.eval::<INT>("question::life::universe::inc(question::life::universe::answer)")?,
        42
    );
    assert!(engine
        .eval::<INT>("inc(question::life::universe::answer)")
        .is_err());
    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>("question::MYSTIC_NUMBER.doubled")?, 84);
    #[cfg(not(feature = "no_object"))]
    assert_eq!(
        engine.eval::<INT>("question::life::universe::answer.doubled")?,
        82
    );
    assert_eq!(
        engine.eval::<INT>("super_inc(question::life::universe::answer)")?,
        42
    );

    Ok(())
}

#[test]
fn test_module_resolver() -> Result<(), Box<EvalAltResult>> {
    let mut resolver = StaticModuleResolver::new();

    let mut module = Module::new();

    module.set_var("answer", 42 as INT);
    module.set_native_fn("sum", |x: INT, y: INT, z: INT, w: INT| Ok(x + y + z + w));
    let double_hash = module.set_native_fn("double", |x: &mut INT| {
        *x *= 2;
        Ok(())
    });
    module.update_fn_namespace(double_hash, FnNamespace::Global);

    #[cfg(not(feature = "no_float"))]
    module.set_native_fn(
        "sum_of_three_args",
        |target: &mut INT, a: INT, b: INT, c: rhai::FLOAT| {
            *target = a + b + c as INT;
            Ok(())
        },
    );

    resolver.insert("hello", module);

    let mut engine = Engine::new();
    engine.set_module_resolver(resolver);

    assert_eq!(
        engine.eval::<INT>(
            r#"
                import "hello" as h1;
                import "hello" as h2;
                h1::sum(h2::answer, -10, 3, 7)
            "#
        )?,
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
        engine.eval::<INT>(
            r#"
                import "hello" as h1;
                import "hello" as h2;
                let x = 42;
                h1::sum(x, -10, 3, 7)
            "#
        )?,
        42
    );

    assert_eq!(
        engine.eval::<INT>(
            r#"
                import "hello" as h1;
                import "hello" as h2;
                let x = 42;
                h1::sum(x, 0, 0, 0);
                x
            "#
        )?,
        42
    );

    assert_eq!(
        engine.eval::<INT>(
            r#"
                import "hello" as h;
                let x = 21;
                h::double(x);
                x
            "#
        )?,
        42
    );
    assert_eq!(
        engine.eval::<INT>(
            r#"
                import "hello" as h;
                let x = 21;
                double(x);
                x
            "#
        )?,
        42
    );
    #[cfg(not(feature = "no_float"))]
    {
        assert_eq!(
            engine.eval::<INT>(
                r#"
                    import "hello" as h;
                    let x = 21;
                    h::sum_of_three_args(x, 14, 26, 2.0);
                    x
                "#
            )?,
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
                .expect_err("should error"),
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
                .expect_err("should error"),
            EvalAltResult::ErrorInFunctionCall(fn_name, _, ..) if fn_name == "foo"
        ));

        engine.set_max_modules(1000);

        #[cfg(not(feature = "no_function"))]
        engine.run(
            r#"
                fn foo() {
                    import "hello" as h;
                }

                for x in 0..10 {
                    foo();
                }
            "#,
        )?;
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
        let mut scope = Scope::new();

        let ast = engine.compile_into_self_contained(&mut scope, script)?;

        engine.set_module_resolver(DummyModuleResolver::new());

        assert_eq!(engine.eval_ast::<INT>(&ast)?, 84);

        assert!(engine.eval::<INT>(script).is_err());

        let result = engine.call_fn::<INT>(&mut Scope::new(), &ast, "foo", (2 as INT,))?;

        assert_eq!(result, 84);

        let mut ast2 = engine.compile("fn foo(x) { 42 }")?;

        #[cfg(feature = "internals")]
        let len = ast.resolver().unwrap().len();

        ast2 += ast;

        #[cfg(feature = "internals")]
        {
            assert!(ast2.resolver().is_some());
            assert_eq!(ast2.resolver().unwrap().len(), len);
        }

        let result = engine.call_fn::<INT>(&mut Scope::new(), &ast2, "foo", (2 as INT,))?;

        assert_eq!(result, 84);
    }

    Ok(())
}

#[test]
#[cfg(not(feature = "no_function"))]
fn test_module_from_ast() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();

    let mut resolver1 = StaticModuleResolver::new();
    let mut sub_module = Module::new();
    sub_module.set_var("foo", true);
    resolver1.insert("another module", sub_module);

    let ast = engine.compile(
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
    )?;

    engine.set_module_resolver(resolver1);

    let module = Module::eval_ast_as_new(Scope::new(), &ast, &engine)?;

    let mut resolver2 = StaticModuleResolver::new();
    resolver2.insert("testing", module);
    engine.set_module_resolver(resolver2);

    assert_eq!(
        engine.eval::<INT>(r#"import "testing" as ttt; ttt::abc"#)?,
        123
    );
    assert_eq!(
        engine.eval::<INT>(r#"import "testing" as ttt; ttt::x"#)?,
        123
    );
    assert_eq!(
        engine.eval::<INT>(r#"import "testing" as ttt; ttt::xxx"#)?,
        123
    );
    assert_eq!(
        engine.eval::<INT>(r#"import "testing" as ttt; ttt::foo"#)?,
        42
    );
    assert!(engine.eval::<bool>(r#"import "testing" as ttt; ttt::extra::foo"#)?);
    assert_eq!(
        engine.eval::<String>(r#"import "testing" as ttt; ttt::hello"#)?,
        "hello, 42 worlds!"
    );
    assert_eq!(
        engine.eval::<INT>(r#"import "testing" as ttt; ttt::calc(999)"#)?,
        1000
    );
    assert_eq!(
        engine.eval::<INT>(r#"import "testing" as ttt; ttt::cross_call(999)"#)?,
        1000
    );
    assert_eq!(
        engine.eval::<INT>(r#"import "testing" as ttt; ttt::add_len(ttt::foo, ttt::hello)"#)?,
        59
    );
    assert!(matches!(
        *engine
            .run(r#"import "testing" as ttt; ttt::hidden()"#)
            .expect_err("should error"),
        EvalAltResult::ErrorFunctionNotFound(fn_name, ..) if fn_name == "ttt::hidden ()"
    ));

    Ok(())
}

#[test]
fn test_module_export() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    assert!(matches!(
        engine.compile("let x = 10; { export x; }").expect_err("should error"),
        ParseError(x, ..) if *x == ParseErrorType::WrongExport
    ));

    #[cfg(not(feature = "no_function"))]
    assert!(matches!(
        engine.compile("fn abc(x) { export x; }").expect_err("should error"),
        ParseError(x, ..) if *x == ParseErrorType::WrongExport
    ));

    Ok(())
}

#[test]
fn test_module_str() -> Result<(), Box<EvalAltResult>> {
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

    assert_eq!(
        engine.eval::<INT>(r#"import "test" as test; test::test("test");"#)?,
        4
    );
    assert_eq!(
        engine.eval::<INT>(r#"import "test" as test; test::test2("test");"#)?,
        4
    );
    assert_eq!(
        engine.eval::<INT>(r#"import "test" as test; test::test3("test");"#)?,
        4
    );

    Ok(())
}

#[cfg(not(feature = "no_function"))]
#[test]
fn test_module_ast_namespace() -> Result<(), Box<EvalAltResult>> {
    let script = "
        fn foo(x) { x + 1 }
        fn bar(x) { foo(x) }
    ";

    let mut engine = Engine::new();

    let ast = engine.compile(script)?;

    let module = Module::eval_ast_as_new(Scope::new(), &ast, &engine)?;

    let mut resolver = StaticModuleResolver::new();
    resolver.insert("testing", module);
    engine.set_module_resolver(resolver);

    assert_eq!(
        engine.eval::<INT>(r#"import "testing" as t; t::foo(41)"#)?,
        42
    );
    assert_eq!(
        engine.eval::<INT>(r#"import "testing" as t; t::bar(41)"#)?,
        42
    );
    assert_eq!(
        engine.eval::<INT>(r#"fn foo(x) { x - 1 } import "testing" as t; t::foo(41)"#)?,
        42
    );
    assert_eq!(
        engine.eval::<INT>(r#"fn foo(x) { x - 1 } import "testing" as t; t::bar(41)"#)?,
        42
    );

    Ok(())
}

#[cfg(not(feature = "no_function"))]
#[test]
fn test_module_ast_namespace2() -> Result<(), Box<EvalAltResult>> {
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
    let module_ast = engine.compile(MODULE_TEXT)?;
    let module = Module::eval_ast_as_new(Scope::new(), &module_ast, &engine)?;
    let mut static_modules = rhai::module_resolvers::StaticModuleResolver::new();
    static_modules.insert("test_module", module);
    engine.set_module_resolver(static_modules);

    engine.run(SCRIPT)?;

    Ok(())
}

#[cfg(not(feature = "no_function"))]
#[cfg(feature = "internals")]
#[test]
fn test_module_context() -> Result<(), Box<EvalAltResult>> {
    let script = "fn bar() { calc(|x| x + 1) }";

    let mut engine = Engine::new();

    let ast = engine.compile(script)?;

    let module = Module::eval_ast_as_new(Scope::new(), &ast, &engine)?;

    let mut resolver = StaticModuleResolver::new();
    resolver.insert("testing", module);
    engine.set_module_resolver(resolver);

    engine.register_fn(
        "calc",
        |context: NativeCallContext, fp: FnPtr| -> Result<INT, Box<EvalAltResult>> {
            // Store fields for later use
            let engine = context.engine();
            let fn_name = context.fn_name().to_string();
            let source = context.source().map(|s| s.to_string());
            let global = context.global_runtime_state().unwrap().clone();
            let pos = context.position();
            let call_level = context.call_level();

            // Store the paths of the stack of call modules up to this point
            let modules_list: Vec<String> = context
                .iter_namespaces()
                .map(|m| m.id().unwrap_or("testing"))
                .filter(|id| !id.is_empty())
                .map(|id| id.to_string())
                .collect();

            // Recreate the 'NativeCallContext' - requires the 'internals' feature
            let mut libraries = Vec::<Shared<Module>>::new();

            for path in modules_list {
                // Recreate the stack of call modules by resolving each path with
                // the module resolver.
                let module = engine.module_resolver().resolve(engine, None, &path, pos)?;

                libraries.push(module);
            }

            let lib: Vec<&Module> = libraries.iter().map(|m| m.as_ref()).collect();

            let new_context = NativeCallContext::new_with_all_fields(
                engine,
                &fn_name,
                source.as_ref().map(|s| s.as_str()),
                &global,
                &lib,
                pos,
                call_level,
            );

            fp.call_within_context(&new_context, (41 as INT,))
        },
    );

    assert_eq!(
        engine.eval::<INT>(r#"import "testing" as t; t::bar()"#)?,
        42
    );

    Ok(())
}

#[test]
fn test_module_file() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();
    let ast = engine.compile(
        r#"
            import "scripts/module";
            print("top");
        "#,
    )?;
    Module::eval_ast_as_new(Scope::new(), &ast, &engine)?;
    Ok(())
}

#[cfg(not(feature = "no_function"))]
#[test]
fn test_module_environ() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();

    let ast = engine.compile(
        r#"
            const SECRET = 42;

            fn foo(x) {
                print(global::SECRET);
                global::SECRET + x
            }
        "#,
    )?;

    let mut m = Module::eval_ast_as_new(Scope::new(), &ast, &engine)?;

    m.set_id("test");
    m.build_index();

    engine.register_static_module("test", m.into());

    assert_eq!(
        engine.eval::<String>(
            r#"
                const SECRET = "hello";

                fn foo(x) {
                    print(global::SECRET);
                    global::SECRET + x
                }

                let t = test::foo(0);

                foo(t)
            "#
        )?,
        "hello42"
    );

    Ok(())
}

#[test]
fn test_module_dynamic() -> Result<(), Box<EvalAltResult>> {
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

    assert_eq!(engine.eval::<INT>(r#"test2("test", 38);"#)?, 42);

    assert_eq!(
        engine.eval::<INT>(r#"import "test" as test; test::test("test", 38);"#)?,
        42
    );

    Ok(())
}
