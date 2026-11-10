#[cfg(not(feature = "no_ast"))]
mod ast {
    use rhai::grain::{Compiler, Program, WriteError};
    use rhai::{Engine, FnAccess, ScriptFnMetadata, AST};

    fn metadata(ast: &AST) -> Vec<ScriptFnMetadata<'_>> {
        let mut functions: Vec<_> = ast.iter_functions().collect();
        #[cfg(feature = "metadata")]
        for function in &mut functions {
            function.comments.clear();
        }
        functions.sort();
        functions
    }

    fn assert_discovery(program: &Program, expected: &[ScriptFnMetadata<'_>]) {
        let mut actual: Vec<_> = program.iter_functions().collect();
        actual.sort();
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), program.num_functions());
        assert_eq!(program.has_functions(), !actual.is_empty());
    }

    #[test]
    fn compiled_metadata_matches_ast() {
        let ast = Engine::new()
            .compile("fn foo() { 1 } fn foo(first, second) { first + second } private fn hidden(value) { value }")
            .unwrap();
        let expected = metadata(&ast);
        assert_eq!(expected.len(), 3);
        assert!(expected.iter().any(|f| f.access == FnAccess::Private));
        assert!(expected.iter().any(|f| f.params == ["first", "second"]));
        let program = Compiler::new().compile(&ast);
        assert_discovery(&program, &expected);

        let bytes = program.write().unwrap();
        let loaded = Program::read(&bytes).unwrap();
        assert_discovery(&loaded, &expected);
        let owned = loaded.into_owned();
        assert_discovery(&owned, &expected);
        let shared = owned.into_shared();
        assert_discovery(&shared, &expected);

        let stripped = program.write_stripped().unwrap();
        let loaded = Program::read(&stripped.artifact).unwrap();
        assert_discovery(&loaded, &expected);
    }

    #[test]
    fn empty_and_fallback_only_programs_have_no_compiled_functions() {
        let engine = Engine::new();
        let empty = Compiler::new().compile(&engine.compile("42").unwrap());
        assert_discovery(&empty, &[]);
        let ast = engine.compile(r#"fn fallback(value) { eval("1"); value }"#).unwrap();
        assert!(ast.has_functions());
        let program = Compiler::new().compile(&ast);
        assert!(matches!(program.write(), Err(WriteError::HasScriptFunctions)));
        assert_discovery(&program, &[]);
    }

    #[test]
    fn retained_ast_library_is_excluded() {
        let ast = Engine::new()
            .compile(r#"fn fallback(value) { eval("1"); value } fn compiled(first, second) { first + second }"#)
            .unwrap();
        let expected: Vec<_> = metadata(&ast).into_iter().filter(|f| f.name == "compiled").collect();
        assert_eq!(expected.len(), 1);
        let program = Compiler::new().compile(&ast);
        assert!(matches!(program.write(), Err(WriteError::HasScriptFunctions)));
        assert_discovery(&program, &expected);
    }

    #[cfg(not(feature = "no_object"))]
    #[test]
    fn typed_overloads_remain_distinct() {
        let ast = Engine::new()
            .compile("fn foo(value) { value } fn int.foo(value) { this + value } private fn string.foo(value) { value }")
            .unwrap();
        let expected = metadata(&ast);
        assert_eq!(expected.len(), 3);
        assert_eq!(expected.iter().filter(|f| f.this_type.is_some()).count(), 2);
        let program = Compiler::new().compile(&ast);
        assert_discovery(&program, &expected);
        let bytes = program.write().unwrap();
        assert_discovery(&Program::read(&bytes).unwrap(), &expected);
    }

    #[cfg(not(feature = "no_closure"))]
    #[test]
    fn anonymous_functions_are_discoverable() {
        let ast = Engine::new().compile("let callback = |value| value + 1;").unwrap();
        let expected = metadata(&ast);
        assert_eq!(expected.len(), 1);
        let program = Compiler::new().compile(&ast);
        assert_discovery(&program, &expected);
        let bytes = program.write().unwrap();
        assert_discovery(&Program::read(&bytes).unwrap(), &expected);
    }

    #[cfg(feature = "metadata")]
    #[test]
    fn compiled_doc_comments_are_empty_even_with_a_retained_library() {
        let ast = Engine::new()
            .compile(
                r#"
                /// Compiled function documentation.
                fn compiled(value) { value }
                /// Fallback function documentation.
                fn fallback() { eval("1"); }
            "#,
            )
            .unwrap();
        assert!(ast.iter_functions().all(|f| !f.comments.is_empty()));
        let expected: Vec<_> = metadata(&ast).into_iter().filter(|f| f.name == "compiled").collect();
        let program = Compiler::new().compile(&ast);
        assert!(matches!(program.write(), Err(WriteError::HasScriptFunctions)));
        assert_discovery(&program, &expected);
        assert!(program.iter_functions().all(|f| f.comments.is_empty()));
    }
}

#[cfg(feature = "grain")]
#[cfg(feature = "no_ast")]
#[cfg(not(feature = "no_float"))]
#[cfg(not(feature = "no_index"))]
#[cfg(not(feature = "no_object"))]
#[cfg(not(feature = "no_closure"))]
#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "f32_float"))]
#[cfg(not(feature = "decimal"))]
#[test]
fn discover_functions_without_an_ast() {
    use rhai::grain::Program;
    use rhai::FnAccess;

    let bytes = include_bytes!("grain/fixtures/golden.rgrn");
    let program = Program::read(bytes).unwrap();
    assert!(program.has_functions());
    assert_eq!(program.iter_functions().count(), program.num_functions());
    assert_eq!(program.num_functions(), 2);

    let tally = program.iter_functions().find(|f| f.name == "tally").unwrap();
    assert_eq!(tally.params, ["items", "bias"]);
    assert_eq!(tally.access, FnAccess::Public);
    assert_eq!(tally.this_type, None);
    #[cfg(feature = "metadata")]
    assert!(tally.comments.is_empty());

    let expected: Vec<_> = program.iter_functions().collect();
    let owned = Program::read(bytes).unwrap().into_owned();
    assert_eq!(owned.iter_functions().collect::<Vec<_>>(), expected);
    let shared = owned.into_shared();
    assert_eq!(shared.iter_functions().collect::<Vec<_>>(), expected);
}
