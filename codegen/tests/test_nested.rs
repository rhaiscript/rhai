use rhai::module_resolvers::*;
use rhai::{Engine, EvalAltResult, RegisterFn, FLOAT, INT};

pub mod one_fn_module_nested_attr {
    use rhai::plugin::*;

    #[export_module]
    pub mod advanced_math {
        use rhai::plugin::*;
        use rhai::FLOAT;

        #[rhai_fn(return_raw)]
        pub fn get_mystic_number() -> Result<Dynamic, Box<EvalAltResult>> {
            Ok(Dynamic::from(42.0 as FLOAT))
        }
    }
}

#[test]
fn one_fn_module_nested_attr_test() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    let m = rhai::exported_module!(crate::one_fn_module_nested_attr::advanced_math);
    let mut r = StaticModuleResolver::new();
    r.insert("Math::Advanced".to_string(), m);
    engine.set_module_resolver(Some(r));

    assert_eq!(
        engine.eval::<FLOAT>(
            r#"import "Math::Advanced" as math;
           let m = math::get_mystic_number();
           m"#
        )?,
        42.0
    );
    Ok(())
}

pub mod one_fn_submodule_nested_attr {
    use rhai::plugin::*;

    #[export_module]
    pub mod advanced_math {
        #[rhai_mod(name = "constants")]
        pub mod my_module {
            use rhai::plugin::*;
            use rhai::FLOAT;
            #[rhai_fn(return_raw)]
            pub fn get_mystic_number() -> Result<Dynamic, Box<EvalAltResult>> {
                Ok(Dynamic::from(42.0 as FLOAT))
            }
        }
    }
}

#[test]
fn one_fn_submodule_nested_attr_test() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    let m = rhai::exported_module!(crate::one_fn_submodule_nested_attr::advanced_math);
    let mut r = StaticModuleResolver::new();
    r.insert("Math::Advanced".to_string(), m);
    engine.set_module_resolver(Some(r));

    assert_eq!(
        engine.eval::<FLOAT>(
            r#"import "Math::Advanced" as math;
           let m = math::constants::get_mystic_number();
           m"#
        )?,
        42.0
    );
    Ok(())
}
