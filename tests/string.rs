use rhai::{Engine, EvalAltResult};

#[test]
fn test_string() -> Result<(), EvalAltResult> {
    let engine = Engine::new();

    assert_eq!(
        engine.eval::<String>(r#""Test string: \u2764""#)?,
        "Test string: ❤"
    );
    assert_eq!(
        engine.eval::<String>(r#""Test string: \x58""#)?,
        "Test string: X"
    );
    assert_eq!(engine.eval::<String>(r#""\"hello\"""#)?, r#""hello""#);

    assert_eq!(engine.eval::<String>(r#""foo" + "bar""#)?, "foobar");

    assert!(engine.eval::<bool>(r#"let y = "hello, world!"; "world" in y"#)?);
    assert!(engine.eval::<bool>(r#"let y = "hello, world!"; 'w' in y"#)?);
    assert!(!engine.eval::<bool>(r#"let y = "hello, world!"; "hey" in y"#)?);

    #[cfg(not(feature = "no_stdlib"))]
    assert_eq!(engine.eval::<String>(r#""foo" + 123"#)?, "foo123");

    #[cfg(not(feature = "no_stdlib"))]
    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<String>("(42).to_string()")?, "42");

    #[cfg(not(feature = "no_float"))]
    #[cfg(not(feature = "no_stdlib"))]
    assert_eq!(engine.eval::<String>(r#""foo" + 123.4556"#)?, "foo123.4556");

    Ok(())
}

#[test]
fn test_reverse() -> Result<(), EvalAltResult> {
    let engine = Engine::new();

    assert_eq!(
        engine.eval::<String>(
            r#"let x = "\u2764\u2764"; x.reverse()"#
        )?,
        "❤❤"
    );
    assert_eq!(
        engine.eval::<String>(
            r#"let x = "\u2764\u2764\u2764 hello!"; x.reverse()"#
        )?,
        "!olleh ❤❤❤"
    );

    Ok(())
}
