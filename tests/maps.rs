#![cfg(not(feature = "no_object"))]
use rhai::{Engine, EvalAltResult, Map, ParseErrorType, Scope, INT};

fn map_template_engines() -> Vec<Engine> {
    #[cfg(feature = "no_optimize")]
    return vec![Engine::new()];

    #[cfg(not(feature = "no_optimize"))]
    [rhai::OptimizationLevel::None, rhai::OptimizationLevel::Simple, rhai::OptimizationLevel::Full]
        .into_iter()
        .map(|level| {
            let mut engine = Engine::new();
            engine.set_optimization_level(level);
            engine
        })
        .collect()
}

#[test]
#[cfg(feature = "internals")]
fn test_map_template_contents() {
    for engine in map_template_engines() {
        let ast = engine
            .compile(
                "
                    const FOLDED = 40;

                    #{
                        literal: 1,
                        unit: (),
                        nested: #{ x: 2 },
                        folded: FOLDED + 2,
                        computed: input
                    }
                ",
            )
            .unwrap();
        let mut found = false;
        ast.walk(&mut |nodes| {
            if let Some(rhai::ASTNode::Expr(rhai::Expr::Map(x, ..))) = nodes.last() {
                if x.1.contains_key("computed") {
                    found = true;
                    assert!(x.0.len() <= 2);
                    assert!(x.0.len() >= 1);
                    assert_eq!(x.1["literal"].as_int().unwrap(), 1);
                    assert!(x.1["unit"].is_unit());
                    assert!(x.1["computed"].is_unit());
                    assert_eq!(x.1["nested"].clone_cast::<Map>()["x"].as_int().unwrap(), 2);
                    #[cfg(not(feature = "no_optimize"))]
                    if engine.optimization_level() != rhai::OptimizationLevel::None {
                        assert_eq!(x.1["folded"].as_int().unwrap(), 42);
                    }
                }
            }
            true
        });
        assert!(found, "the mixed map must retain its ordered expressions and template");
        let mut scope = Scope::new();
        scope.push("input", 9 as INT);
        let result = engine.eval_ast_with_scope::<Map>(&mut scope, &ast).unwrap();
        assert_eq!(result["folded"].as_int().unwrap(), 42);
        assert_eq!(result["computed"].as_int().unwrap(), 9);
    }
}

#[test]
fn test_map_template_evaluation_and_isolation() {
    for engine in map_template_engines() {
        let ast = engine.compile("#{ nested: #{ value: 1 }, unit: (), computed: input }").unwrap();

        let mut scope = Scope::new();
        scope.push("input", 42 as INT);

        let mut first = engine.eval_ast_with_scope::<Map>(&mut scope, &ast).unwrap();
        first.get_mut("nested").unwrap().write_lock::<Map>().unwrap().insert("value".into(), (99 as INT).into());

        let second = engine.eval_ast_with_scope::<Map>(&mut scope, &ast).unwrap();

        assert_eq!(second["nested"].clone_cast::<Map>()["value"].as_int().unwrap(), 1);
        assert!(second["unit"].is_unit());
        assert_eq!(second["computed"].as_int().unwrap(), 42);

        assert_eq!(
            engine
                .eval::<INT>(
                    "
                        let n = 0;
                        let m = #{
                            a: 1,
                            b: { n += 1; n },
                            c: (),
                            d: { n *= 10; n }
                        };
                        m.b + m.d + n
                    "
                )
                .unwrap(),
            21
        );

        assert_eq!(engine.eval::<INT>("#{ a: 42, b: () }.a").unwrap(), 42);
        assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "#{ a: 42, b: input }.a").unwrap(), 42);

        // Values that only become constant after optimization are not in the template yet
        assert_eq!(engine.eval::<INT>("#{ a: 42, b: { 1 } }.b").unwrap(), 1);
        assert_eq!(engine.eval::<INT>("#{ a: if true { 1 } else { 2 } }.a").unwrap(), 1);
        assert_eq!(engine.eval::<INT>("#{ a: 42, b: (() ?? 1) }.b").unwrap(), 1);

        #[cfg(not(feature = "no_index"))]
        {
            assert_eq!(engine.eval::<INT>(r#"#{ a: 42, b: () }["a"]"#).unwrap(), 42);
            assert_eq!(engine.eval_with_scope::<INT>(&mut scope, r#"#{ a: 42, b: input }["a"]"#).unwrap(), 42);
            assert_eq!(engine.eval::<INT>(r#"#{ a: 42, b: { 1 } }["b"]"#).unwrap(), 1);

            let ast = engine.compile("#{ values: [#{ x: 1 }], computed: input }").unwrap();

            let mut first = engine.eval_ast_with_scope::<Map>(&mut scope, &ast).unwrap();

            first.get_mut("values").unwrap().write_lock::<rhai::Array>().unwrap()[0]
                .write_lock::<Map>()
                .unwrap()
                .insert("x".into(), (99 as INT).into());

            let second = engine.eval_ast_with_scope::<Map>(&mut scope, &ast).unwrap();

            assert_eq!(second["values"].clone_cast::<rhai::Array>()[0].clone_cast::<Map>()["x"].as_int().unwrap(), 1);
        }
    }
}

#[test]
#[cfg(not(any(feature = "no_function", feature = "no_module")))]
fn test_map_template_function_pointer_environment() {
    for mut engine in map_template_engines() {
        let module_ast = engine.compile("fn answer() { 42 }").unwrap();
        let mut resolver = rhai::module_resolvers::StaticModuleResolver::new();
        resolver.insert("answers", rhai::Module::eval_ast_as_new(Scope::new(), &module_ast, &engine).unwrap());
        engine.set_module_resolver(resolver);

        let mut scope = Scope::new();
        scope.push("input", 1 as INT);

        let ast = engine
            .compile(
                r#"
                    import "answers" as answers;
                    
                    #{
                        f: || answers::answer(),
                        nested: #{
                            f: || answers::answer(),
                            computed: input
                        },
                        computed: input
                    }
                "#,
            )
            .unwrap();

        let result = engine.eval_ast_with_scope::<Map>(&mut scope, &ast).unwrap();

        for pointer in [result["f"].clone_cast::<rhai::FnPtr>(), result["nested"].clone_cast::<Map>()["f"].clone_cast::<rhai::FnPtr>()] {
            assert_eq!(pointer.call::<INT>(&engine, &ast, ()).unwrap(), 42);
        }
    }
}

#[test]
fn test_map_indexing() {
    let engine = Engine::new();

    #[cfg(not(feature = "no_index"))]
    {
        assert_eq!(engine.eval::<INT>(r#"let x = #{a: 1, b: 2, c: 3}; x["b"]"#).unwrap(), 2);
        assert_eq!(engine.eval::<INT>(r#"let x = #{a: 1, b: 2, c: 3,}; x["b"]"#).unwrap(), 2);
        assert_eq!(
            engine
                .eval::<char>(
                    r#"
                        let y = #{d: 1, "e": #{a: 42, b: 88, "": "hello"}, " 123 xyz": 9};
                        y.e[""][4]
                    "#
                )
                .unwrap(),
            'o'
        );
        assert_eq!(engine.eval::<String>(r#"let a = [#{s:"hello"}]; a[0].s[2] = 'X'; a[0].s"#).unwrap(), "heXlo");
    }

    assert_eq!(engine.eval::<INT>("let y = #{a: 1, b: 2, c: 3}; y.a = 5; y.a").unwrap(), 5);
    assert_eq!(engine.eval::<INT>("let y = #{a: #{x:9, y:8, z:7}, b: 2, c: 3}; (y.a).z").unwrap(), 7);
    assert_eq!(engine.eval::<INT>("let y = #{a: #{x:9, y:8, z:7}, b: 2, c: 3}; (y.a).z = 42; y.a.z").unwrap(), 42);

    engine.run("let y = #{a: 1, b: 2, c: 3}; y.z").unwrap();

    #[cfg(not(feature = "no_index"))]
    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let y = #{`a
b`: 1};
                    y["a\nb"]
                "#
            )
            .unwrap(),
        1
    );

    assert!(matches!(*engine.eval::<INT>("let y = #{`a${1}`: 1}; y.a1").unwrap_err(), EvalAltResult::ErrorParsing(ParseErrorType::PropertyExpected, ..)));

    assert!(engine.eval::<bool>(r#"let y = #{a: 1, b: 2, c: 3}; "c" in y"#).unwrap());
    assert!(engine.eval::<bool>(r#"let y = #{a: 1, b: 2, c: 3}; "b" in y"#).unwrap());
    assert!(!engine.eval::<bool>(r#"let y = #{a: 1, b: 2, c: 3}; "z" in y"#).unwrap());

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let x = #{a: 1, b: 2, c: 3};
                    let c = x.remove("c");
                    x.len() + c
                "#
            )
            .unwrap(),
        5
    );
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = #{a: 1, b: 2, c: 3};
                    let y = #{b: 42, d: 9};
                    x.mixin(y);
                    x.len() + x.b
                "
            )
            .unwrap(),
        46
    );
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = #{a: 1, b: 2, c: 3};
                    x += #{b: 42, d: 9};
                    x.len() + x.b
                "
            )
            .unwrap(),
        46
    );
    assert_eq!(
        engine
            .eval::<Map>(
                "
                    let x = #{a: 1, b: 2, c: 3};
                    let y = #{b: 42, d: 9};
                    x + y
                "
            )
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn test_map_prop() {
    let mut engine = Engine::new();

    engine.eval::<()>("let x = #{a: 42}; x.b").unwrap();

    engine.set_fail_on_invalid_map_property(true);

    assert!(matches!(
        *engine.eval::<()>("let x = #{a: 42}; x.b").unwrap_err(),
        EvalAltResult::ErrorPropertyNotFound(prop, _) if prop == "b"
    ));
}

#[cfg(not(feature = "no_index"))]
#[test]
fn test_map_index_types() {
    let engine = Engine::new();

    engine.compile(r#"#{a:1, b:2, c:3}["a"]['x']"#).unwrap();

    assert!(matches!(engine.compile("#{a:1, b:2, c:3}['x']").unwrap_err().err_type(), ParseErrorType::MalformedIndexExpr(..)));
    assert!(matches!(engine.compile("#{a:1, b:2, c:3}[1]").unwrap_err().err_type(), ParseErrorType::MalformedIndexExpr(..)));
    #[cfg(not(feature = "no_float"))]
    assert!(matches!(engine.compile("#{a:1, b:2, c:3}[123.456]").unwrap_err().err_type(), ParseErrorType::MalformedIndexExpr(..)));
    assert!(matches!(engine.compile("#{a:1, b:2, c:3}[()]").unwrap_err().err_type(), ParseErrorType::MalformedIndexExpr(..)));
    assert!(matches!(engine.compile("#{a:1, b:2, c:3}[true && false]").unwrap_err().err_type(), ParseErrorType::MalformedIndexExpr(..)));
}

#[test]
fn test_map_assign() {
    let engine = Engine::new();

    let x = engine.eval::<Map>(r#"let x = #{a: 1, b: true, "c$": "hello"}; x"#).unwrap();

    assert_eq!(x["a"].as_int().unwrap(), 1);
    assert!(x["b"].as_bool().unwrap());
    assert_eq!(x["c$"].clone_cast::<String>(), "hello");
}

#[test]
fn test_map_return() {
    let engine = Engine::new();

    let x = engine.eval::<Map>(r#"#{a: 1, b: true, "c$": "hello"}"#).unwrap();

    assert_eq!(x["a"].as_int().unwrap(), 1);
    assert!(x["b"].as_bool().unwrap());
    assert_eq!(x["c$"].clone_cast::<String>(), "hello");
}

#[test]
#[cfg(not(feature = "no_index"))]
fn test_map_for() {
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
            )
            .unwrap()
            .len(),
        11
    );
}

#[test]
/// Because a Rhai object map literal is almost the same as JSON,
/// it is possible to convert from JSON into a Rhai object map.
fn test_map_json() {
    let engine = Engine::new();

    let json = r#"{"a":1, "b":true, "c":42, "$d e f!":"hello", "z":null, "n":-1}"#;

    let map = engine.parse_json(json, true).unwrap();

    assert!(!map.contains_key("x"));

    assert_eq!(map["a"].as_int().unwrap(), 1);
    assert!(map["b"].as_bool().unwrap());
    assert_eq!(map["c"].as_int().unwrap(), 42);
    assert_eq!(map["$d e f!"].clone_cast::<String>(), "hello");
    let _: () = map["z"].as_unit().unwrap();
    assert_eq!(map["n"].as_int().unwrap(), -1);

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
                )
                .unwrap()
                .len(),
            12
        );

        assert_eq!(engine.eval::<String>("#{a:[#{b:42}]}.to_json()").unwrap(), r#"{"a":[{"b":42}]}"#);
        assert_eq!(engine.eval::<String>(r#"#{a:[Fn("abc")]}.to_json()"#).unwrap(), r#"{"a":["abc"]}"#);
        assert_eq!(engine.eval::<String>(r#"#{a:[Fn("abc").curry(42).curry(123)]}.to_json()"#).unwrap(), r#"{"a":[["abc",42,123]]}"#);
    }

    engine.parse_json(json, true).unwrap();

    assert!(matches!(*engine.parse_json("123", true).unwrap_err(), EvalAltResult::ErrorMismatchOutputType(..)));
    assert!(matches!(*engine.parse_json("{a:42}", true).unwrap_err(), EvalAltResult::ErrorParsing(..)));
    assert!(matches!(*engine.parse_json("#{a:123}", true).unwrap_err(), EvalAltResult::ErrorParsing(..)));
    assert!(matches!(*engine.parse_json("{a:()}", true).unwrap_err(), EvalAltResult::ErrorParsing(..)));
    assert!(matches!(*engine.parse_json("#{a:123+456}", true).unwrap_err(), EvalAltResult::ErrorParsing(..)));
    assert!(matches!(*engine.parse_json("{a:`hello${world}`}", true).unwrap_err(), EvalAltResult::ErrorParsing(..)));
    assert!(matches!(*engine.parse_json(r#"{"a":print("x")}"#, true).unwrap_err(), EvalAltResult::ErrorParsing(..)));
    assert!(matches!(*engine.parse_json(r#"{"a":eval("40+2")}"#, false).unwrap_err(), EvalAltResult::ErrorParsing(..)));
    assert!(matches!(*engine.parse_json(r#"{"a":"hello".len}"#, true).unwrap_err(), EvalAltResult::ErrorParsing(..)));
    #[cfg(not(feature = "no_float"))]
    assert_eq!(engine.parse_json(r#"{"a":-1.5, "b":1e2}"#, true).unwrap()["b"].as_float().unwrap(), 100.0);
    assert!(matches!(*engine.parse_json(r#"{"a":41+1}"#, true).unwrap_err(), EvalAltResult::ErrorParsing(..)));
    assert!(matches!(*engine.parse_json(r#"{"a":[1]+[2]}"#, true).unwrap_err(), EvalAltResult::ErrorParsing(..)));
    assert!(matches!(*engine.parse_json(r#"{"a":1 in [1]}"#, true).unwrap_err(), EvalAltResult::ErrorParsing(..)));
    assert!(matches!(*engine.parse_json(r#"{"a":- 1}"#, true).unwrap_err(), EvalAltResult::ErrorParsing(..)));
}

#[test]
#[cfg(not(feature = "no_function"))]
fn test_map_oop() {
    let engine = Engine::new();

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let obj = #{ data: 40, action: Fn("abc") };

                    fn abc(x) { this.data += x; }

                    obj.action(2);
                    obj.data
                "#,
            )
            .unwrap(),
        42
    );
}

#[test]
#[cfg(feature = "internals")]
fn test_map_missing_property_callback() {
    use std::convert::TryInto;

    let mut engine = Engine::new();

    engine.on_map_missing_property(|map, prop, _| match prop {
        "x" => {
            map.insert("y".into(), (42 as INT).into());
            map.get_mut("y").unwrap().try_into()
        }
        "z" => Ok(rhai::Dynamic::from(100 as INT).into()),
        _ => Err(rhai::EvalAltResult::ErrorPropertyNotFound(prop.to_string(), rhai::Position::NONE).into()),
    });

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let obj = #{ a:1, b:2 };
                    obj.x += 1;
                    obj.y + obj.z
                "
            )
            .unwrap(),
        143
    );
}

#[test]
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_closure"))]
fn test_map_json_shared_loop() {
    let engine = Engine::new();

    // A closure capturing the object map it is stored in forms a reference loop.
    engine.eval::<String>("let m = #{}; let f = || m; m.f = f; m.to_json()").unwrap();
    engine.eval::<String>("let m = #{}; let f = || m; m.f = f; #{a: m}.to_json()").unwrap();

    // The same shared value appearing more than once is not a loop.
    assert_eq!(engine.eval::<String>("let m = #{x: 1}; let f = || m; #{a: m, b: m}.to_json()").unwrap(), r#"{"a":{"x":1},"b":{"x":1}}"#);
}
