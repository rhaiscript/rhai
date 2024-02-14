use rhai::Engine;

#[test]
fn test_chars() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<char>("'y'").unwrap(), 'y');
    assert_eq!(engine.eval::<char>(r"'\''").unwrap(), '\'');
    assert_eq!(engine.eval::<char>(r#"'"'"#).unwrap(), '"');
    assert_eq!(engine.eval::<char>(r"'\u2764'").unwrap(), '❤');

    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<char>(r#"let x="hello"; x[2]"#).unwrap(), 'l');
    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<String>(r#"let y="hello"; y[2]='$'; y"#).unwrap(), "he$lo");

    assert!(engine.eval::<char>(r"'\uhello'").is_err());
    assert!(engine.eval::<char>("''").is_err());
}
