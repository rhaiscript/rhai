#![cfg(not(feature = "no_object"))]
#[cfg(feature = "internals")]
use rhai::{ASTNode, Expr};
use rhai::{Engine, EvalAltResult, INT};

#[derive(Debug, Clone, Eq, PartialEq)]
struct TestStruct {
    x: INT,
}

impl TestStruct {
    fn update(&mut self, n: INT) {
        self.x += n;
    }

    fn new() -> Self {
        Self { x: 1 }
    }
}

#[test]
fn test_method_call() {
    let mut engine = Engine::new();

    engine.register_type::<TestStruct>().register_fn("update", TestStruct::update).register_fn("new_ts", TestStruct::new);

    assert_eq!(engine.eval::<TestStruct>("let x = new_ts(); x.update(1000); x").unwrap(), TestStruct { x: 1001 });
    assert_eq!(engine.eval::<TestStruct>("let x = new_ts(); update(x, 1000); x").unwrap(), TestStruct { x: 1001 });
}

#[test]
fn test_method_call_style() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = -123; x.abs(); x").unwrap(), -123);
}

#[cfg(not(feature = "no_optimize"))]
// #[test]
fn test_method_call_with_full_optimization() {
    let mut engine = Engine::new();

    engine.set_optimization_level(rhai::OptimizationLevel::Full);

    engine
        .register_fn("new_ts", TestStruct::new)
        .register_fn("ymd", |_: INT, _: INT, _: INT| 42 as INT)
        .register_fn("range", |_: &mut TestStruct, _: INT, _: INT| TestStruct::new());

    assert_eq!(
        engine
            .eval::<TestStruct>(
                "
                    let xs = new_ts();
                    let ys = xs.range(ymd(2022, 2, 1), ymd(2022, 2, 2));
                    ys
                "
            )
            .unwrap(),
        TestStruct::new()
    );
}

#[cfg(not(feature = "no_function"))]
#[test]
fn test_method_call_typed() {
    let mut engine = Engine::new();

    engine
        .register_type_with_name::<TestStruct>("Test-Struct#ABC")
        .register_fn("update", TestStruct::update)
        .register_fn("new_ts", TestStruct::new);

    assert_eq!(
        engine
            .eval::<TestStruct>(
                r#"
                    fn "Test-Struct#ABC".foo(x) {
                        this.update(x);
                    }
                    fn foo(x) {
                        this += x;
                    }
                    
                    let z = 1000;
                    z.foo(1);

                    let x = new_ts();
                    x.foo(z);

                    x
                "#
            )
            .unwrap(),
        TestStruct { x: 1002 }
    );

    assert!(engine
        .eval::<bool>(
            r#"
                fn "Test-Struct#ABC".foo(x) {
                    this.update(x);
                }
                is_def_fn("Test-Struct#ABC", "foo", 1)
            "#
        )
        .unwrap());

    assert!(matches!(
        *engine
            .run(
                r#"
                    fn "Test-Struct#ABC".foo(x) {
                        this.update(x);
                    }
                    foo(1000);
                "#
            )
            .unwrap_err(),
        EvalAltResult::ErrorFunctionNotFound(f, ..) if f.starts_with("foo")
    ));

    assert!(matches!(
        *engine
            .run(
                r#"
                    fn "Test-Struct#ABC".foo(x) {
                        this.update(x);
                    }
                    let x = 42;
                    x.foo(1000);
                "#
            )
            .unwrap_err(),
        EvalAltResult::ErrorFunctionNotFound(f, ..) if f.starts_with("foo")
    ));
}

/// AST walk tests — verify that `walk` visits arguments inside `MethodCall` nodes.

#[test]
#[cfg(feature = "internals")]
fn test_method_call_walk_visits_args() {
    let engine = Engine::new();
    // `my_array.contains(value)` — `value` is an argument of a MethodCall node.
    let ast = engine.compile("my_array.contains(value)").unwrap();

    let mut vars: Vec<String> = Vec::new();
    ast.walk(&mut |nodes: &[ASTNode]| {
        if let Some(ASTNode::Expr(Expr::Variable(info, _, _))) = nodes.last() {
            vars.push(info.1.to_string());
        }
        true
    });

    assert!(vars.contains(&"my_array".to_string()), "walk should visit the receiver `my_array`");
    assert!(vars.contains(&"value".to_string()), "walk should visit the argument `value`");
}

#[test]
#[cfg(feature = "internals")]
fn test_method_call_walk_visits_multiple_args() {
    let engine = Engine::new();
    // Three variable arguments — all must be visited.
    let ast = engine.compile("obj.foo(a, b, c)").unwrap();

    let mut vars: Vec<String> = Vec::new();
    ast.walk(&mut |nodes: &[ASTNode]| {
        if let Some(ASTNode::Expr(Expr::Variable(info, _, _))) = nodes.last() {
            vars.push(info.1.to_string());
        }
        true
    });

    for name in &["obj", "a", "b", "c"] {
        assert!(vars.contains(&name.to_string()), "walk should visit `{name}`", name = name);
    }
}

#[test]
#[cfg(feature = "internals")]
fn test_method_call_walk_visits_nested_expr_in_arg() {
    let engine = Engine::new();
    // The argument itself contains a variable (`n`) inside an expression.
    let ast = engine.compile("obj.foo(n + 1)").unwrap();

    let mut vars: Vec<String> = Vec::new();
    ast.walk(&mut |nodes: &[ASTNode]| {
        if let Some(ASTNode::Expr(Expr::Variable(info, _, _))) = nodes.last() {
            vars.push(info.1.to_string());
        }
        true
    });

    assert!(vars.contains(&"obj".to_string()), "walk should visit the receiver `obj`");
    assert!(vars.contains(&"n".to_string()), "walk should visit `n` nested inside the arg expression");
}

#[test]
#[cfg(feature = "internals")]
fn test_method_call_walk_count_visits_matches_fn_call() {
    // `obj.foo(x)` (method-call syntax) and `foo(obj, x)` (free-function syntax)
    // should both surface the same two variable names via `walk`.
    let engine = Engine::new();

    let count_vars = |src: &str| -> Vec<String> {
        let ast = engine.compile(src).unwrap();
        let mut vars = Vec::new();
        ast.walk(&mut |nodes: &[ASTNode]| {
            if let Some(ASTNode::Expr(Expr::Variable(info, _, _))) = nodes.last() {
                vars.push(info.1.to_string());
            }
            true
        });
        vars
    };

    let method_vars = count_vars("obj.foo(x)");
    let fn_vars = count_vars("foo(obj, x)");

    // Both forms must surface `obj` and `x`.
    for name in &["obj", "x"] {
        assert!(method_vars.contains(&name.to_string()), "method syntax: walk should visit `{name}`", name = name);
        assert!(fn_vars.contains(&name.to_string()), "free-fn syntax: walk should visit `{name}`", name = name);
    }
}
