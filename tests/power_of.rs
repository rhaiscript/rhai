use rhai::{Engine, INT};

#[cfg(not(feature = "no_float"))]
use rhai::FLOAT;

#[cfg(not(feature = "no_float"))]
const EPSILON: FLOAT = FLOAT::EPSILON;

#[test]
fn test_power_of() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("2 ** 3").unwrap(), 8);
    assert_eq!(engine.eval::<INT>("(-2 ** 3)").unwrap(), -8);
    assert_eq!(engine.eval::<INT>("2 ** 3 ** 2").unwrap(), 512);

    #[cfg(not(feature = "no_float"))]
    {
        assert!((engine.eval::<FLOAT>("2.2 ** 3.3").unwrap() - (2.2 as FLOAT).powf(3.3)).abs() <= EPSILON);
        assert!((engine.eval::<FLOAT>("2.0**-2.0").unwrap() - 0.25 as FLOAT).abs() < EPSILON);
        assert!((engine.eval::<FLOAT>("(-2.0**-2.0)").unwrap() - 0.25 as FLOAT).abs() < EPSILON);
        assert!((engine.eval::<FLOAT>("(-2.0**-2)").unwrap() - 0.25 as FLOAT).abs() < EPSILON);
        assert_eq!(engine.eval::<INT>("4**3").unwrap(), 64);
    }
}

#[test]
fn test_power_of_equals() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = 2; x **= 3; x").unwrap(), 8);
    assert_eq!(engine.eval::<INT>("let x = -2; x **= 3; x").unwrap(), -8);

    #[cfg(not(feature = "no_float"))]
    {
        assert!((engine.eval::<FLOAT>("let x = 2.2; x **= 3.3; x").unwrap() - (2.2 as FLOAT).powf(3.3)).abs() <= EPSILON);
        assert!((engine.eval::<FLOAT>("let x = 2.0; x **= -2.0; x").unwrap() - 0.25 as FLOAT).abs() < EPSILON);
        assert!((engine.eval::<FLOAT>("let x = -2.0; x **= -2.0; x").unwrap() - 0.25 as FLOAT).abs() < EPSILON);
        assert!((engine.eval::<FLOAT>("let x = -2.0; x **= -2; x").unwrap() - 0.25 as FLOAT).abs() < EPSILON);
        assert_eq!(engine.eval::<INT>("let x =4; x **= 3; x").unwrap(), 64);
    }
}
