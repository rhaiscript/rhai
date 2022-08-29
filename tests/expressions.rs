use rhai::{Engine, EvalAltResult, Scope, INT};

#[test]
fn test_expressions() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();
    let mut scope = Scope::new();

    scope.push("x", 10 as INT);

    assert_eq!(engine.eval_expression::<INT>("2 + (10 + 10) * 2")?, 42);
    assert_eq!(
        engine.eval_expression_with_scope::<INT>(&mut scope, "2 + (x + 10) * 2")?,
        42
    );
    assert_eq!(
        engine.eval_expression_with_scope::<INT>(&mut scope, "if x > 0 { 42 } else { 123 }")?,
        42
    );
    assert!(engine
        .eval_expression_with_scope::<INT>(&mut scope, "if x > 0 { let y = 42; y } else { 123 }")
        .is_err());
    assert!(engine
        .eval_expression_with_scope::<INT>(&mut scope, "if x > 0 { 42 } else { let y = 123; y }")
        .is_err());
    assert!(engine
        .eval_expression_with_scope::<INT>(&mut scope, "if x > 0 { 42 } else {}")
        .is_err());

    assert_eq!(
        engine.eval_expression_with_scope::<INT>(
            &mut scope,
            "
                switch x {
                    0 => 1,
                    1..10 => 123,
                    10 => 42,
                }
            "
        )?,
        42
    );
    assert!(engine
        .eval_expression_with_scope::<INT>(
            &mut scope,
            "
                switch x {
                    0 => 1,
                    1..10 => {
                        let y = 123;
                        y
                    }
                    10 => 42,
                }
            "
        )
        .is_err());

    assert!(engine.eval_expression::<()>("40 + 2;").is_err());
    assert!(engine.eval_expression::<()>("40 + { 2 }").is_err());
    assert!(engine.eval_expression::<()>("x = 42").is_err());
    assert!(engine.compile_expression("let x = 42").is_err());

    engine.compile("40 + { let x = 2; x }")?;

    Ok(())
}

/// This example taken from https://github.com/rhaiscript/rhai/issues/115
#[test]
#[cfg(not(feature = "no_object"))]
fn test_expressions_eval() -> Result<(), Box<EvalAltResult>> {
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
    let my_agent = AGENT {
        gender: "male".into(),
        age: 42,
    };

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
    let result: bool = engine.eval_expression_with_scope(
        &mut scope,
        r#"
            agent.age > 10 && agent.gender == "male"
        "#,
    )?;

    assert_eq!(result, true);

    Ok(())
}
