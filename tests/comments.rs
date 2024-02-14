use rhai::{Engine, INT};

#[test]
fn test_comments() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 42; x // I am a single line comment, yay!").unwrap(), 42);

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let /* I am a
                        multi-line
                            comment, yay!
                        */ x = 42; x
                "
            )
            .unwrap(),
        42
    );

    engine.run("/* Hello world */").unwrap();

    let _ = engine.run(" /**\n\u{1680}  */");

    #[cfg(not(feature = "no_function"))]
    assert!(engine.compile("fn/*").is_err());
}

#[cfg(not(feature = "no_function"))]
#[cfg(feature = "metadata")]
#[test]
fn test_comments_doc() {
    let engine = Engine::new();

    let ast = engine
        .compile(
            "
                /// Hello world


                fn foo() {}
            ",
        )
        .unwrap();

    assert_eq!(ast.iter_functions().next().unwrap().comments[0], "/// Hello world");

    assert!(engine
        .compile(
            "
                /// Hello world
                let x = 42;
            "
        )
        .is_err());

    engine
        .compile(
            "
                ///////////////
                let x = 42;

                /***************/
                let x = 42;
            ",
        )
        .unwrap();

    let ast = engine
        .compile(
            "
                /** Hello world
                ** how are you?
                **/

                fn foo() {}
            ",
        )
        .unwrap();

    #[cfg(not(feature = "no_position"))]
    assert_eq!(ast.iter_functions().next().unwrap().comments[0], "/** Hello world\n** how are you?\n**/");
    #[cfg(feature = "no_position")]
    assert_eq!(ast.iter_functions().next().unwrap().comments[0], "/** Hello world\n                ** how are you?\n                **/",);

    assert!(engine
        .compile(
            "
                /** Hello world */
                let x = 42;
            "
        )
        .is_err());
}
