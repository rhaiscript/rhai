use rhai::{Engine, ParseErrorType, INT};

#[test]
fn test_assignments() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 42; x = 123; x").unwrap(), 123);
    assert_eq!(engine.eval::<INT>("let x = 42; x += 123; x").unwrap(), 165);

    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<INT>("let x = [42]; x[0] += 123; x[0]").unwrap(), 165);

    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>("let x = #{a:42}; x.a += 123; x.a").unwrap(), 165);
}

#[test]
fn test_assignments_bad_lhs() {
    let engine = Engine::new();

    assert_eq!(*engine.compile("(x+y) = 42;").unwrap_err().err_type(), ParseErrorType::AssignmentToInvalidLHS(String::new()));
    assert_eq!(*engine.compile("foo(x) = 42;").unwrap_err().err_type(), ParseErrorType::AssignmentToInvalidLHS(String::new()));
    assert_eq!(*engine.compile("true = 42;").unwrap_err().err_type(), ParseErrorType::AssignmentToConstant(String::new()));
    assert_eq!(*engine.compile("123 = 42;").unwrap_err().err_type(), ParseErrorType::AssignmentToConstant(String::new()));

    #[cfg(not(feature = "no_object"))]
    {
        assert_eq!(*engine.compile("x.foo() = 42;").unwrap_err().err_type(), ParseErrorType::AssignmentToInvalidLHS(String::new()));
        assert_eq!(*engine.compile("x.foo().x.y = 42;").unwrap_err().err_type(), ParseErrorType::AssignmentToInvalidLHS(String::new()));
        assert_eq!(*engine.compile("x.y.z.foo() = 42;").unwrap_err().err_type(), ParseErrorType::AssignmentToInvalidLHS(String::new()));
        #[cfg(not(feature = "no_index"))]
        assert_eq!(*engine.compile("x.foo()[0] = 42;").unwrap_err().err_type(), ParseErrorType::AssignmentToInvalidLHS(String::new()));
        #[cfg(not(feature = "no_index"))]
        assert_eq!(*engine.compile("x[y].z.foo() = 42;").unwrap_err().err_type(), ParseErrorType::AssignmentToInvalidLHS(String::new()));
    }
}
