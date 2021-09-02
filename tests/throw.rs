use rhai::{Engine, EvalAltResult, INT};

#[test]
fn test_throw() {
    let engine = Engine::new();

    assert!(matches!(
        *engine.eval::<()>("if true { throw 42 }").expect_err("expects error"),
        EvalAltResult::ErrorRuntime(s, _) if s.as_int().unwrap() == 42
    ));

    assert!(matches!(
        *engine.eval::<()>(r#"throw"#).expect_err("expects error"),
        EvalAltResult::ErrorRuntime(s, _) if s.is::<()>()
    ));
}

#[test]
fn test_try_catch() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    assert_eq!(
        engine.eval::<INT>("try { throw 42; } catch (x) { return x; }")?,
        42
    );

    assert_eq!(
        engine.eval::<INT>("try { throw 42; } catch { return 123; }")?,
        123
    );

    #[cfg(not(feature = "no_function"))]
    assert_eq!(
        engine.eval::<INT>(
            "
                fn foo(x) { try { throw 42; } catch (x) { return x; } }
                foo(0)
            "
        )?,
        42
    );

    assert_eq!(
        engine.eval::<INT>(
            "
                let err = 123;
                let x = 0;
                try { throw 42; } catch(err) { return err; }
            "
        )?,
        42
    );

    assert_eq!(
        engine.eval::<INT>(
            "
                let foo = 123;
                let x = 0;
                try { throw 42; } catch(err) { return foo; }
            "
        )?,
        123
    );

    assert_eq!(
        engine.eval::<INT>(
            "
                let foo = 123;
                let x = 0;
                try { throw 42; } catch(err) { return err; }
            "
        )?,
        42
    );

    #[cfg(not(feature = "unchecked"))]
    assert!(matches!(
        *engine
            .eval::<()>("try { 42/0; } catch { throw; }")
            .expect_err("expects error"),
        EvalAltResult::ErrorArithmetic(_, _)
    ));

    Ok(())
}
