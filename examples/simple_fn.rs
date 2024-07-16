//! An example showing how to register a simple Rust function.

use std::ops::Deref;

use rhai::{Dynamic, Engine, EvalAltResult};
use rhai::{Scope, INT};

fn add(x: i64, y: i64) -> i64 {
    x + y
}

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
        Self {
            x: 1,
            y: 123,
            array: vec![1, 2, 3, 4, 5],
        }
    }
}

#[derive(Clone)]
struct UnsafeShared<T> {
    ptr: *const T,
}

impl<T: 'static> Deref for UnsafeShared<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

/// as
#[repr(transparent)]
pub struct RhaiRef<T> {
    value: T,
}

impl<T> Clone for RhaiRef<T> {
    fn clone(&self) -> Self {
        unimplemented!()
    }
}

unsafe impl<T> Send for RhaiRef<T> {}
unsafe impl<T> Sync for RhaiRef<T> {}

impl<T> Deref for RhaiRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl RhaiRef<TestStruct> {
    fn get_y(&self) -> INT {
        self.y
    }
}

fn main() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    let mut scope = Scope::new();
    let target = TestStruct::new();

    scope.set_value("ts", unsafe { Dynamic::from_ref(&target) });
    engine.register_get("y", RhaiRef::<TestStruct>::get_y);

    println!("{}", std::any::type_name::<&RhaiRef<TestStruct>>());

    let result = engine.eval_with_scope::<i64>(
        &mut scope,
        r#"
            ts.y()
        "#,
    )?;

    drop(target);
    println!("Answer: {result}"); // prints 42

    Ok(())
}
