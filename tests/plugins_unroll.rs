#![cfg(not(feature = "no_index"))]
#![cfg(not(feature = "no_module"))]
use rhai::plugin::*;
use rhai::{Engine, Module, INT};

pub fn add_generic<T: std::ops::Add<Output = T>>(x: T, y: T) -> T {
    x + y
}

pub fn mul_generic<T: std::ops::Mul<Output = T>>(x: T, y: T) -> T {
    x * y
}

macro_rules! generate_ops {
    ($op_name:ident, $op_fn:ident, $($type_names:ident),+) => {
        pub mod $op_name {
            $(
                pub mod $type_names {
                    use rhai::plugin::*;
                    use super::super::$op_fn;
                    #[export_fn]
                    pub fn op(x: $type_names, y: $type_names) -> $type_names {
                        $op_fn(x, y)
                    }
                }
            )*
        }
    }
}

macro_rules! register_in_bulk {
    ($mod_name:ident, $op_name:ident, $($type_names:ident),+) => {
        $(
            {
                let type_str = stringify!($type_names);
                set_exported_fn!($mod_name,
                                 &format!(concat!(stringify!($op_name), "_{}"), type_str),
                                 crate::$op_name::$type_names::op);
            }
        )*
    }
}

generate_ops!(add, add_generic, i8, i16, i32, i64);
generate_ops!(mul, mul_generic, i8, i16, i32, i64);

#[test]
fn test_generated_ops() {
    let mut engine = Engine::new();

    let mut m = Module::new();
    register_in_bulk!(m, add, i8, i16, i32, i64);
    register_in_bulk!(m, mul, i8, i16, i32, i64);

    engine.register_global_module(m.into());

    #[cfg(feature = "only_i32")]
    assert_eq!(engine.eval::<INT>("let a = 0; add_i32(a, 1)").unwrap(), 1);
    #[cfg(not(feature = "only_i32"))]
    assert_eq!(engine.eval::<INT>("let a = 0; add_i64(a, 1)").unwrap(), 1);

    #[cfg(feature = "only_i32")]
    assert_eq!(engine.eval::<INT>("let a = 1; mul_i32(a, 2)").unwrap(), 2);
    #[cfg(not(feature = "only_i32"))]
    assert_eq!(engine.eval::<INT>("let a = 1; mul_i64(a, 2)").unwrap(), 2);
}
