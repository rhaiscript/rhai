use crate::any::Dynamic;
use crate::def_package;
use crate::engine::Engine;
use crate::fn_native::FnPtr;
use crate::module::{FuncReturn, Module};
use crate::parser::{ImmutableString, INT};
use crate::utils::StaticVec;

#[cfg(not(feature = "unchecked"))]
use crate::{result::EvalAltResult, token::Position};

#[cfg(not(feature = "no_index"))]
use crate::engine::Array;

#[cfg(not(feature = "no_decimal"))]
use rust_decimal::Decimal;

use crate::stdlib::{
    any::TypeId,
    boxed::Box,
    fmt::Display,
    format, mem,
    string::{String, ToString},
    vec::Vec,
};

fn prepend<T: Display>(x: T, y: ImmutableString) -> FuncReturn<ImmutableString> {
    Ok(format!("{}{}", x, y).into())
}
fn append<T: Display>(x: ImmutableString, y: T) -> FuncReturn<ImmutableString> {
    Ok(format!("{}{}", x, y).into())
}
fn sub_string(s: ImmutableString, start: INT, len: INT) -> FuncReturn<ImmutableString> {
    let offset = if s.is_empty() || len <= 0 {
        return Ok("".to_string().into());
    } else if start < 0 {
        0
    } else if (start as usize) >= s.chars().count() {
        return Ok("".to_string().into());
    } else {
        start as usize
    };

    let chars: StaticVec<_> = s.chars().collect();

    let len = if offset + (len as usize) > chars.len() {
        chars.len() - offset
    } else {
        len as usize
    };

    Ok(chars
        .iter()
        .skip(offset)
        .take(len)
        .cloned()
        .collect::<String>()
        .into())
}
fn crop_string(s: &mut ImmutableString, start: INT, len: INT) -> FuncReturn<()> {
    let offset = if s.is_empty() || len <= 0 {
        s.make_mut().clear();
        return Ok(());
    } else if start < 0 {
        0
    } else if (start as usize) >= s.chars().count() {
        s.make_mut().clear();
        return Ok(());
    } else {
        start as usize
    };

    let chars: StaticVec<_> = s.chars().collect();

    let len = if offset + (len as usize) > chars.len() {
        chars.len() - offset
    } else {
        len as usize
    };

    let copy = s.make_mut();
    copy.clear();
    copy.extend(chars.iter().skip(offset).take(len));

    Ok(())
}

macro_rules! reg_op {
    ($lib:expr, $op:expr, $func:ident, $($par:ty),*) => {
        $( $lib.set_fn_2($op, $func::<$par>); )*
    };
}

def_package!(crate:MoreStringPackage:"Additional string utilities, including string building.", lib, {
    reg_op!(lib, "+", append, INT, bool, char, FnPtr);
    lib.set_fn_2( "+", |x: ImmutableString, _: ()| Ok(x));

    reg_op!(lib, "+", prepend, INT, bool, char, FnPtr);
    lib.set_fn_2("+", |_: (), y: ImmutableString| Ok(y));

    if cfg!(not(feature = "only_i32")) && cfg!(not(feature = "only_i64")) {
        reg_op!(lib, "+", append, i8, u8, i16, u16, i32, i64, u32, u64);
        reg_op!(lib, "+", prepend, i8, u8, i16, u16, i32, i64, u32, u64);

        if cfg!(not(target_arch = "wasm32")) {
            reg_op!(lib, "+", append, i128, u128);
            reg_op!(lib, "+", prepend, i128, u128);
        }
    }

    #[cfg(not(feature = "no_float"))]
    {
        reg_op!(lib, "+", append, f32, f64);
        reg_op!(lib, "+", prepend, f32, f64);
    }

    #[cfg(not(feature = "no_decimal"))]
    {
        reg_op!(lib, "+", append, Decimal);
        reg_op!(lib, "+", prepend, Decimal);
    }

    #[cfg(not(feature = "no_index"))]
    {
        lib.set_fn_2_mut("+", |x: &mut ImmutableString, y: Array| Ok(format!("{}{:?}", x, y)));
        lib.set_fn_2_mut("+", |x: &mut Array, y: ImmutableString| Ok(format!("{:?}{}", x, y)));
    }

    lib.set_fn_1_mut("len", |s: &mut ImmutableString| Ok(s.chars().count() as INT));

    #[cfg(not(feature = "no_object"))]
    lib.set_getter_fn("len", |s: &mut ImmutableString| Ok(s.chars().count() as INT));

    lib.set_fn_2_mut(
        "contains",
        |s: &mut ImmutableString, ch: char| Ok(s.contains(ch)),
    );
    lib.set_fn_2_mut(
        "contains",
        |s: &mut ImmutableString, find: ImmutableString| Ok(s.contains(find.as_str())),
    );
    lib.set_fn_3_mut(
        "index_of",
        |s: &mut ImmutableString, ch: char, start: INT| {
            let start = if start < 0 {
                0
            } else if (start as usize) >= s.chars().count() {
                return Ok(-1 as INT);
            } else {
                s.chars().take(start as usize).collect::<String>().len()
            };

            Ok(s[start..]
                .find(ch)
                .map(|index| s[0..start + index].chars().count() as INT)
                .unwrap_or(-1 as INT))
        },
    );
    lib.set_fn_2_mut(
        "index_of",
        |s: &mut ImmutableString, ch: char| {
            Ok(s.find(ch)
                .map(|index| s[0..index].chars().count() as INT)
                .unwrap_or(-1 as INT))
        },
    );
    lib.set_fn_3_mut(
        "index_of",
        |s: &mut ImmutableString, find: ImmutableString, start: INT| {
            let start = if start < 0 {
                0
            } else if (start as usize) >= s.chars().count() {
                return Ok(-1 as INT);
            } else {
                s.chars().take(start as usize).collect::<String>().len()
            };

            Ok(s[start..]
                .find(find.as_str())
                .map(|index| s[0..start + index].chars().count() as INT)
                .unwrap_or(-1 as INT))
        },
    );
    lib.set_fn_2_mut(
        "index_of",
        |s: &mut ImmutableString, find: ImmutableString| {
            Ok(s.find(find.as_str())
                .map(|index| s[0..index].chars().count() as INT)
                .unwrap_or(-1 as INT))
        },
    );
    lib.set_fn_1_mut("clear", |s: &mut ImmutableString| {
        s.make_mut().clear();
        Ok(())
    });
    lib.set_fn_2_mut("append", |s: &mut ImmutableString, ch: char| {
        s.make_mut().push(ch);
        Ok(())
    });
    lib.set_fn_2_mut(
        "append",
        |s: &mut ImmutableString, add: ImmutableString| {
            s.make_mut().push_str(add.as_str());
            Ok(())
        }
    );
    lib.set_fn_3("sub_string", sub_string);
    lib.set_fn_2(
        "sub_string",
        |s: ImmutableString, start: INT| {
            let len = s.len() as INT;
            sub_string(s, start, len)
        },
    );
    lib.set_fn_3_mut("crop", crop_string);
    lib.set_fn_2_mut(
        "crop",
        |s: &mut ImmutableString, start: INT| crop_string(s, start, s.len() as INT),
    );
    lib.set_fn_2_mut(
        "truncate",
        |s: &mut ImmutableString, len: INT| {
            if len > 0 {
                let chars: StaticVec<_> = s.chars().collect();
                let copy = s.make_mut();
                copy.clear();
                copy.extend(chars.into_iter().take(len as usize));
            } else {
                s.make_mut().clear();
            }
            Ok(())
        },
    );
    lib.set_raw_fn(
        "pad",
        &[TypeId::of::<ImmutableString>(), TypeId::of::<INT>(), TypeId::of::<char>()],
        |_engine: &Engine, _: &Module, args: &mut [&mut Dynamic]| {
            let len = *args[1].read_lock::< INT>().unwrap();

            // Check if string will be over max size limit
            #[cfg(not(feature = "unchecked"))]
            if _engine.limits.max_string_size > 0 && len > 0 && (len as usize) > _engine.limits.max_string_size {
                return EvalAltResult::ErrorDataTooLarge(
                    "Length of string".to_string(),
                    _engine.limits.max_string_size,
                    len as usize,
                    Position::none(),
                ).into();
            }

            if len > 0 {
                let ch = mem::take(args[2]).cast::<char>();
                let mut s = args[0].write_lock::<ImmutableString>().unwrap();

                let orig_len = s.chars().count();

                if len as usize > orig_len  {
                    let p = s.make_mut();

                    for _ in 0..(len as usize - orig_len) {
                        p.push(ch);
                    }

                    #[cfg(not(feature = "unchecked"))]
                    if _engine.limits.max_string_size > 0 && s.len() > _engine.limits.max_string_size {
                        return EvalAltResult::ErrorDataTooLarge(
                            "Length of string".to_string(),
                            _engine.limits.max_string_size,
                            s.len(),
                            Position::none(),
                        ).into();
                    }
                }
            }

            Ok(())
        },
    );
    lib.set_fn_3_mut(
        "replace",
        |s: &mut ImmutableString, find: ImmutableString, sub: ImmutableString| {
            *s = s.replace(find.as_str(), sub.as_str()).into();
            Ok(())
        },
    );
    lib.set_fn_3_mut(
        "replace",
        |s: &mut ImmutableString, find: ImmutableString, sub: char| {
            *s = s.replace(find.as_str(), &sub.to_string()).into();
            Ok(())
        },
    );
    lib.set_fn_3_mut(
        "replace",
        |s: &mut ImmutableString, find: char, sub: ImmutableString| {
            *s = s.replace(&find.to_string(), sub.as_str()).into();
            Ok(())
        },
    );
    lib.set_fn_3_mut(
        "replace",
        |s: &mut ImmutableString, find: char, sub: char| {
            *s = s.replace(&find.to_string(), &sub.to_string()).into();
            Ok(())
        },
    );
    lib.set_fn_1_mut(
        "trim",
        |s: &mut ImmutableString| {
            let trimmed = s.trim();

            if trimmed.len() < s.len() {
                *s = trimmed.to_string().into();
            }
            Ok(())
        },
    );

    // Register string iterator
    lib.set_iter(
        TypeId::of::<ImmutableString>(),
        |arr| Box::new(
            arr.cast::<ImmutableString>().chars().collect::<Vec<_>>().into_iter().map(Into::into)
        ) as Box<dyn Iterator<Item = Dynamic>>,
    );
});
