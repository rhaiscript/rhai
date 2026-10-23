#![cfg(not(feature = "no_module"))]

use super::corpus;
use rhai::grain::Compiler;
use rhai::module_resolvers::{FileModuleResolver, StaticModuleResolver};
use rhai::{EvalAltResult, Module, Scope, INT};

#[test]
fn export_variables_and_aliases() {
    let engine = corpus::engine();
    let ast = engine
        .compile(
            r#"
                export let a = 10;
                export const B = 20;
                let c = 30;
                let d = 40;
                export c;
                export d as xyz;
            "#,
        )
        .expect("must compile");

    let program = Compiler::new().compile(&ast);
    let module = Module::eval_grain_as_new(Scope::new(), program, &engine).expect("must eval as grain module");

    assert_eq!(module.get_var_value::<INT>("a").unwrap(), 10);
    assert_eq!(module.get_var_value::<INT>("B").unwrap(), 20);
    assert_eq!(module.get_var_value::<INT>("c").unwrap(), 30);
    assert_eq!(module.get_var_value::<INT>("xyz").unwrap(), 40);
    assert!(!module.contains_var("d"));
}

#[test]
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "only_i32"))]
fn public_and_private_functions() {
    let mut engine = corpus::engine();
    let ast = engine
        .compile(
            r#"
                fn public_add(x, y) {
                    internal_calc(x, y)
                }

                private fn internal_calc(x, y) {
                    x + y + 10
                }
            "#,
        )
        .expect("must compile");

    let program = Compiler::new().compile(&ast);
    let module = Module::eval_grain_as_new(Scope::new(), program, &engine).expect("must eval grain module");

    let mut resolver = StaticModuleResolver::new();
    resolver.insert("math_mod", module);
    engine.set_module_resolver(resolver);

    assert_eq!(engine.eval::<INT>(r#"import "math_mod" as m; m::public_add(5, 7)"#).unwrap(), 22);

    assert!(matches!(
        *engine
            .run(r#"import "math_mod" as m; m::internal_calc(5, 7)"#)
            .unwrap_err(),
        EvalAltResult::ErrorFunctionNotFound(fn_name, ..) if fn_name == "m::internal_calc (i64, i64)"
    ));
}

#[test]
#[cfg(not(feature = "no_function"))]
fn import_in_grain_script() {
    let mut engine = corpus::engine();

    let mut sub_module = Module::new();
    sub_module.set_var("BASE", 100 as INT);
    sub_module.set_native_fn("triple", |x: INT| Ok(x * 3));
    sub_module.set_native_fn("increment", |x: &mut INT| {
        *x += 1;
        Ok(())
    });

    let mut resolver = StaticModuleResolver::new();
    resolver.insert("sub", sub_module);
    engine.set_module_resolver(resolver);

    let ast = engine
        .compile(
            r#"
                import "sub" as s;
                export let offset = s::BASE;
                fn compute(x) {
                    s::increment(x);
                    s::triple(x) + 100
                }
            "#,
        )
        .expect("must compile");

    let program = Compiler::new().compile(&ast);
    let module = Module::eval_grain_as_new(Scope::new(), program, &engine).expect("must eval as grain module");

    let mut top_resolver = StaticModuleResolver::new();
    top_resolver.insert("top", module);
    engine.set_module_resolver(top_resolver);

    assert_eq!(engine.eval::<INT>(r#"import "top" as t; t::compute(10)"#).unwrap(), 133);
}

#[test]
#[cfg(not(feature = "no_std"))]
#[cfg(any(not(target_family = "wasm"), not(target_os = "unknown")))]
#[cfg(not(feature = "no_function"))]
fn file_module_resolver_grain_artifact() {
    let engine = corpus::engine();

    let module_ast = engine
        .compile(
            r#"
                export const MULTIPLIER = 10;
                fn multiply(x) {
                    x * 10
                }
                private fn private_helper() { 1 }
                fn calculate(x) {
                    multiply(x) + private_helper()
                }
            "#,
        )
        .expect("must compile module");

    let program = Compiler::new().compile(&module_ast);
    let bytes = program.write().expect("must serialize");

    let temp_dir = std::env::temp_dir().join(format!("rhai_file_grain_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let artifact_path = temp_dir.join("calc_mod.rgrn");
    std::fs::write(&artifact_path, bytes).unwrap();

    let mut resolver = FileModuleResolver::new_with_path(&temp_dir);
    resolver.enable_cache(true);

    let mut engine_with_resolver = corpus::engine();
    engine_with_resolver.set_module_resolver(resolver);

    // Test loading without extension
    let val1 = engine_with_resolver.eval::<INT>(r#"import "calc_mod" as c; c::calculate(5)"#).unwrap();
    assert_eq!(val1, 51);

    // Test loading with explicit .rgrn
    let val2 = engine_with_resolver.eval::<INT>(r#"import "calc_mod.rgrn" as c; c::MULTIPLIER"#).unwrap();
    assert_eq!(val2, 10);

    // Test object call on exported constant
    #[cfg(not(feature = "no_object"))]
    {
        let val3 = engine_with_resolver.eval::<INT>(r#"import "calc_mod.rgrn" as c; c::MULTIPLIER.sign()"#).unwrap();
        assert_eq!(val3, 1);
    }

    // Clean up
    let _ = std::fs::remove_file(artifact_path);
    let _ = std::fs::remove_dir(temp_dir);
}

#[test]
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_index"))]
#[cfg(not(feature = "no_object"))]
#[cfg(not(feature = "no_closure"))]
fn grain_fn_ptr_callbacks_and_closures() {
    let mut engine = corpus::engine();

    let ast = engine
        .compile(
            r#"
                fn apply_to_list(list, f) {
                    list.map(f)
                }

                fn make_adder(n) {
                    |x| x + n
                }
            "#,
        )
        .expect("must compile");

    let program = Compiler::new().compile(&ast);
    let module = Module::eval_grain_as_new(Scope::new(), program, &engine).expect("must eval grain module");

    let mut resolver = StaticModuleResolver::new();
    resolver.insert("fn_mod", module);
    engine.set_module_resolver(resolver);

    let res = engine
        .eval::<rhai::Array>(
            r#"
                import "fn_mod" as f;
                let add5 = f::make_adder(5);
                f::apply_to_list([1, 2, 3], add5)
            "#,
        )
        .unwrap();

    let items: Vec<INT> = res.into_iter().map(|d| d.cast::<INT>()).collect();
    assert_eq!(items, vec![6, 7, 8]);
}

/// A Grain module looks its functions up in the program when they are called,
/// so a lookup has to find what registering them would have found: the public
/// ones under their qualified names, and none of the private ones.
#[test]
#[cfg(not(feature = "no_function"))]
fn functions_are_found_without_being_registered() {
    let mut engine = corpus::engine();
    let ast = engine
        .compile(
            r#"
                fn one() { 1 }
                fn add(a, b) { a + b }
                private fn hidden() { 2 }
            "#,
        )
        .expect("must compile");

    let program = Compiler::new().compile(&ast);
    let module = Module::eval_grain_as_new(Scope::new(), program, &engine).expect("must eval grain module");
    assert_eq!(module.count().1, 2, "the private function is not exported");
    assert!(!module.is_empty());

    let mut resolver = StaticModuleResolver::new();
    resolver.insert("m", module);
    engine.set_module_resolver(resolver);

    assert_eq!(engine.eval::<INT>(r#"import "m" as m; m::one() + m::add(m::one(), 2)"#).unwrap(), 4);
    // The same call again resolves to the definition the first one made.
    assert_eq!(engine.eval::<INT>(r#"import "m" as m; let n = 0; for i in 0..3 { n += m::add(i, 1) } n"#).unwrap(), 6);
    assert!(engine.run(r#"import "m" as m; m::add(1)"#).is_err(), "the parameter count is part of the name");
    assert!(engine.run(r#"import "m" as m; m::hidden()"#).is_err());
}

/// A method declared for a receiver type is in the global namespace, so it is
/// found without the module's name.
#[test]
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_object"))]
fn a_typed_method_is_found_globally() {
    let mut engine = corpus::engine();
    let ast = engine.compile(r#"fn int.doubled() { this * 2 }"#).expect("must compile");

    let program = Compiler::new().compile(&ast);
    let module = Module::eval_grain_as_new(Scope::new(), program, &engine).expect("must eval grain module");

    let mut resolver = StaticModuleResolver::new();
    resolver.insert("m", module);
    engine.set_module_resolver(resolver);

    assert_eq!(engine.eval::<INT>(r#"import "m" as m; let x = 21; x.doubled()"#).unwrap(), 42);
}

/// Combining a Grain module into another keeps its functions looked up only
/// when called, taking precedence over functions already there.
#[test]
#[cfg(not(feature = "no_function"))]
fn a_combined_module_keeps_its_functions() {
    let mut engine = corpus::engine();
    let ast = engine.compile(r#"fn seven() { 7 }"#).expect("must compile");

    let program = Compiler::new().compile(&ast);
    let grain = Module::eval_grain_as_new(Scope::new(), program, &engine).expect("must eval grain module");

    let walker = engine.compile(r#"fn seven() { 0 } fn six() { 6 }"#).expect("must compile");
    let mut combined = Module::eval_ast_as_new(Scope::new(), &walker, &engine).expect("must eval module");
    combined.set_native_fn("eight", || Ok(8 as INT));
    combined.combine(grain);
    assert!(combined.has_lazy_functions());
    combined.build_index();
    assert_eq!(combined.count().1, 3);

    let mut resolver = StaticModuleResolver::new();
    resolver.insert("m", combined);
    engine.set_module_resolver(resolver);

    assert_eq!(engine.eval::<INT>(r#"import "m" as m; m::seven() + m::eight()"#).unwrap(), 15);
    assert_eq!(engine.eval::<INT>(r#"import "m" as m; m::six()"#).unwrap(), 6);
}

/// Filling a module with a Grain module adds only the functions it does not
/// have, whether registered or looked up only when called.
#[test]
#[cfg(not(feature = "no_function"))]
fn a_filled_module_keeps_its_own_functions() {
    let mut engine = corpus::engine();
    let program = |engine: &rhai::Engine, script: &str| Compiler::new().compile(&engine.compile(script).expect("must compile"));

    let mut filled = Module::eval_grain_as_new(Scope::new(), program(&engine, r#"fn seven() { 7 }"#), &engine).expect("must eval grain module");
    let other = engine.compile(r#"fn seven() { 0 } fn six() { 0 }"#).expect("must compile");
    filled.fill_with(&Module::eval_ast_as_new(Scope::new(), &other, &engine).expect("must eval module"));
    let other = Module::eval_grain_as_new(Scope::new(), program(&engine, r#"fn six() { 6 } fn five() { 5 }"#), &engine).expect("must eval grain module");
    filled.fill_with(&other);
    filled.build_index();

    let mut resolver = StaticModuleResolver::new();
    resolver.insert("m", filled);
    engine.set_module_resolver(resolver);

    assert_eq!(engine.eval::<INT>(r#"import "m" as m; m::seven()"#).unwrap(), 7);
    assert_eq!(engine.eval::<INT>(r#"import "m" as m; m::six()"#).unwrap(), 0);
    assert_eq!(engine.eval::<INT>(r#"import "m" as m; m::five()"#).unwrap(), 5);
}

/// Metadata lists a Grain module's functions, as it lists those of a plugin
/// module's manifest.
#[test]
#[cfg(feature = "metadata")]
#[cfg(not(feature = "no_function"))]
fn metadata_lists_the_functions() {
    let engine = corpus::engine();
    let ast = engine.compile(r#"fn greet(name) { name }"#).expect("must compile");

    let program = Compiler::new().compile(&ast);
    let mut module = Module::eval_grain_as_new(Scope::new(), program, &engine).expect("must eval grain module");
    assert!(module.has_lazy_functions());

    module.register_lazy_functions();
    assert!(!module.has_lazy_functions());
    let signatures: Vec<String> = module.gen_fn_signatures_with_mapper(Into::into).collect();
    assert_eq!(signatures.len(), 1, "{signatures:?}");
    assert!(signatures[0].starts_with("greet(name"), "{signatures:?}");
}
