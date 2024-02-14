use rhai::{Dynamic, Engine, EvalAltResult, ImmutableString, NativeCallContext, INT};
use std::any::TypeId;

#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "unchecked"))]
#[test]
fn test_native_context() {
    let mut engine = Engine::new();

    engine.set_max_modules(40);
    engine.register_fn("test", |context: NativeCallContext, x: INT| context.engine().max_modules() as INT + x);

    assert_eq!(engine.eval::<INT>("test(2)").unwrap(), 42);
}

#[test]
fn test_native_context_fn_name() {
    fn add_double(context: NativeCallContext, args: &mut [&mut Dynamic]) -> Result<Dynamic, Box<EvalAltResult>> {
        let x = args[0].as_int().unwrap();
        let y = args[1].as_int().unwrap();
        Ok(format!("{}_{}", context.fn_name(), x + 2 * y).into())
    }

    let mut engine = Engine::new();

    engine
        .register_raw_fn("add_double", [TypeId::of::<INT>(), TypeId::of::<INT>()], add_double)
        .register_raw_fn("append_x2", [TypeId::of::<INT>(), TypeId::of::<INT>()], add_double);

    assert_eq!(engine.eval::<String>("add_double(40, 1)").unwrap(), "add_double_42");

    assert_eq!(engine.eval::<String>("append_x2(40, 1)").unwrap(), "append_x2_42");
}

#[test]
fn test_native_overload() {
    let mut engine = Engine::new();

    assert_eq!(engine.eval::<String>(r#"let x = "hello, "; let y = "world"; x + y"#).unwrap(), "hello, world");
    assert_eq!(engine.eval::<String>(r#"let x = "hello"; let y = (); x + y"#).unwrap(), "hello");

    // Overload the `+` operator for strings

    engine
        .register_fn("+", |s1: ImmutableString, s2: ImmutableString| -> ImmutableString { format!("{s1}***{s2}").into() })
        .register_fn("+", |s1: ImmutableString, _: ()| -> ImmutableString { format!("{s1} Foo!").into() });

    assert_eq!(engine.eval::<String>(r#"let x = "hello"; let y = "world"; x + y"#).unwrap(), "helloworld");
    assert_eq!(engine.eval::<String>(r#"let x = "hello"; let y = (); x + y"#).unwrap(), "hello");

    engine.set_fast_operators(false);

    assert_eq!(engine.eval::<String>(r#"let x = "hello"; let y = "world"; x + y"#).unwrap(), "hello***world");
    assert_eq!(engine.eval::<String>(r#"let x = "hello"; let y = (); x + y"#).unwrap(), "hello Foo!");
}
