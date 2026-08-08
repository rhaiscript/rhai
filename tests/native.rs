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

/// A native asking rhai for a function it answers by name rather than by
/// dispatch.
///
/// `type_of` and `is_shared` have no registered implementation anywhere, and a
/// call by name for one used to route past the code that implements them —
/// they are reserved names, and being reserved is what makes such a call
/// native-only. So a host could ask for `type_of` and be told there is no such
/// function, while the same question written in a script answered fine.
#[test]
fn test_native_call_fn_raw_reaches_syntactic_functions() {
    let mut engine = Engine::new();

    engine.register_raw_fn("ask_type", [TypeId::of::<Dynamic>()], |context, args| {
        let mut value = args[0].clone();
        context.call_fn_raw("type_of", false, false, &mut [&mut value])
    });

    // A string, because its type name is the same on every build — `only_i32`
    // and `f32_float` rename the numeric ones, and `no_index` removes arrays.
    assert_eq!(engine.eval::<String>(r#"let s = "a"; ask_type(s)"#).unwrap(), "string");

    // And the answer is the script spelling's, whatever the numeric build is.
    assert_eq!(engine.eval::<String>("let x = 1; ask_type(x)").unwrap(), engine.eval::<String>("let x = 1; type_of(x)").unwrap(),);
}
