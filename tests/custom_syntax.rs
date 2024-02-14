#![cfg(not(feature = "no_custom_syntax"))]

use rhai::{Dynamic, Engine, EvalAltResult, ImmutableString, LexError, ParseErrorType, Position, Scope, INT};

#[test]
fn test_custom_syntax() {
    let mut engine = Engine::new();

    engine.run("while false {}").unwrap();

    // Disable 'while' and make sure it still works with custom syntax
    engine.disable_symbol("while");
    assert!(matches!(engine.compile("while false {}").unwrap_err().err_type(), ParseErrorType::Reserved(err) if err == "while"));
    assert!(matches!(engine.compile("let while = 0").unwrap_err().err_type(), ParseErrorType::Reserved(err) if err == "while"));

    // Implement ternary operator
    engine
        .register_custom_syntax(["iff", "$expr$", "?", "$expr$", ":", "$expr$"], false, |context, inputs| match context.eval_expression_tree(&inputs[0]).unwrap().as_bool() {
            Ok(true) => context.eval_expression_tree(&inputs[1]),
            Ok(false) => context.eval_expression_tree(&inputs[2]),
            Err(typ) => Err(Box::new(EvalAltResult::ErrorMismatchDataType("bool".to_string(), typ.to_string(), inputs[0].position()))),
        })
        .unwrap();

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = 42;
                    let y = iff x > 40 ? 0 : 123;
                    y
                "
            )
            .unwrap(),
        0
    );

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = 42;
                    let y = iff x == 0 ? 0 : 123;
                    y
                "
            )
            .unwrap(),
        123
    );

    // Custom syntax
    engine
        .register_custom_syntax(["exec", "[", "$ident$", "$symbol$", "$int$", "]", "->", "$block$", "while", "$expr$"], true, |context, inputs| {
            let var_name = inputs[0].get_string_value().unwrap();
            let op = inputs[1].get_literal_value::<ImmutableString>().unwrap();
            let max = inputs[2].get_literal_value::<INT>().unwrap();
            let stmt = &inputs[3];
            let condition = &inputs[4];

            context.scope_mut().push(var_name.to_string(), 0 as INT);

            let mut count: INT = 0;

            loop {
                let done = match op.as_str() {
                    "<" => count >= max,
                    "<=" => count > max,
                    ">" => count <= max,
                    ">=" => count < max,
                    "==" => count != max,
                    "!=" => count == max,
                    _ => return Err(format!("Unsupported operator: {op}").into()),
                };

                if done {
                    break;
                }

                // Do not rewind if the variable is upper-case
                let _: Dynamic = if var_name.to_uppercase() == var_name {
                    #[allow(deprecated)] // not deprecated but unstable
                    context.eval_expression_tree_raw(stmt, false)
                } else {
                    context.eval_expression_tree(stmt)
                }?;

                count += 1;

                context.scope_mut().push(format!("{var_name}{count}"), count);

                let stop = !context
                    .eval_expression_tree(condition)
                    .unwrap()
                    .as_bool()
                    .map_err(|err| Box::new(EvalAltResult::ErrorMismatchDataType("bool".to_string(), err.to_string(), condition.position())))
                    .unwrap();

                if stop {
                    break;
                }
            }

            Ok(count.into())
        })
        .unwrap();

    assert!(matches!(*engine.run("let foo = (exec [x<<15] -> { x += 2 } while x < 42) * 10;").unwrap_err(), EvalAltResult::ErrorRuntime(..)));

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = 0;
                    let foo = (exec [x<15] -> { x += 2 } while x < 42) * 10;
                    foo
                "
            )
            .unwrap(),
        150
    );
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = 0;
                    exec [x<100] -> { x += 1 } while x < 42;
                    x
                "
            )
            .unwrap(),
        42
    );
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    exec [x<100] -> { x += 1 } while x < 42;
                    x
                "
            )
            .unwrap(),
        42
    );
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let foo = 123;
                    exec [x<15] -> { x += 1 } while x < 42;
                    foo + x + x1 + x2 + x3
                "
            )
            .unwrap(),
        144
    );
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let foo = 123;
                    exec [x<15] -> { let foo = x; x += 1; } while x < 42;
                    foo
                "
            )
            .unwrap(),
        123
    );
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let foo = 123;
                    exec [ABC<15] -> { let foo = ABC; ABC += 1; } while ABC < 42;
                    foo
                "
            )
            .unwrap(),
        14
    );

    // The first symbol must be an identifier
    assert_eq!(
        *engine.register_custom_syntax(["!"], false, |_, _| Ok(Dynamic::UNIT)).unwrap_err().err_type(),
        ParseErrorType::BadInput(LexError::ImproperSymbol("!".to_string(), "Improper symbol for custom syntax at position #1: '!'".to_string()))
    );

    // Check self-termination
    engine
        .register_custom_syntax(["test1", "$block$"], true, |_, _| Ok(Dynamic::UNIT))
        .unwrap()
        .register_custom_syntax(["test2", "}"], true, |_, _| Ok(Dynamic::UNIT))
        .unwrap()
        .register_custom_syntax(["test3", ";"], true, |_, _| Ok(Dynamic::UNIT))
        .unwrap();

    assert_eq!(engine.eval::<INT>("test1 { x = y + z; } 42").unwrap(), 42);
    assert_eq!(engine.eval::<INT>("test2 } 42").unwrap(), 42);
    assert_eq!(engine.eval::<INT>("test3; 42").unwrap(), 42);

    // Register the custom syntax: var x = ???
    engine
        .register_custom_syntax(["var", "$ident$", "=", "$expr$"], true, |context, inputs| {
            let var_name = inputs[0].get_string_value().unwrap();
            let expr = &inputs[1];

            // Evaluate the expression
            let value = context.eval_expression_tree(expr).unwrap();

            if !context.scope().is_constant(var_name).unwrap_or(false) {
                context.scope_mut().set_value(var_name.to_string(), value);
                Ok(Dynamic::UNIT)
            } else {
                Err(format!("variable {var_name} is constant").into())
            }
        })
        .unwrap();

    let mut scope = Scope::new();

    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "var foo = 42; foo").unwrap(), 42);
    assert_eq!(scope.get_value::<INT>("foo"), Some(42));
    assert_eq!(scope.len(), 1);
    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "var foo = 123; foo").unwrap(), 123);
    assert_eq!(scope.get_value::<INT>("foo"), Some(123));
    assert_eq!(scope.len(), 1);
}

#[test]
fn test_custom_syntax_scope() {
    let mut engine = Engine::new();

    engine
        .register_custom_syntax(["with", "offset", "(", "$expr$", ",", "$expr$", ")", "$block$"], true, |context, inputs| {
            let x = context
                .eval_expression_tree(&inputs[0])
                .unwrap()
                .as_int()
                .map_err(|typ| Box::new(EvalAltResult::ErrorMismatchDataType("integer".to_string(), typ.to_string(), inputs[0].position())))
                .unwrap();

            let y = context
                .eval_expression_tree(&inputs[1])
                .unwrap()
                .as_int()
                .map_err(|typ| Box::new(EvalAltResult::ErrorMismatchDataType("integer".to_string(), typ.to_string(), inputs[1].position())))
                .unwrap();

            let orig_len = context.scope().len();

            context.scope_mut().push_constant("x", x);
            context.scope_mut().push_constant("y", y);

            let result = context.eval_expression_tree(&inputs[2]);

            context.scope_mut().rewind(orig_len);

            result
        })
        .unwrap();

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let y = 1;
                    let x = 0;
                    with offset(44, 2) { x - y }
                "
            )
            .unwrap(),
        42
    );
}

#[test]
fn test_custom_syntax_matrix() {
    let mut engine = Engine::new();

    engine.disable_symbol("|");

    engine
        .register_custom_syntax(
            [
                "@", //
                "|", "$expr$", "$expr$", "$expr$", "|", //
                "|", "$expr$", "$expr$", "$expr$", "|", //
                "|", "$expr$", "$expr$", "$expr$", "|",
            ],
            false,
            |context, inputs| {
                let mut values = [[0; 3]; 3];

                for y in 0..3 {
                    for x in 0..3 {
                        let offset = y * 3 + x;

                        match context.eval_expression_tree(&inputs[offset]).unwrap().as_int() {
                            Ok(v) => values[y][x] = v,
                            Err(typ) => return Err(Box::new(EvalAltResult::ErrorMismatchDataType("integer".to_string(), typ.to_string(), inputs[offset].position()))),
                        }
                    }
                }

                Ok(Dynamic::from(values))
            },
        )
        .unwrap();

    let r = engine
        .eval::<[[INT; 3]; 3]>(
            "
                let a = 42;
                let b = 123;
                let c = 1;
                let d = 99;

                @|  a   b   0  |
                | -b   a   0  |
                |  0   0  c*d |
            ",
        )
        .unwrap();

    assert_eq!(r, [[42, 123, 0], [-123, 42, 0], [0, 0, 99]]);
}

#[test]
fn test_custom_syntax_raw() {
    let mut engine = Engine::new();

    engine.register_custom_syntax_with_state_raw(
        "hello",
        |stream, look_ahead, state| match stream.len() {
            0 => unreachable!(),
            1 if look_ahead == "\"world\"" => {
                *state = Dynamic::TRUE;
                Ok(Some("$string$".into()))
            }
            1 => {
                *state = Dynamic::FALSE;
                Ok(Some("$ident$".into()))
            }
            2 => match stream[1].as_str() {
                "world" if state.as_bool().unwrap_or(false) => Ok(Some("$$world".into())),
                "world" => Ok(Some("$$hello".into())),
                "kitty" => {
                    *state = (42 as INT).into();
                    Ok(None)
                }
                s => Err(LexError::ImproperSymbol(s.to_string(), String::new()).into_err(Position::NONE)),
            },
            _ => unreachable!(),
        },
        true,
        |context, inputs, state| {
            context.scope_mut().push("foo", 999 as INT);

            Ok(match inputs[0].get_string_value().unwrap() {
                "world" => match inputs.last().unwrap().get_string_value().unwrap_or("") {
                    "$$hello" => 0 as INT,
                    "$$world" => 123456 as INT,
                    _ => 123 as INT,
                },
                "kitty" if inputs.len() > 1 => 999 as INT,
                "kitty" => state.as_int().unwrap(),
                _ => unreachable!(),
            }
            .into())
        },
    );

    assert_eq!(engine.eval::<INT>(r#"hello "world""#).unwrap(), 123456);
    assert_eq!(engine.eval::<INT>("hello world").unwrap(), 0);
    assert_eq!(engine.eval::<INT>("hello kitty").unwrap(), 42);
    assert_eq!(engine.eval::<INT>("let foo = 0; (hello kitty) + foo").unwrap(), 1041);
    assert_eq!(engine.eval::<INT>("(hello kitty) + foo").unwrap(), 1041);
    assert_eq!(*engine.compile("hello hey").unwrap_err().err_type(), ParseErrorType::BadInput(LexError::ImproperSymbol("hey".to_string(), String::new())));
}

#[test]
fn test_custom_syntax_raw2() {
    let mut engine = Engine::new();

    engine.register_custom_syntax_with_state_raw(
        "#",
        |symbols, lookahead, _| match symbols.len() {
            1 if lookahead == "-" => Ok(Some("$symbol$".into())),
            1 => Ok(Some("$int$".into())),
            2 if symbols[1] == "-" => Ok(Some("$int$".into())),
            2 => Ok(None),
            3 => Ok(None),
            _ => unreachable!(),
        },
        false,
        move |_, inputs, _| {
            let id = if inputs.len() == 2 { -inputs[1].get_literal_value::<INT>().unwrap() } else { inputs[0].get_literal_value::<INT>().unwrap() };
            Ok(id.into())
        },
    );

    assert_eq!(engine.eval::<INT>("#-1").unwrap(), -1);
    assert_eq!(engine.eval::<INT>("let x = 41; x + #1").unwrap(), 42);
    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>("#-42.abs()").unwrap(), 42);
    assert_eq!(engine.eval::<INT>("#42/2").unwrap(), 21);
    assert_eq!(engine.eval::<INT>("sign(#1)").unwrap(), 1);
}
