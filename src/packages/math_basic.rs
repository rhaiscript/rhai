use crate::def_package;
use crate::parser::INT;

#[cfg(not(feature = "no_float"))]
use crate::parser::FLOAT;

#[cfg(not(feature = "no_float"))]
use crate::{result::EvalAltResult, token::Position};

#[cfg(feature = "no_std")]
#[cfg(not(feature = "no_float"))]
use num_traits::float::Float;

#[cfg(feature = "decimal")]
use rust_decimal::Decimal;

#[cfg(not(feature = "no_float"))]
use crate::stdlib::format;

#[allow(dead_code)]
#[cfg(feature = "only_i32")]
pub const MAX_INT: INT = i32::MAX;
#[allow(dead_code)]
#[cfg(not(feature = "only_i32"))]
pub const MAX_INT: INT = i64::MAX;

def_package!(crate:BasicMathPackage:"Basic mathematic functions.", lib, {
    #[cfg(not(feature = "no_float"))]
    {
        // Advanced math functions
        lib.set_fn_1("sin", |x: FLOAT| Ok(x.to_radians().sin()));
        lib.set_fn_1("cos", |x: FLOAT| Ok(x.to_radians().cos()));
        lib.set_fn_1("tan", |x: FLOAT| Ok(x.to_radians().tan()));
        lib.set_fn_1("sinh", |x: FLOAT| Ok(x.to_radians().sinh()));
        lib.set_fn_1("cosh", |x: FLOAT| Ok(x.to_radians().cosh()));
        lib.set_fn_1("tanh", |x: FLOAT| Ok(x.to_radians().tanh()));
        lib.set_fn_1("asin", |x: FLOAT| Ok(x.asin().to_degrees()));
        lib.set_fn_1("acos", |x: FLOAT| Ok(x.acos().to_degrees()));
        lib.set_fn_1("atan", |x: FLOAT| Ok(x.atan().to_degrees()));
        lib.set_fn_1("asinh", |x: FLOAT| Ok(x.asinh().to_degrees()));
        lib.set_fn_1("acosh", |x: FLOAT| Ok(x.acosh().to_degrees()));
        lib.set_fn_1("atanh", |x: FLOAT| Ok(x.atanh().to_degrees()));
        lib.set_fn_1("sqrt", |x: FLOAT| Ok(x.sqrt()));
        lib.set_fn_1("exp", |x: FLOAT| Ok(x.exp()));
        lib.set_fn_1("ln", |x: FLOAT| Ok(x.ln()));
        lib.set_fn_2("log", |x: FLOAT, base: FLOAT| Ok(x.log(base)));
        lib.set_fn_1("log10", |x: FLOAT| Ok(x.log10()));
        lib.set_fn_1("floor", |x: FLOAT| Ok(x.floor()));
        lib.set_fn_1("ceiling", |x: FLOAT| Ok(x.ceil()));
        lib.set_fn_1("round", |x: FLOAT| Ok(x.ceil()));
        lib.set_fn_1("int", |x: FLOAT| Ok(x.trunc()));
        lib.set_fn_1("fraction", |x: FLOAT| Ok(x.fract()));
        lib.set_fn_1("is_nan", |x: FLOAT| Ok(x.is_nan()));
        lib.set_fn_1("is_finite", |x: FLOAT| Ok(x.is_finite()));
        lib.set_fn_1("is_infinite", |x: FLOAT| Ok(x.is_infinite()));

        #[cfg(not(feature = "no_object"))]
        {
            lib.set_getter_fn("floor", |x: &mut FLOAT| Ok(x.floor()));
            lib.set_getter_fn("ceiling", |x: &mut FLOAT| Ok(x.ceil()));
            lib.set_getter_fn("round", |x: &mut FLOAT| Ok(x.ceil()));
            lib.set_getter_fn("int", |x: &mut FLOAT| Ok(x.trunc()));
            lib.set_getter_fn("fraction", |x: &mut FLOAT| Ok(x.fract()));
            lib.set_getter_fn("is_nan", |x: &mut FLOAT| Ok(x.is_nan()));
            lib.set_getter_fn("is_finite", |x: &mut FLOAT| Ok(x.is_finite()));
            lib.set_getter_fn("is_infinite", |x: &mut FLOAT| Ok(x.is_infinite()));
        }

        // Register conversion functions
        lib.set_fn_1("to_float", |x: INT| Ok(x as FLOAT));
        lib.set_fn_1("to_float", |x: f32| Ok(x as FLOAT));

        if cfg!(not(feature = "only_i32")) && cfg!(not(feature = "only_i64")) {
            lib.set_fn_1("to_float", |x: i8| Ok(x as FLOAT));
            lib.set_fn_1("to_float", |x: u8| Ok(x as FLOAT));
            lib.set_fn_1("to_float", |x: i16| Ok(x as FLOAT));
            lib.set_fn_1("to_float", |x: u16| Ok(x as FLOAT));
            lib.set_fn_1("to_float", |x: i32| Ok(x as FLOAT));
            lib.set_fn_1("to_float", |x: u32| Ok(x as FLOAT));
            lib.set_fn_1("to_float", |x: i64| Ok(x as FLOAT));
            lib.set_fn_1("to_float", |x: u64| Ok(x as FLOAT));

            if cfg!(not(target_arch = "wasm32")) {
                lib.set_fn_1("to_float", |x: i128| Ok(x as FLOAT));
                lib.set_fn_1("to_float", |x: u128| Ok(x as FLOAT));
            }
        }
    }

    #[cfg(feature = "decimal")]
    {
        // Advanced math functions
        lib.set_fn_1("floor", |x: Decimal| Ok(x.floor()));
        lib.set_fn_1("ceiling", |x: Decimal| Ok(x.ceil()));
        lib.set_fn_1("round", |x: Decimal| Ok(x.round()));
        lib.set_fn_2("round_dp", |x: Decimal, dp: INT| Ok(x.round_dp(dp as u32)));
        lib.set_fn_1("int", |x: Decimal| Ok(x.trunc()));
        lib.set_fn_1("fraction", |x: Decimal| Ok(x.fract()));
        lib.set_fn_1("scale", |x: Decimal| Ok(x.scale()));
    }

    lib.set_fn_1("to_int", |ch: char| Ok(ch as INT));

    if cfg!(not(feature = "only_i32")) && cfg!(not(feature = "only_i64")) {
        lib.set_fn_1("to_int", |x: i8| Ok(x as INT));
        lib.set_fn_1("to_int", |x: u8| Ok(x as INT));
        lib.set_fn_1("to_int", |x: i16| Ok(x as INT));
        lib.set_fn_1("to_int", |x: u16| Ok(x as INT));
    }

    if cfg!(not(feature = "only_i32")) {
        lib.set_fn_1("to_int", |x: i32| Ok(x as INT));
        lib.set_fn_1("to_int", |x: u64| Ok(x as INT));

        if cfg!(feature = "only_i64") {
            lib.set_fn_1("to_int", |x: u32| Ok(x as INT));
        }
    }

    #[cfg(not(feature = "no_float"))]
    {
        if cfg!(not(feature = "unchecked")) {
            lib.set_fn_1(
                "to_int",
                |x: f32| {
                    if x > (MAX_INT as f32) {
                        return EvalAltResult::ErrorArithmetic(
                            format!("Integer overflow: to_int({})", x),
                            Position::none(),
                        ).into();
                    }

                    Ok(x.trunc() as INT)
                },
            );
            lib.set_fn_1(
                "to_int",
                |x: FLOAT| {
                    if x > (MAX_INT as FLOAT) {
                        return EvalAltResult::ErrorArithmetic(
                            format!("Integer overflow: to_int({})", x),
                            Position::none(),
                        ).into();
                    }

                    Ok(x.trunc() as INT)
                },
            );
        }

        if cfg!(feature = "unchecked") {
            lib.set_fn_1("to_int", |x: f32| Ok(x as INT));
            lib.set_fn_1("to_int", |x: f64| Ok(x as INT));
        }
    }
});
