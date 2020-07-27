#![cfg(not(feature = "no_function"))]
use rhai::{
    Dynamic, Engine, EvalAltResult, FnPtr, Func, Module, ParseError, ParseErrorType, Scope, INT,
};
use std::any::TypeId;

#[test]
fn test_fn() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Expect duplicated parameters error
    assert!(matches!(
        engine
            .compile("fn hello(x, x) { x }")
            .expect_err("should be error"),
        ParseError(x, _) if *x == ParseErrorType::FnDuplicatedParam("hello".to_string(), "x".to_string())
    ));

    Ok(())
}

#[test]
fn test_call_fn() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();
    let mut scope = Scope::new();

    scope.push("foo", 42 as INT);

    let ast = engine.compile(
        r"
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
        ",
    )?;

    let r: INT = engine.call_fn(&mut scope, &ast, "hello", (42 as INT, 123 as INT))?;
    assert_eq!(r, 165);

    let r: INT = engine.call_fn(&mut scope, &ast, "hello", (123 as INT,))?;
    assert_eq!(r, 5166);

    let r: INT = engine.call_fn(&mut scope, &ast, "hello", ())?;
    assert_eq!(r, 42);

    assert_eq!(
        scope
            .get_value::<INT>("foo")
            .expect("variable foo should exist"),
        1
    );

    Ok(())
}

#[test]
fn test_call_fn_private() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();
    let mut scope = Scope::new();

    let ast = engine.compile("fn add(x, n) { x + n }")?;

    let r: INT = engine.call_fn(&mut scope, &ast, "add", (40 as INT, 2 as INT))?;
    assert_eq!(r, 42);

    let ast = engine.compile("private fn add(x, n, ) { x + n }")?;

    assert!(matches!(
        *engine.call_fn::<_, INT>(&mut scope, &ast, "add", (40 as INT, 2 as INT))
            .expect_err("should error"),
        EvalAltResult::ErrorFunctionNotFound(fn_name, _) if fn_name == "add"
    ));

    Ok(())
}

#[test]
fn test_anonymous_fn() -> Result<(), Box<EvalAltResult>> {
    let calc_func = Func::<(INT, INT, INT), INT>::create_from_script(
        Engine::new(),
        "fn calc(x, y, z,) { (x + y) * z }",
        "calc",
    )?;

    assert_eq!(calc_func(42, 123, 9)?, 1485);

    let calc_func = Func::<(INT, String, INT), INT>::create_from_script(
        Engine::new(),
        "fn calc(x, y, z) { (x + len(y)) * z }",
        "calc",
    )?;

    assert_eq!(calc_func(42, "hello".to_string(), 9)?, 423);

    let calc_func = Func::<(INT, INT, INT), INT>::create_from_script(
        Engine::new(),
        "private fn calc(x, y, z) { (x + y) * z }",
        "calc",
    )?;

    assert!(matches!(
        *calc_func(42, 123, 9).expect_err("should error"),
        EvalAltResult::ErrorFunctionNotFound(fn_name, _) if fn_name == "calc"
    ));

    Ok(())
}

#[test]
#[cfg(not(feature = "no_object"))]
fn test_fn_ptr_raw() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();

    engine.register_raw_fn(
        "bar",
        &[
            TypeId::of::<INT>(),
            TypeId::of::<FnPtr>(),
            TypeId::of::<INT>(),
        ],
        move |engine: &Engine, lib: &Module, args: &mut [&mut Dynamic]| {
            let fp = std::mem::take(args[1]).cast::<FnPtr>();
            let value = args[2].clone();
            let this_ptr = args.get_mut(0).unwrap();

            engine.call_fn_dynamic(
                &mut Scope::new(),
                lib,
                fp.fn_name(),
                Some(this_ptr),
                [value],
            )?;

            Ok(())
        },
    );

    assert_eq!(
        engine.eval::<INT>(
            r#"
                fn foo(x) { this += x; }

                let x = 41;
                x.bar(Fn("foo"), 1);
                x
            "#
        )?,
        42
    );

    Ok(())
}

#[test]
fn test_fn_ptr_curry_call() -> Result<(), Box<EvalAltResult>> {
    let mut module = Module::new();

    module.set_raw_fn(
        "call_with_arg",
        &[TypeId::of::<FnPtr>(), TypeId::of::<INT>()],
        |engine: &Engine, lib: &Module, args: &mut [&mut Dynamic]| {
            let fn_ptr = std::mem::take(args[0]).cast::<FnPtr>();
            fn_ptr.call_dynamic(engine, lib, None, [std::mem::take(args[1])])
        },
    );

    let mut engine = Engine::new();
    engine.load_package(module.into());

    #[cfg(not(feature = "no_object"))]
    assert_eq!(
        engine.eval::<INT>(
            r#"
                let addition = |x, y| { x + y };
                let curried = addition.curry(2);

                call_with_arg(curried, 40)
            "#
        )?,
        42
    );

    Ok(())
}

#[test]
fn test_fn_closures() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();

    let res = engine.eval::<INT>(
        r#"
            let z = shared(150);

            let f1 = || {
                let res;

                for x in [1, 2, 3, 4, 5, 6] {
                    res = (|| (|| x + z).call()).call();
                }

                res;
            };

            f1.call()
        "#
    ).unwrap();

    panic!("{:#?}", res);

    Ok(())
}
