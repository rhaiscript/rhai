#![cfg(not(feature = "no_index"))]

use crate::any::{Dynamic, Variant};
use crate::def_package;
use crate::engine::{Array, Engine};
use crate::module::{FuncReturn, Module};
use crate::parser::{ImmutableString, INT};

#[cfg(not(feature = "unchecked"))]
use crate::{result::EvalAltResult, token::Position};

use crate::stdlib::{any::TypeId, boxed::Box};

#[cfg(not(feature = "unchecked"))]
use crate::stdlib::string::ToString;

// Register array utility functions
fn push<T: Variant + Clone>(list: &mut Array, item: T) -> FuncReturn<()> {
    list.push(Dynamic::from(item));
    Ok(())
}
fn ins<T: Variant + Clone>(list: &mut Array, position: INT, item: T) -> FuncReturn<()> {
    if position <= 0 {
        list.insert(0, Dynamic::from(item));
    } else if (position as usize) >= list.len() - 1 {
        push(list, item)?;
    } else {
        list.insert(position as usize, Dynamic::from(item));
    }
    Ok(())
}
fn pad<T: Variant + Clone>(
    _engine: &Engine,
    _: &Module,
    args: &mut [&mut Dynamic],
) -> FuncReturn<()> {
    let len = *args[1].read_lock::<INT>().unwrap();

    // Check if array will be over max size limit
    #[cfg(not(feature = "unchecked"))]
    if _engine.limits.max_array_size > 0
        && len > 0
        && (len as usize) > _engine.limits.max_array_size
    {
        return Err(Box::new(EvalAltResult::ErrorDataTooLarge(
            "Size of array".to_string(),
            _engine.limits.max_array_size,
            len as usize,
            Position::none(),
        )));
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

macro_rules! reg_op {
    ($lib:expr, $op:expr, $func:ident, $($par:ty),*) => {
        $( $lib.set_fn_2_mut($op, $func::<$par>); )*
    };
}
macro_rules! reg_tri {
    ($lib:expr, $op:expr, $func:ident, $($par:ty),*) => {
        $( $lib.set_fn_3_mut($op, $func::<$par>); )*
    };
}
macro_rules! reg_pad {
    ($lib:expr, $op:expr, $func:ident, $($par:ty),*) => {
        $({
            $lib.set_raw_fn($op,
                &[TypeId::of::<Array>(), TypeId::of::<INT>(), TypeId::of::<$par>()],
                $func::<$par>
            );
        })*
    };
}

#[cfg(not(feature = "no_index"))]
def_package!(crate:BasicArrayPackage:"Basic array utilities.", lib, {
    reg_op!(lib, "push", push, INT, bool, char, ImmutableString, Array, ());
    reg_pad!(lib, "pad", pad, INT, bool, char, ImmutableString, Array, ());
    reg_tri!(lib, "insert", ins, INT, bool, char, ImmutableString, Array, ());

    lib.set_fn_2_mut("append", |x: &mut Array, y: Array| {
        x.extend(y);
        Ok(())
    });
    lib.set_fn_2_mut("+=", |x: &mut Array, y: Array| {
        x.extend(y);
        Ok(())
    });
    lib.set_fn_2(
        "+",
        |mut x: Array, y: Array| {
            x.extend(y);
            Ok(x)
        },
    );

    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    {
        reg_op!(lib, "push", push, i8, u8, i16, u16, i32, i64, u32, u64);
        reg_pad!(lib, "pad", pad, i8, u8, i16, u16, i32, u32, i64, u64);
        reg_tri!(lib, "insert", ins, i8, u8, i16, u16, i32, i64, u32, u64);

        #[cfg(not(target_arch = "wasm32"))]
        {
            reg_op!(lib, "push", push, i128, u128);
            reg_pad!(lib, "pad", pad, i128, u128);
            reg_tri!(lib, "insert", ins, i128, u128);
        }
    }

    #[cfg(not(feature = "no_float"))]
    {
        reg_op!(lib, "push", push, f32, f64);
        reg_pad!(lib, "pad", pad, f32, f64);
        reg_tri!(lib, "insert", ins, f32, f64);
    }

    lib.set_fn_1_mut(
        "pop",
        |list: &mut Array| Ok(list.pop().unwrap_or_else(|| ().into())),
    );
    lib.set_fn_1_mut(
        "shift",
        |list: &mut Array| {
            Ok(if list.is_empty() {
                ().into()
            } else {
                list.remove(0)
            })
        },
    );
    lib.set_fn_2_mut(
        "remove",
        |list: &mut Array, len: INT| {
            Ok(if len < 0 || (len as usize) >= list.len() {
                ().into()
            } else {
                list.remove(len as usize)
            })
        },
    );
    lib.set_fn_1_mut("len", |list: &mut Array| Ok(list.len() as INT));

    #[cfg(not(feature = "no_object"))]
    lib.set_getter_fn("len", |list: &mut Array| Ok(list.len() as INT));

    lib.set_fn_1_mut("clear", |list: &mut Array| {
        list.clear();
        Ok(())
    });
    lib.set_fn_2_mut(
        "truncate",
        |list: &mut Array, len: INT| {
            if len >= 0 {
                list.truncate(len as usize);
            } else {
                list.clear();
            }
            Ok(())
        },
    );

    // Register array iterator
    lib.set_iter(
        TypeId::of::<Array>(),
        |arr| Box::new(arr.cast::<Array>().into_iter()) as Box<dyn Iterator<Item = Dynamic>>,
    );
});
