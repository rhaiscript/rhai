use rhai::module_resolvers::*;
use rhai::{Array, Engine, EvalAltResult, RegisterFn, FLOAT, INT};

pub mod empty_module {
    use rhai::plugin::*;

    #[export_module]
    pub mod EmptyModule {}
}

#[test]
fn empty_module_test() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    let m = rhai::exported_module!(crate::empty_module::EmptyModule);
    let mut r = StaticModuleResolver::new();
    r.insert("Module::Empty".to_string(), m);
    engine.set_module_resolver(Some(r));

    assert_eq!(
        engine.eval::<INT>(r#"import "Module::Empty" as m; 42"#)?,
        42
    );
    Ok(())
}

pub mod one_fn_module {
    use rhai::plugin::*;

    #[export_module]
    pub mod advanced_math {
        use rhai::FLOAT;
        pub fn get_mystic_number() -> FLOAT {
            42.0 as FLOAT
        }
    }
}

#[test]
fn one_fn_module_test() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    let m = rhai::exported_module!(crate::one_fn_module::advanced_math);
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

pub mod one_fn_and_const_module {
    use rhai::plugin::*;

    #[export_module]
    pub mod advanced_math {
        use rhai::FLOAT;

        pub const MYSTIC_NUMBER: FLOAT = 42.0 as FLOAT;

        pub fn euclidean_distance(x1: FLOAT, y1: FLOAT, x2: FLOAT, y2: FLOAT) -> FLOAT {
            ((y2 - y1).abs().powf(2.0) + (x2 - x1).abs().powf(2.0)).sqrt()
        }
    }
}

#[test]
fn one_fn_and_const_module_test() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    let m = rhai::exported_module!(crate::one_fn_and_const_module::advanced_math);
    let mut r = StaticModuleResolver::new();
    r.insert("Math::Advanced".to_string(), m);
    engine.set_module_resolver(Some(r));

    assert_eq!(
        engine.eval::<FLOAT>(
            r#"import "Math::Advanced" as math;
           let m = math::MYSTIC_NUMBER;
           let x = math::euclidean_distance(0.0, 1.0, 0.0, m);
           x"#
        )?,
        41.0
    );
    Ok(())
}

pub mod raw_fn_str_module {
    use rhai::plugin::*;

    #[export_module]
    pub mod host_io {
        pub fn write_out_str(message: &str) -> bool {
            eprintln!("{}", message);
            true
        }
    }
}

#[test]
fn raw_fn_str_module_test() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    let m = rhai::exported_module!(crate::raw_fn_str_module::host_io);
    let mut r = StaticModuleResolver::new();
    r.insert("Host::IO".to_string(), m);
    engine.set_module_resolver(Some(r));

    assert_eq!(
        engine.eval::<bool>(
            r#"import "Host::IO" as io;
           let x = io::write_out_str("hello world!");
           x"#
        )?,
        true
    );
    Ok(())
}

pub mod mut_opaque_ref_module {
    use rhai::plugin::*;
    use rhai::INT;

    #[derive(Clone)]
    pub struct StatusMessage {
        os_code: Option<INT>,
        message: String,
        is_ok: bool,
    }

    #[export_module]
    pub mod host_msg {
        use super::{StatusMessage, INT};

        pub fn new_message(is_ok: bool, message: &str) -> StatusMessage {
            StatusMessage {
                is_ok,
                os_code: None,
                message: message.to_string(),
            }
        }

        pub fn new_os_message(is_ok: bool, os_code: INT) -> StatusMessage {
            StatusMessage {
                is_ok,
                os_code: Some(os_code),
                message: format!("OS Code {}", os_code),
            }
        }

        pub fn write_out_message(message: &mut StatusMessage) -> bool {
            eprintln!("{}", message.message);
            true
        }
    }
}

#[test]
fn mut_opaque_ref_test() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    let m = rhai::exported_module!(crate::mut_opaque_ref_module::host_msg);
    let mut r = StaticModuleResolver::new();
    r.insert("Host::Msg".to_string(), m);
    engine.set_module_resolver(Some(r));

    assert_eq!(
        engine.eval::<bool>(
            r#"import "Host::Msg" as msg;
           let success = "it worked";
           let message1 = msg::new_message(true, success);
           let ok1 = msg::write_out_message(message1);
           let message2 = msg::new_os_message(true, 0);
           let ok2 = msg::write_out_message(message2);
           ok1 && ok2"#
        )?,
        true
    );
    Ok(())
}

mod duplicate_fn_rename {
    use rhai::plugin::*;
    #[export_module]
    pub mod my_adds {
        use rhai::{FLOAT, INT};

        #[rhai_fn(name = "add")]
        pub fn add_float(f1: FLOAT, f2: FLOAT) -> FLOAT {
            f1 + f2
        }

        #[rhai_fn(name = "add")]
        pub fn add_int(i1: INT, i2: INT) -> INT {
            i1 + i2
        }
    }
}

#[test]
fn duplicate_fn_rename_test() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    engine.register_fn("get_mystic_number", || 42 as FLOAT);
    let m = rhai::exported_module!(crate::duplicate_fn_rename::my_adds);
    let mut r = StaticModuleResolver::new();
    r.insert("Math::Advanced".to_string(), m);
    engine.set_module_resolver(Some(r));

    let output_array = engine.eval::<Array>(
        r#"import "Math::Advanced" as math;
       let fx = get_mystic_number();
       let fy = math::add(fx, 1.0);
       let ix = 42;
       let iy = math::add(ix, 1);
       [fy, iy]
       "#,
    )?;
    assert_eq!(&output_array[0].as_float().unwrap(), &43.0);
    assert_eq!(&output_array[1].as_int().unwrap(), &43);
    Ok(())
}
