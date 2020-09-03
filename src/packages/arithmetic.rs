#![allow(non_snake_case)]

use crate::def_package;
use crate::parser::INT;
use crate::plugin::*;

use crate::{result::EvalAltResult, token::Position};

#[cfg(not(feature = "no_float"))]
use crate::parser::FLOAT;

#[cfg(feature = "no_std")]
#[cfg(not(feature = "no_float"))]
use num_traits::float::Float;

#[cfg(feature = "decimal")]
use rust_decimal::prelude::*;

use crate::stdlib::{format, string::String};

#[inline(always)]
pub fn make_err(msg: String) -> Box<EvalAltResult> {
    EvalAltResult::ErrorArithmetic(msg, Position::none()).into()
}

macro_rules! gen_arithmetic_functions {
    ($root:ident => $($arg_type:ident),+) => {
        pub mod $root { $(pub mod $arg_type {
            use super::super::*;

            #[export_module]
            pub mod functions {
                #[rhai_fn(name = "+", return_raw)]
                #[inline]
                pub fn add(x: $arg_type, y: $arg_type) -> Result<Dynamic, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_add(y).ok_or_else(|| make_err(format!("Addition overflow: {} + {}", x, y))).map(Dynamic::from)
                    } else {
                        Ok(Dynamic::from(x + y))
                    }
                }
                #[rhai_fn(name = "-", return_raw)]
                #[inline]
                pub fn subtract(x: $arg_type, y: $arg_type) -> Result<Dynamic, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_sub(y).ok_or_else(|| make_err(format!("Subtraction overflow: {} - {}", x, y))).map(Dynamic::from)
                    } else {
                        Ok(Dynamic::from(x - y))
                    }
                }
                #[rhai_fn(name = "*", return_raw)]
                #[inline]
                pub fn multiply(x: $arg_type, y: $arg_type) -> Result<Dynamic, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_mul(y).ok_or_else(|| make_err(format!("Multiplication overflow: {} * {}", x, y))).map(Dynamic::from)
                    } else {
                        Ok(Dynamic::from(x * y))
                    }
                }
                #[rhai_fn(name = "/", return_raw)]
                #[inline]
                pub fn divide(x: $arg_type, y: $arg_type) -> Result<Dynamic, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        // Detect division by zero
                        if y == 0 {
                            Err(make_err(format!("Division by zero: {} / {}", x, y)))
                        } else {
                            x.checked_div(y).ok_or_else(|| make_err(format!("Division overflow: {} / {}", x, y))).map(Dynamic::from)
                        }
                    } else {
                        Ok(Dynamic::from(x / y))
                    }
                }
                #[rhai_fn(name = "%", return_raw)]
                #[inline]
                pub fn modulo(x: $arg_type, y: $arg_type) -> Result<Dynamic, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_rem(y).ok_or_else(|| make_err(format!("Modulo division by zero or overflow: {} % {}", x, y))).map(Dynamic::from)
                    } else {
                        Ok(Dynamic::from(x % y))
                    }
                }
                #[rhai_fn(name = "~", return_raw)]
                #[inline]
                pub fn power(x: INT, y: INT) -> Result<Dynamic, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        if cfg!(not(feature = "only_i32")) && y > (u32::MAX as INT) {
                            Err(make_err(format!("Integer raised to too large an index: {} ~ {}", x, y)))
                        } else if y < 0 {
                            Err(make_err(format!("Integer raised to a negative index: {} ~ {}", x, y)))
                        } else {
                            x.checked_pow(y as u32).ok_or_else(|| make_err(format!("Power overflow: {} ~ {}", x, y))).map(Dynamic::from)
                        }
                    } else {
                        Ok(Dynamic::from(x.pow(y as u32)))
                    }
                }

                #[rhai_fn(name = "<<", return_raw)]
                #[inline]
                pub fn shift_left(x: $arg_type, y: INT) -> Result<Dynamic, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        if cfg!(not(feature = "only_i32")) && y > (u32::MAX as INT) {
                            Err(make_err(format!("Left-shift by too many bits: {} << {}", x, y)))
                        } else if y < 0 {
                            Err(make_err(format!("Left-shift by a negative number: {} << {}", x, y)))
                        } else {
                            x.checked_shl(y as u32).ok_or_else(|| make_err(format!("Left-shift by too many bits: {} << {}", x, y))).map(Dynamic::from)
                        }
                    } else {
                        Ok(Dynamic::from(x << y))
                    }
                }
                #[rhai_fn(name = ">>", return_raw)]
                #[inline]
                pub fn shift_right(x: $arg_type, y: INT) -> Result<Dynamic, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        if cfg!(not(feature = "only_i32")) && y > (u32::MAX as INT) {
                            Err(make_err(format!("Right-shift by too many bits: {} >> {}", x, y)))
                        } else if y < 0 {
                            Err(make_err(format!("Right-shift by a negative number: {} >> {}", x, y)))
                        } else {
                            x.checked_shr(y as u32).ok_or_else(|| make_err(format!("Right-shift by too many bits: {} >> {}", x, y))).map(Dynamic::from)
                        }
                    } else {
                        Ok(Dynamic::from(x >> y))
                    }
                }
                #[rhai_fn(name = "&")]
                #[inline(always)]
                pub fn binary_and(x: $arg_type, y: $arg_type) -> $arg_type {
                    x & y
                }
                #[rhai_fn(name = "|")]
                #[inline(always)]
                pub fn binary_or(x: $arg_type, y: $arg_type) -> $arg_type {
                    x | y
                }
                #[rhai_fn(name = "^")]
                #[inline(always)]
                pub fn binary_xor(x: $arg_type, y: $arg_type) -> $arg_type {
                    x ^ y
                }
            }
        })* }
    }
}

macro_rules! gen_signed_functions {
    ($root:ident => $($arg_type:ident),+) => {
        pub mod $root { $(pub mod $arg_type {
            use super::super::*;

            #[export_module]
            pub mod functions {
                #[rhai_fn(name = "-", return_raw)]
                #[inline]
                pub fn neg(x: $arg_type) -> Result<Dynamic, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_neg().ok_or_else(|| make_err(format!("Negation overflow: -{}", x))).map(Dynamic::from)
                    } else {
                        Ok(Dynamic::from(-x))
                    }
                }
                #[rhai_fn(return_raw)]
                #[inline]
                pub fn abs(x: $arg_type) -> Result<Dynamic, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_abs().ok_or_else(|| make_err(format!("Negation overflow: -{}", x))).map(Dynamic::from)
                    } else {
                        Ok(Dynamic::from(x.abs()))
                    }
                }
                #[inline]
                pub fn sign(x: $arg_type) -> INT {
                    if x == 0 {
                        0
                    } else if x < 0 {
                        -1
                    } else {
                        1
                    }
                }
            }
        })* }
    }
}

macro_rules! reg_functions {
    ($mod_name:ident += $root:ident ; $($arg_type:ident),+ ) => { $(
        $mod_name.combine_flatten(exported_module!($root::$arg_type::functions));
    )* }
}

def_package!(crate:ArithmeticPackage:"Basic arithmetic", lib, {
    reg_functions!(lib += signed_basic; INT);

    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    {
        reg_functions!(lib += arith_numbers; i8, u8, i16, u16, i32, u32, u64);
        reg_functions!(lib += signed_numbers; i8, i16, i32);

        #[cfg(not(target_arch = "wasm32"))]
        {
            reg_functions!(lib += arith_num_128; i128, u128);
            reg_functions!(lib += signed_num_128; i128);
        }
    }

    // Basic arithmetic for floating-point
    #[cfg(not(feature = "no_float"))]
    {
        lib.combine_flatten(exported_module!(f32_functions));
        lib.combine_flatten(exported_module!(f64_functions));
    }

    #[cfg(feature = "decimal")]
    {
        lib.combine_flatten(exported_module!(decimal_functions));
    }
});

gen_arithmetic_functions!(arith_basic => INT);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_arithmetic_functions!(arith_numbers => i8, u8, i16, u16, i32, u32, u64);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(target_arch = "wasm32"))]
gen_arithmetic_functions!(arith_num_128 => i128, u128);

gen_signed_functions!(signed_basic => INT);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_signed_functions!(signed_numbers => i8, i16, i32);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(target_arch = "wasm32"))]
gen_signed_functions!(signed_num_128 => i128);

#[cfg(not(feature = "no_float"))]
#[export_module]
mod f32_functions {
    #[rhai_fn(name = "+")]
    #[inline(always)]
    pub fn add(x: f32, y: f32) -> f32 {
        x + y
    }
    #[rhai_fn(name = "-")]
    #[inline(always)]
    pub fn subtract(x: f32, y: f32) -> f32 {
        x - y
    }
    #[rhai_fn(name = "*")]
    #[inline(always)]
    pub fn multiply(x: f32, y: f32) -> f32 {
        x * y
    }
    #[rhai_fn(name = "/")]
    #[inline(always)]
    pub fn divide(x: f32, y: f32) -> f32 {
        x / y
    }
    #[rhai_fn(name = "%")]
    #[inline(always)]
    pub fn modulo(x: f32, y: f32) -> f32 {
        x % y
    }
    #[rhai_fn(name = "-")]
    #[inline(always)]
    pub fn neg(x: f32) -> f32 {
        -x
    }
    #[inline(always)]
    pub fn abs(x: f32) -> f32 {
        x.abs()
    }
    #[inline]
    pub fn sign(x: f32) -> INT {
        if x == 0.0 {
            0
        } else if x < 0.0 {
            -1
        } else {
            1
        }
    }
    #[rhai_fn(name = "~", return_raw)]
    #[inline(always)]
    pub fn pow_f_f(x: f32, y: f32) -> Result<Dynamic, Box<EvalAltResult>> {
        Ok(Dynamic::from(x.powf(y)))
    }
    #[rhai_fn(name = "~", return_raw)]
    #[inline]
    pub fn pow_f_i(x: f32, y: INT) -> Result<Dynamic, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) && y > (i32::MAX as INT) {
            Err(make_err(format!(
                "Number raised to too large an index: {} ~ {}",
                x, y
            )))
        } else {
            Ok(Dynamic::from(x.powi(y as i32)))
        }
    }
}

#[cfg(not(feature = "no_float"))]
#[export_module]
mod f64_functions {
    #[rhai_fn(name = "-")]
    #[inline(always)]
    pub fn neg(x: f64) -> f64 {
        -x
    }
    #[inline(always)]
    pub fn abs(x: f64) -> f64 {
        x.abs()
    }
    #[inline]
    pub fn sign(x: f64) -> INT {
        if x == 0.0 {
            0
        } else if x < 0.0 {
            -1
        } else {
            1
        }
    }
    #[rhai_fn(name = "~", return_raw)]
    #[inline]
    pub fn pow_f_i(x: FLOAT, y: INT) -> Result<Dynamic, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) && y > (i32::MAX as INT) {
            Err(make_err(format!(
                "Number raised to too large an index: {} ~ {}",
                x, y
            )))
        } else {
            Ok(x.powi(y as i32).into())
        }
    }
}

#[cfg(feature = "decimal")]
#[export_module]
mod decimal_functions {
    #[rhai_fn(name = "+")]
    #[inline(always)]
    pub fn add(x: Decimal, y: Decimal) -> Decimal {
        x + y
    }
    #[rhai_fn(name = "-")]
    #[inline(always)]
    pub fn subtract(x: Decimal, y: Decimal) -> Decimal {
        x - y
    }
    #[rhai_fn(name = "*")]
    #[inline(always)]
    pub fn multiply(x: Decimal, y: Decimal) -> Decimal {
        x * y
    }
    #[rhai_fn(name = "/")]
    #[inline(always)]
    pub fn divide(x: Decimal, y: Decimal) -> Decimal {
        x / y
    }
    #[rhai_fn(name = "%")]
    #[inline(always)]
    pub fn modulo(x: Decimal, y: Decimal) -> Decimal {
        x % y
    }
    #[rhai_fn(name = "-")]
    #[inline(always)]
    pub fn neg(x: Decimal) -> Decimal {
        -x
    }
    #[inline(always)]
    pub fn abs(x: Decimal) -> Decimal {
        x.abs()
    }
    #[inline]
    pub fn sign(x: Decimal) -> INT {
        if x == Decimal::zero() {
            0
        } else if x.is_sign_positive() {
            1
        } else {
            -1
        }
    }
}
