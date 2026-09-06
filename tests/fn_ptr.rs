use rhai::{Dynamic, Engine, EvalAltResult, FnPtr, NativeCallContext, Scope, INT};

#[test]
fn test_fn_ptr() {
    let mut engine = Engine::new();

    engine.register_fn("bar", |x: &mut INT, y: INT| *x += y);

    #[cfg(not(feature = "no_object"))]
    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let f = Fn("bar");
                    let x = 40;
                    f.call(x, 2);
                    x
                "#
            )
            .unwrap(),
        40
    );

    #[cfg(not(feature = "no_object"))]
    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let f = Fn("bar");
                    let x = 40;
                    x.call(f, 2);
                    x
                "#
            )
            .unwrap(),
        42
    );

    // assert_eq!(
    //     engine
    //         .eval::<INT>(
    //             r#"
    //                 let f = Fn("bar");
    //                 let x = 40;
    //                 call(f, x, 2);
    //                 x
    //             "#
    //         )
    //         .unwrap(),
    //     42
    // );

    #[cfg(not(feature = "no_function"))]
    #[cfg(not(feature = "no_object"))]
    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    fn foo(x) { this += x; }

                    let f = Fn("foo");
                    let x = 40;
                    x.call(f, 2);
                    x
                "#
            )
            .unwrap(),
        42
    );

    #[cfg(not(feature = "no_function"))]
    assert!(matches!(
        *engine
            .eval::<INT>(
                r#"
                    fn foo(x) { this += x; }

                    let f = Fn("foo");
                    call(f, 2);
                    x
                "#
            )
            .unwrap_err(),
        EvalAltResult::ErrorInFunctionCall(fn_name, _, err, ..)
            if fn_name == "foo" && matches!(*err, EvalAltResult::ErrorUnboundThis(..))
    ));

    #[cfg(not(feature = "no_function"))]
    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    fn foo(x) { x + 1 }
                    let f = foo;
                    let g = 42;
                    g = foo;
                    call(f, 39) + call(g, 1)
                "#
            )
            .unwrap(),
        42
    );
}

#[test]
fn test_fn_ptr_curry() {
    let mut engine = Engine::new();

    engine.register_fn("foo", |x: &mut INT, y: INT| *x + y);

    #[cfg(not(feature = "no_object"))]
    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let f = Fn("foo");
                    let f2 = f.curry(40);
                    f2.call(2)
                "#
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let f = Fn("foo");
                    let f2 = curry(f, 40);
                    call(f2, 2)
                "#
            )
            .unwrap(),
        42
    );
}

#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_index"))]
#[cfg(not(feature = "no_object"))]
#[test]
fn test_fn_ptr_curry_arg_order() {
    let engine = Engine::new();

    assert_eq!(
        engine
            .eval::<rhai::Array>(
                r#"
                    fn sub(b) { this - b }
                    [1, 2, 3].map(Fn("sub").curry(10))
                "#
            )
            .unwrap()
            .into_iter()
            .map(|v| v.as_int().unwrap())
            .collect::<Vec<_>>(),
        [-9, -8, -7]
    );

    assert_eq!(
        engine
            .eval::<rhai::Array>(
                r#"
                    fn sub(a, b) { a - b }
                    [1, 2, 3].map(Fn("sub").curry(10))
                "#
            )
            .unwrap()
            .into_iter()
            .map(|v| v.as_int().unwrap())
            .collect::<Vec<_>>(),
        [9, 8, 7]
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    fn sub(a, b) { a - b }
                    Fn("sub").curry(10).call(1)
                "#
            )
            .unwrap(),
        9
    );
}

#[test]
#[cfg(not(feature = "no_object"))]
fn test_fn_ptr_curry_native_call_raw_order() {
    let mut engine = Engine::new();

    engine.register_fn("foo", |context: NativeCallContext, fp: FnPtr| -> Result<INT, Box<rhai::EvalAltResult>> {
        let mut this = Dynamic::from(2 as INT);
        let result: INT = fp.call_as_method_within_context(&context, &mut this, (3 as INT,))?;
        Ok(this.as_int().unwrap() * 1000 + result)
    });
    engine.register_raw_fn::<INT>("native", [std::any::TypeId::of::<INT>(), std::any::TypeId::of::<INT>(), std::any::TypeId::of::<INT>()], |_, args| {
        let a = args[0].as_int().unwrap();
        let c = args[2].as_int().unwrap();
        let b = &mut *args[1].write_lock::<INT>().unwrap();
        *b = a * 100 + *b * 10 + c;
        Ok((*b).into())
    });

    let mut fp = FnPtr::new("native").unwrap();
    fp.add_curry((1 as INT).into());

    let mut scope = Scope::new();
    scope.push("fp", fp);

    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "foo(fp)").unwrap(), 2123);
}

#[test]
#[cfg(not(feature = "no_index"))]
#[cfg(not(feature = "no_object"))]
fn test_fn_ptr_curry_native_callback_arg_order() {
    let mut engine = Engine::new();
    engine.register_fn("sub", |a: INT, b: INT| a - b);

    assert_eq!(
        engine
            .eval::<rhai::Array>(
                r#"
                    [1, 2, 3].map(Fn("sub").curry(10))
                "#,
            )
            .unwrap()
            .into_iter()
            .map(|v| v.as_int().unwrap())
            .collect::<Vec<_>>(),
        [9, 8, 7]
    );
}

#[test]
#[cfg(not(feature = "no_function"))]
fn test_fn_ptr_call() {
    let engine = Engine::new();

    let ast = engine.compile("private fn foo(x, y) { len(x) + y }").unwrap();

    let mut fn_ptr = FnPtr::new("foo").unwrap();
    fn_ptr.set_curry(vec!["abc".into()]);
    let result: INT = fn_ptr.call(&engine, &ast, (39 as INT,)).unwrap();

    assert_eq!(result, 42);
}

/// A bare function name is a function pointer, and stays one after something
/// has set `always_search_scope`.
///
/// That flag means "do not trust the parse-time variable indices". A name that
/// resolves to a function is not a variable and has no index to distrust, but
/// the check for one used to sit behind the flag — so an `eval` that changed
/// the scope made every later use of a bare function name report it as an
/// unknown variable.
#[test]
#[cfg(not(feature = "no_function"))]
fn test_fn_ptr_from_bare_name_survives_a_scope_change() {
    let engine = Engine::new();

    // Called function-style, which `no_object` leaves in the language where it
    // removes the `f.call(..)` spelling.
    assert_eq!(engine.eval::<INT>("fn dbl(x) { x * 2 } let f = dbl; call(f, 4)").unwrap(), 8);

    // And the same once `eval` has changed the scope.
    assert_eq!(engine.eval::<INT>(r#"fn dbl(x) { x * 2 } eval("let m = 2;"); let f = dbl; call(f, 4)"#).unwrap(), 8,);

    // A variable of the same name still wins, which is what the flag is for.
    assert_eq!(engine.eval::<INT>(r#"fn dbl(x) { x * 2 } eval("let m = 2;"); let dbl = 7; dbl"#).unwrap(), 7,);
}

#[test]
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_object"))]
fn test_fn_ptr_method_call() {
    let engine = Engine::new();

    let ast = engine.compile("private fn foo(x, y) { this = x.len() + y }").unwrap();

    let mut fn_ptr = FnPtr::new("foo").unwrap();
    fn_ptr.set_curry(vec!["abc".into()]);
    let mut obj = (123 as INT).into();
    let _ = fn_ptr.call_as_method::<()>(&engine, &ast, &mut obj, (39 as INT,)).unwrap();

    assert_eq!(obj.as_int().unwrap(), 42);
}

#[test]
#[cfg(not(feature = "no_closure"))]
fn test_fn_ptr_make_closure() {
    let f = {
        let engine = Engine::new();

        let ast = engine
            .compile(
                r#"
                    let test = "hello";
                    |x| test + x            // this creates a closure
                "#,
            )
            .unwrap();

        let fn_ptr = engine.eval_ast::<FnPtr>(&ast).unwrap();

        move |x: INT| -> Result<String, _> { fn_ptr.call(&engine, &ast, (x,)) }
    };

    // 'f' captures: the Engine, the AST, and the closure
    assert_eq!(f(42).unwrap(), "hello42");
}

#[test]
fn test_fn_ptr_embed() {
    let engine = Engine::new();

    let f1 = FnPtr::from_fn("foo", |_, args| {
        if args.len() != 2 {
            panic!();
        }
        let y = args[1].as_int().unwrap();
        let x = &mut *args[0].write_lock::<INT>().unwrap();
        *x += y;
        Ok(Dynamic::UNIT)
    })
    .unwrap();

    let mut scope = Scope::new();
    scope.push("f1", f1);

    assert_eq!(
        engine
            .eval_with_scope::<INT>(
                &mut scope,
                r#"
                    let x = 40;
                    call(f1, x, 2);
                    x
                "#,
            )
            .unwrap(),
        40,
    );

    #[cfg(not(feature = "no_object"))]
    {
        let f2 = FnPtr::from_fn("foo", |_, args| {
            if args.len() != 2 {
                panic!();
            }
            let y = args[1].as_int().unwrap();
            let map = &mut *args[0].as_map_mut().unwrap();
            let x = &mut *map.get_mut("a").unwrap().write_lock::<INT>().unwrap();
            *x += y;
            Ok(Dynamic::UNIT)
        })
        .unwrap();

        scope.push("f2", f2);

        assert_eq!(
            engine
                .eval_with_scope::<INT>(
                    &mut scope,
                    r#"
                        let x = 40;
                        f1.call(x, 2);
                        x
                    "#,
                )
                .unwrap(),
            40,
        );
        assert_eq!(
            engine
                .eval_with_scope::<INT>(
                    &mut scope,
                    r#"
                        let x = 40;
                        x.call(f1, 2);
                        x
                    "#,
                )
                .unwrap(),
            42,
        );
        // assert_eq!(
        //     engine
        //         .eval_with_scope::<INT>(
        //             &mut scope,
        //             r#"
        //                 let x = #{ a: 40, xyz: f2 };
        //                 x.xyz(2);
        //                 x.a
        //             "#,
        //         )
        //         .unwrap(),
        //     42,
        // );
    }
}

#[test]
#[cfg(not(feature = "no_object"))]
fn test_fn_ptr_curry_embedded_native_call_raw_order() {
    let mut engine = Engine::new();

    engine.register_fn("foo", |context: NativeCallContext, fp: FnPtr| -> Result<INT, Box<rhai::EvalAltResult>> {
        let mut this = Dynamic::from(2 as INT);
        fp.call_as_method_within_context(&context, &mut this, (3 as INT,))
    });

    let mut fp = FnPtr::from_fn("native", |_, args| {
        let digits = args.iter().map(|value| value.as_int().unwrap()).collect::<Vec<_>>();
        Ok((digits[0] * 100 + digits[1] * 10 + digits[2]).into())
    })
    .unwrap();
    fp.add_curry((1 as INT).into());

    let mut scope = Scope::new();
    scope.push("fp", fp);

    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "foo(fp)").unwrap(), 123);
}
