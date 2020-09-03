#![cfg(not(feature = "no_object"))]

use rhai::{Engine, EvalAltResult, Map, ParseErrorType, Scope, INT};

#[test]
fn test_map_indexing() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    #[cfg(not(feature = "no_index"))]
    {
        assert_eq!(
            engine.eval::<INT>(r#"let x = #{a: 1, b: 2, c: 3}; x["b"]"#)?,
            2
        );
        assert_eq!(
            engine.eval::<INT>(r#"let x = #{a: 1, b: 2, c: 3,}; x["b"]"#)?,
            2
        );
        assert_eq!(
            engine.eval::<char>(
                r#"
                    let y = #{d: 1, "e": #{a: 42, b: 88, "": "hello"}, " 123 xyz": 9};
                    y.e[""][4]
                "#
            )?,
            'o'
        );
        assert_eq!(
            engine.eval::<String>(r#"let a = [#{s:"hello"}]; a[0].s[2] = 'X'; a[0].s"#)?,
            "heXlo"
        );
    }

    assert_eq!(
        engine.eval::<INT>("let y = #{a: 1, b: 2, c: 3}; y.a = 5; y.a")?,
        5
    );
    engine.eval::<()>("let y = #{a: 1, b: 2, c: 3}; y.z")?;

    assert!(engine.eval::<bool>(r#"let y = #{a: 1, b: 2, c: 3}; "c" in y"#)?);
    assert!(engine.eval::<bool>("let y = #{a: 1, b: 2, c: 3}; 'b' in y")?);
    assert!(!engine.eval::<bool>(r#"let y = #{a: 1, b: 2, c: 3}; "z" in y"#)?);

    assert_eq!(
        engine.eval::<INT>(
            r#"
                let x = #{a: 1, b: 2, c: 3};
                let c = x.remove("c");
                x.len() + c
            "#
        )?,
        5
    );
    assert_eq!(
        engine.eval::<INT>(
            r"
                let x = #{a: 1, b: 2, c: 3};
                let y = #{b: 42, d: 9};
                x.mixin(y);
                x.len() + x.b
            "
        )?,
        46
    );
    assert_eq!(
        engine.eval::<INT>(
            r"
                let x = #{a: 1, b: 2, c: 3};
                x += #{b: 42, d: 9};
                x.len() + x.b
            "
        )?,
        46
    );
    assert_eq!(
        engine
            .eval::<Map>(
                r"
                    let x = #{a: 1, b: 2, c: 3};
                    let y = #{b: 42, d: 9};
                    x + y
                "
            )?
            .len(),
        4
    );

    Ok(())
}

#[test]
fn test_map_assign() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    let x = engine.eval::<Map>(r#"let x = #{a: 1, b: true, "c$": "hello"}; x"#)?;

    assert_eq!(x["a"].clone().cast::<INT>(), 1);
    assert_eq!(x["b"].clone().cast::<bool>(), true);
    assert_eq!(x["c$"].clone().cast::<String>(), "hello");

    Ok(())
}

#[test]
fn test_map_return() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    let x = engine.eval::<Map>(r#"#{a: 1, b: true, "c$": "hello"}"#)?;

    assert_eq!(x["a"].clone().cast::<INT>(), 1);
    assert_eq!(x["b"].clone().cast::<bool>(), true);
    assert_eq!(x["c$"].clone().cast::<String>(), "hello");

    Ok(())
}

#[test]
#[cfg(not(feature = "no_index"))]
fn test_map_for() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    assert_eq!(
        engine
            .eval::<String>(
                r#"
                    let map = #{a: 1, b_x: true, "$c d e!": "hello"};
                    let s = "";

                    for key in keys(map) {
                        s += key;
                    }

                    s
                "#
            )?
            .len(),
        11
    );

    Ok(())
}

#[test]
/// Because a Rhai object map literal is almost the same as JSON,
/// it is possible to convert from JSON into a Rhai object map.
fn test_map_json() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    let json = r#"{"a":1, "b":true, "c":42, "$d e f!":"hello", "z":null}"#;

    let map = engine.parse_json(json, true)?;

    assert!(!map.contains_key("x"));

    assert_eq!(map["a"].clone().cast::<INT>(), 1);
    assert_eq!(map["b"].clone().cast::<bool>(), true);
    assert_eq!(map["c"].clone().cast::<INT>(), 42);
    assert_eq!(map["$d e f!"].clone().cast::<String>(), "hello");
    assert_eq!(map["z"].clone().cast::<()>(), ());

    #[cfg(not(feature = "no_index"))]
    {
        let mut scope = Scope::new();
        scope.push_constant("map", map);

        assert_eq!(
            engine
                .eval_with_scope::<String>(
                    &mut scope,
                    r#"
                        let s = "";

                        for key in keys(map) {
                            s += key;
                        }

                        s
                    "#
                )?
                .len(),
            11
        );
    }

    engine.parse_json(&format!("#{}", json), true)?;

    assert!(matches!(
        *engine.parse_json("   123", true).expect_err("should error"),
        EvalAltResult::ErrorParsing(ParseErrorType::MissingToken(token, _), pos)
            if token == "{" && pos.position() == Some(4)
    ));

    Ok(())
}

#[test]
#[cfg(not(feature = "no_function"))]
fn test_map_oop() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    assert_eq!(
        engine.eval::<INT>(
            r#"
                let obj = #{ data: 40, action: Fn("abc") };

                fn abc(x) { this.data += x; }

                obj.action(2);
                obj.data
            "#,
        )?,
        42
    );

    Ok(())
}
