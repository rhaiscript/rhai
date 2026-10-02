use rhai::plugin::*;
use rhai::{Engine, EvalAltResult, FnLoadRequest, FnPtr, LazyFunctionSource, INT};
use std::sync::{Arc, Mutex};

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

    pub mod sub {
        pub fn hello() -> String {
            "hi".into()
        }
    }
}

/// Wraps a source and records every function it loads.
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

fn recording_engine(namespace: Option<&str>) -> (Engine, Arc<Mutex<Vec<String>>>) {
    let mut engine = Engine::new();
    let loads = Arc::new(Mutex::new(Vec::new()));
    engine.register_lazy_source(namespace, Recording { inner: exported_manifest!(kit), loads: loads.clone() });
    (engine, loads)
}

fn take(loads: &Mutex<Vec<String>>) -> Vec<String> {
    std::mem::take(&mut *loads.lock().unwrap())
}

#[test]
fn test_lazy_manifest_is_sorted() {
    let manifest = exported_manifest!(kit);
    let names: Vec<_> = manifest.functions.iter().map(|f| f.name).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert!(names.contains(&"get$size"));
    assert_eq!(names.iter().filter(|&&n| n == "area").count(), 2);
    assert_eq!(manifest.sub_modules.len(), 1);
    assert_eq!(manifest.sub_modules[0].0, "sub");
}

#[test]
fn test_lazy_loads_only_called_functions() -> Result<(), Box<EvalAltResult>> {
    let (engine, loads) = recording_engine(None);

    assert_eq!(engine.eval::<INT>("double(21)")?, 42);
    assert_eq!(take(&loads), ["double"]);

    // Already loaded - in this evaluation and the next
    assert_eq!(engine.eval::<INT>("double(1) + double(2)")?, 6);
    assert_eq!(take(&loads), Vec::<String>::new());

    // Reload after clearing
    engine.clear_loaded_functions();
    assert_eq!(engine.eval::<INT>("double(5)")?, 10);
    assert_eq!(take(&loads), ["double"]);

    Ok(())
}

#[test]
fn test_lazy_overloads() -> Result<(), Box<EvalAltResult>> {
    let (engine, loads) = recording_engine(None);

    assert_eq!(engine.eval::<INT>("area(3)")?, 9);
    assert_eq!(take(&loads), ["area"]);
    assert_eq!(engine.eval::<INT>(r#"area("abcd")"#)?, 4);
    assert_eq!(take(&loads), ["area"]);

    // Exact match is preferred over `Dynamic`
    assert_eq!(engine.eval::<String>("describe(1)")?, "int");
    assert_eq!(engine.eval::<String>("describe(true)")?, "any");
    assert_eq!(take(&loads), ["describe", "describe"]);

    Ok(())
}

#[test]
fn test_lazy_negative_cache() -> Result<(), Box<EvalAltResult>> {
    let (engine, loads) = recording_engine(None);

    // Two-argument misses are cached; a later load must not be hidden
    assert_eq!(engine.eval::<INT>("let t = 0; for i in 0..5 { t = add(t, i); } t")?, 10);
    assert_eq!(take(&loads), ["add"]);

    #[cfg(not(feature = "no_function"))]
    assert_eq!(engine.eval::<INT>(r#"is_def_fn("triple", 1); triple(2)"#)?, 6);

    Ok(())
}

#[test]
#[cfg(not(feature = "no_object"))]
fn test_lazy_methods_getters_operators() -> Result<(), Box<EvalAltResult>> {
    let (mut engine, loads) = recording_engine(None);
    engine.register_type_with_name::<Widget>("Widget");

    assert_eq!(engine.eval::<INT>("let x = 4; x.double()")?, 8);
    assert_eq!(take(&loads), ["double"]);

    assert_eq!(engine.eval::<INT>("let w = widget(3); w.size")?, 3);
    assert_eq!(take(&loads), ["widget", "get$size"]);

    assert_eq!(engine.eval::<INT>("(widget(3) + widget(4)).size")?, 7);
    assert_eq!(take(&loads), ["+"]);

    Ok(())
}

#[test]
fn test_lazy_fn_ptr() -> Result<(), Box<EvalAltResult>> {
    let (engine, loads) = recording_engine(None);

    engine.eval::<FnPtr>(r#"Fn("double")"#)?;
    assert!(take(&loads).is_empty());

    assert_eq!(engine.eval::<INT>(r#"let f = Fn("double"); call(f, 21)"#)?, 42);
    assert_eq!(engine.eval::<INT>(r#"call(Fn("triple"), 2)"#)?, 6);
    #[cfg(not(feature = "no_index"))]
    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<rhai::Array>(r#"[1, 2].map(Fn("add").curry(10))"#)?.len(), 2);
    #[cfg(any(feature = "no_index", feature = "no_object"))]
    assert_eq!(engine.eval::<INT>(r#"call(curry(Fn("add"), 10), 1)"#)?, 11);
    assert_eq!(take(&loads), ["double", "triple", "add"]);

    Ok(())
}

#[test]
fn test_lazy_genuine_miss() {
    let (engine, loads) = recording_engine(None);

    let err = engine.eval::<INT>("nope(1)").unwrap_err();
    assert!(matches!(*err, EvalAltResult::ErrorFunctionNotFound(..)), "{err:?}");

    // Wrong types do not load anything
    let err = engine.eval::<INT>("double(true)").unwrap_err();
    assert!(matches!(*err, EvalAltResult::ErrorFunctionNotFound(..)), "{err:?}");

    assert!(take(&loads).is_empty());
}

#[test]
#[cfg(not(feature = "no_module"))]
fn test_lazy_static_module() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    engine.register_lazy_static_module("kit", exported_manifest!(kit));

    assert_eq!(engine.eval::<INT>("kit::double(21)")?, 42);
    assert_eq!(engine.eval::<INT>("kit::ANSWER")?, 42);
    assert_eq!(engine.eval::<String>("kit::sub::hello()")?, "hi");
    assert_eq!(engine.eval::<INT>("kit::area(3) + kit::area(\"ab\")")?, 11);

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
fn test_lazy_namespace_without_module() -> Result<(), Box<EvalAltResult>> {
    let (engine, loads) = recording_engine(Some("lazy"));

    assert_eq!(engine.eval::<INT>("lazy::double(21)")?, 42);
    assert_eq!(engine.eval::<String>("lazy::sub::hello()")?, "hi");
    assert_eq!(take(&loads), ["double", "hello"]);

    let err = engine.eval::<INT>("other::double(1)").unwrap_err();
    assert!(matches!(*err, EvalAltResult::ErrorModuleNotFound(..)), "{err:?}");

    Ok(())
}

#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_module"))]
mod script_library {
    use super::*;
    use rhai::module_resolvers::StaticModuleResolver;
    use rhai::ScriptLibrary;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const LIBRARY: &str = r#"
        import "helpers" as h;

        const K = 10;

        tick();

        fn a(x) { b(x) + global::K + h::one() }
        private fn b(x) { x * 2 }
        fn unused() { 0 }

        export const EXPORTED = 99;
    "#;

    fn engine_with_library(namespace: Option<&str>) -> (Engine, Arc<Mutex<Vec<String>>>, Arc<AtomicUsize>) {
        let mut engine = Engine::new();

        let ticks = Arc::new(AtomicUsize::new(0));
        let t = ticks.clone();
        engine.register_fn("tick", move || {
            t.fetch_add(1, Ordering::SeqCst);
        });

        let mut helpers = Module::new();
        helpers.set_native_fn("one", || Ok(1 as INT));
        let mut resolver = StaticModuleResolver::new();
        resolver.insert("helpers", helpers);
        engine.set_module_resolver(resolver);

        let ast = engine.compile(LIBRARY).unwrap();
        let loads = Arc::new(Mutex::new(Vec::new()));
        engine.register_lazy_source(namespace, Recording { inner: ScriptLibrary::new(ast), loads: loads.clone() });

        (engine, loads, ticks)
    }

    #[test]
    fn test_lazy_script_library() -> Result<(), Box<EvalAltResult>> {
        let (engine, loads, ticks) = engine_with_library(None);

        assert_eq!(ticks.load(Ordering::SeqCst), 0);

        // `b`, `K` and `h` come from the library's environment
        assert_eq!(engine.eval::<INT>("a(1)")?, 13);
        assert_eq!(take(&loads), ["a"]);
        assert_eq!(ticks.load(Ordering::SeqCst), 1);

        assert_eq!(engine.eval::<INT>("a(2) + a(3)")?, 15 + 17);
        assert!(take(&loads).is_empty());
        assert_eq!(ticks.load(Ordering::SeqCst), 1);

        // Private functions are not exposed
        let err = engine.eval::<INT>("b(1)").unwrap_err();
        assert!(matches!(*err, EvalAltResult::ErrorFunctionNotFound(..)), "{err:?}");

        Ok(())
    }

    #[test]
    fn test_lazy_script_library_namespace() -> Result<(), Box<EvalAltResult>> {
        let (engine, loads, ticks) = engine_with_library(Some("lib"));

        assert_eq!(engine.eval::<INT>("lib::EXPORTED")?, 99);
        assert_eq!(ticks.load(Ordering::SeqCst), 1);

        assert_eq!(engine.eval::<INT>("lib::a(1)")?, 13);
        assert_eq!(take(&loads), ["a"]);
        assert_eq!(ticks.load(Ordering::SeqCst), 1);

        let err = engine.eval::<INT>("lib::NOPE").unwrap_err();
        assert!(matches!(*err, EvalAltResult::ErrorVariableNotFound(..)), "{err:?}");

        Ok(())
    }

    #[test]
    fn test_lazy_script_library_is_def_fn() -> Result<(), Box<EvalAltResult>> {
        #[cfg(not(feature = "no_object"))]
        let lib = "fn a(x) { x + 1 } private fn b(x) { x } fn Widget.m() { 1 }";
        #[cfg(feature = "no_object")]
        let lib = "fn a(x) { x + 1 } private fn b(x) { x }";

        let mut engine = Engine::new();
        let loads = Arc::new(Mutex::new(Vec::new()));
        let source = Recording {
            inner: ScriptLibrary::new(engine.compile(lib)?),
            loads: loads.clone(),
        };
        engine.register_lazy_source(None, source);

        // Answered without loading anything
        assert!(engine.eval::<bool>(r#"is_def_fn("a", 1)"#)?);
        assert!(!engine.eval::<bool>(r#"is_def_fn("a", 2)"#)?);
        assert!(!engine.eval::<bool>(r#"is_def_fn("b", 1)"#)?);
        #[cfg(not(feature = "no_object"))]
        assert!(engine.eval::<bool>(r#"is_def_fn("Widget", "m", 0)"#)?);
        #[cfg(not(feature = "no_object"))]
        assert!(!engine.eval::<bool>(r#"is_def_fn("m", 0)"#)?);
        assert!(take(&loads).is_empty());

        // Native functions are not script-defined
        let (engine2, _) = recording_engine(None);
        assert!(!engine2.eval::<bool>(r#"is_def_fn("double", 1)"#)?);

        // The function is still callable afterwards
        assert_eq!(engine.eval::<INT>(r#"if is_def_fn("a", 1) { a(1) } else { 0 }"#)?, 2);
        assert_eq!(take(&loads), ["a"]);

        // In a namespace, only methods bound to a type are global
        let mut engine = Engine::new();
        engine.register_lazy_source(Some("lib"), ScriptLibrary::new(engine.compile(lib)?));
        assert!(!engine.eval::<bool>(r#"is_def_fn("a", 1)"#)?);
        #[cfg(not(feature = "no_object"))]
        assert!(engine.eval::<bool>(r#"is_def_fn("Widget", "m", 0)"#)?);

        Ok(())
    }

    #[test]
    fn test_lazy_script_library_fn_ptr() -> Result<(), Box<EvalAltResult>> {
        let mut engine = Engine::new();
        let loads = Arc::new(Mutex::new(Vec::new()));
        let source = Recording {
            inner: ScriptLibrary::new(engine.compile("fn a(x) { x + 1 }")?),
            loads: loads.clone(),
        };
        engine.register_lazy_source(None, source);

        // Making a function pointer loads nothing; calling it does
        engine.eval::<FnPtr>(r#"Fn("a")"#)?;
        assert!(take(&loads).is_empty());

        assert_eq!(engine.eval::<INT>(r#"let f = Fn("a"); call(f, 1)"#)?, 2);
        assert_eq!(engine.eval::<INT>(r#"call(Fn("a"), 2)"#)?, 3);
        #[cfg(not(feature = "no_index"))]
        #[cfg(not(feature = "no_object"))]
        assert_eq!(engine.eval::<rhai::Array>(r#"[1, 2].map(Fn("a"))"#)?.len(), 2);
        assert_eq!(take(&loads), ["a"]);

        Ok(())
    }

    #[test]
    fn test_lazy_script_library_without_body() -> Result<(), Box<EvalAltResult>> {
        let mut engine = Engine::new();
        let ast = engine.compile("fn a(x) { b(x) + 1 } private fn b(x) { x * 2 }")?;
        engine.register_lazy_source(None, ScriptLibrary::new(ast));

        assert_eq!(engine.eval::<INT>("a(1)")?, 3);

        Ok(())
    }
}
