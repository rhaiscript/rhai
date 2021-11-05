#![allow(non_snake_case)]

use crate::plugin::*;
use crate::{def_package, EvalAltResult, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(any(
    not(feature = "no_float"),
    all(not(feature = "only_i32"), not(feature = "only_i64"))
))]
macro_rules! gen_cmp_functions {
    ($root:ident => $($arg_type:ident),+) => {
        mod $root { $(pub mod $arg_type {
            use super::super::*;

            #[export_module]
            pub mod functions {
                #[rhai_fn(name = "<")] pub fn lt(x: $arg_type, y: $arg_type) -> bool { x < y }
                #[rhai_fn(name = "<=")] pub fn lte(x: $arg_type, y: $arg_type) -> bool { x <= y }
                #[rhai_fn(name = ">")] pub fn gt(x: $arg_type, y: $arg_type) -> bool { x > y }
                #[rhai_fn(name = ">=")] pub fn gte(x: $arg_type, y: $arg_type) -> bool { x >= y }
                #[rhai_fn(name = "==")] pub fn eq(x: $arg_type, y: $arg_type) -> bool { x == y }
                #[rhai_fn(name = "!=")] pub fn ne(x: $arg_type, y: $arg_type) -> bool { x != y }
            }
        })* }
    };
}

#[cfg(any(
    not(feature = "no_float"),
    all(not(feature = "only_i32"), not(feature = "only_i64"))
))]
macro_rules! reg_functions {
    ($mod_name:ident += $root:ident ; $($arg_type:ident),+) => { $(
        combine_with_exported_module!($mod_name, "logic", $root::$arg_type::functions);
    )* }
}

def_package!(crate:LogicPackage:"Logical operators.", lib, {
    lib.standard = true;

    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    {
        reg_functions!(lib += numbers; i8, u8, i16, u16, i32, u32, u64);

        #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
        reg_functions!(lib += num_128; i128, u128);
    }

    #[cfg(not(feature = "no_float"))]
    {
        #[cfg(not(feature = "f32_float"))]
        reg_functions!(lib += float; f32);
        combine_with_exported_module!(lib, "f32", f32_functions);

        #[cfg(feature = "f32_float")]
        reg_functions!(lib += float; f64);
        combine_with_exported_module!(lib, "f64", f64_functions);
    }

    set_exported_fn!(lib, "!", not);

    combine_with_exported_module!(lib, "bit_field", bit_field_functions);
});

// Logic operators
#[export_fn]
fn not(x: bool) -> bool {
    !x
}

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_cmp_functions!(numbers => i8, u8, i16, u16, i32, u32, u64);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
gen_cmp_functions!(num_128 => i128, u128);

#[cfg(not(feature = "no_float"))]
#[cfg(not(feature = "f32_float"))]
gen_cmp_functions!(float => f32);

#[cfg(not(feature = "no_float"))]
#[cfg(feature = "f32_float")]
gen_cmp_functions!(float => f64);

#[cfg(not(feature = "no_float"))]
#[export_module]
mod f32_functions {
    #[rhai_fn(name = "==")]
    pub fn eq_if(x: INT, y: f32) -> bool {
        (x as f32) == (y as f32)
    }
    #[rhai_fn(name = "==")]
    pub fn eq_fi(x: f32, y: INT) -> bool {
        (x as f32) == (y as f32)
    }
    #[rhai_fn(name = "!=")]
    pub fn neq_if(x: INT, y: f32) -> bool {
        (x as f32) != (y as f32)
    }
    #[rhai_fn(name = "!=")]
    pub fn neq_fi(x: f32, y: INT) -> bool {
        (x as f32) != (y as f32)
    }
    #[rhai_fn(name = ">")]
    pub fn gt_if(x: INT, y: f32) -> bool {
        (x as f32) > (y as f32)
    }
    #[rhai_fn(name = ">")]
    pub fn gt_fi(x: f32, y: INT) -> bool {
        (x as f32) > (y as f32)
    }
    #[rhai_fn(name = ">=")]
    pub fn gte_if(x: INT, y: f32) -> bool {
        (x as f32) >= (y as f32)
    }
    #[rhai_fn(name = ">=")]
    pub fn gte_fi(x: f32, y: INT) -> bool {
        (x as f32) >= (y as f32)
    }
    #[rhai_fn(name = "<")]
    pub fn lt_if(x: INT, y: f32) -> bool {
        (x as f32) < (y as f32)
    }
    #[rhai_fn(name = "<")]
    pub fn lt_fi(x: f32, y: INT) -> bool {
        (x as f32) < (y as f32)
    }
    #[rhai_fn(name = "<=")]
    pub fn lte_if(x: INT, y: f32) -> bool {
        (x as f32) <= (y as f32)
    }
    #[rhai_fn(name = "<=")]
    pub fn lte_fi(x: f32, y: INT) -> bool {
        (x as f32) <= (y as f32)
    }
}

#[cfg(not(feature = "no_float"))]
#[export_module]
mod f64_functions {
    #[rhai_fn(name = "==")]
    pub fn eq_if(x: INT, y: f64) -> bool {
        (x as f64) == (y as f64)
    }
    #[rhai_fn(name = "==")]
    pub fn eq_fi(x: f64, y: INT) -> bool {
        (x as f64) == (y as f64)
    }
    #[rhai_fn(name = "!=")]
    pub fn neq_if(x: INT, y: f64) -> bool {
        (x as f64) != (y as f64)
    }
    #[rhai_fn(name = "!=")]
    pub fn neq_fi(x: f64, y: INT) -> bool {
        (x as f64) != (y as f64)
    }
    #[rhai_fn(name = ">")]
    pub fn gt_if(x: INT, y: f64) -> bool {
        (x as f64) > (y as f64)
    }
    #[rhai_fn(name = ">")]
    pub fn gt_fi(x: f64, y: INT) -> bool {
        (x as f64) > (y as f64)
    }
    #[rhai_fn(name = ">=")]
    pub fn gte_if(x: INT, y: f64) -> bool {
        (x as f64) >= (y as f64)
    }
    #[rhai_fn(name = ">=")]
    pub fn gte_fi(x: f64, y: INT) -> bool {
        (x as f64) >= (y as f64)
    }
    #[rhai_fn(name = "<")]
    pub fn lt_if(x: INT, y: f64) -> bool {
        (x as f64) < (y as f64)
    }
    #[rhai_fn(name = "<")]
    pub fn lt_fi(x: f64, y: INT) -> bool {
        (x as f64) < (y as f64)
    }
    #[rhai_fn(name = "<=")]
    pub fn lte_if(x: INT, y: f64) -> bool {
        (x as f64) <= (y as f64)
    }
    #[rhai_fn(name = "<=")]
    pub fn lte_fi(x: f64, y: INT) -> bool {
        (x as f64) <= (y as f64)
    }
}

#[export_module]
mod bit_field_functions {
    const BITS: usize = std::mem::size_of::<INT>() * 8;

    #[rhai_fn(return_raw)]
    pub fn get_bit(value: INT, index: INT) -> Result<bool, Box<EvalAltResult>> {
        if index >= 0 {
            let offset = index as usize;

            if offset >= BITS {
                EvalAltResult::ErrorBitFieldBounds(BITS, index, Position::NONE).into()
            } else {
                Ok((value & (1 << offset)) != 0)
            }
        } else if let Some(abs_index) = index.checked_abs() {
            let offset = abs_index as usize;

            // Count from end if negative
            if offset > BITS {
                EvalAltResult::ErrorBitFieldBounds(BITS, index, Position::NONE).into()
            } else {
                Ok((value & (1 << (BITS - offset))) != 0)
            }
        } else {
            EvalAltResult::ErrorBitFieldBounds(BITS, index, Position::NONE).into()
        }
    }
    #[rhai_fn(return_raw)]
    pub fn set_bit(value: &mut INT, index: INT, new_value: bool) -> Result<(), Box<EvalAltResult>> {
        if index >= 0 {
            let offset = index as usize;

            if offset >= BITS {
                EvalAltResult::ErrorBitFieldBounds(BITS, index, Position::NONE).into()
            } else {
                let mask = 1 << offset;
                if new_value {
                    *value |= mask;
                } else {
                    *value &= !mask;
                }
                Ok(())
            }
        } else if let Some(abs_index) = index.checked_abs() {
            let offset = abs_index as usize;

            // Count from end if negative
            if offset > BITS {
                EvalAltResult::ErrorBitFieldBounds(BITS, index, Position::NONE).into()
            } else {
                let mask = 1 << offset;
                if new_value {
                    *value |= mask;
                } else {
                    *value &= !mask;
                }
                Ok(())
            }
        } else {
            EvalAltResult::ErrorBitFieldBounds(BITS, index, Position::NONE).into()
        }
    }
    #[rhai_fn(return_raw)]
    pub fn get_bits(value: INT, index: INT, bits: INT) -> Result<INT, Box<EvalAltResult>> {
        if bits < 1 {
            return Ok(0);
        }

        let offset = if index >= 0 {
            let offset = index as usize;

            if offset >= BITS {
                return EvalAltResult::ErrorBitFieldBounds(BITS, index, Position::NONE).into();
            }

            offset
        } else if let Some(abs_index) = index.checked_abs() {
            let offset = abs_index as usize;

            // Count from end if negative
            if offset > BITS {
                return EvalAltResult::ErrorBitFieldBounds(BITS, index, Position::NONE).into();
            }
            BITS - offset
        } else {
            return EvalAltResult::ErrorBitFieldBounds(BITS, index, Position::NONE).into();
        };

        let bits = if offset + bits as usize > BITS {
            BITS - offset
        } else {
            bits as usize
        };

        let mut base = 1;
        let mut mask = 0;

        for _ in 0..bits {
            mask |= base;
            base <<= 1;
        }

        Ok(((value & (mask << index)) >> index) & mask)
    }
    #[rhai_fn(return_raw)]
    pub fn set_bits(
        value: &mut INT,
        index: INT,
        bits: INT,
        new_value: INT,
    ) -> Result<(), Box<EvalAltResult>> {
        if bits < 1 {
            return Ok(());
        }

        let offset = if index >= 0 {
            let offset = index as usize;

            if offset >= BITS {
                return EvalAltResult::ErrorBitFieldBounds(BITS, index, Position::NONE).into();
            }

            offset
        } else if let Some(abs_index) = index.checked_abs() {
            let offset = abs_index as usize;

            // Count from end if negative
            if offset > BITS {
                return EvalAltResult::ErrorBitFieldBounds(BITS, index, Position::NONE).into();
            }
            BITS - offset
        } else {
            return EvalAltResult::ErrorBitFieldBounds(BITS, index, Position::NONE).into();
        };

        let bits = if offset + bits as usize > BITS {
            BITS - offset
        } else {
            bits as usize
        };

        let mut base = 1;
        let mut mask = 0;

        for _ in 0..bits {
            mask |= base;
            base <<= 1;
        }

        *value &= !(mask << index);
        *value |= (new_value & mask) << index;

        Ok(())
    }
}
