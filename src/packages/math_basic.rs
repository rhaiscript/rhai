#![allow(non_snake_case)]

use crate::module::ModuleFlags;
use crate::plugin::*;
use crate::{def_package, Position, RhaiResultOf, ERR, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

#[cfg(feature = "no_std")]
#[cfg(not(feature = "no_float"))]
use num_traits::Float;

#[cfg(feature = "decimal")]
use rust_decimal::Decimal;

#[cfg(feature = "decimal")]
use super::arithmetic::make_err;

macro_rules! gen_conversion_as_functions {
    ($root:ident => $func_name:ident ( $($arg_type:ident),+ ) -> $result_type:ty) => {
        pub mod $root { $(pub mod $arg_type {
            use super::super::*;

            #[export_fn]
            #[allow(clippy::missing_const_for_fn)]
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

def_package! {
    /// Basic mathematical package.
    pub BasicMathPackage(lib) {
        lib.flags |= ModuleFlags::STANDARD_LIB;

        // Integer functions
        combine_with_exported_module!(lib, "int", int_functions);

        reg_functions!(lib += basic_to_int::to_int(char));

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            reg_functions!(lib += numbers_to_int::to_int(i8, u8, i16, u16, i32, u32, i64, u64));

            #[cfg(not(target_family = "wasm"))]

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

                #[cfg(not(target_family = "wasm"))]

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
    }
}

#[export_module]
mod int_functions {
    /// Parse a string into an integer number.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = parse_int("123");
    ///
    /// print(x);       // prints 123
    /// ```
    #[rhai_fn(name = "parse_int", return_raw)]
    pub fn parse_int(string: &str) -> RhaiResultOf<INT> {
        parse_int_radix(string, 10)
    }
    /// Parse a string into an integer number of the specified `radix`.
    ///
    /// `radix` must be between 2 and 36.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = parse_int("123");
    ///
    /// print(x);       // prints 123
    ///
    /// let y = parse_int("123abc", 16);
    ///
    /// print(y);       // prints 1194684 (0x123abc)
    /// ```
    #[rhai_fn(name = "parse_int", return_raw)]
    pub fn parse_int_radix(string: &str, radix: INT) -> RhaiResultOf<INT> {
        if !(2..=36).contains(&radix) {
            return Err(
                ERR::ErrorArithmetic(format!("Invalid radix: '{radix}'"), Position::NONE).into(),
            );
        }

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        INT::from_str_radix(string.trim(), radix as u32).map_err(|err| {
            ERR::ErrorArithmetic(
                format!("Error parsing integer number '{string}': {err}"),
                Position::NONE,
            )
            .into()
        })
    }
}

#[cfg(not(feature = "no_float"))]
#[export_module]
mod trig_functions {
    /// Return the sine of the floating-point number in radians.
    pub fn sin(x: FLOAT) -> FLOAT {
        x.sin()
    }
    /// Return the cosine of the floating-point number in radians.
    pub fn cos(x: FLOAT) -> FLOAT {
        x.cos()
    }
    /// Return the tangent of the floating-point number in radians.
    pub fn tan(x: FLOAT) -> FLOAT {
        x.tan()
    }
    /// Return the hyperbolic sine of the floating-point number in radians.
    pub fn sinh(x: FLOAT) -> FLOAT {
        x.sinh()
    }
    /// Return the hyperbolic cosine of the floating-point number in radians.
    pub fn cosh(x: FLOAT) -> FLOAT {
        x.cosh()
    }
    /// Return the hyperbolic tangent of the floating-point number in radians.
    pub fn tanh(x: FLOAT) -> FLOAT {
        x.tanh()
    }
    /// Return the arc-sine of the floating-point number, in radians.
    pub fn asin(x: FLOAT) -> FLOAT {
        x.asin()
    }
    /// Return the arc-cosine of the floating-point number, in radians.
    pub fn acos(x: FLOAT) -> FLOAT {
        x.acos()
    }
    /// Return the arc-tangent of the floating-point number, in radians.
    pub fn atan(x: FLOAT) -> FLOAT {
        x.atan()
    }
    /// Return the arc-tangent of the floating-point numbers `x` and `y`, in radians.
    #[rhai_fn(name = "atan")]
    pub fn atan2(x: FLOAT, y: FLOAT) -> FLOAT {
        x.atan2(y)
    }
    /// Return the arc-hyperbolic-sine of the floating-point number, in radians.
    pub fn asinh(x: FLOAT) -> FLOAT {
        x.asinh()
    }
    /// Return the arc-hyperbolic-cosine of the floating-point number, in radians.
    pub fn acosh(x: FLOAT) -> FLOAT {
        x.acosh()
    }
    /// Return the arc-hyperbolic-tangent of the floating-point number, in radians.
    pub fn atanh(x: FLOAT) -> FLOAT {
        x.atanh()
    }
    /// Return the hypotenuse of a triangle with sides `x` and `y`.
    pub fn hypot(x: FLOAT, y: FLOAT) -> FLOAT {
        x.hypot(y)
    }
}

#[cfg(not(feature = "no_float"))]
#[export_module]
mod float_functions {
    /// Return the natural number _e_.
    #[rhai_fn(name = "E")]
    pub const fn e() -> FLOAT {
        #[cfg(not(feature = "f32_float"))]
        return std::f64::consts::E;
        #[cfg(feature = "f32_float")]
        return std::f32::consts::E;
    }
    /// Return the number π.
    #[rhai_fn(name = "PI")]
    pub const fn pi() -> FLOAT {
        #[cfg(not(feature = "f32_float"))]
        return std::f64::consts::PI;
        #[cfg(feature = "f32_float")]
        return std::f32::consts::PI;
    }
    /// Convert degrees to radians.
    pub fn to_radians(x: FLOAT) -> FLOAT {
        x.to_radians()
    }
    /// Convert radians to degrees.
    pub fn to_degrees(x: FLOAT) -> FLOAT {
        x.to_degrees()
    }
    /// Return the square root of the floating-point number.
    pub fn sqrt(x: FLOAT) -> FLOAT {
        x.sqrt()
    }
    /// Return the exponential of the floating-point number.
    pub fn exp(x: FLOAT) -> FLOAT {
        x.exp()
    }
    /// Return the natural log of the floating-point number.
    pub fn ln(x: FLOAT) -> FLOAT {
        x.ln()
    }
    /// Return the log of the floating-point number with `base`.
    pub fn log(x: FLOAT, base: FLOAT) -> FLOAT {
        x.log(base)
    }
    /// Return the log of the floating-point number with base 10.
    #[rhai_fn(name = "log")]
    pub fn log10(x: FLOAT) -> FLOAT {
        x.log10()
    }
    /// Return the largest whole number less than or equals to the floating-point number.
    #[rhai_fn(name = "floor", get = "floor")]
    pub fn floor(x: FLOAT) -> FLOAT {
        x.floor()
    }
    /// Return the smallest whole number larger than or equals to the floating-point number.
    #[rhai_fn(name = "ceiling", get = "ceiling")]
    pub fn ceiling(x: FLOAT) -> FLOAT {
        x.ceil()
    }
    /// Return the nearest whole number closest to the floating-point number.
    /// Rounds away from zero.
    #[rhai_fn(name = "round", get = "round")]
    pub fn round(x: FLOAT) -> FLOAT {
        x.round()
    }
    /// Return the integral part of the floating-point number.
    #[rhai_fn(name = "int", get = "int")]
    pub fn int(x: FLOAT) -> FLOAT {
        x.trunc()
    }
    /// Return the fractional part of the floating-point number.
    #[rhai_fn(name = "fraction", get = "fraction")]
    pub fn fraction(x: FLOAT) -> FLOAT {
        x.fract()
    }
    /// Return `true` if the floating-point number is `NaN` (Not A Number).
    #[rhai_fn(name = "is_nan", get = "is_nan")]
    pub fn is_nan(x: FLOAT) -> bool {
        x.is_nan()
    }
    /// Return `true` if the floating-point number is finite.
    #[rhai_fn(name = "is_finite", get = "is_finite")]
    pub fn is_finite(x: FLOAT) -> bool {
        x.is_finite()
    }
    /// Return `true` if the floating-point number is infinite.
    #[rhai_fn(name = "is_infinite", get = "is_infinite")]
    pub fn is_infinite(x: FLOAT) -> bool {
        x.is_infinite()
    }
    /// Convert the floating-point number into an integer.
    #[rhai_fn(name = "to_int", return_raw)]
    pub fn f32_to_int(x: f32) -> RhaiResultOf<INT> {
        #[allow(clippy::cast_precision_loss)]
        if cfg!(not(feature = "unchecked")) && (x > (INT::MAX as f32) || x < (INT::MIN as f32)) {
            return Err(ERR::ErrorArithmetic(
                format!("Integer overflow: to_int({x})"),
                Position::NONE,
            )
            .into());
        }

        Ok(x.trunc() as INT)
    }
    /// Convert the floating-point number into an integer.
    #[rhai_fn(name = "to_int", return_raw)]
    pub fn f64_to_int(x: f64) -> RhaiResultOf<INT> {
        #[allow(clippy::cast_precision_loss)]
        if cfg!(not(feature = "unchecked")) && (x > (INT::MAX as f64) || x < (INT::MIN as f64)) {
            return Err(ERR::ErrorArithmetic(
                format!("Integer overflow: to_int({x})"),
                Position::NONE,
            )
            .into());
        }

        Ok(x.trunc() as INT)
    }
    /// Parse a string into a floating-point number.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = parse_int("123.456");
    ///
    /// print(x);       // prints 123.456
    /// ```
    #[rhai_fn(return_raw)]
    pub fn parse_float(string: &str) -> RhaiResultOf<FLOAT> {
        string.trim().parse::<FLOAT>().map_err(|err| {
            ERR::ErrorArithmetic(
                format!("Error parsing floating-point number '{string}': {err}"),
                Position::NONE,
            )
            .into()
        })
    }
    /// Convert the 32-bit floating-point number to 64-bit.
    #[cfg(not(feature = "f32_float"))]
    #[rhai_fn(name = "to_float")]
    pub fn f32_to_f64(x: f32) -> f64 {
        x.into()
    }
}

#[cfg(feature = "decimal")]
#[export_module]
mod decimal_functions {
    use num_traits::ToPrimitive;
    use rust_decimal::{
        prelude::{FromStr, RoundingStrategy},
        Decimal, MathematicalOps,
    };
    #[cfg(not(feature = "no_float"))]
    use std::convert::TryFrom;

    /// Return the natural number _e_.
    #[cfg(feature = "no_float")]
    #[rhai_fn(name = "PI")]
    pub const fn pi() -> Decimal {
        Decimal::PI
    }
    /// Return the number π.
    #[cfg(feature = "no_float")]
    #[rhai_fn(name = "E")]
    pub const fn e() -> Decimal {
        Decimal::E
    }
    /// Parse a string into a decimal number.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = parse_float("123.456");
    ///
    /// print(x);       // prints 123.456
    /// ```
    #[cfg(feature = "no_float")]
    #[rhai_fn(return_raw)]
    pub fn parse_float(s: &str) -> RhaiResultOf<Decimal> {
        parse_decimal(s)
    }

    /// Return the sine of the decimal number in radians.
    pub fn sin(x: Decimal) -> Decimal {
        x.sin()
    }
    /// Return the cosine of the decimal number in radians.
    pub fn cos(x: Decimal) -> Decimal {
        x.cos()
    }
    /// Return the tangent of the decimal number in radians.
    pub fn tan(x: Decimal) -> Decimal {
        x.tan()
    }
    /// Return the square root of the decimal number.
    #[rhai_fn(return_raw)]
    pub fn sqrt(x: Decimal) -> RhaiResultOf<Decimal> {
        x.sqrt()
            .ok_or_else(|| make_err(format!("Error taking the square root of {x}")))
    }
    /// Return the exponential of the decimal number.
    #[rhai_fn(return_raw)]
    pub fn exp(x: Decimal) -> RhaiResultOf<Decimal> {
        if cfg!(not(feature = "unchecked")) {
            x.checked_exp()
                .ok_or_else(|| make_err(format!("Exponential overflow: e ** {x}")))
        } else {
            Ok(x.exp())
        }
    }
    /// Return the natural log of the decimal number.
    #[rhai_fn(return_raw)]
    pub fn ln(x: Decimal) -> RhaiResultOf<Decimal> {
        if cfg!(not(feature = "unchecked")) {
            x.checked_ln()
                .ok_or_else(|| make_err(format!("Error taking the natural log of {x}")))
        } else {
            Ok(x.ln())
        }
    }
    /// Return the log of the decimal number with base 10.
    #[rhai_fn(name = "log", return_raw)]
    pub fn log10(x: Decimal) -> RhaiResultOf<Decimal> {
        if cfg!(not(feature = "unchecked")) {
            x.checked_log10()
                .ok_or_else(|| make_err(format!("Error taking the log of {x}")))
        } else {
            Ok(x.log10())
        }
    }
    /// Return the largest whole number less than or equals to the decimal number.
    #[rhai_fn(name = "floor", get = "floor")]
    pub fn floor(x: Decimal) -> Decimal {
        x.floor()
    }
    /// Return the smallest whole number larger than or equals to the decimal number.
    #[rhai_fn(name = "ceiling", get = "ceiling")]
    pub fn ceiling(x: Decimal) -> Decimal {
        x.ceil()
    }
    /// Return the nearest whole number closest to the decimal number.
    /// Always round mid-point towards the closest even number.
    #[rhai_fn(name = "round", get = "round")]
    pub fn round(x: Decimal) -> Decimal {
        x.round()
    }
    /// Round the decimal number to the specified number of `digits` after the decimal point and return it.
    /// Always round mid-point towards the closest even number.
    #[rhai_fn(name = "round", return_raw)]
    pub fn round_dp(x: Decimal, digits: INT) -> RhaiResultOf<Decimal> {
        if cfg!(not(feature = "unchecked")) {
            if digits < 0 {
                return Err(make_err(format!(
                    "Invalid number of digits for rounding: {digits}"
                )));
            }
            if cfg!(not(feature = "only_i32")) && digits > (u32::MAX as INT) {
                return Ok(x);
            }
        }

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        Ok(x.round_dp(digits as u32))
    }
    /// Round the decimal number to the specified number of `digits` after the decimal point and return it.
    /// Always round away from zero.
    #[rhai_fn(return_raw)]
    pub fn round_up(x: Decimal, digits: INT) -> RhaiResultOf<Decimal> {
        if cfg!(not(feature = "unchecked")) {
            if digits < 0 {
                return Err(make_err(format!(
                    "Invalid number of digits for rounding: {digits}"
                )));
            }
            if cfg!(not(feature = "only_i32")) && digits > (u32::MAX as INT) {
                return Ok(x);
            }
        }

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        Ok(x.round_dp_with_strategy(digits as u32, RoundingStrategy::AwayFromZero))
    }
    /// Round the decimal number to the specified number of `digits` after the decimal point and return it.
    /// Always round towards zero.
    #[rhai_fn(return_raw)]
    pub fn round_down(x: Decimal, digits: INT) -> RhaiResultOf<Decimal> {
        if cfg!(not(feature = "unchecked")) {
            if digits < 0 {
                return Err(make_err(format!(
                    "Invalid number of digits for rounding: {digits}"
                )));
            }
            if cfg!(not(feature = "only_i32")) && digits > (u32::MAX as INT) {
                return Ok(x);
            }
        }

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        Ok(x.round_dp_with_strategy(digits as u32, RoundingStrategy::ToZero))
    }
    /// Round the decimal number to the specified number of `digits` after the decimal point and return it.
    /// Always round mid-points away from zero.
    #[rhai_fn(return_raw)]
    pub fn round_half_up(x: Decimal, digits: INT) -> RhaiResultOf<Decimal> {
        if cfg!(not(feature = "unchecked")) {
            if digits < 0 {
                return Err(make_err(format!(
                    "Invalid number of digits for rounding: {digits}"
                )));
            }
            if cfg!(not(feature = "only_i32")) && digits > (u32::MAX as INT) {
                return Ok(x);
            }
        }

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        Ok(x.round_dp_with_strategy(digits as u32, RoundingStrategy::MidpointAwayFromZero))
    }
    /// Round the decimal number to the specified number of `digits` after the decimal point and return it.
    /// Always round mid-points towards zero.
    #[rhai_fn(return_raw)]
    pub fn round_half_down(x: Decimal, digits: INT) -> RhaiResultOf<Decimal> {
        if cfg!(not(feature = "unchecked")) {
            if digits < 0 {
                return Err(make_err(format!(
                    "Invalid number of digits for rounding: {digits}"
                )));
            }
            if cfg!(not(feature = "only_i32")) && digits > (u32::MAX as INT) {
                return Ok(x);
            }
        }

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        Ok(x.round_dp_with_strategy(digits as u32, RoundingStrategy::MidpointTowardZero))
    }
    /// Convert the decimal number into an integer.
    #[rhai_fn(return_raw)]
    pub fn to_int(x: Decimal) -> RhaiResultOf<INT> {
        #[allow(clippy::bind_instead_of_map)]
        x.to_i64()
            .and_then(|n| {
                #[cfg(feature = "only_i32")]
                return if n > (INT::MAX as i64) || n < (INT::MIN as i64) {
                    None
                } else {
                    Some(n as i32)
                };

                #[cfg(not(feature = "only_i32"))]
                return Some(n);
            })
            .map_or_else(
                || {
                    Err(ERR::ErrorArithmetic(
                        format!("Integer overflow: to_int({x})"),
                        Position::NONE,
                    )
                    .into())
                },
                Ok,
            )
    }
    /// Return the integral part of the decimal number.
    #[rhai_fn(name = "int", get = "int")]
    pub fn int(x: Decimal) -> Decimal {
        x.trunc()
    }
    /// Return the fractional part of the decimal number.
    #[rhai_fn(name = "fraction", get = "fraction")]
    pub fn fraction(x: Decimal) -> Decimal {
        x.fract()
    }
    /// Parse a string into a decimal number.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = parse_decimal("123.456");
    ///
    /// print(x);       // prints 123.456
    /// ```
    #[rhai_fn(return_raw)]
    pub fn parse_decimal(string: &str) -> RhaiResultOf<Decimal> {
        Decimal::from_str(string)
            .or_else(|_| Decimal::from_scientific(string))
            .map_err(|err| {
                ERR::ErrorArithmetic(
                    format!("Error parsing decimal number '{string}': {err}"),
                    Position::NONE,
                )
                .into()
            })
    }

    /// Convert the floating-point number to decimal.
    #[cfg(not(feature = "no_float"))]
    #[rhai_fn(name = "to_decimal", return_raw)]
    pub fn f32_to_decimal(x: f32) -> RhaiResultOf<Decimal> {
        Decimal::try_from(x).map_err(|_| {
            ERR::ErrorArithmetic(
                format!("Cannot convert to Decimal: to_decimal({x})"),
                Position::NONE,
            )
            .into()
        })
    }
    /// Convert the floating-point number to decimal.
    #[cfg(not(feature = "no_float"))]
    #[rhai_fn(name = "to_decimal", return_raw)]
    pub fn f64_to_decimal(x: f64) -> RhaiResultOf<Decimal> {
        Decimal::try_from(x).map_err(|_| {
            ERR::ErrorArithmetic(
                format!("Cannot convert to Decimal: to_decimal({x})"),
                Position::NONE,
            )
            .into()
        })
    }
    /// Convert the decimal number to floating-point.
    #[cfg(not(feature = "no_float"))]
    #[rhai_fn(return_raw)]
    pub fn to_float(x: Decimal) -> RhaiResultOf<FLOAT> {
        FLOAT::try_from(x).map_err(|_| {
            ERR::ErrorArithmetic(
                format!("Cannot convert to floating-point: to_float({x})"),
                Position::NONE,
            )
            .into()
        })
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
#[cfg(not(target_family = "wasm"))]

gen_conversion_as_functions!(num_128_to_float => to_float (i128, u128) -> FLOAT);

gen_conversion_as_functions!(basic_to_int => to_int (char) -> INT);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_conversion_as_functions!(numbers_to_int => to_int (i8, u8, i16, u16, i32, u32, i64, u64) -> INT);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(target_family = "wasm"))]

gen_conversion_as_functions!(num_128_to_int => to_int (i128, u128) -> INT);

#[cfg(feature = "decimal")]
gen_conversion_into_functions!(basic_to_decimal => to_decimal (INT) -> Decimal);

#[cfg(feature = "decimal")]
#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_conversion_into_functions!(numbers_to_decimal => to_decimal (i8, u8, i16, u16, i32, u32, i64, u64) -> Decimal);
