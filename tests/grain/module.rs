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

    let mut resolver = StaticModuleResolver::new();
    resolver.insert("sub", sub_module);
    engine.set_module_resolver(resolver);

    let ast = engine
        .compile(
            r#"
                import "sub" as s;
                export let offset = s::BASE;
                fn compute(x) {
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

    assert_eq!(engine.eval::<INT>(r#"import "top" as t; t::compute(10)"#).unwrap(), 130);
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
#[cfg(not(any(feature = "no_function", feature = "no_index", feature = "no_object")))]
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
