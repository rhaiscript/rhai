#![allow(non_snake_case)]

use crate::plugin::*;
use crate::{def_package, Position, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

#[cfg(not(feature = "no_float"))]
use crate::error::EvalAltResult;

#[cfg(feature = "no_std")]
#[cfg(not(feature = "no_float"))]
use num_traits::Float;

#[cfg(feature = "decimal")]
use rust_decimal::Decimal;

#[cfg(feature = "decimal")]
use super::arithmetic::make_err;

#[allow(dead_code)]
#[cfg(feature = "only_i32")]
pub const MAX_INT: INT = i32::MAX;
#[allow(dead_code)]
#[cfg(not(feature = "only_i32"))]
pub const MAX_INT: INT = i64::MAX;

macro_rules! gen_conversion_as_functions {
    ($root:ident => $func_name:ident ( $($arg_type:ident),+ ) -> $result_type:ty) => {
        pub mod $root { $(pub mod $arg_type {
            use super::super::*;

            #[export_fn]
            pub fn $func_name(x: $arg_type) -> $result_type {
                x as $result_type
            }
        })* }
    }
}

#[cfg(feature = "decimal")]
macro_rules! gen_conversion_into_functions {
    ($root:ident => $func_name:ident ( $($arg_type:ident),+ ) -> $result_type:ty) => {
        pub mod $root { $(pub mod $arg_type {
            use super::super::*;

            #[export_fn]
            pub fn $func_name(x: $arg_type) -> $result_type {
                x.into()
            }
        })* }
    }
}

macro_rules! reg_functions {
    ($mod_name:ident += $root:ident :: $func_name:ident ( $($arg_type:ident),+ ) ) => { $(
        set_exported_fn!($mod_name, stringify!($func_name), $root::$arg_type::$func_name);
    )* }
}

def_package!(crate:BasicMathPackage:"Basic mathematic functions.", lib, {
    lib.standard = true;

    // Integer functions
    combine_with_exported_module!(lib, "int", int_functions);

    reg_functions!(lib += basic_to_int::to_int(char));

    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    {
        reg_functions!(lib += numbers_to_int::to_int(i8, u8, i16, u16, i32, u32, i64, u64));

        #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
        reg_functions!(lib += num_128_to_int::to_int(i128, u128));
    }

    #[cfg(not(feature = "no_float"))]
    {
        // Floating point functions
        combine_with_exported_module!(lib, "float", float_functions);

        // Trig functions
        combine_with_exported_module!(lib, "trig", trig_functions);

        reg_functions!(lib += basic_to_float::to_float(INT));

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            reg_functions!(lib += numbers_to_float::to_float(i8, u8, i16, u16, i32, u32, i64, u32));

            #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
            reg_functions!(lib += num_128_to_float::to_float(i128, u128));
        }
    }

    // Decimal functions
    #[cfg(feature = "decimal")]
    {
        combine_with_exported_module!(lib, "decimal", decimal_functions);

        reg_functions!(lib += basic_to_decimal::to_decimal(INT));

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        reg_functions!(lib += numbers_to_decimal::to_decimal(i8, u8, i16, u16, i32, u32, i64, u64));
    }
});

#[export_module]
mod int_functions {
    #[rhai_fn(name = "parse_int", return_raw)]
    pub fn parse_int_radix(string: &str, radix: INT) -> Result<INT, Box<EvalAltResult>> {
        if !(2..=36).contains(&radix) {
            return EvalAltResult::ErrorArithmetic(
                format!("Invalid radix: '{}'", radix),
                Position::NONE,
            )
            .into();
        }

        INT::from_str_radix(string.trim(), radix as u32).map_err(|err| {
            EvalAltResult::ErrorArithmetic(
                format!("Error parsing integer number '{}': {}", string, err),
                Position::NONE,
            )
            .into()
        })
    }
    #[rhai_fn(name = "parse_int", return_raw)]
    pub fn parse_int(string: &str) -> Result<INT, Box<EvalAltResult>> {
        parse_int_radix(string, 10)
    }
}

#[cfg(not(feature = "no_float"))]
#[export_module]
mod trig_functions {
    use crate::FLOAT;

    pub fn sin(x: FLOAT) -> FLOAT {
        x.sin()
    }
    pub fn cos(x: FLOAT) -> FLOAT {
        x.cos()
    }
    pub fn tan(x: FLOAT) -> FLOAT {
        x.tan()
    }
    pub fn sinh(x: FLOAT) -> FLOAT {
        x.sinh()
    }
    pub fn cosh(x: FLOAT) -> FLOAT {
        x.cosh()
    }
    pub fn tanh(x: FLOAT) -> FLOAT {
        x.tanh()
    }
    pub fn asin(x: FLOAT) -> FLOAT {
        x.asin()
    }
    pub fn acos(x: FLOAT) -> FLOAT {
        x.acos()
    }
    pub fn atan(x: FLOAT) -> FLOAT {
        x.atan()
    }
    #[rhai_fn(name = "atan")]
    pub fn atan2(x: FLOAT, y: FLOAT) -> FLOAT {
        x.atan2(y)
    }
    pub fn asinh(x: FLOAT) -> FLOAT {
        x.asinh()
    }
    pub fn acosh(x: FLOAT) -> FLOAT {
        x.acosh()
    }
    pub fn atanh(x: FLOAT) -> FLOAT {
        x.atanh()
    }
    pub fn hypot(x: FLOAT, y: FLOAT) -> FLOAT {
        x.hypot(y)
    }
}

#[cfg(not(feature = "no_float"))]
#[export_module]
mod float_functions {
    use crate::FLOAT;

    #[rhai_fn(name = "E")]
    pub fn e() -> FLOAT {
        #[cfg(not(feature = "f32_float"))]
        return std::f64::consts::E;
        #[cfg(feature = "f32_float")]
        return std::f32::consts::E;
    }
    #[rhai_fn(name = "PI")]
    pub fn pi() -> FLOAT {
        #[cfg(not(feature = "f32_float"))]
        return std::f64::consts::PI;
        #[cfg(feature = "f32_float")]
        return std::f32::consts::PI;
    }
    pub fn to_radians(x: FLOAT) -> FLOAT {
        x.to_radians()
    }
    pub fn to_degrees(x: FLOAT) -> FLOAT {
        x.to_degrees()
    }
    pub fn sqrt(x: FLOAT) -> FLOAT {
        x.sqrt()
    }
    pub fn exp(x: FLOAT) -> FLOAT {
        x.exp()
    }
    pub fn ln(x: FLOAT) -> FLOAT {
        x.ln()
    }
    pub fn log(x: FLOAT, base: FLOAT) -> FLOAT {
        x.log(base)
    }
    #[rhai_fn(name = "log")]
    pub fn log10(x: FLOAT) -> FLOAT {
        x.log10()
    }
    #[rhai_fn(name = "floor", get = "floor")]
    pub fn floor(x: FLOAT) -> FLOAT {
        x.floor()
    }
    #[rhai_fn(name = "ceiling", get = "ceiling")]
    pub fn ceiling(x: FLOAT) -> FLOAT {
        x.ceil()
    }
    #[rhai_fn(name = "round", get = "round")]
    pub fn round(x: FLOAT) -> FLOAT {
        x.round()
    }
    #[rhai_fn(name = "int", get = "int")]
    pub fn int(x: FLOAT) -> FLOAT {
        x.trunc()
    }
    #[rhai_fn(name = "fraction", get = "fraction")]
    pub fn fraction(x: FLOAT) -> FLOAT {
        x.fract()
    }
    #[rhai_fn(name = "is_nan", get = "is_nan")]
    pub fn is_nan(x: FLOAT) -> bool {
        x.is_nan()
    }
    #[rhai_fn(name = "is_finite", get = "is_finite")]
    pub fn is_finite(x: FLOAT) -> bool {
        x.is_finite()
    }
    #[rhai_fn(name = "is_infinite", get = "is_infinite")]
    pub fn is_infinite(x: FLOAT) -> bool {
        x.is_infinite()
    }
    #[rhai_fn(name = "to_int", return_raw)]
    pub fn f32_to_int(x: f32) -> Result<INT, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) && x > (MAX_INT as f32) {
            EvalAltResult::ErrorArithmetic(
                format!("Integer overflow: to_int({})", x),
                Position::NONE,
            )
            .into()
        } else {
            Ok(x.trunc() as INT)
        }
    }
    #[rhai_fn(name = "to_int", return_raw)]
    pub fn f64_to_int(x: f64) -> Result<INT, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) && x > (MAX_INT as f64) {
            EvalAltResult::ErrorArithmetic(
                format!("Integer overflow: to_int({})", x),
                Position::NONE,
            )
            .into()
        } else {
            Ok(x.trunc() as INT)
        }
    }
    #[rhai_fn(return_raw)]
    pub fn parse_float(string: &str) -> Result<FLOAT, Box<EvalAltResult>> {
        string.trim().parse::<FLOAT>().map_err(|err| {
            EvalAltResult::ErrorArithmetic(
                format!("Error parsing floating-point number '{}': {}", string, err),
                Position::NONE,
            )
            .into()
        })
    }
    #[cfg(not(feature = "f32_float"))]
    pub mod f32_f64 {
        #[rhai_fn(name = "to_float")]
        pub fn f32_to_f64(x: f32) -> f64 {
            x as f64
        }
    }
}

#[cfg(feature = "decimal")]
#[export_module]
mod decimal_functions {
    use rust_decimal::{
        prelude::{FromStr, RoundingStrategy},
        Decimal, MathematicalOps,
    };

    #[cfg(feature = "no_float")]
    pub mod float_polyfills {
        #[rhai_fn(name = "PI")]
        pub fn pi() -> Decimal {
            Decimal::PI
        }
        #[rhai_fn(name = "E")]
        pub fn e() -> Decimal {
            Decimal::E
        }
        #[rhai_fn(return_raw)]
        pub fn parse_float(s: &str) -> Result<Decimal, Box<EvalAltResult>> {
            super::parse_decimal(s)
        }
    }

    pub fn sin(x: Decimal) -> Decimal {
        x.sin()
    }
    pub fn cos(x: Decimal) -> Decimal {
        x.cos()
    }
    pub fn tan(x: Decimal) -> Decimal {
        x.tan()
    }
    #[rhai_fn(return_raw)]
    pub fn sqrt(x: Decimal) -> Result<Decimal, Box<EvalAltResult>> {
        x.sqrt()
            .ok_or_else(|| make_err(format!("Error taking the square root of {}", x,)))
    }
    #[rhai_fn(return_raw)]
    pub fn exp(x: Decimal) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            x.checked_exp()
                .ok_or_else(|| make_err(format!("Exponential overflow: e ** {}", x,)))
        } else {
            Ok(x.exp())
        }
    }
    #[rhai_fn(return_raw)]
    pub fn ln(x: Decimal) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            x.checked_ln()
                .ok_or_else(|| make_err(format!("Error taking the natural log of {}", x)))
        } else {
            Ok(x.ln())
        }
    }
    #[rhai_fn(name = "log", return_raw)]
    pub fn log10(x: Decimal) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            x.checked_log10()
                .ok_or_else(|| make_err(format!("Error taking the log of {}", x)))
        } else {
            Ok(x.log10())
        }
    }
    #[rhai_fn(name = "floor", get = "floor")]
    pub fn floor(x: Decimal) -> Decimal {
        x.floor()
    }
    #[rhai_fn(name = "ceiling", get = "ceiling")]
    pub fn ceiling(x: Decimal) -> Decimal {
        x.ceil()
    }
    #[rhai_fn(name = "round", get = "round")]
    pub fn round(x: Decimal) -> Decimal {
        x.round()
    }
    #[rhai_fn(name = "round", return_raw)]
    pub fn round_dp(x: Decimal, dp: INT) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            if dp < 0 {
                return Err(make_err(format!(
                    "Invalid number of digits for rounding: {}",
                    dp
                )));
            }
            if cfg!(not(feature = "only_i32")) && dp > (u32::MAX as INT) {
                return Ok(x);
            }
        }

        Ok(x.round_dp(dp as u32))
    }
    #[rhai_fn(return_raw)]
    pub fn round_up(x: Decimal, dp: INT) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            if dp < 0 {
                return Err(make_err(format!(
                    "Invalid number of digits for rounding: {}",
                    dp
                )));
            }
            if cfg!(not(feature = "only_i32")) && dp > (u32::MAX as INT) {
                return Ok(x);
            }
        }

        Ok(x.round_dp_with_strategy(dp as u32, RoundingStrategy::AwayFromZero))
    }
    #[rhai_fn(return_raw)]
    pub fn round_down(x: Decimal, dp: INT) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            if dp < 0 {
                return Err(make_err(format!(
                    "Invalid number of digits for rounding: {}",
                    dp
                )));
            }
            if cfg!(not(feature = "only_i32")) && dp > (u32::MAX as INT) {
                return Ok(x);
            }
        }

        Ok(x.round_dp_with_strategy(dp as u32, RoundingStrategy::ToZero))
    }
    #[rhai_fn(return_raw)]
    pub fn round_half_up(x: Decimal, dp: INT) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            if dp < 0 {
                return Err(make_err(format!(
                    "Invalid number of digits for rounding: {}",
                    dp
                )));
            }
            if cfg!(not(feature = "only_i32")) && dp > (u32::MAX as INT) {
                return Ok(x);
            }
        }

        Ok(x.round_dp_with_strategy(dp as u32, RoundingStrategy::MidpointAwayFromZero))
    }
    #[rhai_fn(return_raw)]
    pub fn round_half_down(x: Decimal, dp: INT) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            if dp < 0 {
                return Err(make_err(format!(
                    "Invalid number of digits for rounding: {}",
                    dp
                )));
            }
            if cfg!(not(feature = "only_i32")) && dp > (u32::MAX as INT) {
                return Ok(x);
            }
        }

        Ok(x.round_dp_with_strategy(dp as u32, RoundingStrategy::MidpointTowardZero))
    }
    #[rhai_fn(name = "int", get = "int")]
    pub fn int(x: Decimal) -> Decimal {
        x.trunc()
    }
    #[rhai_fn(name = "fraction", get = "fraction")]
    pub fn fraction(x: Decimal) -> Decimal {
        x.fract()
    }
    #[rhai_fn(return_raw)]
    pub fn parse_decimal(string: &str) -> Result<Decimal, Box<EvalAltResult>> {
        Decimal::from_str(string)
            .or_else(|_| Decimal::from_scientific(string))
            .map_err(|err| {
                EvalAltResult::ErrorArithmetic(
                    format!("Error parsing decimal number '{}': {}", string, err),
                    Position::NONE,
                )
                .into()
            })
    }

    #[cfg(not(feature = "no_float"))]
    pub mod float {
        use std::convert::TryFrom;

        #[rhai_fn(name = "to_decimal", return_raw)]
        pub fn f32_to_decimal(x: f32) -> Result<Decimal, Box<EvalAltResult>> {
            Decimal::try_from(x).map_err(|_| {
                EvalAltResult::ErrorArithmetic(
                    format!("Cannot convert to Decimal: to_decimal({})", x),
                    Position::NONE,
                )
                .into()
            })
        }
        #[rhai_fn(name = "to_decimal", return_raw)]
        pub fn f64_to_decimal(x: f64) -> Result<Decimal, Box<EvalAltResult>> {
            Decimal::try_from(x).map_err(|_| {
                EvalAltResult::ErrorArithmetic(
                    format!("Cannot convert to Decimal: to_decimal({})", x),
                    Position::NONE,
                )
                .into()
            })
        }
        #[rhai_fn(return_raw)]
        pub fn to_float(x: Decimal) -> Result<FLOAT, Box<EvalAltResult>> {
            FLOAT::try_from(x).map_err(|_| {
                EvalAltResult::ErrorArithmetic(
                    format!("Cannot convert to floating-point: to_float({})", x),
                    Position::NONE,
                )
                .into()
            })
        }
    }
}

#[cfg(not(feature = "no_float"))]
gen_conversion_as_functions!(basic_to_float => to_float (INT) -> FLOAT);

#[cfg(not(feature = "no_float"))]
#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_conversion_as_functions!(numbers_to_float => to_float (i8, u8, i16, u16, i32, u32, i64, u64) -> FLOAT);

#[cfg(not(feature = "no_float"))]
#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
gen_conversion_as_functions!(num_128_to_float => to_float (i128, u128) -> FLOAT);

gen_conversion_as_functions!(basic_to_int => to_int (char) -> INT);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_conversion_as_functions!(numbers_to_int => to_int (i8, u8, i16, u16, i32, u32, i64, u64) -> INT);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
gen_conversion_as_functions!(num_128_to_int => to_int (i128, u128) -> INT);

#[cfg(feature = "decimal")]
gen_conversion_into_functions!(basic_to_decimal => to_decimal (INT) -> Decimal);

#[cfg(feature = "decimal")]
#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_conversion_into_functions!(numbers_to_decimal => to_decimal (i8, u8, i16, u16, i32, u32, i64, u64) -> Decimal);
