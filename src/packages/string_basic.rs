#![allow(non_snake_case)]

use crate::plugin::*;
use crate::{def_package, FnPtr, INT};
use std::fmt::{Binary, LowerHex, Octal};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_index"))]
use crate::Array;

#[cfg(not(feature = "no_object"))]
use crate::Map;

pub const FUNC_TO_STRING: &str = "to_string";
pub const FUNC_TO_DEBUG: &str = "to_debug";

def_package!(crate:BasicStringPackage:"Basic string utilities, including printing.", lib, {
    lib.standard = true;

    combine_with_exported_module!(lib, "print_debug", print_debug_functions);
    combine_with_exported_module!(lib, "number_formatting", number_formatting);
});

// Register print and debug

#[inline]
pub fn print_with_func(
    fn_name: &str,
    ctx: &NativeCallContext,
    value: &mut Dynamic,
) -> crate::ImmutableString {
    match ctx.call_fn_dynamic_raw(fn_name, true, &mut [value]) {
        Ok(result) if result.is::<crate::ImmutableString>() => result
            .into_immutable_string()
            .expect("result is `ImmutableString`"),
        Ok(result) => ctx.engine().map_type_name(result.type_name()).into(),
        Err(_) => ctx.engine().map_type_name(value.type_name()).into(),
    }
}

#[export_module]
mod print_debug_functions {
    use crate::ImmutableString;

    #[rhai_fn(name = "print", pure)]
    pub fn print_generic(ctx: NativeCallContext, item: &mut Dynamic) -> ImmutableString {
        print_with_func(FUNC_TO_STRING, &ctx, item)
    }
    #[rhai_fn(name = "to_string", pure)]
    pub fn to_string_generic(ctx: NativeCallContext, item: &mut Dynamic) -> ImmutableString {
        ctx.engine().map_type_name(&item.to_string()).into()
    }
    #[rhai_fn(name = "debug", pure)]
    pub fn debug_generic(ctx: NativeCallContext, item: &mut Dynamic) -> ImmutableString {
        print_with_func(FUNC_TO_DEBUG, &ctx, item)
    }
    #[rhai_fn(name = "to_debug", pure)]
    pub fn to_debug_generic(ctx: NativeCallContext, item: &mut Dynamic) -> ImmutableString {
        ctx.engine().map_type_name(&format!("{:?}", item)).into()
    }
    #[rhai_fn(name = "print", name = "debug")]
    pub fn print_empty_string() -> ImmutableString {
        Default::default()
    }
    #[rhai_fn(name = "print", name = "to_string")]
    pub fn print_string(s: ImmutableString) -> ImmutableString {
        s
    }
    #[rhai_fn(name = "debug", name = "to_debug", pure)]
    pub fn debug_fn_ptr(f: &mut FnPtr) -> ImmutableString {
        f.to_string().into()
    }

    #[cfg(not(feature = "no_float"))]
    pub mod float_functions {
        use crate::ast::FloatWrapper;

        #[rhai_fn(name = "print", name = "to_string")]
        pub fn print_f64(number: f64) -> ImmutableString {
            FloatWrapper::new(number).to_string().into()
        }
        #[rhai_fn(name = "print", name = "to_string")]
        pub fn print_f32(number: f32) -> ImmutableString {
            FloatWrapper::new(number).to_string().into()
        }
        #[rhai_fn(name = "debug", name = "to_debug")]
        pub fn debug_f64(number: f64) -> ImmutableString {
            format!("{:?}", FloatWrapper::new(number)).into()
        }
        #[rhai_fn(name = "debug", name = "to_debug")]
        pub fn debug_f32(number: f32) -> ImmutableString {
            format!("{:?}", FloatWrapper::new(number)).into()
        }
    }

    #[cfg(not(feature = "no_index"))]
    pub mod array_functions {
        use super::*;

        #[rhai_fn(
            name = "print",
            name = "to_string",
            name = "debug",
            name = "to_debug",
            pure
        )]
        pub fn format_array(ctx: NativeCallContext, array: &mut Array) -> ImmutableString {
            let len = array.len();
            let mut result = String::with_capacity(len * 5 + 2);
            result.push('[');

            array.iter_mut().enumerate().for_each(|(i, x)| {
                result.push_str(&print_with_func(FUNC_TO_DEBUG, &ctx, x));
                if i < len - 1 {
                    result.push_str(", ");
                }
            });

            result.push(']');
            result.into()
        }
    }
    #[cfg(not(feature = "no_object"))]
    pub mod map_functions {
        use super::*;

        #[rhai_fn(
            name = "print",
            name = "to_string",
            name = "debug",
            name = "to_debug",
            pure
        )]
        pub fn format_map(ctx: NativeCallContext, map: &mut Map) -> ImmutableString {
            let len = map.len();
            let mut result = String::with_capacity(len * 5 + 3);
            result.push_str("#{");

            map.iter_mut().enumerate().for_each(|(i, (k, v))| {
                result.push_str(&format!(
                    "{:?}: {}{}",
                    k,
                    &print_with_func(FUNC_TO_DEBUG, &ctx, v),
                    if i < len - 1 { ", " } else { "" }
                ));
            });

            result.push('}');
            result.into()
        }
    }
}

#[export_module]
mod number_formatting {
    #[rhai_fn(skip)]
    pub fn to_hex<T: LowerHex>(value: T) -> ImmutableString {
        format!("{:x}", value).into()
    }
    #[rhai_fn(skip)]
    pub fn to_octal<T: Octal>(value: T) -> ImmutableString {
        format!("{:o}", value).into()
    }
    #[rhai_fn(skip)]
    pub fn to_binary<T: Binary>(value: T) -> ImmutableString {
        format!("{:b}", value).into()
    }

    #[rhai_fn(name = "to_hex")]
    pub fn int_to_hex(value: INT) -> ImmutableString {
        to_hex(value)
    }
    #[rhai_fn(name = "to_octal")]
    pub fn int_to_octal(value: INT) -> ImmutableString {
        to_octal(value)
    }
    #[rhai_fn(name = "to_binary")]
    pub fn int_to_binary(value: INT) -> ImmutableString {
        to_binary(value)
    }

    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    pub mod numbers {
        #[rhai_fn(name = "to_hex")]
        pub fn u8_to_hex(value: u8) -> ImmutableString {
            to_hex(value)
        }
        #[rhai_fn(name = "to_hex")]
        pub fn u16_to_hex(value: u16) -> ImmutableString {
            to_hex(value)
        }
        #[rhai_fn(name = "to_hex")]
        pub fn u32_to_hex(value: u32) -> ImmutableString {
            to_hex(value)
        }
        #[rhai_fn(name = "to_hex")]
        pub fn u64_to_hex(value: u64) -> ImmutableString {
            to_hex(value)
        }
        #[rhai_fn(name = "to_hex")]
        pub fn i8_to_hex(value: i8) -> ImmutableString {
            to_hex(value)
        }
        #[rhai_fn(name = "to_hex")]
        pub fn i16_to_hex(value: i16) -> ImmutableString {
            to_hex(value)
        }
        #[rhai_fn(name = "to_hex")]
        pub fn i32_to_hex(value: i32) -> ImmutableString {
            to_hex(value)
        }
        #[rhai_fn(name = "to_hex")]
        pub fn i64_to_hex(value: i64) -> ImmutableString {
            to_hex(value)
        }
        #[rhai_fn(name = "to_octal")]
        pub fn u8_to_octal(value: u8) -> ImmutableString {
            to_octal(value)
        }
        #[rhai_fn(name = "to_octal")]
        pub fn u16_to_octal(value: u16) -> ImmutableString {
            to_octal(value)
        }
        #[rhai_fn(name = "to_octal")]
        pub fn u32_to_octal(value: u32) -> ImmutableString {
            to_octal(value)
        }
        #[rhai_fn(name = "to_octal")]
        pub fn u64_to_octal(value: u64) -> ImmutableString {
            to_octal(value)
        }
        #[rhai_fn(name = "to_octal")]
        pub fn i8_to_octal(value: i8) -> ImmutableString {
            to_octal(value)
        }
        #[rhai_fn(name = "to_octal")]
        pub fn i16_to_octal(value: i16) -> ImmutableString {
            to_octal(value)
        }
        #[rhai_fn(name = "to_octal")]
        pub fn i32_to_octal(value: i32) -> ImmutableString {
            to_octal(value)
        }
        #[rhai_fn(name = "to_octal")]
        pub fn i64_to_octal(value: i64) -> ImmutableString {
            to_octal(value)
        }
        #[rhai_fn(name = "to_binary")]
        pub fn u8_to_binary(value: u8) -> ImmutableString {
            to_binary(value)
        }
        #[rhai_fn(name = "to_binary")]
        pub fn u16_to_binary(value: u16) -> ImmutableString {
            to_binary(value)
        }
        #[rhai_fn(name = "to_binary")]
        pub fn u32_to_binary(value: u32) -> ImmutableString {
            to_binary(value)
        }
        #[rhai_fn(name = "to_binary")]
        pub fn u64_to_binary(value: u64) -> ImmutableString {
            to_binary(value)
        }
        #[rhai_fn(name = "to_binary")]
        pub fn i8_to_binary(value: i8) -> ImmutableString {
            to_binary(value)
        }
        #[rhai_fn(name = "to_binary")]
        pub fn i16_to_binary(value: i16) -> ImmutableString {
            to_binary(value)
        }
        #[rhai_fn(name = "to_binary")]
        pub fn i32_to_binary(value: i32) -> ImmutableString {
            to_binary(value)
        }
        #[rhai_fn(name = "to_binary")]
        pub fn i64_to_binary(value: i64) -> ImmutableString {
            to_binary(value)
        }

        #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
        pub mod num_128 {
            #[rhai_fn(name = "to_hex")]
            pub fn u128_to_hex(value: u128) -> ImmutableString {
                to_hex(value)
            }
            #[rhai_fn(name = "to_octal")]
            pub fn i128_to_octal(value: i128) -> ImmutableString {
                to_octal(value)
            }
            #[rhai_fn(name = "to_binary")]
            pub fn i128_to_binary(value: i128) -> ImmutableString {
                to_binary(value)
            }
        }
    }
}
