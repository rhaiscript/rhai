//! Functions loaded on demand resolve the same way from bytecode as from the walker.

use rhai::grain::{Compiler, Vm};
use rhai::plugin::*;
use rhai::{Engine, EvalAltResult, FnLoadRequest, LazyFunctionSource, Scope, INT};
use std::sync::{Arc, Mutex};

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
    pub fn unused() -> INT {
        0
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

struct Recording<S> {
    inner: S,
    loads: Arc<Mutex<Vec<String>>>,
}

impl<S: LazyFunctionSource> LazyFunctionSource for Recording<S> {
    fn load(&self, engine: &Engine, request: &FnLoadRequest) -> Result<Option<Module>, Box<EvalAltResult>> {
        let module = self.inner.load(engine, request)?;
        if module.is_some() {
            self.loads.lock().unwrap().push(request.name.to_string());
        }
        Ok(module)
    }
    fn load_var(&self, engine: &Engine, path: &[&str], name: &str) -> Result<Option<Dynamic>, Box<EvalAltResult>> {
        self.inner.load_var(engine, path, name)
    }
    fn has_script_fn(&self, name: &str, num_params: usize, this_type: Option<&str>, global_only: bool) -> bool {
        self.inner.has_script_fn(name, num_params, this_type, global_only)
    }
}

fn engine(source: impl LazyFunctionSource + 'static, namespace: Option<&str>) -> (Engine, Arc<Mutex<Vec<String>>>) {
    let mut engine = Engine::new();
    engine.register_type_with_name::<Widget>("Widget");
    let loads = Arc::new(Mutex::new(Vec::new()));
    engine.register_lazy_source(namespace, Recording { inner: source, loads: loads.clone() });
    (engine, loads)
}

/// Run `script` on a fresh engine through the walker and through the VM, checking that both
/// return the same result and load the same functions.
fn agree(make: impl Fn() -> (Engine, Arc<Mutex<Vec<String>>>), script: &str, lowers: bool) -> String {
    let (walker, walker_loads) = make();
    let expected = format!("{:?}", walker.eval::<Dynamic>(script));

    let (vm_engine, vm_loads) = make();
    let ast = vm_engine.compile(script).expect("must compile");
    let program = Compiler::new().compile(&ast);
    assert_eq!(program.residual_count() == 0, lowers, "{script:?} fragments: {:?}", program.first_unsupported());
    let actual = format!("{:?}", Vm::new(&vm_engine).eval_with_scope(&mut Scope::new(), &program));

    assert_eq!(actual, expected, "{script:?}");
    let walker_loads = walker_loads.lock().unwrap().clone();
    assert_eq!(*vm_loads.lock().unwrap(), walker_loads, "{script:?}");
    walker_loads.join(",")
}

fn manifest_engine() -> (Engine, Arc<Mutex<Vec<String>>>) {
    engine(exported_manifest!(kit), None)
}

#[test]
fn only_called_functions_are_loaded() {
    assert_eq!(agree(manifest_engine, "double(21)", true), "double");
    assert_eq!(agree(manifest_engine, "double(1) + double(2) + double(3)", true), "double");
    assert_eq!(agree(manifest_engine, r#"area(3) + area("ab")"#, true), "area,area");
    assert_eq!(agree(manifest_engine, "let t = 0; for i in 0..5 { t = add(t, i); } t", true), "add");
    assert_eq!(agree(manifest_engine, "nope(1)", true), "");
}

#[test]
#[cfg(not(feature = "no_object"))]
fn methods_getters_and_operators_are_loaded() {
    assert_eq!(agree(manifest_engine, "let x = 4; x.double()", true), "double");
    assert_eq!(agree(manifest_engine, "let w = widget(3); w.size", true), "widget,get$size");
    assert_eq!(agree(manifest_engine, "let w = widget(3) + widget(4); w.size", true), "widget,+,get$size");
}

#[test]
#[cfg(not(feature = "no_module"))]
fn qualified_calls_are_loaded() {
    let make = || engine(exported_manifest!(kit), Some("kit"));
    // Qualified calls are left to the walker as fragments
    assert_eq!(agree(make, "kit::double(21)", false), "double");
    assert_eq!(agree(make, "kit::nope(21)", false), "");
}

#[test]
#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "no_function"))]
fn script_library_functions_are_loaded_with_their_environment() {
    let make = || {
        let compiler = Engine::new();
        let ast = compiler
            .compile("const K = 10; fn a(x) { b(x) + global::K } private fn b(x) { x * 2 } fn unused() { 0 }")
            .expect("library must compile");
        engine(rhai::ScriptLibrary::new(ast), None)
    };
    assert_eq!(agree(make, "a(1) + a(2)", true), "a");
    assert_eq!(agree(make, "b(1)", true), "");
    assert_eq!(agree(make, r#"is_def_fn("a", 1)"#, true), "");
    assert_eq!(agree(make, r#"is_def_fn("b", 1)"#, true), "");
    assert_eq!(agree(make, r#"if is_def_fn("a", 1) { a(1) } else { 0 }"#, true), "a");
    assert_eq!(agree(make, r#"let f = Fn("a"); call(f, 1) + call(f, 2)"#, true), "a");
}

#[test]
fn fn_ptrs_load_when_called() {
    assert_eq!(agree(manifest_engine, r#"let f = Fn("double"); call(f, 21)"#, true), "double");
    assert_eq!(agree(manifest_engine, r#"call(curry(Fn("add"), 1), 41)"#, true), "add");
    #[cfg(not(feature = "no_index"))]
    #[cfg(not(feature = "no_object"))]
    assert_eq!(agree(manifest_engine, r#"[1, 2, 3].map(Fn("double"))"#, true), "double");
}
