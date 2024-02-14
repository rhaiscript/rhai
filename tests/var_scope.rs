use rhai::{Dynamic, Engine, EvalAltResult, Module, ParseErrorType, Position, Scope, INT};

#[test]
fn test_var_scope() {
    let engine = Engine::new();
    let mut scope = Scope::new();

    engine.run_with_scope(&mut scope, "let x = 4 + 5").unwrap();
    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "x").unwrap(), 9);
    engine.run_with_scope(&mut scope, "x += 1; x += 2;").unwrap();
    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "x").unwrap(), 12);

    scope.set_value("x", 42 as INT);
    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "x").unwrap(), 42);

    engine.run_with_scope(&mut scope, "{ let x = 3 }").unwrap();
    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "x").unwrap(), 42);

    scope.clear();
    engine.run_with_scope(&mut scope, "let x = 3; let x = 42; let x = 123;").unwrap();
    assert_eq!(scope.len(), 1);
    assert_eq!(scope.get_value::<INT>("x").unwrap(), 123);

    scope.clear();
    engine.run_with_scope(&mut scope, "let x = 3; let y = 0; let x = 42; let y = 999; let x = 123;").unwrap();
    assert_eq!(scope.len(), 2);
    assert_eq!(scope.get_value::<INT>("x").unwrap(), 123);
    assert_eq!(scope.get_value::<INT>("y").unwrap(), 999);

    scope.clear();
    engine.run_with_scope(&mut scope, "const x = 3; let y = 0; let x = 42; let y = 999;").unwrap();
    assert_eq!(scope.len(), 2);
    assert_eq!(scope.get_value::<INT>("x").unwrap(), 42);
    assert_eq!(scope.get_value::<INT>("y").unwrap(), 999);
    assert!(!scope.is_constant("x").unwrap());
    assert!(!scope.is_constant("y").unwrap());

    scope.clear();
    engine.run_with_scope(&mut scope, "const x = 3; let y = 0; let x = 42; let y = 999; const x = 123;").unwrap();
    assert_eq!(scope.len(), 2);
    assert_eq!(scope.get_value::<INT>("x").unwrap(), 123);
    assert_eq!(scope.get_value::<INT>("y").unwrap(), 999);
    assert!(scope.is_constant("x").unwrap());
    assert!(!scope.is_constant("y").unwrap());

    scope.clear();
    engine.run_with_scope(&mut scope, "let x = 3; let y = 0; { let x = 42; let y = 999; } let x = 123;").unwrap();

    assert_eq!(scope.len(), 2);
    assert_eq!(scope.get_value::<INT>("x").unwrap(), 123);
    assert_eq!(scope.get_value::<INT>("y").unwrap(), 0);

    assert_eq!(
        engine
            .eval::<INT>(
                "
                        let sum = 0;
                        for x in 0..10 {
                            let x = 42;
                            sum += x;
                        }
                        sum
                    ",
            )
            .unwrap(),
        420
    );

    scope.clear();

    scope.push("x", 42 as INT);
    scope.push_constant("x", 42 as INT);

    let scope2 = scope.clone();
    let scope3 = scope.clone_visible();

    assert_eq!(scope2.is_constant("x"), Some(true));
    assert_eq!(scope3.is_constant("x"), Some(true));
}

#[cfg(not(feature = "unchecked"))]
#[test]
fn test_var_scope_max() {
    let mut engine = Engine::new();
    let mut scope = Scope::new();

    engine.set_max_variables(5);

    engine
        .run_with_scope(
            &mut scope,
            "
                let a = 0;
                let b = 0;
                let c = 0;
                let d = 0;
                let e = 0;
            ",
        )
        .unwrap();

    scope.clear();

    engine
        .run_with_scope(
            &mut scope,
            "
                let a = 0;
                let b = 0;
                let c = 0;
                let d = 0;
                let e = 0;
                let a = 42;     // reuse variable
            ",
        )
        .unwrap();

    scope.clear();

    #[cfg(not(feature = "no_function"))]
    engine
        .run_with_scope(
            &mut scope,
            "
                fn foo(n) {
                    if n > 3 { return; }

                    let w = 0;
                    let x = 0;
                    let y = 0;
                    let z = 0;

                    foo(n + 1);
                }

                let a = 0;
                let b = 0;
                let c = 0;
                let d = 0;
                let e = 0;

                foo(0);
            ",
        )
        .unwrap();

    scope.clear();

    #[cfg(not(feature = "no_function"))]
    engine
        .run_with_scope(
            &mut scope,
            "
                fn foo(a, b, c, d, e) {
                    42
                }

                foo(0, 0, 0, 0, 0);
            ",
        )
        .unwrap();

    scope.clear();

    assert!(matches!(
        *engine
            .run_with_scope(
                &mut scope,
                "
                    let a = 0;
                    let b = 0;
                    let c = 0;
                    let d = 0;
                    let e = 0;
                    let f = 0;
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorTooManyVariables(..)
    ));

    scope.clear();

    #[cfg(not(feature = "no_function"))]
    assert!(matches!(
        *engine
            .run_with_scope(
                &mut scope,
                "
                    fn foo(n) {
                        if n > 3 { return; }

                        let v = 0;
                        let w = 0;
                        let x = 0;
                        let y = 0;
                        let z = 0;

                        foo(n + 1);
                    }
        
                    let a = 0;
                    let b = 0;
                    let c = 0;
                    let d = 0;
                    let e = 0;
                    let f = 0;

                    foo(0);
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorTooManyVariables(..)
    ));

    scope.clear();

    #[cfg(not(feature = "no_function"))]
    assert!(matches!(
        *engine
            .run_with_scope(
                &mut scope,
                "
                    fn foo(a, b, c, d, e, f) {
                        42
                    }
        
                    foo(0, 0, 0, 0, 0, 0);
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorTooManyVariables(..)
    ));
}

#[cfg(not(feature = "no_module"))]
#[test]
fn test_var_scope_alias() {
    let engine = Engine::new();
    let mut scope = Scope::new();

    scope.push("x", 42 as INT);
    scope.set_alias("x", "a");
    scope.set_alias("x", "b");
    scope.set_alias("x", "y");
    scope.push("x", 123 as INT);
    scope.set_alias("x", "b");
    scope.set_alias("x", "c");

    let ast = engine
        .compile(
            "
                let x = 999;
                export x as a;
                export x as c;
                let x = 0;
                export x as z;
            ",
        )
        .unwrap();

    let m = Module::eval_ast_as_new(scope, &ast, &engine).unwrap();

    assert_eq!(m.get_var_value::<INT>("a").unwrap(), 999);
    assert_eq!(m.get_var_value::<INT>("b").unwrap(), 123);
    assert_eq!(m.get_var_value::<INT>("c").unwrap(), 999);
    assert_eq!(m.get_var_value::<INT>("y").unwrap(), 42);
    assert_eq!(m.get_var_value::<INT>("z").unwrap(), 0);
}

#[test]
fn test_var_is_def() {
    let engine = Engine::new();

    assert!(engine
        .eval::<bool>(
            r#"
                let x = 42;
                is_def_var("x")
            "#
        )
        .unwrap());
    assert!(!engine
        .eval::<bool>(
            r#"
                let x = 42;
                is_def_var("y")
            "#
        )
        .unwrap());
    assert!(engine
        .eval::<bool>(
            r#"
                const x = 42;
                is_def_var("x")
            "#
        )
        .unwrap());
}

#[test]
fn test_scope_eval() {
    let engine = Engine::new();

    // First create the state
    let mut scope = Scope::new();

    // Then push some initialized variables into the state
    // NOTE: Remember the default numbers used by Rhai are INT and f64.
    //       Better stick to them or it gets hard to work with other variables in the script.
    scope.push("y", 42 as INT);
    scope.push("z", 999 as INT);

    // First invocation
    engine.run_with_scope(&mut scope, " let x = 4 + 5 - y + z; y = 1;").expect("variables y and z should exist");

    // Second invocation using the same state
    let result = engine.eval_with_scope::<INT>(&mut scope, "x").unwrap();

    println!("result: {result}"); // should print 966

    // Variable y is changed in the script
    assert_eq!(scope.get_value::<INT>("y").expect("variable y should exist"), 1);
}

#[test]
fn test_var_resolver() {
    let mut engine = Engine::new();

    let mut scope = Scope::new();
    scope.push("innocent", 1 as INT);
    scope.push("chameleon", 123 as INT);
    scope.push("DO_NOT_USE", 999 as INT);

    #[cfg(not(feature = "no_closure"))]
    let mut base = Dynamic::ONE.into_shared();
    #[cfg(not(feature = "no_closure"))]
    let shared = base.clone();

    #[allow(deprecated)] // not deprecated but unstable
    engine.on_var(move |name, _, context| {
        match name {
            "MYSTIC_NUMBER" => Ok(Some((42 as INT).into())),
            #[cfg(not(feature = "no_closure"))]
            "HELLO" => Ok(Some(shared.clone())),
            // Override a variable - make it not found even if it exists!
            "DO_NOT_USE" => Err(EvalAltResult::ErrorVariableNotFound(name.to_string(), Position::NONE).into()),
            // Silently maps 'chameleon' into 'innocent'.
            "chameleon" => context
                .scope()
                .get_value("innocent")
                .map(Some)
                .ok_or_else(|| EvalAltResult::ErrorVariableNotFound(name.to_string(), Position::NONE).into()),
            // Return Ok(None) to continue with the normal variable resolution process.
            _ => Ok(None),
        }
    });

    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "MYSTIC_NUMBER").unwrap(), 42);

    #[cfg(not(feature = "no_closure"))]
    {
        assert_eq!(engine.eval::<INT>("HELLO").unwrap(), 1);
        *base.write_lock::<INT>().unwrap() = 42;
        assert_eq!(engine.eval::<INT>("HELLO").unwrap(), 42);
        engine.run("HELLO = 123").unwrap();
        assert_eq!(base.as_int().unwrap(), 123);
        assert_eq!(engine.eval::<INT>("HELLO = HELLO + 1; HELLO").unwrap(), 124);
        assert_eq!(engine.eval::<INT>("HELLO = HELLO * 2; HELLO").unwrap(), 248);
        assert_eq!(base.as_int().unwrap(), 248);
    }

    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "chameleon").unwrap(), 1);
    assert!(matches!(
        *engine.eval_with_scope::<INT>(&mut scope, "DO_NOT_USE").unwrap_err(),
        EvalAltResult::ErrorVariableNotFound(n, ..) if n == "DO_NOT_USE"));
}

#[test]
fn test_var_def_filter() {
    let mut engine = Engine::new();

    let ast = engine.compile("let x = 42;").unwrap();
    engine.run_ast(&ast).unwrap();

    #[allow(deprecated)] // not deprecated but unstable
    engine.on_def_var(|_, info, _| match (info.name(), info.nesting_level()) {
        ("x", 0 | 1) => Ok(false),
        _ => Ok(true),
    });

    assert_eq!(engine.eval::<INT>("let y = 42; let y = 123; let z = y + 1; z").unwrap(), 124);
    assert!(matches!(engine.compile("let x = 42;").unwrap_err().err_type(), ParseErrorType::ForbiddenVariable(s) if s == "x"));
    assert!(matches!(*engine.run_ast(&ast).expect_err("should err"), EvalAltResult::ErrorForbiddenVariable(s, _) if s == "x"));
    assert!(engine.run("const x = 42;").is_err());
    assert!(engine.run("let y = 42; { let x = y + 1; }").is_err());
    assert!(engine.run("let y = 42; { let x = y + 1; }").is_err());
    engine.run("let y = 42; { let z = y + 1; { let x = z + 1; } }").unwrap();
}

#[cfg(not(feature = "no_object"))]
#[test]
fn test_var_scope_cloning() {
    struct Foo {
        field: INT,
    }

    impl Clone for Foo {
        fn clone(&self) -> Self {
            panic!("forbidden to clone!");
        }
    }

    let mut engine = Engine::new();
    engine.register_get_set("field", |foo: &mut Foo| foo.field, |foo: &mut Foo, value| foo.field = value);

    let mut scope = Scope::new();
    scope.push("foo", Foo { field: 1 });

    engine.run_with_scope(&mut scope, "let x = 42; print(x + foo.field);").unwrap();
    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "let x = 42; x + foo.field").unwrap(), 43);
}
