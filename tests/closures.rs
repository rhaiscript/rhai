#![cfg(not(feature = "no_function"))]
use rhai::{Dynamic, Engine, EvalAltResult, FnPtr, ParseErrorType, Scope, INT};
use std::any::TypeId;
use std::cell::RefCell;
use std::mem::take;
use std::rc::Rc;

#[cfg(not(feature = "no_object"))]
use rhai::Map;

#[test]
fn test_fn_ptr_curry_call() {
    let mut engine = Engine::new();

    engine.register_raw_fn("call_with_arg", [TypeId::of::<FnPtr>(), TypeId::of::<INT>()], |context, args| {
        let fn_ptr = args[0].take().cast::<FnPtr>();
        fn_ptr.call_raw(&context, None, [args[1].take()])
    });

    #[cfg(not(feature = "no_object"))]
    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let addition = |x, y| { x + y };
                    let curried = addition.curry(2);

                    call_with_arg(curried, 40)
                "
            )
            .unwrap(),
        42
    );
}

#[test]
#[cfg(not(feature = "no_closure"))]
#[cfg(not(feature = "no_object"))]
fn test_closures() {
    let mut engine = Engine::new();
    let mut scope = Scope::new();

    scope.push("x", 42 as INT);

    assert!(matches!(engine.compile_expression("|x| {}").unwrap_err().err_type(), ParseErrorType::BadInput(..)));

    assert_eq!(
        engine
            .eval_with_scope::<INT>(
                &mut scope,
                "
                    let f = || { x };
                    f.call()
                ",
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let foo = #{ x: 42 };
                    let f = || { this.x };
                    foo.call(f)
                ",
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let x = 8;

                    let res = |y, z| {
                        let w = 12;

                        return (|| x + y + z + w).call();
                    }.curry(15).call(2);

                    res + (|| x - 3).call()
                "
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let a = 41;
                    let foo = |x| { a += x };
                    foo.call(1);
                    a
                "
            )
            .unwrap(),
        42
    );

    assert!(engine
        .eval::<bool>(
            "
                let a = 41;
                let foo = |x| { a += x };
                a.is_shared()
            "
        )
        .unwrap());

    assert!(engine
        .eval::<bool>(
            "
                let a = 41;
                let foo = |x| { a += x };
                is_shared(a)
            "
        )
        .unwrap());

    engine.register_fn("plus_one", |x: INT| x + 1);

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let a = 41;
                    let f = || plus_one(a);
                    f.call()
                "
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let a = 40;
                    let f = |x| {
                        let f = |x| {
                            let f = |x| plus_one(a) + x;
                            f.call(x)
                        };
                        f.call(x)
                    };
                    f.call(1)
                "
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let a = 21;
                    let f = |x| a += x;
                    f.call(a);
                    a
                "
            )
            .unwrap(),
        42
    );

    engine.register_raw_fn("custom_call", [TypeId::of::<INT>(), TypeId::of::<FnPtr>()], |context, args| {
        let func = take(args[1]).cast::<FnPtr>();

        func.call_raw(&context, None, [])
    });

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let a = 41;
                    let b = 0;
                    let f = || b.custom_call(|| a + 1);
                    
                    f.call()
                "
            )
            .unwrap(),
        42
    );
}

#[test]
#[cfg(not(feature = "no_closure"))]
fn test_closures_sharing() {
    let mut engine = Engine::new();

    engine.register_fn("foo", |x: INT, s: &str| s.len() as INT + x);
    engine.register_fn("bar", |x: INT, s: String| s.len() as INT + x);

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let s = "hello";
                    let f = || s;
                    foo(1, s)
                "#
            )
            .unwrap(),
        6
    );

    assert_eq!(
        engine
            .eval::<String>(
                r#"
                    let s = "hello";
                    let f = || s;
                    let n = foo(1, s);
                    s
                "#
            )
            .unwrap(),
        "hello"
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let s = "hello";
                    let f = || s;
                    bar(1, s)
                "#
            )
            .unwrap(),
        6
    );

    #[cfg(not(feature = "no_object"))]
    {
        let mut m = Map::new();
        m.insert("hello".into(), "world".into());
        let m = Dynamic::from(m).into_shared();

        engine.register_fn("baz", move || m.clone());

        assert!(!engine
            .eval::<bool>(
                "
                    let m = baz();
                    m.is_shared()
                "
            )
            .unwrap());

        assert_eq!(
            engine
                .eval::<String>(
                    "
                        let m = baz();
                        m.hello
                    "
                )
                .unwrap(),
            "world"
        );

        assert_eq!(engine.eval::<String>("baz().hello").unwrap(), "world");
    }
}

#[test]
#[cfg(not(feature = "no_closure"))]
#[cfg(not(feature = "no_object"))]
#[cfg(not(feature = "sync"))]
fn test_closures_data_race() {
    let engine = Engine::new();

    assert_eq!(
        engine
            .eval::<INT>(
                "
                    let a = 1;
                    let b = 40;
                    let foo = |x| { this += a + x };
                    b.call(foo, 1);
                    b
                "
            )
            .unwrap(),
        42
    );

    assert!(matches!(
        *engine
            .eval::<INT>(
                "
                    let a = 20;
                    let foo = |x| { this += a + x };
                    a.call(foo, 1);
                    a
                "
            )
            .unwrap_err(),
        EvalAltResult::ErrorDataRace(..)
    ));
}

type TestStruct = Rc<RefCell<INT>>;

#[test]
#[cfg(not(feature = "no_object"))]
#[cfg(not(feature = "sync"))]
fn test_closures_shared_obj() {
    let mut engine = Engine::new();

    // Register API on TestStruct
    engine
        .register_type_with_name::<TestStruct>("TestStruct")
        .register_get_set("data", |p: &mut TestStruct| *p.borrow(), |p: &mut TestStruct, value: INT| *p.borrow_mut() = value)
        .register_fn("+=", |p1: &mut TestStruct, p2: TestStruct| *p1.borrow_mut() += *p2.borrow())
        .register_fn("-=", |p1: &mut TestStruct, p2: TestStruct| *p1.borrow_mut() -= *p2.borrow());

    let engine = engine; // Make engine immutable

    let code = r#"
        #{
            name: "A",
            description: "B",
            cost: 1,
            health_added: 0,
            action: |p1, p2| { p1 += p2 }
        }
    "#;

    let ast = engine.compile(code).unwrap();
    let res = engine.eval_ast::<Map>(&ast).unwrap();

    // Make closure
    let f = move |p1: TestStruct, p2: TestStruct| -> Result<(), _> {
        let action_ptr = res["action"].clone_cast::<FnPtr>();
        let name = action_ptr.fn_name();
        engine.call_fn(&mut Scope::new(), &ast, name, (p1, p2))
    };

    // Test closure
    let p1 = Rc::new(RefCell::new(41));
    let p2 = Rc::new(RefCell::new(1));

    f(p1.clone(), p2).unwrap();

    assert_eq!(*p1.borrow(), 42);
}

#[test]
#[cfg(not(feature = "no_closure"))]
fn test_closures_external() {
    let engine = Engine::new();

    let ast = engine
        .compile(
            r#"
                let test = "hello";
                |x| test + x
            "#,
        )
        .unwrap();

    let fn_ptr = engine.eval_ast::<FnPtr>(&ast).unwrap();

    let f = move |x: INT| fn_ptr.call::<String>(&engine, &ast, (x,));

    assert_eq!(f(42).unwrap(), "hello42");
}

#[test]
#[cfg(not(feature = "no_closure"))]
#[cfg(not(feature = "sync"))]
fn test_closures_callback() {
    type SingleNode = Rc<dyn Node>;

    trait Node {
        fn run(&self, x: INT) -> Result<INT, Box<EvalAltResult>>;
    }

    struct PhaserNode {
        func: Box<dyn Fn(INT) -> Result<INT, Box<EvalAltResult>>>,
    }

    impl Node for PhaserNode {
        fn run(&self, x: INT) -> Result<INT, Box<EvalAltResult>> {
            (self.func)(x)
        }
    }

    fn phaser(callback: impl Fn(INT) -> Result<INT, Box<EvalAltResult>> + 'static) -> impl Node {
        PhaserNode { func: Box::new(callback) }
    }

    let mut engine = Engine::new();

    let ast = Rc::new(
        engine
            .compile(
                "
                    const FACTOR = 2;
                    phaser(|x| x * FACTOR)
                ",
            )
            .unwrap(),
    );

    let shared_engine = Rc::new(RefCell::new(Engine::new_raw()));
    let engine2 = shared_engine.clone();
    let ast2 = ast.clone();

    engine.register_fn("phaser", move |fp: FnPtr| {
        let engine = engine2.clone();
        let ast = ast2.clone();

        let callback = Box::new(move |x: INT| fp.call(&engine.borrow(), &ast, (x,)));

        Rc::new(phaser(callback)) as SingleNode
    });

    *shared_engine.borrow_mut() = engine;

    let cb = shared_engine.borrow().eval_ast::<SingleNode>(&ast).unwrap();

    assert_eq!(cb.run(21).unwrap(), 42);
}
