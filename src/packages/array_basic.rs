#![cfg(not(feature = "no_index"))]
#![allow(non_snake_case)]

use crate::any::{Dynamic, Variant};
use crate::def_package;
use crate::engine::{Array, Engine};
use crate::fn_native::FnPtr;
use crate::parser::{ImmutableString, INT};
use crate::plugin::*;

#[cfg(not(feature = "unchecked"))]
use crate::{result::EvalAltResult, token::Position};

#[cfg(not(feature = "no_object"))]
use crate::engine::Map;

use crate::stdlib::{any::TypeId, boxed::Box};

#[cfg(not(feature = "unchecked"))]
use crate::stdlib::string::ToString;

#[cfg(feature = "decimal")]
use rust_decimal::Decimal;

pub type Unit = ();

macro_rules! gen_array_functions {
    ($root:ident => $($arg_type:ident),+ ) => {
        pub mod $root { $(pub mod $arg_type {
            use super::super::*;

            #[export_fn]
            #[inline(always)]
            pub fn push(list: &mut Array, item: $arg_type) {
                list.push(Dynamic::from(item));
            }

            #[export_fn]
            pub fn insert(list: &mut Array, position: INT, item: $arg_type) {
                if position <= 0 {
                    list.insert(0, Dynamic::from(item));
                } else if (position as usize) >= list.len() - 1 {
                    push(list, item);
                } else {
                    list.insert(position as usize, Dynamic::from(item));
                }
            }
        })* }
    }
}

macro_rules! reg_functions {
    ($mod_name:ident += $root:ident ; $($arg_type:ident),+) => { $(
        set_exported_fn!($mod_name, "push", $root::$arg_type::push);
        set_exported_fn!($mod_name, "insert", $root::$arg_type::insert);

        $mod_name.set_raw_fn("pad",
            &[TypeId::of::<Array>(), TypeId::of::<INT>(), TypeId::of::<$arg_type>()],
            pad::<$arg_type>);
    )* }
}

def_package!(crate:BasicArrayPackage:"Basic array utilities.", lib, {
    lib.combine_flatten(exported_module!(array_functions));

    reg_functions!(lib += basic; INT, bool, char, ImmutableString, FnPtr, Array, Unit);

    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    {
        reg_functions!(lib += numbers; i8, u8, i16, u16, i32, i64, u32, u64);

        #[cfg(not(target_arch = "wasm32"))]
        reg_functions!(lib += num_128; i128, u128);
    }

    #[cfg(not(feature = "no_float"))]
    reg_functions!(lib += float; f32, f64);

    #[cfg(feature = "decimal")]
    reg_functions!(lib += decimal; Decimal);

    #[cfg(not(feature = "no_object"))]
    reg_functions!(lib += map; Map);

    // Register array iterator
    lib.set_iter(
        TypeId::of::<Array>(),
        |arr| Box::new(arr.cast::<Array>().into_iter()) as Box<dyn Iterator<Item = Dynamic>>,
    );
});

#[export_module]
mod array_functions {
    #[inline(always)]
    pub fn len(list: &mut Array) -> INT {
        list.len() as INT
    }
    #[rhai_fn(get = "len")]
    #[inline(always)]
    pub fn len_prop(list: &mut Array) -> INT {
        len(list)
    }
    #[inline(always)]
    pub fn append(x: &mut Array, y: Array) {
        x.extend(y);
    }
    #[rhai_fn(name = "+=")]
    #[inline(always)]
    pub fn append_operator(x: &mut Array, y: Array) {
        append(x, y)
    }
    #[rhai_fn(name = "+")]
    #[inline]
    pub fn concat(mut x: Array, y: Array) -> Array {
        x.extend(y);
        x
    }
    #[inline]
    pub fn pop(list: &mut Array) -> Dynamic {
        list.pop().unwrap_or_else(|| ().into())
    }
    pub fn shift(list: &mut Array) -> Dynamic {
        if list.is_empty() {
            ().into()
        } else {
            list.remove(0)
        }
    }
    pub fn remove(list: &mut Array, len: INT) -> Dynamic {
        if len < 0 || (len as usize) >= list.len() {
            ().into()
        } else {
            list.remove(len as usize)
        }
    }
    #[inline(always)]
    pub fn clear(list: &mut Array) {
        list.clear();
    }
    pub fn truncate(list: &mut Array, len: INT) {
        if len >= 0 {
            list.truncate(len as usize);
        } else {
            list.clear();
        }
    }
}

fn pad<T: Variant + Clone>(
    _engine: &Engine,
    _: &Module,
    args: &mut [&mut Dynamic],
) -> Result<(), Box<EvalAltResult>> {
    let len = *args[1].read_lock::<INT>().unwrap();

    // Check if array will be over max size limit
    #[cfg(not(feature = "unchecked"))]
    if _engine.limits.max_array_size > 0
        && len > 0
        && (len as usize) > _engine.limits.max_array_size
    {
        return EvalAltResult::ErrorDataTooLarge(
            "Size of array".to_string(),
            _engine.limits.max_array_size,
            len as usize,
            Position::none(),
        )
        .into();
    }

    if len > 0 {
        let item = args[2].clone();
        let mut list = args[0].write_lock::<Array>().unwrap();

        if len as usize > list.len() {
            list.resize(len as usize, item);
        }
    }
    Ok(())
}

gen_array_functions!(basic => INT, bool, char, ImmutableString, FnPtr, Array, Unit);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_array_functions!(numbers => i8, u8, i16, u16, i32, i64, u32, u64);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(target_arch = "wasm32"))]
gen_array_functions!(num_128 => i128, u128);

#[cfg(not(feature = "no_float"))]
gen_array_functions!(float => f32, f64);

#[cfg(feature = "decimal")]
gen_array_functions!(decimal => Decimal);

#[cfg(not(feature = "no_object"))]
gen_array_functions!(map => Map);
