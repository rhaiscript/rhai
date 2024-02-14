use rhai::{Engine, Scope, INT};

#[test]
fn test_expressions() {
    let engine = Engine::new();
    let mut scope = Scope::new();

    scope.push("x", 10 as INT);

    assert_eq!(engine.eval_expression::<INT>("2 + (10 + 10) * 2").unwrap(), 42);
    assert_eq!(engine.eval_expression_with_scope::<INT>(&mut scope, "2 + (x + 10) * 2").unwrap(), 42);
    assert_eq!(engine.eval_expression_with_scope::<INT>(&mut scope, "if x > 0 { 42 } else { 123 }").unwrap(), 42);
    #[cfg(not(feature = "no_index"))]
    #[cfg(not(feature = "no_object"))]
    #[cfg(not(feature = "no_function"))]
    {
        assert_eq!(engine.eval_expression_with_scope::<INT>(&mut scope, "[1, 2, 3, 4].map(|x| x * x).reduce(|a, v| a + v, 0)").unwrap(), 30);
        assert!(engine
            .eval_expression_with_scope::<INT>(&mut scope, "[1, 2, 3, 4].map(|x| { let r = 2; x * r }).reduce(|a, v| a + v, 0)")
            .is_err());
    }
    assert!(engine.eval_expression_with_scope::<INT>(&mut scope, "if x > 0 { let y = 42; y } else { 123 }").is_err());
    assert!(engine.eval_expression_with_scope::<INT>(&mut scope, "if x > 0 { 42 } else { let y = 123; y }").is_err());
    assert!(engine.eval_expression_with_scope::<INT>(&mut scope, "if x > 0 { 42 } else {}").is_err());

    assert_eq!(
        engine
            .eval_expression_with_scope::<INT>(
                &mut scope,
                "
                    switch x {
                        0 => 1,
                        10 => 42,
                        1..10 => 123,
                    }
                "
            )
            .unwrap(),
        42
    );
    assert!(engine
        .eval_expression_with_scope::<INT>(
            &mut scope,
            "
                switch x {
                    0 => 1,
                    10 => 42,
                    1..10 => {
                        let y = 123;
                        y
                    }
                }
            "
        )
        .is_err());

    assert!(engine.compile_expression("40 + 2;").is_err());
    assert!(engine.compile_expression("40 + { 2 }").is_err());
    assert!(engine.compile_expression("x = 42").is_err());
    assert!(engine.compile_expression("let x = 42").is_err());
    assert!(engine.compile_expression("do { break 42; } while true").is_err());

    engine.compile("40 + { let x = 2; x }").unwrap();
}

/// This example taken from https://github.com/rhaiscript/rhai/issues/115
#[test]
#[cfg(not(feature = "no_object"))]
fn test_expressions_eval() {
    #[allow(clippy::upper_case_acronyms)]
    #[derive(Debug, Clone)]
    struct AGENT {
        pub gender: String,
        pub age: INT,
    }

    impl AGENT {
        pub fn get_gender(&mut self) -> String {
            self.gender.clone()
        }
        pub fn get_age(&mut self) -> INT {
            self.age
        }
    }

    // This is your agent
    let my_agent = AGENT { gender: "male".into(), age: 42 };

    // Create the engine
    let mut engine = Engine::new();

    // Register your AGENT type
    engine.register_type_with_name::<AGENT>("AGENT");
    engine.register_get("gender", AGENT::get_gender);
    engine.register_get("age", AGENT::get_age);

    // Create your scope, add the agent as a constant
    let mut scope = Scope::new();
    scope.push_constant("agent", my_agent);

    // Evaluate the expression
    assert!(engine
        .eval_expression_with_scope::<bool>(
            &mut scope,
            r#"
                agent.age > 10 && agent.gender == "male"
            "#,
        )
        .unwrap());
}
