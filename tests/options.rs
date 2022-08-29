use rhai::{Engine, EvalAltResult, Scope, INT};

#[test]
fn test_options_allow() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();

    engine.compile("let x = if y { z } else { w };")?;

    engine.set_allow_if_expression(false);

    assert!(engine.compile("let x = if y { z } else { w };").is_err());

    engine.compile("let x = { let z = 0; z + 1 };")?;

    engine.set_allow_statement_expression(false);

    assert!(engine.compile("let x = { let z = 0; z + 1 };").is_err());

    #[cfg(not(feature = "no_function"))]
    {
        engine.compile("let x = || 42;")?;

        engine.set_allow_anonymous_fn(false);

        assert!(engine.compile("let x = || 42;").is_err());
    }

    let ast = engine.compile("let x = 0; while x < 10 { x += 1; }")?;

    engine.set_allow_looping(false);

    engine.run_ast(&ast)?;

    assert!(engine
        .compile("let x = 0; while x < 10 { x += 1; }")
        .is_err());

    engine.compile("let x = 42; let x = 123;")?;

    engine.set_allow_shadowing(false);

    assert!(engine.compile("let x = 42; let x = 123;").is_err());
    assert!(engine.compile("const x = 42; let x = 123;").is_err());
    assert!(engine.compile("let x = 42; const x = 123;").is_err());
    assert!(engine.compile("const x = 42; const x = 123;").is_err());

    let mut scope = Scope::new();
    scope.push("x", 42 as INT);

    assert!(engine.run_with_scope(&mut scope, "let x = 42;").is_err());

    Ok(())
}

#[test]
fn test_options_strict_var() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();

    engine.compile("let x = if y { z } else { w };")?;

    #[cfg(not(feature = "no_function"))]
    engine.compile("fn foo(x) { x + y }")?;

    #[cfg(not(feature = "no_module"))]
    engine.compile("print(h::y::z);")?;

    #[cfg(not(feature = "no_module"))]
    engine.compile("let x = h::y::foo();")?;

    #[cfg(not(feature = "no_function"))]
    #[cfg(not(feature = "no_module"))]
    engine.compile("fn foo() { h::y::foo() }")?;

    #[cfg(not(feature = "no_function"))]
    engine.compile("let f = |y| x * y;")?;

    let mut scope = Scope::new();
    scope.push("x", 42 as INT);
    scope.push_constant("y", 0 as INT);

    engine.set_strict_variables(true);

    assert!(engine.compile("let x = if y { z } else { w };").is_err());

    engine.compile_with_scope(&mut scope, "if x.abs() { y } else { x + y.len };")?;

    engine.compile("let y = 42; let x = y;")?;

    assert_eq!(
        engine.eval_with_scope::<INT>(&mut scope, "{ let y = 42; x * y }")?,
        42 * 42
    );

    #[cfg(not(feature = "no_function"))]
    assert!(engine.compile("fn foo(x) { x + y }").is_err());

    #[cfg(not(feature = "no_function"))]
    #[cfg(not(feature = "no_module"))]
    {
        assert!(engine.compile("print(h::y::z);").is_err());
        assert!(engine.compile("fn foo() { h::y::z }").is_err());
        assert!(engine.compile("fn foo() { h::y::foo() }").is_err());
        engine.compile(r#"import "hello" as h; fn foo() { h::a::b::c } print(h::y::z);"#)?;
        assert!(engine.compile("let x = h::y::foo();").is_err());
        engine.compile(r#"import "hello" as h; fn foo() { h::a::b::c() } let x = h::y::foo();"#)?;
    }

    #[cfg(not(feature = "no_function"))]
    {
        assert_eq!(
            engine.eval_with_scope::<INT>(&mut scope, "fn foo(z) { z } let f = foo; call(f, x)")?,
            42
        );
        assert!(engine.compile("let f = |y| x * y;").is_err());
        #[cfg(not(feature = "no_closure"))]
        {
            engine.compile("let x = 42; let f = |y| x * y;")?;
            engine.compile("let x = 42; let f = |y| { || x + y };")?;
            assert!(engine.compile("fn foo() { |y| { || x + y } }").is_err());
        }
        #[cfg(not(feature = "no_optimize"))]
        assert_eq!(
            engine.eval_with_scope::<INT>(&mut scope, "fn foo(z) { y + z } foo(x)")?,
            42
        );
    }

    Ok(())
}
