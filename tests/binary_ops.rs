use rhai::{Engine, INT};

#[test]
fn test_binary_ops() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("10 + 4").unwrap(), 14);
    assert_eq!(engine.eval::<INT>("10 - 4").unwrap(), 6);
    assert_eq!(engine.eval::<INT>("10 * 4").unwrap(), 40);
    assert_eq!(engine.eval::<INT>("10 / 4").unwrap(), 2);
    assert_eq!(engine.eval::<INT>("10 % 4").unwrap(), 2);
    assert_eq!(engine.eval::<INT>("10 ** 4").unwrap(), 10000);
    assert_eq!(engine.eval::<INT>("10 << 4").unwrap(), 160);
    assert_eq!(engine.eval::<INT>("10 >> 4").unwrap(), 0);
    assert_eq!(engine.eval::<INT>("10 & 4").unwrap(), 0);
    assert_eq!(engine.eval::<INT>("10 | 4").unwrap(), 14);
    assert_eq!(engine.eval::<INT>("10 ^ 4").unwrap(), 14);

    assert!(engine.eval::<bool>("42 == 42").unwrap());
    assert!(!engine.eval::<bool>("42 != 42").unwrap());
    assert!(!engine.eval::<bool>("42 > 42").unwrap());
    assert!(engine.eval::<bool>("42 >= 42").unwrap());
    assert!(!engine.eval::<bool>("42 < 42").unwrap());
    assert!(engine.eval::<bool>("42 <= 42").unwrap());

    assert_eq!(engine.eval::<INT>("let x = 10; x += 4; x").unwrap(), 14);
    assert_eq!(engine.eval::<INT>("let x = 10; x -= 4; x").unwrap(), 6);
    assert_eq!(engine.eval::<INT>("let x = 10; x *= 4; x").unwrap(), 40);
    assert_eq!(engine.eval::<INT>("let x = 10; x /= 4; x").unwrap(), 2);
    assert_eq!(engine.eval::<INT>("let x = 10; x %= 4; x").unwrap(), 2);
    assert_eq!(engine.eval::<INT>("let x = 10; x **= 4; x").unwrap(), 10000);
    assert_eq!(engine.eval::<INT>("let x = 10; x <<= 4; x").unwrap(), 160);
    assert_eq!(engine.eval::<INT>("let x = 10; x <<= -1; x").unwrap(), 5);
    assert_eq!(engine.eval::<INT>("let x = 10; x >>= 4; x").unwrap(), 0);
    assert_eq!(engine.eval::<INT>("let x = 10; x >>= -2; x").unwrap(), 40);
    assert_eq!(engine.eval::<INT>("let x = 10; x &= 4; x").unwrap(), 0);
    assert_eq!(engine.eval::<INT>("let x = 10; x |= 4; x").unwrap(), 14);
    assert_eq!(engine.eval::<INT>("let x = 10; x ^= 4; x").unwrap(), 14);

    #[cfg(not(feature = "no_float"))]
    {
        use rhai::FLOAT;

        assert_eq!(engine.eval::<FLOAT>("10.0 + 4.0").unwrap(), 14.0);
        assert_eq!(engine.eval::<FLOAT>("10.0 - 4.0").unwrap(), 6.0);
        assert_eq!(engine.eval::<FLOAT>("10.0 * 4.0").unwrap(), 40.0);
        assert_eq!(engine.eval::<FLOAT>("10.0 / 4.0").unwrap(), 2.5);
        assert_eq!(engine.eval::<FLOAT>("10.0 % 4.0").unwrap(), 2.0);
        assert_eq!(engine.eval::<FLOAT>("10.0 ** 4.0").unwrap(), 10000.0);

        assert_eq!(engine.eval::<FLOAT>("10.0 + 4").unwrap(), 14.0);
        assert_eq!(engine.eval::<FLOAT>("10.0 - 4").unwrap(), 6.0);
        assert_eq!(engine.eval::<FLOAT>("10.0 * 4").unwrap(), 40.0);
        assert_eq!(engine.eval::<FLOAT>("10.0 / 4").unwrap(), 2.5);
        assert_eq!(engine.eval::<FLOAT>("10.0 % 4").unwrap(), 2.0);
        assert_eq!(engine.eval::<FLOAT>("10.0 ** 4").unwrap(), 10000.0);

        assert_eq!(engine.eval::<FLOAT>("10 + 4.0").unwrap(), 14.0);
        assert_eq!(engine.eval::<FLOAT>("10 - 4.0").unwrap(), 6.0);
        assert_eq!(engine.eval::<FLOAT>("10 * 4.0").unwrap(), 40.0);
        assert_eq!(engine.eval::<FLOAT>("10 / 4.0").unwrap(), 2.5);
        assert_eq!(engine.eval::<FLOAT>("10 % 4.0").unwrap(), 2.0);
        assert_eq!(engine.eval::<FLOAT>("10 ** 4.0").unwrap(), 10000.0);

        assert!(engine.eval::<bool>("42 == 42.0").unwrap());
        assert!(!engine.eval::<bool>("42 != 42.0").unwrap());
        assert!(!engine.eval::<bool>("42 > 42.0").unwrap());
        assert!(engine.eval::<bool>("42 >= 42.0").unwrap());
        assert!(!engine.eval::<bool>("42 < 42.0").unwrap());
        assert!(engine.eval::<bool>("42 <= 42.0").unwrap());

        assert!(engine.eval::<bool>("42.0 == 42").unwrap());
        assert!(!engine.eval::<bool>("42.0 != 42").unwrap());
        assert!(!engine.eval::<bool>("42.0 > 42").unwrap());
        assert!(engine.eval::<bool>("42.0 >= 42").unwrap());
        assert!(!engine.eval::<bool>("42.0 < 42").unwrap());
        assert!(engine.eval::<bool>("42.0 <= 42").unwrap());

        assert_eq!(engine.eval::<FLOAT>("let x = 10.0; x += 4.0; x").unwrap(), 14.0);
        assert_eq!(engine.eval::<FLOAT>("let x = 10.0; x -= 4.0; x").unwrap(), 6.0);
        assert_eq!(engine.eval::<FLOAT>("let x = 10.0; x *= 4.0; x").unwrap(), 40.0);
        assert_eq!(engine.eval::<FLOAT>("let x = 10.0; x /= 4.0; x").unwrap(), 2.5);
        assert_eq!(engine.eval::<FLOAT>("let x = 10.0; x %= 4.0; x").unwrap(), 2.0);
        assert_eq!(engine.eval::<FLOAT>("let x = 10.0; x **= 4.0; x").unwrap(), 10000.0);

        assert_eq!(engine.eval::<FLOAT>("let x = 10.0; x += 4; x").unwrap(), 14.0);
        assert_eq!(engine.eval::<FLOAT>("let x = 10.0; x -= 4; x").unwrap(), 6.0);
        assert_eq!(engine.eval::<FLOAT>("let x = 10.0; x *= 4; x").unwrap(), 40.0);
        assert_eq!(engine.eval::<FLOAT>("let x = 10.0; x /= 4; x").unwrap(), 2.5);
        assert_eq!(engine.eval::<FLOAT>("let x = 10.0; x %= 4; x").unwrap(), 2.0);
        assert_eq!(engine.eval::<FLOAT>("let x = 10.0; x **= 4; x").unwrap(), 10000.0);
    }

    assert_eq!(engine.eval::<String>(r#""hello" + ", world""#).unwrap(), "hello, world");
    assert_eq!(engine.eval::<String>(r#""hello" + '!'"#).unwrap(), "hello!");
    assert_eq!(engine.eval::<String>(r#""hello" - "el""#).unwrap(), "hlo");
    assert_eq!(engine.eval::<String>(r#""hello" - 'l'"#).unwrap(), "heo");

    assert!(!engine.eval::<bool>(r#""a" == "x""#).unwrap());
    assert!(engine.eval::<bool>(r#""a" != "x""#).unwrap());
    assert!(!engine.eval::<bool>(r#""a" > "x""#).unwrap());
    assert!(!engine.eval::<bool>(r#""a" >= "x""#).unwrap());
    assert!(engine.eval::<bool>(r#""a" < "x""#).unwrap());
    assert!(engine.eval::<bool>(r#""a" <= "x""#).unwrap());

    assert!(engine.eval::<bool>(r#""x" == 'x'"#).unwrap());
    assert!(!engine.eval::<bool>(r#""x" != 'x'"#).unwrap());
    assert!(!engine.eval::<bool>(r#""x" > 'x'"#).unwrap());
    assert!(engine.eval::<bool>(r#""x" >= 'x'"#).unwrap());
    assert!(!engine.eval::<bool>(r#""x" < 'x'"#).unwrap());
    assert!(engine.eval::<bool>(r#""x" <= 'x'"#).unwrap());

    assert!(engine.eval::<bool>(r#"'x' == "x""#).unwrap());
    assert!(!engine.eval::<bool>(r#"'x' != "x""#).unwrap());
    assert!(!engine.eval::<bool>(r#"'x' > "x""#).unwrap());
    assert!(engine.eval::<bool>(r#"'x' >= "x""#).unwrap());
    assert!(!engine.eval::<bool>(r#"'x' < "x""#).unwrap());
    assert!(engine.eval::<bool>(r#"'x' <= "x""#).unwrap());

    // Incompatible types compare to false
    assert!(!engine.eval::<bool>("true == 42").unwrap());
    assert!(engine.eval::<bool>("true != 42").unwrap());
    assert!(!engine.eval::<bool>("true > 42").unwrap());
    assert!(!engine.eval::<bool>("true >= 42").unwrap());
    assert!(!engine.eval::<bool>("true < 42").unwrap());
    assert!(!engine.eval::<bool>("true <= 42").unwrap());

    assert!(!engine.eval::<bool>(r#""42" == 42"#).unwrap());
    assert!(engine.eval::<bool>(r#""42" != 42"#).unwrap());
    assert!(!engine.eval::<bool>(r#""42" > 42"#).unwrap());
    assert!(!engine.eval::<bool>(r#""42" >= 42"#).unwrap());
    assert!(!engine.eval::<bool>(r#""42" < 42"#).unwrap());
    assert!(!engine.eval::<bool>(r#""42" <= 42"#).unwrap());
}

#[test]
fn test_binary_ops_null_coalesce() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 42; x ?? 123").unwrap(), 42);
    assert_eq!(engine.eval::<INT>("let x = (); x ?? 123").unwrap(), 123);
}
