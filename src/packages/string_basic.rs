use crate::module::ModuleFlags;
use crate::plugin::*;
use crate::{def_package, FnPtr, ImmutableString, SmartString, INT};
use std::any::TypeId;
use std::fmt::{Binary, LowerHex, Octal, Write};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_index"))]
use crate::Array;

#[cfg(not(feature = "no_object"))]
use crate::Map;

/// Standard pretty-print function.
///
/// This function is called to convert any type into text format for display.
pub const FUNC_TO_STRING: &str = "to_string";

/// Standard debug-print function.
///
/// This function is called to convert any type into text format suitable for debugging.
pub const FUNC_TO_DEBUG: &str = "to_debug";

def_package! {
    /// Package of basic string utilities (e.g. printing)
    pub BasicStringPackage(lib) {
        lib.flags |= ModuleFlags::STANDARD_LIB;

        combine_with_exported_module!(lib, "print_debug", print_debug_functions);
        combine_with_exported_module!(lib, "number_formatting", number_formatting);
        combine_with_exported_module!(lib, "char", char_functions);

        // Register characters iterator
        #[cfg(not(feature = "no_index"))]
        lib.set_iter(TypeId::of::<ImmutableString>(), |value| Box::new(
            value.cast::<ImmutableString>().chars().map(Into::into).collect::<Array>().into_iter()
        ));
    }
}

// Register print and debug

#[inline]
pub fn print_with_func(
    fn_name: &str,
    ctx: &NativeCallContext,
    value: &mut Dynamic,
) -> ImmutableString {
    match ctx.call_native_fn_raw(fn_name, true, &mut [value]) {
        Ok(result) if result.is_string() => result.into_immutable_string().unwrap(),
        Ok(result) => ctx.engine().map_type_name(result.type_name()).into(),
        Err(_) => {
            let mut buf = SmartString::new_const();
            match fn_name {
                FUNC_TO_DEBUG => write!(&mut buf, "{value:?}").unwrap(),
                _ => write!(&mut buf, "{value}").unwrap(),
            }
            ctx.engine().map_type_name(&buf).into()
        }
    }
}

#[export_module]
mod print_debug_functions {
    /// Convert the value of the `item` into a string.
    #[rhai_fn(name = "print", pure)]
    pub fn print_generic(ctx: NativeCallContext, item: &mut Dynamic) -> ImmutableString {
        print_with_func(FUNC_TO_STRING, &ctx, item)
    }
    /// Convert the value of the `item` into a string.
    #[rhai_fn(name = "to_string", pure)]
    pub fn to_string_generic(ctx: NativeCallContext, item: &mut Dynamic) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{item}").unwrap();
        ctx.engine().map_type_name(&buf).into()
    }
    /// Convert the value of the `item` into a string in debug format.
    #[rhai_fn(name = "debug", pure)]
    pub fn debug_generic(ctx: NativeCallContext, item: &mut Dynamic) -> ImmutableString {
        print_with_func(FUNC_TO_DEBUG, &ctx, item)
    }
    /// Convert the value of the `item` into a string in debug format.
    #[rhai_fn(name = "to_debug", pure)]
    pub fn to_debug_generic(ctx: NativeCallContext, item: &mut Dynamic) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{item:?}").unwrap();
        ctx.engine().map_type_name(&buf).into()
    }

    /// Return the empty string.
    #[rhai_fn(name = "print", name = "debug")]
    pub fn print_empty_string(ctx: NativeCallContext) -> ImmutableString {
        ctx.engine().const_empty_string()
    }

    /// Return the `string`.
    #[rhai_fn(name = "print", name = "to_string")]
    pub const fn print_string(string: ImmutableString) -> ImmutableString {
        string
    }
    /// Convert the string into debug format.
    #[rhai_fn(name = "debug", name = "to_debug")]
    pub fn debug_string(string: &str) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{string:?}").unwrap();
        buf.into()
    }

    /// Return the character into a string.
    #[rhai_fn(name = "print", name = "to_string")]
    pub fn print_char(character: char) -> ImmutableString {
        let mut buf = SmartString::new_const();
        buf.push(character);
        buf.into()
    }
    /// Convert the string into debug format.
    #[rhai_fn(name = "debug", name = "to_debug")]
    pub fn debug_char(character: char) -> ImmutableString {
        let mut buf = SmartString::new_const();
        buf.push(character);
        buf.into()
    }

    /// Convert the function pointer into a string in debug format.
    #[rhai_fn(name = "debug", name = "to_debug", pure)]
    pub fn debug_fn_ptr(f: &mut FnPtr) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{f}").unwrap();
        buf.into()
    }

    /// Return the boolean value into a string.
    #[rhai_fn(name = "print", name = "to_string")]
    pub fn print_bool(value: bool) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{value}").unwrap();
        buf.into()
    }
    /// Convert the boolean value into a string in debug format.
    #[rhai_fn(name = "debug", name = "to_debug")]
    pub fn debug_bool(value: bool) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{value:?}").unwrap();
        buf.into()
    }

    /// Return the empty string.
    #[allow(unused_variables)]
    #[rhai_fn(name = "print", name = "to_string")]
    pub fn print_unit(ctx: NativeCallContext, unit: ()) -> ImmutableString {
        ctx.engine().const_empty_string()
    }
    /// Convert the unit into a string in debug format.
    #[allow(unused_variables)]
    #[rhai_fn(name = "debug", name = "to_debug")]
    pub fn debug_unit(unit: ()) -> ImmutableString {
        "()".into()
    }

    /// Convert the value of `number` into a string.
    #[cfg(not(feature = "no_float"))]
    #[rhai_fn(name = "print", name = "to_string")]
    pub fn print_f64(number: f64) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{}", crate::types::FloatWrapper::new(number)).unwrap();
        buf.into()
    }
    /// Convert the value of `number` into a string.
    #[cfg(not(feature = "no_float"))]
    #[rhai_fn(name = "print", name = "to_string")]
    pub fn print_f32(number: f32) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{}", crate::types::FloatWrapper::new(number)).unwrap();
        buf.into()
    }
    /// Convert the value of `number` into a string.
    #[cfg(not(feature = "no_float"))]
    #[rhai_fn(name = "debug", name = "to_debug")]
    pub fn debug_f64(number: f64) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{:?}", crate::types::FloatWrapper::new(number)).unwrap();
        buf.into()
    }
    /// Convert the value of `number` into a string.
    #[cfg(not(feature = "no_float"))]
    #[rhai_fn(name = "debug", name = "to_debug")]
    pub fn debug_f32(number: f32) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{:?}", crate::types::FloatWrapper::new(number)).unwrap();
        buf.into()
    }

    /// Convert the array into a string.
    #[cfg(not(feature = "no_index"))]
    #[rhai_fn(
        name = "print",
        name = "to_string",
        name = "debug",
        name = "to_debug",
        pure
    )]
    pub fn format_array(ctx: NativeCallContext, array: &mut Array) -> ImmutableString {
        let len = array.len();
        let mut result = SmartString::new_const();
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

    /// Convert the object map into a string.
    #[cfg(not(feature = "no_object"))]
    #[rhai_fn(
        name = "print",
        name = "to_string",
        name = "debug",
        name = "to_debug",
        pure
    )]
    pub fn format_map(ctx: NativeCallContext, map: &mut Map) -> ImmutableString {
        let len = map.len();
        let mut result = SmartString::new_const();
        result.push_str("#{");

        map.iter_mut().enumerate().for_each(|(i, (k, v))| {
            write!(
                result,
                "{:?}: {}{}",
                k,
                &print_with_func(FUNC_TO_DEBUG, &ctx, v),
                if i < len - 1 { ", " } else { "" }
            )
            .unwrap();
        });

        result.push('}');
        result.into()
    }
}

#[export_module]
mod number_formatting {
    fn to_hex<T: LowerHex>(value: T) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{value:x}").unwrap();
        buf.into()
    }
    fn to_octal<T: Octal>(value: T) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{value:o}").unwrap();
        buf.into()
    }
    fn to_binary<T: Binary>(value: T) -> ImmutableString {
        let mut buf = SmartString::new_const();
        write!(&mut buf, "{value:b}").unwrap();
        buf.into()
    }

    /// Convert the `value` into a string in hex format.
    #[rhai_fn(name = "to_hex")]
    pub fn int_to_hex(value: INT) -> ImmutableString {
        to_hex(value)
    }
    /// Convert the `value` into a string in octal format.
    #[rhai_fn(name = "to_octal")]
    pub fn int_to_octal(value: INT) -> ImmutableString {
        to_octal(value)
    }
    /// Convert the `value` into a string in binary format.
    #[rhai_fn(name = "to_binary")]
    pub fn int_to_binary(value: INT) -> ImmutableString {
        to_binary(value)
    }

    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    pub mod numbers {
        /// Convert the `value` into a string in hex format.
        #[rhai_fn(name = "to_hex")]
        pub fn u8_to_hex(value: u8) -> ImmutableString {
            to_hex(value)
        }
        /// Convert the `value` into a string in hex format.
        #[rhai_fn(name = "to_hex")]
        pub fn u16_to_hex(value: u16) -> ImmutableString {
            to_hex(value)
        }
        /// Convert the `value` into a string in hex format.
        #[rhai_fn(name = "to_hex")]
        pub fn u32_to_hex(value: u32) -> ImmutableString {
            to_hex(value)
        }
        /// Convert the `value` into a string in hex format.
        #[rhai_fn(name = "to_hex")]
        pub fn u64_to_hex(value: u64) -> ImmutableString {
            to_hex(value)
        }
        /// Convert the `value` into a string in hex format.
        #[rhai_fn(name = "to_hex")]
        pub fn i8_to_hex(value: i8) -> ImmutableString {
            to_hex(value)
        }
        /// Convert the `value` into a string in hex format.
        #[rhai_fn(name = "to_hex")]
        pub fn i16_to_hex(value: i16) -> ImmutableString {
            to_hex(value)
        }
        /// Convert the `value` into a string in hex format.
        #[rhai_fn(name = "to_hex")]
        pub fn i32_to_hex(value: i32) -> ImmutableString {
            to_hex(value)
        }
        /// Convert the `value` into a string in hex format.
        #[rhai_fn(name = "to_hex")]
        pub fn i64_to_hex(value: i64) -> ImmutableString {
            to_hex(value)
        }
        /// Convert the `value` into a string in octal format.
        #[rhai_fn(name = "to_octal")]
        pub fn u8_to_octal(value: u8) -> ImmutableString {
            to_octal(value)
        }
        /// Convert the `value` into a string in octal format.
        #[rhai_fn(name = "to_octal")]
        pub fn u16_to_octal(value: u16) -> ImmutableString {
            to_octal(value)
        }
        /// Convert the `value` into a string in octal format.
        #[rhai_fn(name = "to_octal")]
        pub fn u32_to_octal(value: u32) -> ImmutableString {
            to_octal(value)
        }
        /// Convert the `value` into a string in octal format.
        #[rhai_fn(name = "to_octal")]
        pub fn u64_to_octal(value: u64) -> ImmutableString {
            to_octal(value)
        }
        /// Convert the `value` into a string in octal format.
        #[rhai_fn(name = "to_octal")]
        pub fn i8_to_octal(value: i8) -> ImmutableString {
            to_octal(value)
        }
        /// Convert the `value` into a string in octal format.
        #[rhai_fn(name = "to_octal")]
        pub fn i16_to_octal(value: i16) -> ImmutableString {
            to_octal(value)
        }
        /// Convert the `value` into a string in octal format.
        #[rhai_fn(name = "to_octal")]
        pub fn i32_to_octal(value: i32) -> ImmutableString {
            to_octal(value)
        }
        /// Convert the `value` into a string in octal format.
        #[rhai_fn(name = "to_octal")]
        pub fn i64_to_octal(value: i64) -> ImmutableString {
            to_octal(value)
        }
        /// Convert the `value` into a string in binary format.
        #[rhai_fn(name = "to_binary")]
        pub fn u8_to_binary(value: u8) -> ImmutableString {
            to_binary(value)
        }
        /// Convert the `value` into a string in binary format.
        #[rhai_fn(name = "to_binary")]
        pub fn u16_to_binary(value: u16) -> ImmutableString {
            to_binary(value)
        }
        /// Convert the `value` into a string in binary format.
        #[rhai_fn(name = "to_binary")]
        pub fn u32_to_binary(value: u32) -> ImmutableString {
            to_binary(value)
        }
        /// Convert the `value` into a string in binary format.
        #[rhai_fn(name = "to_binary")]
        pub fn u64_to_binary(value: u64) -> ImmutableString {
            to_binary(value)
        }
        /// Convert the `value` into a string in binary format.
        #[rhai_fn(name = "to_binary")]
        pub fn i8_to_binary(value: i8) -> ImmutableString {
            to_binary(value)
        }
        /// Convert the `value` into a string in binary format.
        #[rhai_fn(name = "to_binary")]
        pub fn i16_to_binary(value: i16) -> ImmutableString {
            to_binary(value)
        }
        /// Convert the `value` into a string in binary format.
        #[rhai_fn(name = "to_binary")]
        pub fn i32_to_binary(value: i32) -> ImmutableString {
            to_binary(value)
        }
        /// Convert the `value` into a string in binary format.
        #[rhai_fn(name = "to_binary")]
        pub fn i64_to_binary(value: i64) -> ImmutableString {
            to_binary(value)
        }

        #[cfg(not(target_family = "wasm"))]
        pub mod num_128 {
            /// Convert the `value` into a string in hex format.
            #[rhai_fn(name = "to_hex")]
            pub fn u128_to_hex(value: u128) -> ImmutableString {
                to_hex(value)
            }
            /// Convert the `value` into a string in hex format.
            #[rhai_fn(name = "to_hex")]
            pub fn i128_to_hex(value: i128) -> ImmutableString {
                to_hex(value)
            }
            /// Convert the `value` into a string in octal format.
            #[rhai_fn(name = "to_octal")]
            pub fn u128_to_octal(value: u128) -> ImmutableString {
                to_octal(value)
            }
            /// Convert the `value` into a string in octal format.
            #[rhai_fn(name = "to_octal")]
            pub fn i128_to_octal(value: i128) -> ImmutableString {
                to_octal(value)
            }
            /// Convert the `value` into a string in binary format.
            #[rhai_fn(name = "to_binary")]
            pub fn u128_to_binary(value: u128) -> ImmutableString {
                to_binary(value)
            }
            /// Convert the `value` into a string in binary format.
            #[rhai_fn(name = "to_binary")]
            pub fn i128_to_binary(value: i128) -> ImmutableString {
                to_binary(value)
            }
        }
    }
}

#[export_module]
mod char_functions {
    /// Convert the Unicode character into a 32-bit integer value.
    pub const fn to_int(ch: char) -> INT {
        (ch as u32) as INT
    }
}
