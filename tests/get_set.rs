#![cfg(not(feature = "no_object"))]
use rhai::{Engine, EvalAltResult, NativeCallContext, Scope, INT};

#[test]
fn test_get_set() {
    #[derive(Clone)]
    struct TestStruct {
        x: INT,
        y: INT,
        array: Vec<INT>,
    }

    impl TestStruct {
        fn get_x(&mut self) -> INT {
            self.x
        }

        fn set_x(&mut self, new_x: INT) {
            self.x = new_x;
        }

        fn get_y(&mut self) -> INT {
            self.y
        }

        fn new() -> Self {
            Self { x: 1, y: 0, array: vec![1, 2, 3, 4, 5] }
        }
    }

    let mut engine = Engine::new();

    engine.register_type::<TestStruct>();

    engine.register_get_set("x", TestStruct::get_x, TestStruct::set_x);
    engine.register_get("y", TestStruct::get_y);
    engine.register_fn("add", |value: &mut INT| *value += 41);
    engine.register_fn("new_ts", TestStruct::new);

    assert_eq!(engine.eval::<INT>("let a = new_ts(); a.x = 500; a.x").unwrap(), 500);
    assert_eq!(engine.eval::<INT>("let a = new_ts(); a.x.add(); a.x").unwrap(), 42);
    assert_eq!(engine.eval::<INT>("let a = new_ts(); a.y.add(); a.y").unwrap(), 0);

    engine.register_indexer_get_set(|value: &mut TestStruct, index: &str| value.array[index.len()], |value: &mut TestStruct, index: &str, new_val: INT| value.array[index.len()] = new_val);

    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<INT>(r#"let a = new_ts(); a["abc"]"#).unwrap(), 4);

    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<INT>(r#"let a = new_ts(); a["abc"] = 42; a["abc"]"#).unwrap(), 42);

    assert_eq!(engine.eval::<INT>(r"let a = new_ts(); a.abc").unwrap(), 4);
    assert_eq!(engine.eval::<INT>(r"let a = new_ts(); a.abc = 42; a.abc").unwrap(), 42);
}

#[test]
fn test_get_set_chain_with_write_back() {
    #[derive(Clone)]
    struct TestChild {
        x: INT,
    }

    impl TestChild {
        fn get_x(&mut self) -> INT {
            self.x
        }

        fn set_x(&mut self, new_x: INT) {
            self.x = new_x;
        }

        fn new() -> TestChild {
            TestChild { x: 1 }
        }
    }

    #[derive(Clone)]
    struct TestParent {
        child: TestChild,
    }

    impl TestParent {
        fn get_child(&mut self) -> TestChild {
            self.child.clone()
        }

        fn set_child(&mut self, new_child: TestChild) {
            self.child = new_child;
        }

        fn new() -> TestParent {
            TestParent { child: TestChild::new() }
        }
    }

    let mut engine = Engine::new();

    engine.register_type::<TestChild>();
    engine.register_type_with_name::<TestParent>("TestParent");

    engine.register_get_set("x", TestChild::get_x, TestChild::set_x);
    engine.register_get_set("child", TestParent::get_child, TestParent::set_child);

    #[cfg(not(feature = "no_index"))]
    engine.register_indexer_get_set(
        |parent: &mut TestParent, _: INT| parent.child.clone(),
        |parent: &mut TestParent, n: INT, mut new_child: TestChild| {
            new_child.x *= n;
            parent.child = new_child;
        },
    );

    engine.register_fn("new_tp", TestParent::new);
    engine.register_fn("new_tc", TestChild::new);

    assert_eq!(engine.eval::<INT>("let a = new_tp(); a.child.x").unwrap(), 1);
    assert_eq!(engine.eval::<INT>("let a = new_tp(); a.child.x = 42; a.child.x").unwrap(), 42);
    assert_eq!(engine.eval::<String>("let a = new_tp(); type_of(a)").unwrap(), "TestParent");
    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<INT>("let a = new_tp(); let c = new_tc(); c.x = 123; a[2] = c; a.child.x").unwrap(), 246);
    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval::<INT>("let a = new_tp(); a[2].x = 42; a.child.x").unwrap(), 84);
}

#[test]
fn test_get_set_op_assignment() {
    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Num(INT);

    impl Num {
        fn get(&mut self) -> INT {
            self.0
        }
        fn set(&mut self, x: INT) {
            self.0 = x;
        }
    }

    let mut engine = Engine::new();

    engine.register_type::<Num>().register_fn("new_ts", || Num(40)).register_get_set("v", Num::get, Num::set);

    assert_eq!(engine.eval::<Num>("let a = new_ts(); a.v = a.v + 2; a").unwrap(), Num(42));
    assert_eq!(engine.eval::<Num>("let a = new_ts(); a.v += 2; a").unwrap(), Num(42));
}

#[test]
fn test_get_set_chain_without_write_back() {
    #[derive(Debug, Clone)]
    struct Outer {
        pub inner: Inner,
    }

    #[derive(Debug, Clone)]
    struct Inner {
        pub value: INT,
    }

    let mut engine = Engine::new();
    let mut scope = Scope::new();

    scope.push("outer", Outer { inner: Inner { value: 42 } });

    engine
        .register_type::<Inner>()
        .register_get_set("value", |t: &mut Inner| t.value, |_: NativeCallContext, _: &mut Inner, new: INT| panic!("Inner::value setter called with {}", new))
        .register_type::<Outer>()
        .register_get_set("inner", |_: NativeCallContext, t: &mut Outer| t.inner.clone(), |_: &mut Outer, new: Inner| panic!("Outer::inner setter called with {:?}", new));

    #[cfg(not(feature = "no_index"))]
    engine.register_indexer_get_set(|t: &mut Outer, n: INT| Inner { value: t.inner.value * n }, |_: &mut Outer, n: INT, new: Inner| panic!("Outer::inner index setter called with {} and {:?}", n, new));

    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "outer.inner.value").unwrap(), 42);

    #[cfg(not(feature = "no_index"))]
    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "outer[2].value").unwrap(), 84);

    engine.run_with_scope(&mut scope, "print(outer.inner.value)").unwrap();

    #[cfg(not(feature = "no_index"))]
    engine.run_with_scope(&mut scope, "print(outer[0].value)").unwrap();
}

#[test]
fn test_get_set_collection() {
    type MyItem = INT;
    type MyBag = std::collections::BTreeSet<MyItem>;

    let mut engine = Engine::new();

    engine
        .register_type_with_name::<MyBag>("MyBag")
        .register_iterator::<MyBag>()
        .register_fn("new_bag", MyBag::new)
        .register_fn("len", |col: &mut MyBag| col.len() as INT)
        .register_get("len", |col: &mut MyBag| col.len() as INT)
        .register_fn("clear", |col: &mut MyBag| col.clear())
        .register_fn("contains", |col: &mut MyBag, item: INT| col.contains(&item))
        .register_fn("add", |col: &mut MyBag, item: MyItem| col.insert(item))
        .register_fn("+=", |col: &mut MyBag, item: MyItem| col.insert(item))
        .register_fn("remove", |col: &mut MyBag, item: MyItem| col.remove(&item))
        .register_fn("-=", |col: &mut MyBag, item: MyItem| col.remove(&item))
        .register_fn("+", |mut col1: MyBag, col2: MyBag| {
            col1.extend(col2.into_iter());
            col1
        });

    let result = engine
        .eval::<INT>(
            "
                let bag = new_bag();

                bag += 1;
                bag += 2;
                bag += 39;
                bag -= 2;

                if !bag.contains(2) {
                    let sum = 0;
                    for n in bag { sum += n; }
                    sum + bag.len
                } else {
                    -1
                }
            ",
        )
        .unwrap();

    assert_eq!(result, 42);
}

#[cfg(not(feature = "no_index"))]
#[test]
fn test_get_set_indexer() {
    type MyMap = std::collections::BTreeMap<String, INT>;

    let mut engine = Engine::new();

    engine
        .register_type_with_name::<MyMap>("MyMap")
        .register_fn("new_map", MyMap::new)
        .register_indexer_get(|map: &mut MyMap, index: &str| -> Result<_, Box<EvalAltResult>> { map.get(index).cloned().ok_or_else(|| EvalAltResult::ErrorIndexNotFound(index.into(), rhai::Position::NONE).into()) })
        .register_indexer_set(|map: &mut MyMap, index: &str, value: INT| {
            map.insert(index.to_string(), value);
        });

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let my_map = new_map();
                    my_map["eggs"] = 42;
                    my_map["eggs"]
                "#,
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let my_map = new_map();
                    my_map["eggs"] = 41;
                    my_map["eggs"] = my_map["eggs"] + 1;
                    my_map["eggs"]
                "#,
            )
            .unwrap(),
        42
    );

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let my_map = new_map();
                    my_map["eggs"] = 41;
                    my_map["eggs"] += 1;
                    my_map["eggs"]
                "#,
            )
            .unwrap(),
        42
    );

    assert!(engine
        .eval::<INT>(
            r#"
                let my_map = new_map();
                my_map["eggs"] = 42;
                my_map["not_found"]
            "#,
        )
        .is_err());

    assert_eq!(
        engine
            .eval::<INT>(
                r#"
                    let my_map = new_map();
                    my_map["eggs"] = 42;

                    try {
                        let eggs = my_map["eggs"];
                        let eggs = my_map["not found"];
                    }
                    catch(x)
                    {
                        print("Not found!");
                    }

                    my_map["eggs"]
                "#,
            )
            .unwrap(),
        42
    );
}

#[test]
fn test_get_set_elvis() {
    let engine = Engine::new();

    engine.eval::<()>("let x = (); x?.foo.bar.baz").unwrap();
    engine.eval::<()>("let x = (); x?.foo(1,2,3)").unwrap();
    engine.eval::<()>("let x = #{a:()}; x.a?.foo.bar.baz").unwrap();
    assert_eq!(engine.eval::<String>("let x = 'x'; x?.type_of()").unwrap(), "char");
}
