//! Functions in plugin modules resolved lazily from their manifests.

use rhai::packages::{Package, StandardPackage};
use rhai::plugin::*;
use rhai::{Engine, EvalAltResult, INT};

#[derive(Debug, Clone, PartialEq)]
pub struct Widget(INT);

#[export_module(manifest)]
mod kit {
    use super::Widget;
    use rhai::{Dynamic, INT};

    pub const ANSWER: INT = 42;

    pub fn double(x: INT) -> INT {
        x * 2
    }
    #[rhai_fn(name = "area")]
    pub fn area_int(x: INT) -> INT {
        x * x
    }
    #[rhai_fn(name = "area")]
    pub fn area_str(s: &str) -> INT {
        s.len() as INT
    }
    #[rhai_fn(name = "describe")]
    pub fn describe_any(_x: Dynamic) -> String {
        "any".into()
    }
    #[rhai_fn(name = "describe")]
    pub fn describe_int(_x: INT) -> String {
        "int".into()
    }
    pub fn add(a: INT, b: INT) -> INT {
        a + b
    }
    #[rhai_fn(global)]
    pub fn triple(x: INT) -> INT {
        x * 3
    }
    pub fn widget(x: INT) -> Widget {
        Widget(x)
    }
    #[rhai_fn(get = "size", pure)]
    pub fn size(w: &mut Widget) -> INT {
        w.0
    }
    #[rhai_fn(name = "+")]
    pub fn add_widgets(a: Widget, b: Widget) -> Widget {
        Widget(a.0 + b.0)
    }

    pub mod sub {
        pub fn hello() -> String {
            "hi".into()
        }
    }
}

/// A module with the plugin module's manifest combined, flattening sub-modules, as in a package.
fn lazy_global_module() -> Module {
    let mut module = Module::new();
    module.combine_manifest(exported_manifest!(kit));
    module.build_index();
    module
}

fn engine_with(module: Module) -> Engine {
    let mut engine = Engine::new();
    engine.register_type_with_name::<Widget>("Widget");
    engine.register_global_module(module.into());
    engine
}

#[test]
fn test_lazy_manifest() {
    let manifest = exported_manifest!(kit);
    let names: Vec<_> = manifest.functions.iter().map(|f| f.name).collect();
    assert!(names.contains(&"get$size"));
    assert_eq!(names.iter().filter(|&&n| n == "area").count(), 2);
    assert_eq!(manifest.sub_modules.len(), 1);
    assert_eq!(manifest.sub_modules[0].0, "sub");
}

#[test]
fn test_lazy_module_registers_nothing() -> Result<(), Box<EvalAltResult>> {
    let module = lazy_global_module();
    assert!(module.has_lazy_functions());
    assert_eq!(module.count().1, 0);
    assert!(module.get_var_value::<INT>("ANSWER").is_some());

    let engine = engine_with(module);

    assert_eq!(engine.eval::<INT>("double(21)")?, 42);
    assert_eq!(engine.eval::<INT>("add(double(1), triple(2))")?, 8);
    assert_eq!(engine.eval::<String>("hello()")?, "hi");
    assert_eq!(engine.eval::<INT>("ANSWER")?, 42);

    Ok(())
}

#[test]
fn test_lazy_overloads() -> Result<(), Box<EvalAltResult>> {
    let engine = engine_with(lazy_global_module());

    assert_eq!(engine.eval::<INT>("area(3)")?, 9);
    assert_eq!(engine.eval::<INT>(r#"area("abcd")"#)?, 4);

    // Exact match is preferred over `Dynamic`
    assert_eq!(engine.eval::<String>("describe(1)")?, "int");
    assert_eq!(engine.eval::<String>("describe(true)")?, "any");

    Ok(())
}

#[test]
fn test_lazy_two_arguments_in_a_loop() -> Result<(), Box<EvalAltResult>> {
    let engine = engine_with(lazy_global_module());

    assert_eq!(engine.eval::<INT>("let t = 0; for i in 0..5 { t = add(t, i); } t")?, 10);

    Ok(())
}

#[test]
#[cfg(not(feature = "no_object"))]
fn test_lazy_methods_getters_operators() -> Result<(), Box<EvalAltResult>> {
    let engine = engine_with(lazy_global_module());

    assert_eq!(engine.eval::<INT>("let x = 4; x.double()")?, 8);
    assert_eq!(engine.eval::<INT>("let w = widget(3); w.size")?, 3);
    assert_eq!(engine.eval::<INT>("(widget(3) + widget(4)).size")?, 7);

    Ok(())
}

#[test]
fn test_lazy_fn_ptr() -> Result<(), Box<EvalAltResult>> {
    let engine = engine_with(lazy_global_module());

    assert_eq!(engine.eval::<INT>(r#"let f = Fn("double"); call(f, 21)"#)?, 42);
    assert_eq!(engine.eval::<INT>(r#"call(curry(Fn("add"), 10), 1)"#)?, 11);
    #[cfg(not(feature = "no_index"))]
    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<rhai::Array>(r#"[1, 2].map(Fn("double"))"#)?.len(), 2);

    Ok(())
}

#[test]
fn test_lazy_genuine_miss() {
    let engine = engine_with(lazy_global_module());

    let err = engine.eval::<INT>("nope(1)").unwrap_err();
    assert!(matches!(*err, EvalAltResult::ErrorFunctionNotFound(..)), "{err:?}");

    let err = engine.eval::<INT>("double(true)").unwrap_err();
    assert!(matches!(*err, EvalAltResult::ErrorFunctionNotFound(..)), "{err:?}");
}

#[test]
#[cfg(not(feature = "no_module"))]
fn test_lazy_static_module() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    engine.register_static_module("kit", Module::from_manifest(exported_manifest!(kit)).into());

    assert_eq!(engine.eval::<INT>("kit::double(21)")?, 42);
    assert_eq!(engine.eval::<INT>("kit::ANSWER")?, 42);
    assert_eq!(engine.eval::<String>("kit::sub::hello()")?, "hi");
    assert_eq!(engine.eval::<INT>(r#"kit::area(3) + kit::area("ab")"#)?, 11);
    assert_eq!(engine.eval::<String>("kit::describe(true)")?, "any");

    // Global functions are available unqualified
    assert_eq!(engine.eval::<INT>("triple(2)")?, 6);

    // Non-global functions are not
    let err = engine.eval::<INT>("double(1)").unwrap_err();
    assert!(matches!(*err, EvalAltResult::ErrorFunctionNotFound(..)), "{err:?}");

    let err = engine.eval::<INT>("kit::nope(1)").unwrap_err();
    assert!(matches!(*err, EvalAltResult::ErrorFunctionNotFound(..)), "{err:?}");

    Ok(())
}

#[test]
#[cfg(not(feature = "no_module"))]
fn test_lazy_imported_module() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    let mut resolver = rhai::module_resolvers::StaticModuleResolver::new();
    resolver.insert("kit", Module::from_manifest(exported_manifest!(kit)));
    engine.set_module_resolver(resolver);

    assert_eq!(engine.eval::<INT>(r#"import "kit" as k; k::double(21)"#)?, 42);
    assert_eq!(engine.eval::<String>(r#"import "kit" as k; k::sub::hello()"#)?, "hi");
    assert_eq!(engine.eval::<INT>(r#"import "kit" as k; triple(2)"#)?, 6);

    Ok(())
}

#[test]
fn test_lazy_standard_library_is_default() {
    let std = StandardPackage::new().as_shared_module();
    assert!(std.has_lazy_functions());

    let mut eager = (*std).clone();
    eager.register_lazy_functions();
    assert!(!eager.has_lazy_functions());
    assert!(eager.count().1 > std.count().1);
}

#[test]
#[cfg(feature = "metadata")]
fn test_lazy_functions_have_metadata() {
    let engine = engine_with(lazy_global_module());
    let signatures = engine.gen_fn_signatures(true);

    assert!(signatures.iter().any(|s| s.starts_with("double(x: ")), "{signatures:?}");
    assert!(signatures.iter().any(|s| s.starts_with("to_upper(")), "{signatures:?}");

    let json = engine.gen_fn_metadata_to_json(true).unwrap();
    assert!(json.contains("\"name\": \"double\""));
    assert!(json.contains("\"name\": \"to_upper\""));
}

#[test]
fn test_lazy_registered_functions_take_precedence() -> Result<(), Box<EvalAltResult>> {
    let mut engine = engine_with(lazy_global_module());
    engine.register_fn("double", |x: INT| x * 100);
    assert_eq!(engine.eval::<INT>("double(2)")?, 200);

    // Also within the same module, before and after registering the functions in manifests
    let mut module = Module::new();
    module.set_native_fn("double", |x: INT| Ok(x * 100));
    module.combine_manifest(exported_manifest!(kit));
    module.build_index();

    let engine = engine_with(module.clone());
    assert_eq!(engine.eval::<INT>("double(2)")?, 200);
    assert_eq!(engine.eval::<INT>("triple(2)")?, 6);

    module.register_lazy_functions();
    module.build_index();
    assert!(!module.has_lazy_functions());

    let engine = engine_with(module);
    assert_eq!(engine.eval::<INT>("double(2)")?, 200);
    assert_eq!(engine.eval::<INT>("triple(2)")?, 6);

    Ok(())
}
