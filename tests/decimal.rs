#![cfg(not(feature = "no_decimal"))]
use rhai::{Engine, EvalAltResult, RegisterFn};

use rust_decimal::prelude::*;

#[test]
fn test_decimal() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    let source = r#"
        let x = 3.3333;
        let y = 6.6666;
        print("x: " + x);
        print("type_of(x): " + type_of(x));
        let ans = x + y;
        ans
    "#;

    assert_eq!(
        engine.eval::<Decimal>(source)?.round_dp(3),
        Decimal::from_str("10.0").unwrap()
    );

    assert_eq!(
        engine.eval::<Decimal>(
            r#"
            let x = -0.5; 
            x.abs()"#
        )?,
        Decimal::from_str("0.5").unwrap()
    );

    assert_eq!(
        engine.eval::<bool>(
            r#"
            let x = 0.0; 
            let y = 1.0; 
            x < y"#
        )?,
        true
    );

    assert_eq!(
        engine.eval::<bool>(
            r#"
            let x = 0.0; 
            let y = 1.0; 
            x < y"#
        )?,
        true
    );

    assert_eq!(
        engine.eval::<bool>(
            r#"
            let x = 0.0; 
            let y = 1.0;
            x > y"#
        )?,
        false
    );

    assert_eq!(
        engine.eval::<bool>(
            r#"
            let x = 0.666; 
            let y = 0.666; 
            x == y"#
        )?,
        true
    );

    assert_eq!(
        engine.eval::<Decimal>(
            r#"
            let x = 10.0;
            let y = 4.0;
            x % y"#
        )?,
        Decimal::from_str("2.0").unwrap()
    );

    assert_eq!(
        engine.eval::<Decimal>(
            r#"
            let x = -1.0;
            x.abs()"#
        )?,
        Decimal::from_str("1.0").unwrap()
    );

    assert_eq!(
        engine.eval::<Decimal>(
            r#"
            let x = 1.0;
            let a = -x;
            a"#
        )?,
        Decimal::from_str("-1.0").unwrap()
    );

    let res = engine.eval::<Decimal>(
        r#"
        let x = 9.9999;
        x"#,
    )?;

    assert!(res - (Decimal::from_str("9.9999").unwrap()) == Decimal::zero());

    Ok(())
}

#[test]
#[cfg(not(feature = "no_object"))]
fn test_struct_with_decimal() -> Result<(), Box<EvalAltResult>> {
    #[derive(Clone)]
    struct TestStruct {
        x: Decimal,
    }

    impl TestStruct {
        fn update(&mut self) {
            self.x += Decimal::from_str("5.789").unwrap();
        }

        fn get_x(&mut self) -> Decimal {
            self.x
        }

        fn set_x(&mut self, new_x: Decimal) {
            self.x = new_x;
        }

        fn new() -> Self {
            TestStruct {
                x: Decimal::from_str("1.0").unwrap(),
            }
        }
    }

    let mut engine = Engine::new();

    engine.register_type::<TestStruct>();

    engine.register_get_set("x", TestStruct::get_x, TestStruct::set_x);
    engine.register_fn("update", TestStruct::update);
    engine.register_fn("new_ts", TestStruct::new);

    assert!(
        (engine.eval::<Decimal>("let ts = new_ts(); ts.update(); ts.x")?
            - Decimal::from_str("6.789").unwrap())
        .abs()
            == Decimal::zero()
    );
    assert!(
        (engine.eval::<Decimal>("let ts = new_ts(); ts.x = 10.1001; ts.x")?
            - Decimal::from_str("10.1001").unwrap())
        .abs()
            == Decimal::zero()
    );

    Ok(())
}

#[test]
fn test_decimal_func() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();

    engine.register_fn("sum", |x: Decimal, y: Decimal, z: Decimal, w: Decimal| {
        x + y + z + w
    });

    assert_eq!(
        engine.eval::<Decimal>("sum(1.0, 2.0, 3.0, 4.0)")?,
        Decimal::from_str("10.0").unwrap()
    );

    Ok(())
}
