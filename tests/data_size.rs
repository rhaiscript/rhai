#![cfg(not(feature = "unchecked"))]
use rhai::{Engine, EvalAltResult, ParseErrorType, INT};

#[cfg(not(feature = "no_index"))]
use rhai::Array;

#[cfg(not(feature = "no_object"))]
use rhai::Map;

#[test]
#[cfg(not(feature = "no_object"))]
fn test_map_template_size() {
    let mut engine = Engine::new();
    #[cfg(not(feature = "no_optimize"))]
    engine.set_optimization_level(rhai::OptimizationLevel::None);

    let mut scope = rhai::Scope::new();
    scope.push("input", "abc");
    for source in [r#"#{ a: "abc", b: "def" }"#, r#"#{ a: "abc", b: input }"#, "#{ a: input, b: input }"] {
        let ast = engine.compile(source).unwrap();
        engine.set_max_string_size(6);
        assert_eq!(engine.eval_ast_with_scope::<Map>(&mut scope, &ast).unwrap().len(), 2);
        engine.set_max_string_size(5);
        assert!(matches!(*engine.eval_ast_with_scope::<Map>(&mut scope, &ast).unwrap_err(), EvalAltResult::ErrorDataTooLarge(..)));
        engine.set_max_string_size(0);
    }

    let ast = engine.compile("#{ a: #{ x: 1 }, b: input }").unwrap();
    scope.set_value("input", Map::new());
    engine.set_max_map_size(3);
    assert_eq!(engine.eval_ast_with_scope::<Map>(&mut scope, &ast).unwrap().len(), 2);
    engine.set_max_map_size(2);
    assert!(matches!(*engine.eval_ast_with_scope::<Map>(&mut scope, &ast).unwrap_err(), EvalAltResult::ErrorDataTooLarge(..)));

    #[cfg(not(feature = "no_index"))]
    {
        engine.set_max_map_size(0);
        let ast = engine.compile("#{ a: [1, 2], b: input }").unwrap();
        scope.set_value("input", vec![rhai::Dynamic::from_int(3), rhai::Dynamic::from_int(4)]);
        engine.set_max_array_size(4);
        assert_eq!(engine.eval_ast_with_scope::<Map>(&mut scope, &ast).unwrap().len(), 2);
        engine.set_max_array_size(3);
        assert!(matches!(*engine.eval_ast_with_scope::<Map>(&mut scope, &ast).unwrap_err(), EvalAltResult::ErrorDataTooLarge(..)));
    }
}

#[test]
fn test_max_string_size() {
    let mut engine = Engine::new();
    engine.set_max_string_size(10);

    assert_eq!(*engine.compile(r#"let x = "hello, world!";"#).unwrap_err().err_type(), ParseErrorType::LiteralTooLarge("Length of string".to_string(), 10));

    assert_eq!(*engine.compile(r#"let x = "朝に紅顔、暮に白骨";"#).unwrap_err().err_type(), ParseErrorType::LiteralTooLarge("Length of string".to_string(), 10));

    assert_eq!(*engine.compile("let x = `朝に紅顔、暮に白骨`;").unwrap_err().err_type(), ParseErrorType::LiteralTooLarge("Length of string".to_string(), 10));

    assert_eq!(*engine.compile(r#"let x = `Say ${"Hi!"} and so be it.`;"#).unwrap_err().err_type(), ParseErrorType::LiteralTooLarge("Length of string".to_string(), 10));

    assert!(matches!(
        *engine
            .run(
                r#"
                    let x = "hello, ";
                    let y = "world!";
                    x + y
                "#
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    #[cfg(not(feature = "no_object"))]
    assert!(matches!(
        *engine
            .run(
                r#"
                    let x = "hello";
                    x.pad(100, '!');
                    x
                "#
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    engine.set_max_string_size(0);

    assert_eq!(
        engine
            .eval::<String>(
                r#"
                    let x = "hello, ";
                    let y = "world!";
                    x + y
                "#
            )
            .unwrap(),
        "hello, world!"
    );
}

#[test]
#[cfg(not(feature = "no_index"))]
fn test_max_array_size() {
    let mut engine = Engine::new();
    engine.set_max_array_size(10);

    #[cfg(not(feature = "no_object"))]
    engine.set_max_map_size(10);

    assert_eq!(*engine.compile("let x = [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15];").unwrap_err().err_type(), ParseErrorType::LiteralTooLarge("Size of array literal".to_string(), 10));

    assert!(matches!(
        *engine
            .run(
                "
                    let x = [1,2,3,4,5,6];
                    let y = [7,8,9,10,11,12];
                    x + y
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    #[cfg(not(feature = "no_closure"))]
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = 42;
                    let y = [];
                    let f = || x;
                    for n in 0..10 {
                        y += x;
                    }
                    len(y)
                "
            )
            .unwrap(),
        10
    );

    #[cfg(not(feature = "no_closure"))]
    assert!(matches!(
        *engine
            .run(
                "
                    let x = 42;
                    let y = [];
                    let f = || x;
                    for n in 0..11 {
                        y += x;
                    }
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    assert!(matches!(
        *engine
            .run(
                "
                    let x = [ 42 ];
                    loop { x[0] = x; }
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    #[cfg(not(feature = "no_object"))]
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = [1,2,3,4,5,6];
                    x.pad(10, 42);
                    len(x)
                "
            )
            .unwrap(),
        10
    );

    #[cfg(not(feature = "no_object"))]
    assert!(matches!(
        *engine
            .run(
                "
                    let x = [1,2,3,4,5,6];
                    x.pad(11, 42);
                    x
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    assert_eq!(
        engine
            .eval::<INT>(
                "
                let x = [1,2];
                len([x, x, x])
            "
            )
            .unwrap(),
        3
    );

    assert!(matches!(
        *engine
            .run(
                "
                    let x = [1,2,3];
                    [x, x, x, x]
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    #[cfg(not(feature = "no_object"))]
    assert!(matches!(
        *engine
            .run(
                "
                    let x = #{a:1, b:2, c:3};
                    [x, x, x, x]
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    assert!(matches!(
        *engine
            .run(
                "
                    let x = [1];
                    let y = [x, x];
                    let z = [y, y];
                    [z, z, z]
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    engine.set_max_array_size(0);

    assert_eq!(
        engine
            .eval::<Array>(
                "
                    let x = [1,2,3,4,5,6];
                    let y = [7,8,9,10,11,12];
                    x + y
                "
            )
            .unwrap()
            .len(),
        12
    );

    assert_eq!(
        engine
            .eval::<Array>(
                "
                    let x = [1,2,3];
                    [x, x, x, x]
                "
            )
            .unwrap()
            .len(),
        4
    );
}

#[test]
#[cfg(not(feature = "no_object"))]
fn test_max_map_size_assignment() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    let cases = [
        ("let m = #{a:1,b:2,c:3};", "m.d = 4;", false),
        ("let m = #{a:1,b:2,c:3,d:4};", "m.a = 42;", false),
        ("let m = #{a:1,b:2,c:3,d:4};", "m.extra = 42;", true),
        ("let m = #{child: #{a:1,b:2,c:3}};", "m.child.extra = 42;", true),
        ("let m = #{a:1,b:2,c:3,d:4};", "m.a = #{x:42};", true),
        ("let m = #{child: #{a:1,b:2,c:3}};", "m.child += #{extra:42};", true),
        ("let m = #{child: #{a:1,b:2,c:3}};", "m.child.grow_map().type_of();", true),
        ("", "(#{a:1,b:2,c:3,d:4}).grow_map().type_of();", true),
        #[cfg(not(feature = "no_index"))]
        ("let m = #{a:1,b:2,c:3};", r#"m["d"] = 4;"#, false),
        #[cfg(not(feature = "no_index"))]
        ("let m = #{a:1,b:2,c:3,d:4};", r#"m["a"] = 42;"#, false),
        #[cfg(not(feature = "no_index"))]
        ("let m = #{a:1,b:2,c:3,d:4};", r#"m["extra"] = 42;"#, true),
        #[cfg(not(feature = "no_index"))]
        ("let m = #{child: #{a:1,b:2,c:3}};", r#"m["child"]["extra"] = 42;"#, true),
        #[cfg(not(feature = "no_index"))]
        ("let m = #{child: #{a:1,b:2,c:3}};", r#"m["child"].extra = 42;"#, true),
        #[cfg(not(feature = "no_index"))]
        ("let m = #{child: #{a:1,b:2,c:3}};", r#"m.child["extra"] = 42;"#, true),
        #[cfg(not(feature = "no_index"))]
        ("let m = #{a:1,b:2,c:3,d:4};", r#"m["a"] = #{x:42};"#, true),
        #[cfg(not(feature = "no_index"))]
        ("let m = [#{a:1,b:2,c:3,d:4}];", r#"m[0]["extra"] = 42;"#, true),
        #[cfg(not(feature = "no_index"))]
        ("let m = [#{a:1,b:2,c:3,d:4}];", "m[0].extra = 42;", true),
        #[cfg(not(feature = "no_index"))]
        ("let m = [#{a:1,b:2}, #{c:3,d:4}];", "m[0].extra = 42;", true),
        #[cfg(not(feature = "no_index"))]
        ("let m = #{child: [#{a:1,b:2,c:3}]};", "m.child.grow_array().type_of();", true),
        #[cfg(not(feature = "no_function"))]
        ("fn add() { this.child.extra = 42; } let m = #{child: #{a:1,b:2,c:3}};", "m.add();", true),
        #[cfg(not(feature = "no_closure"))]
        ("let m = #{child: #{a:1,b:2,c:3}}; let f = || { m.child.extra = 42; };", "f.call();", true),
        #[cfg(all(not(feature = "no_closure"), not(feature = "no_index")))]
        ("let m = #{a:1,b:2,c:3,d:4}; let f = || { m[\"extra\"] = 42; };", "f.call();", true),
    ];

    for (setup, assignment, oversized) in cases {
        for limit in [4, 5, 0] {
            let hits = Arc::new(AtomicUsize::new(0));
            let mut engine = Engine::new();
            engine.set_max_map_size(limit);
            engine.register_fn("grow_map", |map: &mut Map| {
                map.insert("extra".into(), rhai::Dynamic::from_int(42));
                42 as INT
            });
            #[cfg(not(feature = "no_index"))]
            engine.register_fn("grow_array", |array: &mut Array| {
                let mut map = Map::new();
                map.insert("extra".into(), rhai::Dynamic::from_int(42));
                array.push(map.into());
                42 as INT
            });
            let counter = hits.clone();
            engine.register_fn("marker", move || {
                counter.fetch_add(1, Ordering::SeqCst);
            });

            let script = format!("{setup} marker(); {assignment} marker();");
            let result = engine.run(&script);
            if oversized && limit == 4 {
                assert!(matches!(*result.unwrap_err(), EvalAltResult::ErrorDataTooLarge(..)), "{script}");
                assert_eq!(hits.load(Ordering::SeqCst), 1, "{script}");
            } else {
                result.unwrap();
                assert_eq!(hits.load(Ordering::SeqCst), 2, "{script}");
            }
        }
    }
}

#[test]
#[cfg(not(feature = "no_object"))]
fn test_max_map_size() {
    let mut engine = Engine::new();
    engine.set_max_map_size(10);

    #[cfg(not(feature = "no_index"))]
    engine.set_max_array_size(10);

    assert_eq!(
        *engine.compile("let x = #{a:1,b:2,c:3,d:4,e:5,f:6,g:7,h:8,i:9,j:10,k:11,l:12,m:13,n:14,o:15};").unwrap_err().err_type(),
        ParseErrorType::LiteralTooLarge("Number of properties in object map literal".to_string(), 10)
    );

    assert!(matches!(
        *engine
            .run(
                "
                    let x = #{};
                    loop { x.a = x; }
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    assert!(matches!(
        *engine
            .run(
                "
                    let x = #{a:1,b:2,c:3,d:4,e:5,f:6};
                    let y = #{g:7,h:8,i:9,j:10,k:11,l:12};
                    x + y
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    assert!(matches!(
        *engine
            .run(
                "
                    let x = #{a:1,b:2,c:3};
                    #{u:x, v:x, w:x, z:x}
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    #[cfg(not(feature = "no_index"))]
    assert!(matches!(
        *engine
            .run(
                "
                    let x = [1, 2, 3];
                    #{u:x, v:x, w:x, z:x}
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataTooLarge(..)
    ));

    engine.set_max_map_size(0);

    assert_eq!(
        engine
            .eval::<Map>(
                "
                    let x = #{a:1,b:2,c:3,d:4,e:5,f:6};
                    let y = #{g:7,h:8,i:9,j:10,k:11,l:12};
                    x + y
                "
            )
            .unwrap()
            .len(),
        12
    );

    assert_eq!(
        engine
            .eval::<Map>(
                "
                    let x = #{a:1,b:2,c:3};
                    #{u:x, v:x, w:x, z:x}
                "
            )
            .unwrap()
            .len(),
        4
    );
}
