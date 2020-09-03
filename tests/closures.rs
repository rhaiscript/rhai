#![cfg(not(feature = "no_function"))]
use rhai::{Dynamic, Engine, EvalAltResult, FnPtr, Module, ParseErrorType, RegisterFn, Scope, INT};
use std::any::TypeId;
use std::cell::RefCell;
use std::mem::take;
use std::rc::Rc;

#[cfg(not(feature = "no_object"))]
use rhai::Map;

#[test]
fn test_fn_ptr_curry_call() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();

    #[allow(deprecated)]
    engine.register_raw_fn(
        "call_with_arg",
        &[TypeId::of::<FnPtr>(), TypeId::of::<INT>()],
        |engine: &Engine, lib: &Module, args: &mut [&mut Dynamic]| {
            let fn_ptr = std::mem::take(args[0]).cast::<FnPtr>();
            fn_ptr.call_dynamic(engine, lib, None, [std::mem::take(args[1])])
        },
    );

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
#[cfg(not(feature = "no_closure"))]
#[cfg(not(feature = "no_object"))]
fn test_closures() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();

    assert!(matches!(
        *engine
            .compile_expression("let f = |x| {};")
            .expect_err("should error")
            .0,
        ParseErrorType::BadInput(_)
    ));

    assert_eq!(
        engine.eval::<INT>(
            r#"
                let x = 8;

                let res = |y, z| {
                    let w = 12;

                    return (|| x + y + z + w).call();
                }.curry(15).call(2);

                res + (|| x - 3).call()
            "#
        )?,
        42
    );

    assert_eq!(
        engine.eval::<INT>(
            r#"
                let a = 41;
                let foo = |x| { a += x };
                foo.call(1);
                a
            "#
        )?,
        42
    );

    assert!(engine.eval::<bool>(
        r#"
            let a = 41;
            let foo = |x| { a += x };
            a.is_shared()
        "#
    )?);

    engine.register_fn("plus_one", |x: INT| x + 1);

    assert_eq!(
        engine.eval::<INT>(
            r#"
                let a = 41;
                let f = || plus_one(a);
                f.call()
            "#
        )?,
        42
    );

    assert_eq!(
        engine.eval::<INT>(
            r#"
                let a = 40;
                let f = |x| {
                    let f = |x| {
                        let f = |x| plus_one(a) + x;
                        f.call(x)
                    };
                    f.call(x)
                };
                f.call(1)
            "#
        )?,
        42
    );

    assert_eq!(
        engine.eval::<INT>(
            r#"
                let a = 21;
                let f = |x| a += x;
                f.call(a);
                a
            "#
        )?,
        42
    );

    #[allow(deprecated)]
    engine.register_raw_fn(
        "custom_call",
        &[TypeId::of::<INT>(), TypeId::of::<FnPtr>()],
        |engine: &Engine, module: &Module, args: &mut [&mut Dynamic]| {
            let func = take(args[1]).cast::<FnPtr>();

            func.call_dynamic(engine, module, None, [])
        },
    );

    assert_eq!(
        engine.eval::<INT>(
            r#"
                let a = 41;
                let b = 0;
                let f = || b.custom_call(|| a + 1);
                
                f.call()
            "#
        )?,
        42
    );

    Ok(())
}

#[test]
#[cfg(not(feature = "no_closure"))]
#[cfg(not(feature = "no_object"))]
#[cfg(not(feature = "sync"))]
fn test_closures_data_race() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    assert_eq!(
        engine.eval::<INT>(
            r#"
                let a = 1;
                let b = 40;
                let foo = |x| { this += a + x };
                b.call(foo, 1);
                b
            "#
        )?,
        42
    );

    assert!(matches!(
        *engine
            .eval::<INT>(
                r#"
                    let a = 20;
                    let foo = |x| { this += a + x };
                    a.call(foo, 1);
                    a
                "#
            )
            .expect_err("should error"),
        EvalAltResult::ErrorDataRace(_, _)
    ));

    Ok(())
}

type MyType = Rc<RefCell<INT>>;

#[test]
#[cfg(not(feature = "no_object"))]
#[cfg(not(feature = "sync"))]
fn test_closures_shared_obj() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();

    // Register API on MyType
    engine
        .register_type_with_name::<MyType>("MyType")
        .register_get_set(
            "data",
            |p: &mut MyType| *p.borrow(),
            |p: &mut MyType, value: INT| *p.borrow_mut() = value,
        )
        .register_fn("+=", |p1: &mut MyType, p2: MyType| {
            *p1.borrow_mut() += *p2.borrow()
        })
        .register_fn("-=", |p1: &mut MyType, p2: MyType| {
            *p1.borrow_mut() -= *p2.borrow()
        });

    let engine = engine; // Make engine immutable

    let code = r#"
        #{
            name: "A",
            description: "B",
            cost: 1,
            health_added: 0,
            action: |p1, p2| { p1 += p2 }
        }
    "#;

    let ast = engine.compile(code)?;
    let res = engine.eval_ast::<Map>(&ast)?;

    // Make closure
    let f = move |p1: MyType, p2: MyType| -> Result<(), Box<EvalAltResult>> {
        let action_ptr = res["action"].clone().cast::<FnPtr>();
        let name = action_ptr.fn_name();
        engine.call_fn::<_, ()>(&mut Scope::new(), &ast, name, (p1, p2))
    };

    // Test closure
    let p1 = Rc::new(RefCell::new(41));
    let p2 = Rc::new(RefCell::new(1));

    f(p1.clone(), p2.clone())?;

    assert_eq!(*p1.borrow(), 42);

    Ok(())
}

#[test]
#[cfg(not(feature = "no_closure"))]
fn test_closures_external() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    let mut ast = engine.compile(
        r#"
            let test = "hello";

            |x| test + x
    "#,
    )?;

    // Save the function pointer together with captured variables
    let fn_ptr = engine.eval_ast::<FnPtr>(&ast)?;

    // Get rid of the script, retaining only functions
    ast.retain_functions(|_, _, _| true);

    // Closure 'f' captures: the engine, the AST, and the curried function pointer
    let f = move |x: INT| fn_ptr.call_dynamic(&engine, ast, None, [x.into()]);

    assert_eq!(f(42)?.as_str(), Ok("hello42"));

    Ok(())
}
