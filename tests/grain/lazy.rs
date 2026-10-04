//! Functions resolved lazily from plugin manifests mean the same from bytecode as from the walker.

use rhai::grain::{Compiler, Vm};
use rhai::plugin::*;
use rhai::{Engine, Scope, INT};

#[derive(Debug, Clone)]
pub struct Widget(INT);

#[export_module(manifest)]
mod kit {
    use super::Widget;
    use rhai::INT;

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
    pub fn add(a: INT, b: INT) -> INT {
        a + b
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
}

fn engine() -> Engine {
    let mut module = Module::new();
    module.combine_manifest(exported_manifest!(kit));

    let mut engine = Engine::new();
    engine.register_type_with_name::<Widget>("Widget");
    engine.register_global_module(module.into());
    #[cfg(not(feature = "no_module"))]
    engine.register_static_module("kit", Module::from_manifest(exported_manifest!(kit)).into());
    engine
}

/// Run `script` through the walker and through the VM, checking that both agree.
fn agree(script: &str, lowers: bool) -> String {
    let engine = engine();
    let expected = format!("{:?}", engine.eval::<rhai::Dynamic>(script));

    let ast = engine.compile(script).expect("must compile");
    let program = Compiler::new().compile(&ast);
    assert_eq!(program.residual_count() == 0, lowers, "{script:?} fragments: {:?}", program.first_unsupported());
    let actual = format!("{:?}", Vm::new(&engine).eval_with_scope(&mut Scope::new(), &program));

    assert_eq!(actual, expected, "{script:?}");
    expected
}

#[test]
fn lazy_functions_resolve() {
    assert_eq!(agree("double(21)", true), "Ok(42)");
    assert_eq!(agree(r#"area(3) + area("ab")"#, true), "Ok(11)");
    assert_eq!(agree("let t = 0; for i in 0..5 { t = add(t, i); } t", true), "Ok(10)");
    assert_eq!(agree(r#"let f = Fn("double"); call(f, 21)"#, true), "Ok(42)");
    assert!(agree("nope(1)", true).contains("ErrorFunctionNotFound"));
}

#[test]
#[cfg(not(feature = "no_object"))]
fn lazy_methods_getters_and_operators_resolve() {
    assert_eq!(agree("let x = 4; x.double()", true), "Ok(8)");
    assert_eq!(agree("let w = widget(3); w.size", true), "Ok(3)");
    assert_eq!(agree("let w = widget(3) + widget(4); w.size", true), "Ok(7)");
}

#[test]
#[cfg(not(feature = "no_module"))]
fn lazy_qualified_calls_resolve() {
    // Qualified calls are left to the walker as fragments
    assert_eq!(agree("kit::double(21)", false), "Ok(42)");
    assert!(agree("kit::nope(21)", false).contains("ErrorFunctionNotFound"));
}
