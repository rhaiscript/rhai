#![cfg(not(feature = "no_std"))]
#![cfg(not(target_arch = "wasm32"))]

use rhai::{Engine, EvalAltResult};

#[cfg(not(feature = "no_float"))]
use rhai::FLOAT;

#[cfg(feature = "no_float")]
use rhai::INT;

#[test]
fn test_timestamp() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    assert_eq!(engine.eval::<String>("type_of(timestamp())")?, "timestamp");

    #[cfg(not(feature = "no_float"))]
    assert!(
        engine.eval::<FLOAT>(
            r#"
                let time = timestamp();
                let x = 10_000;
                while x > 0 { x -= 1; }
                elapsed(time)
    "#
        )? < 10.0
    );

    #[cfg(feature = "no_float")]
    assert!(
        engine.eval::<INT>(
            r#"
                let time = timestamp();
                let x = 10_000;
                while x > 0 { x -= 1; }
                elapsed(time)
    "#
        )? < 10
    );

    assert!(engine.eval::<bool>(
        r"
            let time1 = timestamp();
            for x in range(0, 10000) {}
            let time2 = timestamp();
            time1 <= time2
        "
    )?);

    Ok(())
}
