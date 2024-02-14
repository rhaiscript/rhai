#![cfg(not(feature = "no_index"))]
#![cfg(not(feature = "no_module"))]
use rhai::plugin::*;
use rhai::{Engine, Scope, INT};

mod test {
    use super::*;

    #[export_module]
    pub mod special_array_package {
        use rhai::{Array, INT};

        pub const MYSTIC_NUMBER: INT = 42;

        #[cfg(not(feature = "no_object"))]
        pub mod feature {
            use rhai::{Array, Dynamic, EvalAltResult};

            #[rhai_fn(get = "foo", return_raw)]
            #[inline(always)]
            pub fn foo(array: &mut Array) -> Result<Dynamic, Box<EvalAltResult>> {
                Ok(array[0].clone())
            }
        }

        pub fn hash(_text: String) -> INT {
            42
        }
        pub fn hash2(_text: &str) -> INT {
            42
        }

        #[rhai_fn(name = "test", name = "hi")]
        pub fn len(array: &mut Array, mul: INT) -> INT {
            (array.len() as INT) * mul
        }
        #[rhai_fn(name = "+")]
        pub fn funky_add(x: INT, y: INT) -> INT {
            x / 2 + y * 2
        }
        #[rhai_fn(name = "no_effect", set = "no_effect", pure)]
        pub fn no_effect(array: &mut Array, value: INT) {
            // array is not modified
            println!("Array = {array:?}, Value = {value}");
        }
    }
}

macro_rules! gen_unary_functions {
    ($op_name:ident = $op_fn:ident ( $($arg_type:ident),+ ) -> $return_type:ident) => {
        mod $op_name { $(
            #[allow(non_snake_case)]
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

fn make_greeting(n: impl std::fmt::Display) -> String {
    format!("{n} kitties")
}

gen_unary_functions!(greet = make_greeting(INT, bool, char) -> String);

macro_rules! expand_enum {
    ($module:ident : $typ:ty => $($variant:ident),+) => {
        #[export_module]
        pub mod $module {
            $(
                #[allow(non_upper_case_globals)]
                pub const $variant: $typ = <$typ>::$variant;
            )*
        }
    };
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum MyEnum {
    Foo,
    Bar,
    Baz,
    Hello,
    World,
}

expand_enum! { my_enum_module: MyEnum => Foo, Bar, Baz, Hello, World }

#[test]
fn test_plugins_package() {
    let mut engine = Engine::new();

    let mut m = Module::new();
    combine_with_exported_module!(&mut m, "test", test::special_array_package);
    combine_with_exported_module!(&mut m, "enum", my_enum_module);
    engine.register_global_module(m.into());

    reg_functions!(engine += greet::single(INT, bool, char));

    assert_eq!(engine.eval::<INT>("MYSTIC_NUMBER").unwrap(), 42);

    #[cfg(not(feature = "no_object"))]
    {
        assert_eq!(engine.eval::<INT>("let a = [1, 2, 3]; a.foo").unwrap(), 1);
        engine.run("const A = [1, 2, 3]; A.no_effect(42);").unwrap();
        engine.run("const A = [1, 2, 3]; A.no_effect = 42;").unwrap();

        assert!(matches!(
            *engine.run("const A = [1, 2, 3]; A.test(42);").unwrap_err(),
            EvalAltResult::ErrorNonPureMethodCallOnConstant(x, ..) if x == "test"));
    }

    assert_eq!(engine.eval::<INT>(r#"hash("hello")"#).unwrap(), 42);
    assert_eq!(engine.eval::<INT>(r#"hash2("hello")"#).unwrap(), 42);
    assert_eq!(engine.eval::<INT>("let a = [1, 2, 3]; test(a, 2)").unwrap(), 6);
    assert_eq!(engine.eval::<INT>("let a = [1, 2, 3]; hi(a, 2)").unwrap(), 6);
    assert_eq!(engine.eval::<INT>("let a = [1, 2, 3]; test(a, 2)").unwrap(), 6);
    assert_eq!(engine.eval::<String>("let a = [1, 2, 3]; greet(test(a, 2))").unwrap(), "6 kitties");
    assert_eq!(engine.eval::<INT>("2 + 2").unwrap(), 4);

    engine.set_fast_operators(false);
    assert_eq!(engine.eval::<INT>("2 + 2").unwrap(), 5);

    engine.register_static_module("test", exported_module!(test::special_array_package).into());

    assert_eq!(engine.eval::<INT>("test::MYSTIC_NUMBER").unwrap(), 42);
}

#[test]
fn test_plugins_parameters() {
    #[export_module]
    mod rhai_std {
        pub fn noop(_: &str) {}
    }

    let mut engine = Engine::new();

    let std = exported_module!(rhai_std);

    engine.register_static_module("std", std.into());

    assert_eq!(
        engine
            .eval::<String>(
                r#"
                    let s = "hello";
                    std::noop(s);
                    s
                "#
            )
            .unwrap(),
        "hello"
    );
}

#[cfg(target_pointer_width = "64")]
mod handle {
    use super::*;

    #[derive(Debug, Eq, PartialEq, Clone, Copy, Hash)]
    pub struct WorldHandle(usize);
    pub type World = Vec<i64>;

    impl From<&mut World> for WorldHandle {
        fn from(world: &mut World) -> Self {
            Self::new(world)
        }
    }

    impl AsMut<World> for WorldHandle {
        fn as_mut(&mut self) -> &mut World {
            unsafe { std::mem::transmute(self.0) }
        }
    }

    impl WorldHandle {
        pub fn new(world: &mut World) -> Self {
            Self(unsafe { std::mem::transmute(world) })
        }
    }

    #[export_module]
    pub mod handle_module {
        pub type Handle = WorldHandle;

        #[rhai_fn(get = "len")]
        pub fn len(world: &mut Handle) -> INT {
            world.as_mut().len() as INT
        }
    }

    #[test]
    fn test_module_handle() {
        let mut engine = Engine::new();

        engine.register_global_module(exported_module!(handle_module).into());

        let mut scope = Scope::new();

        let world: &mut World = &mut vec![42];
        scope.push("world", WorldHandle::from(world));

        #[cfg(not(feature = "no_object"))]
        assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "world.len").unwrap(), 1);
    }
}
