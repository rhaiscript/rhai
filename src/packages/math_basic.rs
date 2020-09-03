#![allow(non_snake_case)]

use crate::def_package;
use crate::parser::INT;
use crate::plugin::*;

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

macro_rules! gen_conversion_functions {
    ($root:ident => $func_name:ident ( $($arg_type:ident),+ ) -> $result_type:ty) => {
        pub mod $root { $(pub mod $arg_type {
            use super::super::*;

            #[export_fn]
            #[inline(always)]
            pub fn $func_name(x: $arg_type) -> $result_type {
                x as $result_type
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
    #[cfg(not(feature = "no_float"))]
    {
        // Floating point functions
        lib.combine_flatten(exported_module!(float_functions));

        // Trig functions
        lib.combine_flatten(exported_module!(trig_functions));

        reg_functions!(lib += basic_to_float::to_float(INT));

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            reg_functions!(lib += numbers_to_float::to_float(i8, u8, i16, u16, i32, u32, i64, u32));

            #[cfg(not(target_arch = "wasm32"))]
            reg_functions!(lib += num_128_to_float::to_float(i128, u128));
        }
    }

    #[cfg(feature = "decimal")]
    {
        lib.combine_flatten(exported_module!(decimal_functions));
    }

    reg_functions!(lib += basic_to_int::to_int(char));

    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    {
        reg_functions!(lib += numbers_to_int::to_int(i8, u8, i16, u16, i32, u32, i64, u64));

        #[cfg(not(target_arch = "wasm32"))]
        reg_functions!(lib += num_128_to_int::to_int(i128, u128));
    }
});

#[cfg(not(feature = "no_float"))]
#[export_module]
mod trig_functions {
    use crate::parser::FLOAT;

    #[inline(always)]
    pub fn sin(x: FLOAT) -> FLOAT {
        x.to_radians().sin()
    }
    #[inline(always)]
    pub fn cos(x: FLOAT) -> FLOAT {
        x.to_radians().cos()
    }
    #[inline(always)]
    pub fn tan(x: FLOAT) -> FLOAT {
        x.to_radians().tan()
    }
    #[inline(always)]
    pub fn sinh(x: FLOAT) -> FLOAT {
        x.to_radians().sinh()
    }
    #[inline(always)]
    pub fn cosh(x: FLOAT) -> FLOAT {
        x.to_radians().cosh()
    }
    #[inline(always)]
    pub fn tanh(x: FLOAT) -> FLOAT {
        x.to_radians().tanh()
    }
    #[inline(always)]
    pub fn asin(x: FLOAT) -> FLOAT {
        x.asin().to_degrees()
    }
    #[inline(always)]
    pub fn acos(x: FLOAT) -> FLOAT {
        x.acos().to_degrees()
    }
    #[inline(always)]
    pub fn atan(x: FLOAT) -> FLOAT {
        x.atan().to_degrees()
    }
    #[inline(always)]
    pub fn asinh(x: FLOAT) -> FLOAT {
        x.asinh().to_degrees()
    }
    #[inline(always)]
    pub fn acosh(x: FLOAT) -> FLOAT {
        x.acosh().to_degrees()
    }
    #[inline(always)]
    pub fn atanh(x: FLOAT) -> FLOAT {
        x.atanh().to_degrees()
    }
}

#[cfg(not(feature = "no_float"))]
#[export_module]
mod float_functions {
    use crate::parser::FLOAT;

    #[inline(always)]
    pub fn sqrt(x: FLOAT) -> FLOAT {
        x.sqrt()
    }
    #[inline(always)]
    pub fn exp(x: FLOAT) -> FLOAT {
        x.exp()
    }
    #[inline(always)]
    pub fn ln(x: FLOAT) -> FLOAT {
        x.ln()
    }
    #[inline(always)]
    pub fn log(x: FLOAT, base: FLOAT) -> FLOAT {
        x.log(base)
    }
    #[inline(always)]
    pub fn log10(x: FLOAT) -> FLOAT {
        x.log10()
    }
    #[inline(always)]
    pub fn floor(x: FLOAT) -> FLOAT {
        x.floor()
    }
    #[rhai_fn(get = "floor")]
    #[inline(always)]
    pub fn floor_prop(x: FLOAT) -> FLOAT {
        floor(x)
    }
    #[inline(always)]
    pub fn ceiling(x: FLOAT) -> FLOAT {
        x.ceil()
    }
    #[rhai_fn(get = "ceiling")]
    #[inline(always)]
    pub fn ceiling_prop(x: FLOAT) -> FLOAT {
        ceiling(x)
    }
    #[inline(always)]
    pub fn round(x: FLOAT) -> FLOAT {
        x.ceil()
    }
    #[rhai_fn(get = "round")]
    #[inline(always)]
    pub fn round_prop(x: FLOAT) -> FLOAT {
        ceiling(x)
    }
    #[inline(always)]
    pub fn int(x: FLOAT) -> FLOAT {
        x.trunc()
    }
    #[rhai_fn(get = "int")]
    #[inline(always)]
    pub fn int_prop(x: FLOAT) -> FLOAT {
        int(x)
    }
    #[inline(always)]
    pub fn fraction(x: FLOAT) -> FLOAT {
        x.fract()
    }
    #[rhai_fn(get = "fraction")]
    #[inline(always)]
    pub fn fraction_prop(x: FLOAT) -> FLOAT {
        fraction(x)
    }
    #[inline(always)]
    pub fn is_nan(x: FLOAT) -> bool {
        x.is_nan()
    }
    #[rhai_fn(get = "is_nan")]
    #[inline(always)]
    pub fn is_nan_prop(x: FLOAT) -> bool {
        is_nan(x)
    }
    #[inline(always)]
    pub fn is_finite(x: FLOAT) -> bool {
        x.is_finite()
    }
    #[rhai_fn(get = "is_finite")]
    #[inline(always)]
    pub fn is_finite_prop(x: FLOAT) -> bool {
        is_finite(x)
    }
    #[inline(always)]
    pub fn is_infinite(x: FLOAT) -> bool {
        x.is_infinite()
    }
    #[rhai_fn(get = "is_infinite")]
    #[inline(always)]
    pub fn is_infinite_prop(x: FLOAT) -> bool {
        is_infinite(x)
    }
    #[rhai_fn(name = "to_int", return_raw)]
    #[inline]
    pub fn f32_to_int(x: f32) -> Result<Dynamic, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) && x > (MAX_INT as f32) {
            EvalAltResult::ErrorArithmetic(
                format!("Integer overflow: to_int({})", x),
                Position::none(),
            )
            .into()
        } else {
            Ok((x.trunc() as INT).into())
        }
    }
    #[rhai_fn(name = "to_int", return_raw)]
    #[inline]
    pub fn f64_to_int(x: f64) -> Result<Dynamic, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) && x > (MAX_INT as f64) {
            EvalAltResult::ErrorArithmetic(
                format!("Integer overflow: to_int({})", x),
                Position::none(),
            )
            .into()
        } else {
            Ok((x.trunc() as INT).into())
        }
    }
}

#[cfg(feature = "decimal")]
#[export_module]
mod decimal_functions {
    #[inline(always)]
    pub fn floor(x: Decimal) -> Decimal {
        x.floor()
    }
    #[inline(always)]
    pub fn ceiling(x: Decimal) -> Decimal {
        x.ceil()
    }
    #[rhai_fn(get = "ceiling")]
    #[inline(always)]
    pub fn ceiling_prop(x: Decimal) -> Decimal {
        ceiling(x)
    }
    #[inline(always)]
    pub fn round(x: Decimal) -> Decimal {
        x.ceil()
    }
    #[rhai_fn(get = "round")]
    #[inline(always)]
    pub fn round_dp(x: Decimal, dp: INT) -> Decimal {
        x.round_dp(dp as u32)
    }
    #[inline(always)]
    pub fn int(x: Decimal) -> Decimal {
        x.trunc()
    }
    #[inline(always)]
    pub fn fraction(x: Decimal) -> Decimal {
        x.fract()
    }
    #[inline(always)]
    pub fn scale(x: Decimal) -> INT {
        x.scale() as INT
    }
}

#[cfg(not(feature = "no_float"))]
gen_conversion_functions!(basic_to_float => to_float (INT) -> FLOAT);

#[cfg(not(feature = "no_float"))]
#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_conversion_functions!(numbers_to_float => to_float (i8, u8, i16, u16, i32, u32, i64, u64) -> FLOAT);

#[cfg(not(feature = "no_float"))]
#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(target_arch = "wasm32"))]
gen_conversion_functions!(num_128_to_float => to_float (i128, u128) -> FLOAT);

gen_conversion_functions!(basic_to_int => to_int (char) -> INT);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_conversion_functions!(numbers_to_int => to_int (i8, u8, i16, u16, i32, u32, i64, u64) -> INT);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(target_arch = "wasm32"))]
gen_conversion_functions!(num_128_to_int => to_int (i128, u128) -> INT);
