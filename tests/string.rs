use rhai::{Engine, EvalAltResult, ImmutableString, Scope, INT};

#[test]
fn test_string() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<String>(r#""Test string: \u2764""#).unwrap(), "Test string: ❤");
    assert_eq!(engine.eval::<String>(r#""Test string: ""\u2764""""#).unwrap(), r#"Test string: "❤""#);
    assert_eq!(engine.eval::<String>("\"Test\rstring: \\u2764\"").unwrap(), "Test\rstring: ❤");
    assert_eq!(
        engine.eval::<String>("   \"Test string: \\u2764\\\n     hello, world!\"").unwrap(),
        if cfg!(not(feature = "no_position")) { "Test string: ❤ hello, world!" } else { "Test string: ❤     hello, world!" }
    );
    assert_eq!(engine.eval::<String>("     `Test string: \\u2764\nhello,\\nworld!`").unwrap(), "Test string: \\u2764\nhello,\\nworld!");
    assert_eq!(engine.eval::<String>(r#"     `Test string: \\u2764\n``hello``,\\n"world"!`"#).unwrap(), r#"Test string: \\u2764\n`hello`,\\n"world"!"#);
    assert_eq!(engine.eval::<String>("     `\nTest string: \\u2764\nhello,\\nworld!`").unwrap(), "Test string: \\u2764\nhello,\\nworld!");
    assert_eq!(engine.eval::<String>("     `\r\nTest string: \\u2764\nhello,\\nworld!`").unwrap(), "Test string: \\u2764\nhello,\\nworld!");
    assert_eq!(engine.eval::<String>(r#""Test string: \x58""#).unwrap(), "Test string: X");
    assert_eq!(engine.eval::<String>(r#""\"hello\"""#).unwrap(), r#""hello""#);

    assert_eq!(engine.eval::<String>(r#""foo" + "bar""#).unwrap(), "foobar");

    assert!(engine.eval::<bool>(r#"let y = "hello, world!"; "world" in y"#).unwrap());
    assert!(engine.eval::<bool>(r#"let y = "hello, world!"; 'w' in y"#).unwrap());
    assert!(!engine.eval::<bool>(r#"let y = "hello, world!"; "hey" in y"#).unwrap());

    assert_eq!(engine.eval::<String>(r#""foo" + 123"#).unwrap(), "foo123");

    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<String>("to_string(42)").unwrap(), "42");

    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<char>(r#"let y = "hello"; y[1]"#).unwrap(), 'e');

    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<char>(r#"let y = "hello"; y[-1]"#).unwrap(), 'o');

    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<char>(r#"let y = "hello"; y[-4]"#).unwrap(), 'e');

    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>(r#"let y = "hello"; y.len"#).unwrap(), 5);

    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>(r#"let y = "hello"; y.clear(); y.len"#).unwrap(), 0);

    assert_eq!(engine.eval::<INT>(r#"let y = "hello"; len(y)"#).unwrap(), 5);

    #[cfg(not(feature = "no_object"))]
    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<char>(r#"let y = "hello"; y[y.len-1]"#).unwrap(), 'o');

    #[cfg(not(feature = "no_float"))]
    assert_eq!(engine.eval::<String>(r#""foo" + 123.4556"#).unwrap(), "foo123.4556");
}

#[test]
fn test_string_dynamic() {
    let engine = Engine::new();
    let mut scope = Scope::new();
    scope.push("x", "foo");
    scope.push("y", "foo");
    scope.push("z", "foo");

    assert!(engine.eval_with_scope::<bool>(&mut scope, r#"x == "foo""#).unwrap());
    assert!(engine.eval_with_scope::<bool>(&mut scope, r#"y == "foo""#).unwrap());
    assert!(engine.eval_with_scope::<bool>(&mut scope, r#"z == "foo""#).unwrap());
}

#[test]
fn test_string_mut() {
    let mut engine = Engine::new();

    engine.register_fn("foo", |s: &str| s.len() as INT);
    engine.register_fn("bar", |s: String| s.len() as INT);
    engine.register_fn("baz", |s: &mut String| s.len());

    assert_eq!(engine.eval::<char>(r#"pop("hello")"#).unwrap(), 'o');
    assert_eq!(engine.eval::<String>(r#"pop("hello", 3)"#).unwrap(), "llo");
    assert_eq!(engine.eval::<String>(r#"pop("hello", 10)"#).unwrap(), "hello");
    assert_eq!(engine.eval::<String>(r#"pop("hello", -42)"#).unwrap(), "");

    assert_eq!(engine.eval::<INT>(r#"foo("hello")"#).unwrap(), 5);
    assert_eq!(engine.eval::<INT>(r#"bar("hello")"#).unwrap(), 5);
    assert!(matches!(
        *engine.eval::<INT>(r#"baz("hello")"#).unwrap_err(),
        EvalAltResult::ErrorFunctionNotFound(f, ..) if f == "baz (&str | ImmutableString | String)"
    ));
}

#[cfg(not(feature = "no_object"))]
#[test]
fn test_string_substring() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<String>(r#"let x = "hello! \u2764\u2764\u2764"; x.sub_string(-2, 2)"#).unwrap(), "❤❤");
    assert_eq!(engine.eval::<String>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.sub_string(1, 5)"#).unwrap(), "❤❤ he");
    assert_eq!(engine.eval::<String>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.sub_string(1)"#).unwrap(), "❤❤ hello! ❤❤❤");
    assert_eq!(engine.eval::<String>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.sub_string(99)"#).unwrap(), "");
    assert_eq!(engine.eval::<String>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.sub_string(1, -1)"#).unwrap(), "");
    assert_eq!(engine.eval::<String>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.sub_string(1, 999)"#).unwrap(), "❤❤ hello! ❤❤❤");
    assert_eq!(engine.eval::<String>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.crop(1, -1); x"#).unwrap(), "");
    assert_eq!(engine.eval::<String>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.crop(4, 6); x"#).unwrap(), "hello!");
    assert_eq!(engine.eval::<String>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.crop(1, 999); x"#).unwrap(), "❤❤ hello! ❤❤❤");
    assert_eq!(engine.eval::<String>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x -= 'l'; x"#).unwrap(), "❤❤❤ heo! ❤❤❤");
    assert_eq!(engine.eval::<String>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x -= "\u2764\u2764"; x"#).unwrap(), "❤ hello! ❤");
    assert_eq!(engine.eval::<INT>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.index_of('\u2764')"#).unwrap(), 0);
    assert_eq!(engine.eval::<INT>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.index_of('\u2764', 5)"#).unwrap(), 11);
    assert_eq!(engine.eval::<INT>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.index_of('\u2764', -6)"#).unwrap(), 11);
    assert_eq!(engine.eval::<INT>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.index_of('\u2764', 999)"#).unwrap(), -1);
    assert_eq!(engine.eval::<INT>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.index_of('x')"#).unwrap(), -1);
}

#[cfg(not(feature = "no_object"))]
#[test]
fn test_string_format() {
    #[derive(Debug, Clone)]
    struct TestStruct {
        field: i64,
    }

    let mut engine = Engine::new();

    engine
        .register_type_with_name::<TestStruct>("TestStruct")
        .register_fn("new_ts", || TestStruct { field: 42 })
        .register_fn("to_string", |ts: TestStruct| format!("TS={}", ts.field))
        .register_fn("to_debug", |ts: TestStruct| format!("!!!TS={}!!!", ts.field));

    assert_eq!(engine.eval::<String>(r#"let x = new_ts(); "foo" + x"#).unwrap(), "fooTS=42");
    assert_eq!(engine.eval::<String>(r#"let x = new_ts(); x + "foo""#).unwrap(), "TS=42foo");
    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<String>(r#"let x = [new_ts()]; "foo" + x"#).unwrap(), "foo[!!!TS=42!!!]");
}

#[test]
fn test_string_fn() {
    let mut engine = Engine::new();

    engine.register_fn("set_to_x", |ch: &mut char| *ch = 'X');

    #[cfg(not(feature = "no_index"))]
    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<String>(r#"let x="foo"; x[0].set_to_x(); x"#).unwrap(), "Xoo");
    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<String>(r#"let x="foo"; set_to_x(x[0]); x"#).unwrap(), "foo");

    engine
        .register_fn("foo1", |s: &str| s.len() as INT)
        .register_fn("foo2", |s: ImmutableString| s.len() as INT)
        .register_fn("foo3", |s: String| s.len() as INT)
        .register_fn("foo4", |s: &mut ImmutableString| s.len() as INT);

    assert_eq!(engine.eval::<INT>(r#"foo1("hello")"#).unwrap(), 5);
    assert_eq!(engine.eval::<INT>(r#"foo2("hello")"#).unwrap(), 5);
    assert_eq!(engine.eval::<INT>(r#"foo3("hello")"#).unwrap(), 5);
    assert_eq!(engine.eval::<INT>(r#"foo4("hello")"#).unwrap(), 5);
}

#[cfg(not(feature = "no_object"))]
#[cfg(not(feature = "no_index"))]
#[test]
fn test_string_split() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.split(' ').len"#).unwrap(), 3);
    assert_eq!(engine.eval::<INT>(r#"let x = "\u2764\u2764\u2764 hello! \u2764\u2764\u2764"; x.split("hello").len"#).unwrap(), 2);
}

#[test]
fn test_string_interpolated() {
    // Make sure strings interpolation works even under raw
    let engine = Engine::new_raw();

    assert_eq!(engine.eval::<String>("`${}`").unwrap(), "");

    assert_eq!(
        engine
            .eval::<String>(
                "
                    let x = 40;
                    `hello ${x+2} worlds!`
                "
            )
            .unwrap(),
        "hello 42 worlds!"
    );

    assert_eq!(
        engine
            .eval::<String>(
                r#"
                    let x = 40;
                    "hello ${x+2} worlds!"
                "#
            )
            .unwrap(),
        "hello ${x+2} worlds!"
    );

    assert_eq!(
        engine
            .eval::<String>(
                "
                    const x = 42;
                    `hello ${x} worlds!`
                "
            )
            .unwrap(),
        "hello 42 worlds!"
    );

    assert_eq!(engine.eval::<String>("`hello ${}world!`").unwrap(), "hello world!");

    assert_eq!(
        engine
            .eval::<String>(
                "
                    const x = 42;
                    `${x} worlds!`
                "
            )
            .unwrap(),
        "42 worlds!"
    );

    assert_eq!(
        engine
            .eval::<String>(
                "
                    const x = 42;
                    `hello ${x}`
                "
            )
            .unwrap(),
        "hello 42"
    );

    assert_eq!(
        engine
            .eval::<String>(
                "
                    const x = 20;
                    `hello ${let y = x + 1; `${y * 2}`} worlds!`
                "
            )
            .unwrap(),
        "hello 42 worlds!"
    );

    assert_eq!(
        engine
            .eval::<String>(
                r#"
                    let x = 42;
                    let y = 123;
                
                `
Undeniable logic:
1) Hello, ${let w = `${x} world`; if x > 1 { w += "s" } w}!
2) If ${y} > ${x} then it is ${y > x}!
`
                "#
            )
            .unwrap(),
        "Undeniable logic:\n1) Hello, 42 worlds!\n2) If 123 > 42 then it is true!\n",
    );
}

#[test]
fn test_immutable_string() {
    let x: ImmutableString = "hello".into();
    assert_eq!(x, "hello");
    assert!(x == "hello");
    assert!(&x == "hello");
    assert!("hello" == x);
    assert!("hello" == &x);

    let s2 = String::from("hello");
    let s3: ImmutableString = s2.clone().into();
    assert_eq!(s2, s3);
    let s3: ImmutableString = (&s2).into();
    assert_eq!(s2, s3);

    assert!(x == s2);
    assert!(&x == s2);
    assert!(x == &s2);
    assert!(&x == &s2);

    assert!(s2 == x);
    assert!(&s2 == x);
    assert!(s2 == &x);
    assert!(&s2 == &x);

    assert!(x >= s2);
    assert!(&x >= s2);
    assert!(x >= &s2);
    assert!(&x >= &s2);

    assert!(s2 >= x);
    assert!(&s2 >= x);
    assert!(s2 >= &x);
    assert!(&s2 >= &x);

    let _sx: String = x.clone().into();
    let _sx: String = (&x).into();
    let _ssx: Box<str> = x.clone().into();
    let _ssx: Box<str> = (&x).into();
}
