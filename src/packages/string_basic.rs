use crate::def_package;
use crate::engine::{FN_TO_STRING, KEYWORD_DEBUG, KEYWORD_PRINT};
use crate::fn_native::FnPtr;
use crate::module::FuncReturn;
use crate::parser::{ImmutableString, INT};

#[cfg(not(feature = "no_index"))]
use crate::engine::Array;

#[cfg(not(feature = "no_object"))]
use crate::engine::Map;

#[cfg(not(feature = "no_decimal"))]
use rust_decimal::Decimal;

use crate::stdlib::{
    fmt::{Debug, Display},
    format,
    string::ToString,
};

// Register print and debug
fn to_debug<T: Debug>(x: &mut T) -> FuncReturn<ImmutableString> {
    Ok(format!("{:?}", x).into())
}
fn to_string<T: Display>(x: &mut T) -> FuncReturn<ImmutableString> {
    Ok(x.to_string().into())
}
#[cfg(not(feature = "no_object"))]
fn format_map(x: &mut Map) -> FuncReturn<ImmutableString> {
    Ok(format!("#{:?}", x).into())
}

macro_rules! reg_op {
    ($lib:expr, $op:expr, $func:ident, $($par:ty),*) => {
        $( $lib.set_fn_1_mut($op, $func::<$par>); )*
    };
}

def_package!(crate:BasicStringPackage:"Basic string utilities, including printing.", lib, {
    reg_op!(lib, KEYWORD_PRINT, to_string, INT, bool, char, FnPtr);
    reg_op!(lib, FN_TO_STRING, to_string, INT, bool, char, FnPtr);
    lib.set_fn_1_mut(KEYWORD_DEBUG, |f: &mut FnPtr| Ok(f.to_string()));

    lib.set_fn_0(KEYWORD_PRINT, || Ok("".to_string()));
    lib.set_fn_1(KEYWORD_PRINT, |_: ()| Ok("".to_string()));
    lib.set_fn_1(FN_TO_STRING, |_: ()| Ok("".to_string()));

    lib.set_fn_1(KEYWORD_PRINT, |s: ImmutableString| Ok(s));
    lib.set_fn_1(FN_TO_STRING, |s: ImmutableString| Ok(s));

    reg_op!(lib, KEYWORD_DEBUG, to_debug, INT, bool, (), char, ImmutableString);

    if cfg!(not(feature = "only_i32")) && cfg!(not(feature = "only_i64")) {
        reg_op!(lib, KEYWORD_PRINT, to_string, i8, u8, i16, u16, i32, u32);
        reg_op!(lib, FN_TO_STRING, to_string, i8, u8, i16, u16, i32, u32);
        reg_op!(lib, KEYWORD_DEBUG, to_debug, i8, u8, i16, u16, i32, u32);
        reg_op!(lib, KEYWORD_PRINT, to_string, i64, u64);
        reg_op!(lib, FN_TO_STRING, to_string, i64, u64);
        reg_op!(lib, KEYWORD_DEBUG, to_debug, i64, u64);

        if cfg!(not(target_arch = "wasm32")) {
            reg_op!(lib, KEYWORD_PRINT, to_string, i128, u128);
            reg_op!(lib, FN_TO_STRING, to_string, i128, u128);
            reg_op!(lib, KEYWORD_DEBUG, to_debug, i128, u128);
        }
    }

    #[cfg(not(feature = "no_float"))]
    {
        reg_op!(lib, KEYWORD_PRINT, to_string, f32, f64);
        reg_op!(lib, FN_TO_STRING, to_string, f32, f64);
        reg_op!(lib, KEYWORD_DEBUG, to_debug, f32, f64);
    }

    #[cfg(not(feature = "no_decimal"))]
    {
        reg_op!(lib, KEYWORD_PRINT, to_string, Decimal);
        reg_op!(lib, FN_TO_STRING, to_string, Decimal);
        reg_op!(lib, KEYWORD_DEBUG, to_debug, Decimal);
    }

    #[cfg(not(feature = "no_index"))]
    {
        reg_op!(lib, KEYWORD_PRINT, to_debug, Array);
        reg_op!(lib, FN_TO_STRING, to_debug, Array);
        reg_op!(lib, KEYWORD_DEBUG, to_debug, Array);
    }

    #[cfg(not(feature = "no_object"))]
    {
        lib.set_fn_1_mut(KEYWORD_PRINT, format_map);
        lib.set_fn_1_mut(FN_TO_STRING, format_map);
        lib.set_fn_1_mut(KEYWORD_DEBUG, format_map);
    }

    lib.set_fn_2("+", |s: ImmutableString, ch: char| Ok(s + ch));
    lib.set_fn_2_mut("+=", |s: &mut ImmutableString, ch: char| { *s += ch; Ok(()) });
    lib.set_fn_2_mut("append", |s: &mut ImmutableString, ch: char| { *s += ch; Ok(()) });
    lib.set_fn_2_mut("append", |s: &mut ImmutableString, s2: ImmutableString| { *s += &s2; Ok(()) });
});
