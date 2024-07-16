//! An example showing how to register a simple Rust function.

use rhai::{dynamic_scope, Dynamic, Engine, EvalAltResult};
use rhai::{Scope, INT};

fn add(x: i64, y: i64) -> i64 {
    x + y
}

struct TestStruct {
    x: INT,
    y: INT,
    array: Vec<INT>,
}

impl TestStruct {
    fn new() -> Self {
        Self {
            x: 1,
            y: 833,
            array: vec![1, 2, 3, 4, 5],
        }
    }

    fn get_y(&self) -> INT {
        self.y
    }
}

impl Clone for TestStruct {
    fn clone(&self) -> Self {
        unimplemented!()
    }
}
fn main() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    let mut scope = Scope::new();
    let target = TestStruct::new();

    let result = dynamic_scope(|| {
        scope.set_value("ts", unsafe { Dynamic::from_ref(&target) });
        engine.register_get("y", TestStruct::get_y);

        engine.register_fn("xx1", |_: TestStruct| {});
        engine.register_fn("xx2", |_: &'static TestStruct| {});

        // THIS CAUSE ERROR
        // engine.register_get("xx2", |x: &TestStruct| -> &'static INT { &x.y });

        engine.eval_with_scope::<i64>(
            &mut scope,
            r#"
            ts.y
            // xx1(ts) // <- this is impossible 
            // xx2(ts) // <- this is also impossible
        "#,
        )
    })?;

    drop(target);
    println!("Answer: {result}"); // prints 42

    Ok(())
}
