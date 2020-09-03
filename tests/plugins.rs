#![cfg(not(any(feature = "no_index", feature = "no_module")))]

use rhai::plugin::*;
use rhai::{Engine, EvalAltResult, INT};

mod test {
    use rhai::plugin::*;

    #[export_module]
    pub mod special_array_package {
        use rhai::{Array, INT};

        #[cfg(not(feature = "no_object"))]
        pub mod feature {
            #[rhai_fn(get = "foo", return_raw)]
            #[inline(always)]
            pub fn foo(array: &mut Array) -> Result<Dynamic, Box<EvalAltResult>> {
                Ok(array[0].clone())
            }
        }

        #[rhai_fn(name = "test")]
        #[inline(always)]
        pub fn len(array: &mut Array, mul: INT) -> INT {
            (array.len() as INT) * mul
        }
        #[rhai_fn(name = "+")]
        #[inline(always)]
        pub fn funky_add(x: INT, y: INT) -> INT {
            x / 2 + y * 2
        }
    }
}

macro_rules! gen_unary_functions {
    ($op_name:ident = $op_fn:ident ( $($arg_type:ident),+ ) -> $return_type:ident) => {
        mod $op_name { $(
            pub mod $arg_type {
                use super::super::*;

                #[export_fn(name="test")]
                pub fn single(x: $arg_type) -> $return_type {
                    super::super::$op_fn(x)
                }
            }
        )* }
    }
}

macro_rules! reg_functions {
    ($mod_name:ident += $op_name:ident :: $func:ident ( $($arg_type:ident),+ )) => {
        $(register_exported_fn!($mod_name, stringify!($op_name), $op_name::$arg_type::$func);)*
    }
}

fn make_greeting<T: std::fmt::Display>(n: T) -> String {
    format!("{} kitties", n)
}

gen_unary_functions!(greet = make_greeting(INT, bool, char) -> String);

#[test]
fn test_plugins_package() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();

    let mut m = Module::new();
    m.combine_flatten(exported_module!(test::special_array_package));
    engine.load_package(m);

    reg_functions!(engine += greet::single(INT, bool, char));

    #[cfg(not(feature = "no_object"))]
    assert_eq!(engine.eval::<INT>("let a = [1, 2, 3]; a.foo")?, 1);

    assert_eq!(engine.eval::<INT>("let a = [1, 2, 3]; test(a, 2)")?, 6);
    assert_eq!(engine.eval::<INT>("2 + 2")?, 5);
    assert_eq!(
        engine.eval::<String>("let a = [1, 2, 3]; greet(test(a, 2))")?,
        "6 kitties"
    );

    Ok(())
}
