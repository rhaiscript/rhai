#![allow(non_snake_case)]

use crate::plugin::*;
use crate::{def_package, EvalAltResult, Position, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(feature = "no_std")]
#[cfg(not(feature = "no_float"))]
use num_traits::Float;

#[inline(always)]
pub fn make_err(msg: impl Into<String>) -> Box<EvalAltResult> {
    EvalAltResult::ErrorArithmetic(msg.into(), Position::NONE).into()
}

macro_rules! gen_arithmetic_functions {
    ($root:ident => $($arg_type:ident),+) => {
        pub mod $root { $(pub mod $arg_type {
            use super::super::*;

            #[export_module]
            pub mod functions {
                #[rhai_fn(name = "+", return_raw)]
                pub fn add(x: $arg_type, y: $arg_type) -> Result<$arg_type, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_add(y).ok_or_else(|| make_err(format!("Addition overflow: {} + {}", x, y)))
                    } else {
                        Ok(x + y)
                    }
                }
                #[rhai_fn(name = "-", return_raw)]
                pub fn subtract(x: $arg_type, y: $arg_type) -> Result<$arg_type, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_sub(y).ok_or_else(|| make_err(format!("Subtraction overflow: {} - {}", x, y)))
                    } else {
                        Ok(x - y)
                    }
                }
                #[rhai_fn(name = "*", return_raw)]
                pub fn multiply(x: $arg_type, y: $arg_type) -> Result<$arg_type, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_mul(y).ok_or_else(|| make_err(format!("Multiplication overflow: {} * {}", x, y)))
                    } else {
                        Ok(x * y)
                    }
                }
                #[rhai_fn(name = "/", return_raw)]
                pub fn divide(x: $arg_type, y: $arg_type) -> Result<$arg_type, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        // Detect division by zero
                        if y == 0 {
                            Err(make_err(format!("Division by zero: {} / {}", x, y)))
                        } else {
                            x.checked_div(y).ok_or_else(|| make_err(format!("Division overflow: {} / {}", x, y)))
                        }
                    } else {
                        Ok(x / y)
                    }
                }
                #[rhai_fn(name = "%", return_raw)]
                pub fn modulo(x: $arg_type, y: $arg_type) -> Result<$arg_type, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_rem(y).ok_or_else(|| make_err(format!("Modulo division by zero or overflow: {} % {}", x, y)))
                    } else {
                        Ok(x % y)
                    }
                }
                #[rhai_fn(name = "**", return_raw)]
                pub fn power(x: $arg_type, y: INT) -> Result<$arg_type, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        if cfg!(not(feature = "only_i32")) && y > (u32::MAX as INT) {
                            Err(make_err(format!("Integer raised to too large an index: {} ~ {}", x, y)))
                        } else if y < 0 {
                            Err(make_err(format!("Integer raised to a negative index: {} ~ {}", x, y)))
                        } else {
                            x.checked_pow(y as u32).ok_or_else(|| make_err(format!("Exponential overflow: {} ~ {}", x, y)))
                        }
                    } else {
                        Ok(x.pow(y as u32))
                    }
                }

                #[rhai_fn(name = "<<", return_raw)]
                pub fn shift_left(x: $arg_type, y: INT) -> Result<$arg_type, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        if cfg!(not(feature = "only_i32")) && y > (u32::MAX as INT) {
                            Err(make_err(format!("Left-shift by too many bits: {} << {}", x, y)))
                        } else if y < 0 {
                            Err(make_err(format!("Left-shift by a negative number: {} << {}", x, y)))
                        } else {
                            x.checked_shl(y as u32).ok_or_else(|| make_err(format!("Left-shift by too many bits: {} << {}", x, y)))
                        }
                    } else {
                        Ok(x << y)
                    }
                }
                #[rhai_fn(name = ">>", return_raw)]
                pub fn shift_right(x: $arg_type, y: INT) -> Result<$arg_type, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        if cfg!(not(feature = "only_i32")) && y > (u32::MAX as INT) {
                            Err(make_err(format!("Right-shift by too many bits: {} >> {}", x, y)))
                        } else if y < 0 {
                            Err(make_err(format!("Right-shift by a negative number: {} >> {}", x, y)))
                        } else {
                            x.checked_shr(y as u32).ok_or_else(|| make_err(format!("Right-shift by too many bits: {} >> {}", x, y)))
                        }
                    } else {
                        Ok(x >> y)
                    }
                }
                #[rhai_fn(name = "&")]
                pub fn binary_and(x: $arg_type, y: $arg_type) -> $arg_type {
                    x & y
                }
                #[rhai_fn(name = "|")]
                pub fn binary_or(x: $arg_type, y: $arg_type) -> $arg_type {
                    x | y
                }
                #[rhai_fn(name = "^")]
                pub fn binary_xor(x: $arg_type, y: $arg_type) -> $arg_type {
                    x ^ y
                }
                pub fn is_zero(x: $arg_type) -> bool {
                    x == 0
                }
                pub fn is_odd(x: $arg_type) -> bool {
                    x % 2 != 0
                }
                pub fn is_even(x: $arg_type) -> bool {
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
                pub fn neg(x: $arg_type) -> Result<$arg_type, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_neg().ok_or_else(|| make_err(format!("Negation overflow: -{}", x)))
                    } else {
                        Ok(-x)
                    }
                }
                #[rhai_fn(name = "+")]
                pub fn plus(x: $arg_type) -> $arg_type {
                    x
                }
                #[rhai_fn(return_raw)]
                pub fn abs(x: $arg_type) -> Result<$arg_type, Box<EvalAltResult>> {
                    if cfg!(not(feature = "unchecked")) {
                        x.checked_abs().ok_or_else(|| make_err(format!("Negation overflow: -{}", x)))
                    } else {
                        Ok(x.abs())
                    }
                }
                pub fn sign(x: $arg_type) -> INT {
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

def_package!(crate:ArithmeticPackage:"Basic arithmetic", lib, {
    combine_with_exported_module!(lib, "int", int_functions);
    reg_functions!(lib += signed_basic; INT);

    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    {
        reg_functions!(lib += arith_numbers; i8, u8, i16, u16, i32, u32, u64);
        reg_functions!(lib += signed_numbers; i8, i16, i32);

        #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
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
});

#[export_module]
mod int_functions {
    pub fn is_zero(x: INT) -> bool {
        x == 0
    }
    pub fn is_odd(x: INT) -> bool {
        x % 2 != 0
    }
    pub fn is_even(x: INT) -> bool {
        x % 2 == 0
    }
}

gen_arithmetic_functions!(arith_basic => INT);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_arithmetic_functions!(arith_numbers => i8, u8, i16, u16, i32, u32, u64);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
gen_arithmetic_functions!(arith_num_128 => i128, u128);

gen_signed_functions!(signed_basic => INT);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_signed_functions!(signed_numbers => i8, i16, i32);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
gen_signed_functions!(signed_num_128 => i128);

#[cfg(not(feature = "no_float"))]
#[export_module]
mod f32_functions {
    use crate::EvalAltResult;

    #[cfg(not(feature = "f32_float"))]
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
            (x as f32) + (y as f32)
        }
        #[rhai_fn(name = "+")]
        pub fn add_fi(x: f32, y: INT) -> f32 {
            (x as f32) + (y as f32)
        }
        #[rhai_fn(name = "-")]
        pub fn subtract_if(x: INT, y: f32) -> f32 {
            (x as f32) - (y as f32)
        }
        #[rhai_fn(name = "-")]
        pub fn subtract_fi(x: f32, y: INT) -> f32 {
            (x as f32) - (y as f32)
        }
        #[rhai_fn(name = "*")]
        pub fn multiply_if(x: INT, y: f32) -> f32 {
            (x as f32) * (y as f32)
        }
        #[rhai_fn(name = "*")]
        pub fn multiply_fi(x: f32, y: INT) -> f32 {
            (x as f32) * (y as f32)
        }
        #[rhai_fn(name = "/")]
        pub fn divide_if(x: INT, y: f32) -> f32 {
            (x as f32) / (y as f32)
        }
        #[rhai_fn(name = "/")]
        pub fn divide_fi(x: f32, y: INT) -> f32 {
            (x as f32) / (y as f32)
        }
        #[rhai_fn(name = "%")]
        pub fn modulo_if(x: INT, y: f32) -> f32 {
            (x as f32) % (y as f32)
        }
        #[rhai_fn(name = "%")]
        pub fn modulo_fi(x: f32, y: INT) -> f32 {
            (x as f32) % (y as f32)
        }
    }

    #[rhai_fn(name = "-")]
    pub fn neg(x: f32) -> f32 {
        -x
    }
    #[rhai_fn(name = "+")]
    pub fn plus(x: f32) -> f32 {
        x
    }
    pub fn abs(x: f32) -> f32 {
        x.abs()
    }
    #[rhai_fn(return_raw)]
    pub fn sign(x: f32) -> Result<INT, Box<EvalAltResult>> {
        if x == 0.0 {
            Ok(0)
        } else {
            match x.signum() {
                x if x.is_nan() => Err(make_err("Sign of NaN is undefined")),
                x => Ok(x as INT),
            }
        }
    }
    pub fn is_zero(x: f32) -> bool {
        x == 0.0
    }
    #[rhai_fn(name = "**", return_raw)]
    pub fn pow_f_i(x: f32, y: INT) -> Result<f32, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) && y > (i32::MAX as INT) {
            Err(make_err(format!(
                "Number raised to too large an index: {} ~ {}",
                x, y
            )))
        } else {
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
            (x as f64) + (y as f64)
        }
        #[rhai_fn(name = "+")]
        pub fn add_fi(x: f64, y: INT) -> f64 {
            (x as f64) + (y as f64)
        }
        #[rhai_fn(name = "-")]
        pub fn subtract_if(x: INT, y: f64) -> f64 {
            (x as f64) - (y as f64)
        }
        #[rhai_fn(name = "-")]
        pub fn subtract_fi(x: f64, y: INT) -> f64 {
            (x as f64) - (y as f64)
        }
        #[rhai_fn(name = "*")]
        pub fn multiply_if(x: INT, y: f64) -> f64 {
            (x as f64) * (y as f64)
        }
        #[rhai_fn(name = "*")]
        pub fn multiply_fi(x: f64, y: INT) -> f64 {
            (x as f64) * (y as f64)
        }
        #[rhai_fn(name = "/")]
        pub fn divide_if(x: INT, y: f64) -> f64 {
            (x as f64) / (y as f64)
        }
        #[rhai_fn(name = "/")]
        pub fn divide_fi(x: f64, y: INT) -> f64 {
            (x as f64) / (y as f64)
        }
        #[rhai_fn(name = "%")]
        pub fn modulo_if(x: INT, y: f64) -> f64 {
            (x as f64) % (y as f64)
        }
        #[rhai_fn(name = "%")]
        pub fn modulo_fi(x: f64, y: INT) -> f64 {
            (x as f64) % (y as f64)
        }
    }

    #[rhai_fn(name = "-")]
    pub fn neg(x: f64) -> f64 {
        -x
    }
    #[rhai_fn(name = "+")]
    pub fn plus(x: f64) -> f64 {
        x
    }
    pub fn abs(x: f64) -> f64 {
        x.abs()
    }
    #[rhai_fn(return_raw)]
    pub fn sign(x: f64) -> Result<INT, Box<EvalAltResult>> {
        if x == 0.0 {
            Ok(0)
        } else {
            match x.signum() {
                x if x.is_nan() => Err(make_err("Sign of NaN is undefined")),
                x => Ok(x as INT),
            }
        }
    }
    pub fn is_zero(x: f64) -> bool {
        x == 0.0
    }
}

#[cfg(feature = "decimal")]
#[export_module]
pub mod decimal_functions {
    use num_traits::Pow;
    use rust_decimal::{prelude::Zero, Decimal, MathematicalOps};

    #[rhai_fn(skip, return_raw)]
    pub fn add(x: Decimal, y: Decimal) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            x.checked_add(y)
                .ok_or_else(|| make_err(format!("Addition overflow: {} + {}", x, y)))
        } else {
            Ok(x + y)
        }
    }
    #[rhai_fn(skip, return_raw)]
    pub fn subtract(x: Decimal, y: Decimal) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            x.checked_sub(y)
                .ok_or_else(|| make_err(format!("Subtraction overflow: {} - {}", x, y)))
        } else {
            Ok(x - y)
        }
    }
    #[rhai_fn(skip, return_raw)]
    pub fn multiply(x: Decimal, y: Decimal) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            x.checked_mul(y)
                .ok_or_else(|| make_err(format!("Multiplication overflow: {} * {}", x, y)))
        } else {
            Ok(x * y)
        }
    }
    #[rhai_fn(skip, return_raw)]
    pub fn divide(x: Decimal, y: Decimal) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            // Detect division by zero
            if y == Decimal::zero() {
                Err(make_err(format!("Division by zero: {} / {}", x, y)))
            } else {
                x.checked_div(y)
                    .ok_or_else(|| make_err(format!("Division overflow: {} / {}", x, y)))
            }
        } else {
            Ok(x / y)
        }
    }
    #[rhai_fn(skip, return_raw)]
    pub fn modulo(x: Decimal, y: Decimal) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            x.checked_rem(y).ok_or_else(|| {
                make_err(format!(
                    "Modulo division by zero or overflow: {} % {}",
                    x, y
                ))
            })
        } else {
            Ok(x % y)
        }
    }
    #[rhai_fn(skip, return_raw)]
    pub fn power(x: Decimal, y: Decimal) -> Result<Decimal, Box<EvalAltResult>> {
        if cfg!(not(feature = "unchecked")) {
            x.checked_powd(y)
                .ok_or_else(|| make_err(format!("Exponential overflow: {} + {}", x, y)))
        } else {
            Ok(x.pow(y))
        }
    }
    #[rhai_fn(name = "-")]
    pub fn neg(x: Decimal) -> Decimal {
        -x
    }
    #[rhai_fn(name = "+")]
    pub fn plus(x: Decimal) -> Decimal {
        x
    }
    pub fn abs(x: Decimal) -> Decimal {
        x.abs()
    }
    pub fn sign(x: Decimal) -> INT {
        if x == Decimal::zero() {
            0
        } else if x.is_sign_negative() {
            -1
        } else {
            1
        }
    }
    pub fn is_zero(x: Decimal) -> bool {
        x.is_zero()
    }
}
