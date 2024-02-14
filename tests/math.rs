use rhai::{Engine, EvalAltResult, INT};

#[cfg(not(feature = "no_float"))]
use rhai::FLOAT;

#[test]
fn test_math() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("1 + 2").unwrap(), 3);
    assert_eq!(engine.eval::<INT>("1 - 2").unwrap(), -1);
    assert_eq!(engine.eval::<INT>("2 * 3").unwrap(), 6);
    assert_eq!(engine.eval::<INT>("1 / 2").unwrap(), 0);
    assert_eq!(engine.eval::<INT>("3 % 2").unwrap(), 1);

    #[cfg(not(feature = "no_float"))]
    assert!((engine.eval::<FLOAT>("sin(PI()/6.0)").unwrap() - 0.5).abs() < 0.001);

    #[cfg(not(feature = "no_float"))]
    assert!(engine.eval::<FLOAT>("cos(PI()/2.0)").unwrap().abs() < 0.001);

    #[cfg(not(feature = "only_i32"))]
    assert_eq!(engine.eval::<INT>("abs(-9223372036854775807)").unwrap(), 9_223_372_036_854_775_807);

    #[cfg(feature = "only_i32")]
    assert_eq!(engine.eval::<INT>("abs(-2147483647)").unwrap(), 2147483647);

    // Overflow/underflow/division-by-zero errors
    #[cfg(not(feature = "unchecked"))]
    {
        #[cfg(not(feature = "only_i32"))]
        {
            assert!(matches!(*engine.eval::<INT>("abs(-9223372036854775808)").expect_err("expects negation overflow"), EvalAltResult::ErrorArithmetic(..)));
            assert!(matches!(*engine.eval::<INT>("9223372036854775807 + 1").expect_err("expects overflow"), EvalAltResult::ErrorArithmetic(..)));
            assert!(matches!(*engine.eval::<INT>("-9223372036854775808 - 1").expect_err("expects underflow"), EvalAltResult::ErrorArithmetic(..)));
            assert!(matches!(*engine.eval::<INT>("9223372036854775807 * 9223372036854775807").expect_err("expects overflow"), EvalAltResult::ErrorArithmetic(..)));
            assert!(matches!(*engine.eval::<INT>("9223372036854775807 / 0").expect_err("expects division by zero"), EvalAltResult::ErrorArithmetic(..)));
            assert!(matches!(*engine.eval::<INT>("9223372036854775807 % 0").expect_err("expects division by zero"), EvalAltResult::ErrorArithmetic(..)));
        }

        #[cfg(feature = "only_i32")]
        {
            assert!(matches!(*engine.eval::<INT>("2147483647 + 1").expect_err("expects overflow"), EvalAltResult::ErrorArithmetic(..)));
            assert!(matches!(*engine.eval::<INT>("-2147483648 - 1").expect_err("expects underflow"), EvalAltResult::ErrorArithmetic(..)));
            assert!(matches!(*engine.eval::<INT>("2147483647 * 2147483647").expect_err("expects overflow"), EvalAltResult::ErrorArithmetic(..)));
            assert!(matches!(*engine.eval::<INT>("2147483647 / 0").expect_err("expects division by zero"), EvalAltResult::ErrorArithmetic(..)));
            assert!(matches!(*engine.eval::<INT>("2147483647 % 0").expect_err("expects division by zero"), EvalAltResult::ErrorArithmetic(..)));
        }
    }
}

#[test]
fn test_math_parse() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>(r#"parse_int("42")"#).unwrap(), 42);
    assert_eq!(engine.eval::<INT>(r#"parse_int("42", 16)"#).unwrap(), 0x42);
    assert_eq!(engine.eval::<INT>(r#"parse_int("abcdef", 16)"#).unwrap(), 0xabcdef);
}
