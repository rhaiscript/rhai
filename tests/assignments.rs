use rhai::{Engine, EvalAltResult, ParseErrorType, INT};

#[test]
fn test_assignments() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 42; x = 123; x")?, 123);
    assert_eq!(engine.eval::<INT>("let x = 42; x += 123; x")?, 165);

    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<INT>("let x = [42]; x[0] += 123; x[0]")?, 165);

    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>("let x = #{a:42}; x.a += 123; x.a")?, 165);

    Ok(())
}

#[test]
fn test_assignments_bad_lhs() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    assert_eq!(
        *engine
            .compile("(x+y) = 42;")
            .expect_err("should error")
            .err_type(),
        ParseErrorType::AssignmentToInvalidLHS(String::new())
    );
    assert_eq!(
        *engine
            .compile("foo(x) = 42;")
            .expect_err("should error")
            .err_type(),
        ParseErrorType::AssignmentToInvalidLHS(String::new())
    );
    assert_eq!(
        *engine
            .compile("true = 42;")
            .expect_err("should error")
            .err_type(),
        ParseErrorType::AssignmentToConstant(String::new())
    );
    assert_eq!(
        *engine
            .compile("123 = 42;")
            .expect_err("should error")
            .err_type(),
        ParseErrorType::AssignmentToConstant(String::new())
    );

    #[cfg(not(feature = "no_object"))]
    {
        assert_eq!(
            *engine
                .compile("x.foo() = 42;")
                .expect_err("should error")
                .err_type(),
            ParseErrorType::AssignmentToInvalidLHS(String::new())
        );
        assert_eq!(
            *engine
                .compile("x.foo().x.y = 42;")
                .expect_err("should error")
                .err_type(),
            ParseErrorType::AssignmentToInvalidLHS(String::new())
        );
        assert_eq!(
            *engine
                .compile("x.y.z.foo() = 42;")
                .expect_err("should error")
                .err_type(),
            ParseErrorType::AssignmentToInvalidLHS(String::new())
        );
        #[cfg(not(feature = "no_index"))]
        assert_eq!(
            *engine
                .compile("x.foo()[0] = 42;")
                .expect_err("should error")
                .err_type(),
            ParseErrorType::AssignmentToInvalidLHS(String::new())
        );
        #[cfg(not(feature = "no_index"))]
        assert_eq!(
            *engine
                .compile("x[y].z.foo() = 42;")
                .expect_err("should error")
                .err_type(),
            ParseErrorType::AssignmentToInvalidLHS(String::new())
        );
    }

    Ok(())
}
