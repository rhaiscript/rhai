#![allow(non_snake_case)]

use crate::module::ModuleFlags;
use crate::plugin::*;
use crate::{def_package, Position, RhaiError, RhaiResultOf, ERR, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(feature = "no_std")]
#[cfg(not(feature = "no_float"))]
use num_traits::Float;

#[cold]
#[inline(never)]
pub fn make_err(msg: impl Into<String>) -> RhaiError {
    ERR::ErrorArithmetic(msg.into(), Position::NONE).into()
}

macro_rules! gen_arithmetic_functions {
    ($root:ident => $($arg_type:ident),+) => {
        pub mod $root { $(pub mod $arg_type {
            use super::super::*;

            #[export_module]
            pub mod functions {
                #[rhai_fn(name = "+", return_raw)]
                pub fn add(x: $arg_type, y: $arg_type) -> RhaiResultOf<$arg_type> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_add(y).ok_or_else(|| make_err(format!("Addition overflow: {x} + {y}")))
                    } else {
                        Ok(x + y)
                    }
                }
                #[rhai_fn(name = "-", return_raw)]
                pub fn subtract(x: $arg_type, y: $arg_type) -> RhaiResultOf<$arg_type> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_sub(y).ok_or_else(|| make_err(format!("Subtraction overflow: {x} - {y}")))
                    } else {
                        Ok(x - y)
                    }
                }
                #[rhai_fn(name = "*", return_raw)]
                pub fn multiply(x: $arg_type, y: $arg_type) -> RhaiResultOf<$arg_type> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_mul(y).ok_or_else(|| make_err(format!("Multiplication overflow: {x} * {y}")))
                    } else {
                        Ok(x * y)
                    }
                }
                #[rhai_fn(name = "/", return_raw)]
                pub fn divide(x: $arg_type, y: $arg_type) -> RhaiResultOf<$arg_type> {
                    if cfg!(not(feature = "unchecked")) {
                        // Detect division by zero
                        if y == 0 {
                            Err(make_err(format!("Division by zero: {x} / {y}")))
                        } else {
                            x.checked_div(y).ok_or_else(|| make_err(format!("Division overflow: {x} / {y}")))
                        }
                    } else {
                        Ok(x / y)
                    }
                }
                #[rhai_fn(name = "%", return_raw)]
                pub fn modulo(x: $arg_type, y: $arg_type) -> RhaiResultOf<$arg_type> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_rem(y).ok_or_else(|| make_err(format!("Modulo division by zero or overflow: {x} % {y}")))
                    } else {
                        Ok(x % y)
                    }
                }
                #[rhai_fn(name = "**", return_raw)]
                pub fn power(x: $arg_type, y: INT) -> RhaiResultOf<$arg_type> {
                    if cfg!(not(feature = "unchecked")) {
                        if cfg!(not(feature = "only_i32")) && y > (u32::MAX as INT) {
                            Err(make_err(format!("Exponential overflow: {x} ** {y}")))
                        } else if y < 0 {
                            Err(make_err(format!("Integer raised to a negative power: {x} ** {y}")))
                        } else {
                            x.checked_pow(y as u32).ok_or_else(|| make_err(format!("Exponential overflow: {x} ** {y}")))
                        }
                    } else {
                        Ok(x.pow(y as u32))
                    }
                }

                #[rhai_fn(name = "<<")]
                pub fn shift_left(x: $arg_type, y: INT) -> $arg_type {
                    if cfg!(not(feature = "unchecked")) {
                        if cfg!(not(feature = "only_i32")) && y > (u32::MAX as INT) {
                            0
                        } else if y < 0 {
                            shift_right(x, y.checked_abs().unwrap_or(INT::MAX))
                        } else {
                            x.checked_shl(y as u32).unwrap_or_else(|| 0)
                        }
                    } else if y < 0 {
                        x >> -y
                    } else {
                        x << y
                    }
                }
                #[rhai_fn(name = ">>")]
                pub fn shift_right(x: $arg_type, y: INT) -> $arg_type {
                    if cfg!(not(feature = "unchecked")) {
                        if cfg!(not(feature = "only_i32")) && y > (u32::MAX as INT) {
                            x.wrapping_shr(u32::MAX)
                        } else if y < 0 {
                            shift_left(x, y.checked_abs().unwrap_or(INT::MAX))
                        } else {
                            x.checked_shr(y as u32).unwrap_or_else(|| x.wrapping_shr(u32::MAX))
                        }
                    } else if y < 0 {
                        x << -y
                    } else {
                        x >> y
                    }
                }
                #[rhai_fn(name = "&")]
                pub const fn binary_and(x: $arg_type, y: $arg_type) -> $arg_type {
                    x & y
                }
                #[rhai_fn(name = "|")]
                pub const fn binary_or(x: $arg_type, y: $arg_type) -> $arg_type {
                    x | y
                }
                #[rhai_fn(name = "^")]
                pub const fn binary_xor(x: $arg_type, y: $arg_type) -> $arg_type {
                    x ^ y
                }
                /// Return true if the number is zero.
                #[rhai_fn(get = "is_zero", name = "is_zero")]
                pub const fn is_zero(x: $arg_type) -> bool {
                    x == 0
                }
                /// Return true if the number is odd.
                #[rhai_fn(get = "is_odd", name = "is_odd")]
                pub const fn is_odd(x: $arg_type) -> bool {
                    x % 2 != 0
                }
                /// Return true if the number is even.
                #[rhai_fn(get = "is_even", name = "is_even")]
                pub const fn is_even(x: $arg_type) -> bool {
                    x % 2 == 0
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
                pub fn neg(x: $arg_type) -> RhaiResultOf<$arg_type> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_neg().ok_or_else(|| make_err(format!("Negation overflow: -{x}")))
                    } else {
                        Ok(-x)
                    }
                }
                #[rhai_fn(name = "+")]
                pub const fn plus(x: $arg_type) -> $arg_type {
                    x
                }
                /// Return the absolute value of the number.
                #[rhai_fn(return_raw)]
                pub fn abs(x: $arg_type) -> RhaiResultOf<$arg_type> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_abs().ok_or_else(|| make_err(format!("Negation overflow: -{x}")))
                    } else {
                        Ok(x.abs())
                    }
                }
                /// Return the sign (as an integer) of the number according to the following:
                ///
                /// * `0` if the number is zero
                /// * `1` if the number is positive
                /// * `-1` if the number is negative
                pub const fn sign(x: $arg_type) -> INT {
                    x.signum() as INT
                }
            }
        })* }
    }
}

macro_rules! reg_functions {
    ($mod_name:ident += $root:ident ; $($arg_type:ident),+ ) => { $(
        combine_with_exported_module!($mod_name, "arithmetic", $root::$arg_type::functions);
    )* }
}

def_package! {
    /// Basic arithmetic package.
    pub ArithmeticPackage(lib) {
        lib.flags |= ModuleFlags::STANDARD_LIB;

        combine_with_exported_module!(lib, "int", int_functions);
        reg_functions!(lib += signed_basic; INT);

        #[cfg(not(feature = "only_i32"))]
        #[cfg(not(feature = "only_i64"))]
        {
            reg_functions!(lib += arith_numbers; i8, u8, i16, u16, i32, u32, u64);
            reg_functions!(lib += signed_numbers; i8, i16, i32);

            #[cfg(not(target_family = "wasm"))]
            {
                reg_functions!(lib += arith_num_128; i128, u128);
                reg_functions!(lib += signed_num_128; i128);
            }
        }

        // Basic arithmetic for floating-point
        #[cfg(not(feature = "no_float"))]
        {
            combine_with_exported_module!(lib, "f32", f32_functions);
            combine_with_exported_module!(lib, "f64", f64_functions);
        }

        // Decimal functions
        #[cfg(feature = "decimal")]
        combine_with_exported_module!(lib, "decimal", decimal_functions);
    }
}

#[export_module]
mod int_functions {
    /// Return true if the number is zero.
    #[rhai_fn(get = "is_zero", name = "is_zero")]
    pub const fn is_zero(x: INT) -> bool {
        x == 0
    }
    /// Return true if the number is odd.
    #[rhai_fn(get = "is_odd", name = "is_odd")]
    pub const fn is_odd(x: INT) -> bool {
        x % 2 != 0
    }
    /// Return true if the number is even.
    #[rhai_fn(get = "is_even", name = "is_even")]
    pub const fn is_even(x: INT) -> bool {
        x % 2 == 0
    }
}

gen_arithmetic_functions!(arith_basic => INT);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_arithmetic_functions!(arith_numbers => i8, u8, i16, u16, i32, u32, u64);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(target_family = "wasm"))]

gen_arithmetic_functions!(arith_num_128 => i128, u128);

gen_signed_functions!(signed_basic => INT);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_signed_functions!(signed_numbers => i8, i16, i32);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(target_family = "wasm"))]

gen_signed_functions!(signed_num_128 => i128);

#[cfg(not(feature = "no_float"))]
#[export_module]
mod f32_functions {
    #[cfg(not(feature = "f32_float"))]
    #[allow(clippy::cast_precision_loss)]
    pub mod basic_arithmetic {
        #[rhai_fn(name = "+")]
        pub fn add(x: f32, y: f32) -> f32 {
            x + y
        }
        #[rhai_fn(name = "-")]
        pub fn subtract(x: f32, y: f32) -> f32 {
            x - y
        }
        #[rhai_fn(name = "*")]
        pub fn multiply(x: f32, y: f32) -> f32 {
            x * y
        }
        #[rhai_fn(name = "/")]
        pub fn divide(x: f32, y: f32) -> f32 {
            x / y
        }
        #[rhai_fn(name = "%")]
        pub fn modulo(x: f32, y: f32) -> f32 {
            x % y
        }
        #[rhai_fn(name = "**")]
        pub fn pow_f_f(x: f32, y: f32) -> f32 {
            x.powf(y)
        }

        #[rhai_fn(name = "+")]
        pub fn add_if(x: INT, y: f32) -> f32 {
            (x as f32) + y
        }
        #[rhai_fn(name = "+")]
        pub fn add_fi(x: f32, y: INT) -> f32 {
            x + (y as f32)
        }
        #[rhai_fn(name = "-")]
        pub fn subtract_if(x: INT, y: f32) -> f32 {
            (x as f32) - y
        }
        #[rhai_fn(name = "-")]
        pub fn subtract_fi(x: f32, y: INT) -> f32 {
            x - (y as f32)
        }
        #[rhai_fn(name = "*")]
        pub fn multiply_if(x: INT, y: f32) -> f32 {
            (x as f32) * y
        }
        #[rhai_fn(name = "*")]
        pub fn multiply_fi(x: f32, y: INT) -> f32 {
            x * (y as f32)
        }
        #[rhai_fn(name = "/")]
        pub fn divide_if(x: INT, y: f32) -> f32 {
            (x as f32) / y
        }
        #[rhai_fn(name = "/")]
        pub fn divide_fi(x: f32, y: INT) -> f32 {
            x / (y as f32)
        }
        #[rhai_fn(name = "%")]
        pub fn modulo_if(x: INT, y: f32) -> f32 {
            (x as f32) % y
        }
        #[rhai_fn(name = "%")]
        pub fn modulo_fi(x: f32, y: INT) -> f32 {
            x % (y as f32)
        }
    }

    #[rhai_fn(name = "-")]
    pub fn neg(x: f32) -> f32 {
        -x
    }
    #[rhai_fn(name = "+")]
    pub const fn plus(x: f32) -> f32 {
        x
    }
    /// Return the absolute value of the floating-point number.
    pub fn abs(x: f32) -> f32 {
        x.abs()
    }
    /// Return the sign (as an integer) of the floating-point number according to the following:
    ///
    /// * `0` if the number is zero
    /// * `1` if the number is positive
    /// * `-1` if the number is negative
    #[rhai_fn(return_raw)]
    pub fn sign(x: f32) -> RhaiResultOf<INT> {
        match x.signum() {
            _ if x == 0.0 => Ok(0),
            x if x.is_nan() => Err(make_err("Sign of NaN is undefined")),
            x => Ok(x as INT),
        }
    }
    /// Return true if the floating-point number is zero.
    #[rhai_fn(get = "is_zero", name = "is_zero")]
    pub fn is_zero(x: f32) -> bool {
        x == 0.0
    }
    #[rhai_fn(name = "**", return_raw)]
    pub fn pow_f_i(x: f32, y: INT) -> RhaiResultOf<f32> {
        if cfg!(not(feature = "unchecked")) && y > (i32::MAX as INT) {
            Err(make_err(format!(
                "Number raised to too large an index: {x} ** {y}"
            )))
        } else {
            #[allow(clippy::cast_possible_truncation, clippy::unnecessary_cast)]
            Ok(x.powi(y as i32))
        }
    }
}

#[cfg(not(feature = "no_float"))]
#[export_module]
mod f64_functions {
    #[cfg(feature = "f32_float")]
    pub mod basic_arithmetic {
        #[rhai_fn(name = "+")]
        pub fn add(x: f64, y: f64) -> f64 {
            x + y
        }
        #[rhai_fn(name = "-")]
        pub fn subtract(x: f64, y: f64) -> f64 {
            x - y
        }
        #[rhai_fn(name = "*")]
        pub fn multiply(x: f64, y: f64) -> f64 {
            x * y
        }
        #[rhai_fn(name = "/")]
        pub fn divide(x: f64, y: f64) -> f64 {
            x / y
        }
        #[rhai_fn(name = "%")]
        pub fn modulo(x: f64, y: f64) -> f64 {
            x % y
        }
        #[rhai_fn(name = "**")]
        pub fn pow_f_f(x: f64, y: f64) -> f64 {
            x.powf(y)
        }

        #[rhai_fn(name = "+")]
        pub fn add_if(x: INT, y: f64) -> f64 {
            (x as f64) + y
        }
        #[rhai_fn(name = "+")]
        pub fn add_fi(x: f64, y: INT) -> f64 {
            x + (y as f64)
        }
        #[rhai_fn(name = "-")]
        pub fn subtract_if(x: INT, y: f64) -> f64 {
            (x as f64) - y
        }
        #[rhai_fn(name = "-")]
        pub fn subtract_fi(x: f64, y: INT) -> f64 {
            x - (y as f64)
        }
        #[rhai_fn(name = "*")]
        pub fn multiply_if(x: INT, y: f64) -> f64 {
            (x as f64) * y
        }
        #[rhai_fn(name = "*")]
        pub fn multiply_fi(x: f64, y: INT) -> f64 {
            x * (y as f64)
        }
        #[rhai_fn(name = "/")]
        pub fn divide_if(x: INT, y: f64) -> f64 {
            (x as f64) / y
        }
        #[rhai_fn(name = "/")]
        pub fn divide_fi(x: f64, y: INT) -> f64 {
            x / (y as f64)
        }
        #[rhai_fn(name = "%")]
        pub fn modulo_if(x: INT, y: f64) -> f64 {
            (x as f64) % y
        }
        #[rhai_fn(name = "%")]
        pub fn modulo_fi(x: f64, y: INT) -> f64 {
            x % (y as f64)
        }
    }

    #[rhai_fn(name = "-")]
    pub fn neg(x: f64) -> f64 {
        -x
    }
    #[rhai_fn(name = "+")]
    pub const fn plus(x: f64) -> f64 {
        x
    }
    /// Return the absolute value of the floating-point number.
    pub fn abs(x: f64) -> f64 {
        x.abs()
    }
    /// Return the sign (as an integer) of the floating-point number according to the following:
    ///
    /// * `0` if the number is zero
    /// * `1` if the number is positive
    /// * `-1` if the number is negative
    #[rhai_fn(return_raw)]
    pub fn sign(x: f64) -> RhaiResultOf<INT> {
        match x.signum() {
            _ if x == 0.0 => Ok(0),
            x if x.is_nan() => Err(make_err("Sign of NaN is undefined")),
            x => Ok(x as INT),
        }
    }
    /// Return true if the floating-point number is zero.
    #[rhai_fn(get = "is_zero", name = "is_zero")]
    pub fn is_zero(x: f64) -> bool {
        x == 0.0
    }
}

#[cfg(feature = "decimal")]
#[export_module]
pub mod decimal_functions {
    use rust_decimal::{prelude::Zero, Decimal};

    #[cfg(not(feature = "unchecked"))]
    pub mod builtin {
        use rust_decimal::MathematicalOps;

        #[rhai_fn(return_raw)]
        pub fn add(x: Decimal, y: Decimal) -> RhaiResultOf<Decimal> {
            x.checked_add(y)
                .ok_or_else(|| make_err(format!("Addition overflow: {x} + {y}")))
        }
        #[rhai_fn(return_raw)]
        pub fn subtract(x: Decimal, y: Decimal) -> RhaiResultOf<Decimal> {
            x.checked_sub(y)
                .ok_or_else(|| make_err(format!("Subtraction overflow: {x} - {y}")))
        }
        #[rhai_fn(return_raw)]
        pub fn multiply(x: Decimal, y: Decimal) -> RhaiResultOf<Decimal> {
            x.checked_mul(y)
                .ok_or_else(|| make_err(format!("Multiplication overflow: {x} * {y}")))
        }
        #[rhai_fn(return_raw)]
        pub fn divide(x: Decimal, y: Decimal) -> RhaiResultOf<Decimal> {
            // Detect division by zero
            if y == Decimal::zero() {
                Err(make_err(format!("Division by zero: {x} / {y}")))
            } else {
                x.checked_div(y)
                    .ok_or_else(|| make_err(format!("Division overflow: {x} / {y}")))
            }
        }
        #[rhai_fn(return_raw)]
        pub fn modulo(x: Decimal, y: Decimal) -> RhaiResultOf<Decimal> {
            x.checked_rem(y)
                .ok_or_else(|| make_err(format!("Modulo division by zero or overflow: {x} % {y}")))
        }
        #[rhai_fn(return_raw)]
        pub fn power(x: Decimal, y: Decimal) -> RhaiResultOf<Decimal> {
            // Raising to a very large power can take exponential time, so limit it to 1 million.
            // TODO: Remove this limit when `rust-decimal` is updated with the fix.
            if std::convert::TryInto::<u32>::try_into(y.round()).map_or(true, |v| v > 1000000) {
                return Err(make_err(format!("Exponential overflow: {x} ** {y}")));
            }
            x.checked_powd(y)
                .ok_or_else(|| make_err(format!("Exponential overflow: {x} ** {y}")))
        }
    }
    #[rhai_fn(name = "-")]
    pub fn neg(x: Decimal) -> Decimal {
        -x
    }
    #[rhai_fn(name = "+")]
    pub const fn plus(x: Decimal) -> Decimal {
        x
    }
    /// Return the absolute value of the decimal number.
    pub fn abs(x: Decimal) -> Decimal {
        x.abs()
    }
    /// Return the sign (as an integer) of the decimal number according to the following:
    ///
    /// * `0` if the number is zero
    /// * `1` if the number is positive
    /// * `-1` if the number is negative
    pub fn sign(x: Decimal) -> INT {
        if x == Decimal::zero() {
            0
        } else if x.is_sign_negative() {
            -1
        } else {
            1
        }
    }
    /// Return true if the decimal number is zero.
    #[rhai_fn(get = "is_zero", name = "is_zero")]
    pub const fn is_zero(x: Decimal) -> bool {
        x.is_zero()
    }
}
