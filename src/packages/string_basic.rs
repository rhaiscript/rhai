#![allow(non_snake_case)]

use crate::def_package;
use crate::engine::{FN_TO_STRING, KEYWORD_DEBUG, KEYWORD_PRINT};
use crate::fn_native::FnPtr;
use crate::parser::{ImmutableString, INT};
use crate::plugin::*;

#[cfg(not(feature = "no_index"))]
use crate::engine::Array;

#[cfg(not(feature = "no_object"))]
use crate::engine::Map;

#[cfg(feature = "decimal")]
use rust_decimal::Decimal;

use crate::stdlib::{
    fmt::{Debug, Display},
    format,
    string::ToString,
};

type Unit = ();

macro_rules! gen_functions {
    ($root:ident => $fn_name:ident ( $($arg_type:ident),+ )) => {
        pub mod $root { $(pub mod $arg_type {
            use super::super::*;

            #[export_fn]
            #[inline(always)]
            pub fn to_string_func(x: &mut $arg_type) -> ImmutableString {
                super::super::$fn_name(x)
            }
        })* }
    }
}

macro_rules! reg_print_functions {
    ($mod_name:ident += $root:ident ; $($arg_type:ident),+) => { $(
        set_exported_fn!($mod_name, FN_TO_STRING, $root::$arg_type::to_string_func);
        set_exported_fn!($mod_name, KEYWORD_PRINT, $root::$arg_type::to_string_func);
    )* }
}

macro_rules! reg_debug_functions {
    ($mod_name:ident += $root:ident ; $($arg_type:ident),+) => { $(
        set_exported_fn!($mod_name, KEYWORD_DEBUG, $root::$arg_type::to_string_func);
    )* }
}

def_package!(crate:BasicStringPackage:"Basic string utilities, including printing.", lib, {
    reg_print_functions!(lib += print_basic; INT, bool, char, FnPtr);
    set_exported_fn!(lib, KEYWORD_PRINT, print_empty_string);
    set_exported_fn!(lib, KEYWORD_PRINT, print_unit);
    set_exported_fn!(lib, FN_TO_STRING, print_unit);
    set_exported_fn!(lib, KEYWORD_PRINT, print_string);
    set_exported_fn!(lib, FN_TO_STRING, print_string);

    reg_debug_functions!(lib += debug_basic; INT, bool, Unit, char, ImmutableString);
    set_exported_fn!(lib, KEYWORD_DEBUG, print_empty_string);
    set_exported_fn!(lib, KEYWORD_DEBUG, debug_fn_ptr);

    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    {
        reg_print_functions!(lib += print_numbers; i8, u8, i16, u16, i32, u32, i64, u64);
        reg_debug_functions!(lib += debug_numbers; i8, u8, i16, u16, i32, u32, i64, u64);

        #[cfg(not(target_arch = "wasm32"))]
        {
            reg_print_functions!(lib += print_num_128; i128, u128);
            reg_debug_functions!(lib += debug_num_128; i128, u128);
        }
    }

    #[cfg(not(feature = "no_float"))]
    {
        reg_print_functions!(lib += print_float; f32, f64);
        reg_debug_functions!(lib += debug_float; f32, f64);
    }

    #[cfg(feature = "decimal")]
    {
        reg_print_functions!(lib += print_decimal; Decimal);
        reg_debug_functions!(lib += debug_decimal; Decimal);
    }

    #[cfg(not(feature = "no_index"))]
    {
        reg_print_functions!(lib += print_array; Array);
        reg_debug_functions!(lib += print_array; Array);
    }

    #[cfg(not(feature = "no_object"))]
    {
        set_exported_fn!(lib, KEYWORD_PRINT, format_map::format_map);
        set_exported_fn!(lib, FN_TO_STRING, format_map::format_map);
        set_exported_fn!(lib, KEYWORD_DEBUG, format_map::format_map);
    }
});

gen_functions!(print_basic => to_string(INT, bool, char, FnPtr));
gen_functions!(debug_basic => to_debug(INT, bool, Unit, char, ImmutableString));

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_functions!(print_numbers => to_string(i8, u8, i16, u16, i32, u32, i64, u64));

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_functions!(debug_numbers => to_debug(i8, u8, i16, u16, i32, u32, i64, u64));

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(target_arch = "wasm32"))]
gen_functions!(print_num_128 => to_string(i128, u128));

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(target_arch = "wasm32"))]
gen_functions!(debug_num_128 => to_debug(i128, u128));

#[cfg(not(feature = "no_float"))]
gen_functions!(print_float => to_string(f32, f64));

#[cfg(not(feature = "no_float"))]
gen_functions!(debug_float => to_debug(f32, f64));

#[cfg(feature = "decimal")]
gen_functions!(print_decimal => to_string(Decimal));

#[cfg(feature = "decimal")]
gen_functions!(debug_decimal => to_debug(Decimal));

#[cfg(not(feature = "no_index"))]
gen_functions!(print_array => to_debug(Array));

// Register print and debug
#[export_fn]
#[inline(always)]
fn print_empty_string() -> ImmutableString {
    "".to_string().into()
}
#[export_fn]
#[inline(always)]
fn print_unit(_x: ()) -> ImmutableString {
    "".to_string().into()
}
#[export_fn]
#[inline(always)]
fn print_string(s: ImmutableString) -> ImmutableString {
    s
}
#[export_fn]
#[inline(always)]
fn debug_fn_ptr(f: &mut FnPtr) -> ImmutableString {
    to_string(f)
}
#[inline(always)]
fn to_string<T: Display>(x: &mut T) -> ImmutableString {
    x.to_string().into()
}
#[inline]
fn to_debug<T: Debug>(x: &mut T) -> ImmutableString {
    format!("{:?}", x).into()
}
#[cfg(not(feature = "no_object"))]
mod format_map {
    use super::*;
    #[inline]
    #[export_fn]
    pub fn format_map(x: &mut Map) -> ImmutableString {
        format!("#{:?}", x).into()
    }
}
