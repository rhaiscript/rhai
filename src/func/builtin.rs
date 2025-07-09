//! Built-in implementations for common operators.

#![allow(clippy::float_cmp)]

use super::call::FnCallArgs;
use super::native::FnBuiltin;
#[allow(clippy::enum_glob_use)]
use crate::tokenizer::{Token, Token::*};
use crate::types::dynamic::Union;
use crate::{
    Dynamic, ExclusiveRange, ImmutableString, InclusiveRange, NativeCallContext, RhaiResult,
    SmartString, INT,
};
use std::any::TypeId;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

#[cfg(not(feature = "no_float"))]
#[cfg(feature = "no_std")]
use num_traits::Float;

#[cfg(feature = "decimal")]
use rust_decimal::Decimal;

/// The `unchecked` feature is not active.
const CHECKED_BUILD: bool = cfg!(not(feature = "unchecked"));

/// A function that returns `true`.
#[inline(always)]
#[allow(clippy::unnecessary_wraps)]
fn const_true_fn(_: Option<NativeCallContext>, _: &mut [&mut Dynamic]) -> RhaiResult {
    Ok(Dynamic::TRUE)
}
/// A function that returns `false`.
#[inline(always)]
#[allow(clippy::unnecessary_wraps)]
fn const_false_fn(_: Option<NativeCallContext>, _: &mut [&mut Dynamic]) -> RhaiResult {
    Ok(Dynamic::FALSE)
}
/// Returns true if the type is numeric.
#[inline(always)]
fn is_numeric(typ: TypeId) -> bool {
    if typ == TypeId::of::<INT>() {
        return true;
    }

    #[cfg(not(feature = "no_float"))]
    if typ == TypeId::of::<f32>() || typ == TypeId::of::<f64>() {
        return true;
    }

    #[cfg(feature = "decimal")]
    if typ == TypeId::of::<Decimal>() {
        return true;
    }

    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    if typ == TypeId::of::<u8>()
        || typ == TypeId::of::<u16>()
        || typ == TypeId::of::<u32>()
        || typ == TypeId::of::<u64>()
        || typ == TypeId::of::<i8>()
        || typ == TypeId::of::<i16>()
        || typ == TypeId::of::<i32>()
        || typ == TypeId::of::<i64>()
    {
        return true;
    }

    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    #[cfg(not(target_family = "wasm"))]
    if typ == TypeId::of::<u128>() || typ == TypeId::of::<i128>() {
        return true;
    }

    false
}

/// Build in common binary operator implementations to avoid the cost of calling a registered function.
///
/// The return function will be registered as a _method_, so the first parameter cannot be consumed.
#[must_use]
pub fn get_builtin_binary_op_fn(op: &Token, x: &Dynamic, y: &Dynamic) -> Option<FnBuiltin> {
    macro_rules! impl_op {
        ($xx:ident $op:tt $yy:ident) => { Some((|_, args| {
            let x = &*args[0].read_lock::<$xx>().unwrap();
            let y = &*args[1].read_lock::<$yy>().unwrap();
            Ok((x $op y).into())
        }, false)) };
        ($xx:ident . $func:ident ( $yy:ty )) => { Some((|_, args| {
            let x = &*args[0].read_lock::<$xx>().unwrap();
            let y = &*args[1].read_lock::<$yy>().unwrap();
            Ok(x.$func(y).into())
        }, false)) };
        ($xx:ident . $func:ident ( $yy:ident . $yyy:ident () )) => { Some((|_, args| {
            let x = &*args[0].read_lock::<$xx>().unwrap();
            let y = &*args[1].read_lock::<$yy>().unwrap();
            Ok(x.$func(y.$yyy()).into())
        }, false)) };
        ($func:ident ( $op:tt )) => { Some((|_, args| {
            let (x, y) = $func(args);
            Ok((x $op y).into())
        }, false)) };
        ($base:ty => $xx:ident $op:tt $yy:ident) => { Some((|_, args| {
            let x = args[0].$xx().unwrap() as $base;
            let y = args[1].$yy().unwrap() as $base;
            Ok((x $op y).into())
        }, false)) };
        ($base:ty => $xx:ident . $func:ident ( $yy:ident as $yyy:ty)) => { Some((|_, args| {
            let x = args[0].$xx().unwrap() as $base;
            let y = args[1].$yy().unwrap() as $base;
            Ok(x.$func(y as $yyy).into())
        }, false)) };
        ($base:ty => Ok($func:ident ( $xx:ident, $yy:ident ))) => { Some((|_, args| {
            let x = args[0].$xx().unwrap() as $base;
            let y = args[1].$yy().unwrap() as $base;
            Ok($func(x, y).into())
        }, false)) };
        ($base:ty => $func:ident ( $xx:ident, $yy:ident )) => { Some((|_, args| {
            let x = args[0].$xx().unwrap() as $base;
            let y = args[1].$yy().unwrap() as $base;
            $func(x, y).map(Into::into)
        }, false)) };
        (from $base:ty => $xx:ident $op:tt $yy:ident) => { Some((|_, args| {
            let x = <$base>::from(args[0].$xx().unwrap());
            let y = <$base>::from(args[1].$yy().unwrap());
            Ok((x $op y).into())
        }, false)) };
        (from $base:ty => $xx:ident . $func:ident ( $yy:ident )) => { Some((|_, args| {
            let x = <$base>::from(args[0].$xx().unwrap());
            let y = <$base>::from(args[1].$yy().unwrap());
            Ok(x.$func(y).into())
        }, false)) };
        (from $base:ty => Ok($func:ident ( $xx:ident, $yy:ident ))) => { Some((|_, args| {
            let x = <$base>::from(args[0].$xx().unwrap());
            let y = <$base>::from(args[1].$yy().unwrap());
            Ok($func(x, y).into())
        }, false)) };
        (from $base:ty => $func:ident ( $xx:ident, $yy:ident )) => { Some((|_, args| {
            let x = <$base>::from(args[0].$xx().unwrap());
            let y = <$base>::from(args[1].$yy().unwrap());
            $func(x, y).map(Into::into)
        }, false)) };
    }

    #[cfg(not(feature = "no_float"))]
    macro_rules! impl_float {
        ($xx:ident, $yy:ident) => {
            return match op {
                Plus                => impl_op!(FLOAT => $xx + $yy),
                Minus               => impl_op!(FLOAT => $xx - $yy),
                Multiply            => impl_op!(FLOAT => $xx * $yy),
                Divide              => impl_op!(FLOAT => $xx / $yy),
                Modulo              => impl_op!(FLOAT => $xx % $yy),
                PowerOf             => impl_op!(FLOAT => $xx.powf($yy as FLOAT)),

                #[cfg(feature = "unchecked")]
                EqualsTo            => impl_op!(FLOAT => $xx == $yy),
                #[cfg(not(feature = "unchecked"))]
                EqualsTo            => Some((|_, args| {
                    let x = args[0].$xx().unwrap() as FLOAT;
                    let y = args[1].$yy().unwrap() as FLOAT;
                    let max = if x * y == 0.0 { 1.0 } else { x.abs().max(y.abs()) };
                    if max == 0.0 { return Ok(Dynamic::TRUE); }
                    Ok(((x - y).abs()/max <= FLOAT::EPSILON).into())
                }, false)),

                #[cfg(feature = "unchecked")]
                NotEqualsTo         => impl_op!(FLOAT => $xx != $yy),
                #[cfg(not(feature = "unchecked"))]
                NotEqualsTo         => Some((|_, args| {
                    let x = args[0].$xx().unwrap() as FLOAT;
                    let y = args[1].$yy().unwrap() as FLOAT;
                    let max = if x * y == 0.0 { 1.0 } else { x.abs().max(y.abs()) };
                    if max == 0.0 { return Ok(Dynamic::FALSE); }
                    Ok(((x - y).abs()/max > FLOAT::EPSILON).into())
                }, false)),

                #[cfg(feature = "unchecked")]
                GreaterThan         => impl_op!(FLOAT => $xx > $yy),
                #[cfg(not(feature = "unchecked"))]
                GreaterThan         => Some((|_, args| {
                    let x = args[0].$xx().unwrap() as FLOAT;
                    let y = args[1].$yy().unwrap() as FLOAT;
                    let max = if x * y == 0.0 { 1.0 } else { x.abs().max(y.abs()) };
                    if max == 0.0 { return Ok(Dynamic::FALSE); }
                    Ok(((x - y)/max > FLOAT::EPSILON).into())
                }, false)),

                #[cfg(feature = "unchecked")]
                GreaterThanEqualsTo => impl_op!(FLOAT => $xx >= $yy),
                #[cfg(not(feature = "unchecked"))]
                GreaterThanEqualsTo => Some((|_, args| {
                    let x = args[0].$xx().unwrap() as FLOAT;
                    let y = args[1].$yy().unwrap() as FLOAT;
                    let max = if x * y == 0.0 { 1.0 } else { x.abs().max(y.abs()) };
                    if max == 0.0 { return Ok(Dynamic::TRUE); }
                    Ok(((x - y)/max > -FLOAT::EPSILON).into())
                }, false)),

                #[cfg(feature = "unchecked")]
                LessThan            => impl_op!(FLOAT => $xx < $yy),
                #[cfg(not(feature = "unchecked"))]
                LessThan            => Some((|_, args| {
                    let x = args[0].$xx().unwrap() as FLOAT;
                    let y = args[1].$yy().unwrap() as FLOAT;
                    let max = if x * y == 0.0 { 1.0 } else { x.abs().max(y.abs()) };
                    if max == 0.0 { return Ok(Dynamic::FALSE); }
                    Ok(((y - x)/max > FLOAT::EPSILON).into())
                }, false)),

                #[cfg(feature = "unchecked")]
                LessThanEqualsTo    => impl_op!(FLOAT => $xx <= $yy),
                #[cfg(not(feature = "unchecked"))]
                LessThanEqualsTo    => Some((|_, args| {
                    let x = args[0].$xx().unwrap() as FLOAT;
                    let y = args[1].$yy().unwrap() as FLOAT;
                    let max = if x * y == 0.0 { 1.0 } else { x.abs().max(y.abs()) };
                    if max == 0.0 { return Ok(Dynamic::TRUE); }
                    Ok(((y - x)/max > -FLOAT::EPSILON).into())
                }, false)),

                _                   => None,
            }
        };
    }

    #[cfg(feature = "decimal")]
    macro_rules! impl_decimal {
        ($xx:ident, $yy:ident) => {
            {
                #[cfg(not(feature = "unchecked"))]
                #[allow(clippy::wildcard_imports)]
                use crate::packages::arithmetic::decimal_functions::builtin::*;

                #[cfg(not(feature = "unchecked"))]
                match op {
                    Plus     => return impl_op!(from Decimal => add($xx, $yy)),
                    Minus    => return impl_op!(from Decimal => subtract($xx, $yy)),
                    Multiply => return impl_op!(from Decimal => multiply($xx, $yy)),
                    Divide   => return impl_op!(from Decimal => divide($xx, $yy)),
                    Modulo   => return impl_op!(from Decimal => modulo($xx, $yy)),
                    PowerOf  => return impl_op!(from Decimal => power($xx, $yy)),
                    _        => ()
                }

                #[cfg(feature = "unchecked")]
                use rust_decimal::MathematicalOps;

                #[cfg(feature = "unchecked")]
                match op {
                    Plus     => return impl_op!(from Decimal => $xx + $yy),
                    Minus    => return impl_op!(from Decimal => $xx - $yy),
                    Multiply => return impl_op!(from Decimal => $xx * $yy),
                    Divide   => return impl_op!(from Decimal => $xx / $yy),
                    Modulo   => return impl_op!(from Decimal => $xx % $yy),
                    PowerOf  => return impl_op!(from Decimal => $xx.powd($yy)),
                    _        => ()
                }

                return match op {
                    EqualsTo            => impl_op!(from Decimal => $xx == $yy),
                    NotEqualsTo         => impl_op!(from Decimal => $xx != $yy),
                    GreaterThan         => impl_op!(from Decimal => $xx > $yy),
                    GreaterThanEqualsTo => impl_op!(from Decimal => $xx >= $yy),
                    LessThan            => impl_op!(from Decimal => $xx < $yy),
                    LessThanEqualsTo    => impl_op!(from Decimal => $xx <= $yy),
                    _                   => None
                };
            }
        };
    }

    // Check for common patterns
    match (&x.0, &y.0, op) {
        (Union::Int(..), Union::Int(..), _) => {
            #[cfg(not(feature = "unchecked"))]
            #[allow(clippy::wildcard_imports)]
            use crate::packages::arithmetic::arith_basic::INT::functions::*;

            #[cfg(not(feature = "unchecked"))]
            match op {
                Plus => return impl_op!(INT => add(as_int, as_int)),
                Minus => return impl_op!(INT => subtract(as_int, as_int)),
                Multiply => return impl_op!(INT => multiply(as_int, as_int)),
                Divide => return impl_op!(INT => divide(as_int, as_int)),
                Modulo => return impl_op!(INT => modulo(as_int, as_int)),
                PowerOf => return impl_op!(INT => power(as_int, as_int)),
                RightShift => return impl_op!(INT => Ok(shift_right(as_int, as_int))),
                LeftShift => return impl_op!(INT => Ok(shift_left(as_int, as_int))),
                _ => (),
            }

            #[cfg(feature = "unchecked")]
            match op {
                Plus => return impl_op!(INT => as_int + as_int),
                Minus => return impl_op!(INT => as_int - as_int),
                Multiply => return impl_op!(INT => as_int * as_int),
                Divide => return impl_op!(INT => as_int / as_int),
                Modulo => return impl_op!(INT => as_int % as_int),
                PowerOf => return impl_op!(INT => as_int.pow(as_int as u32)),
                RightShift => {
                    return Some((
                        |_, args| {
                            let x = args[0].as_int().unwrap();
                            let y = args[1].as_int().unwrap();
                            Ok((if y < 0 { x << -y } else { x >> y }).into())
                        },
                        false,
                    ))
                }
                LeftShift => {
                    return Some((
                        |_, args| {
                            let x = args[0].as_int().unwrap();
                            let y = args[1].as_int().unwrap();
                            Ok((if y < 0 { x >> -y } else { x << y }).into())
                        },
                        false,
                    ))
                }
                _ => (),
            }

            return match op {
                EqualsTo => impl_op!(INT => as_int == as_int),
                NotEqualsTo => impl_op!(INT => as_int != as_int),
                GreaterThan => impl_op!(INT => as_int > as_int),
                GreaterThanEqualsTo => impl_op!(INT => as_int >= as_int),
                LessThan => impl_op!(INT => as_int < as_int),
                LessThanEqualsTo => impl_op!(INT => as_int <= as_int),
                Ampersand => impl_op!(INT => as_int & as_int),
                Pipe => impl_op!(INT => as_int | as_int),
                XOr => impl_op!(INT => as_int ^ as_int),
                ExclusiveRange => impl_op!(INT => as_int .. as_int),
                InclusiveRange => impl_op!(INT => as_int ..= as_int),
                _ => None,
            };
        }

        (Union::Bool(..), Union::Bool(..), _) => {
            return match op {
                EqualsTo => impl_op!(bool => as_bool == as_bool),
                NotEqualsTo => impl_op!(bool => as_bool != as_bool),
                GreaterThan => impl_op!(bool => as_bool > as_bool),
                GreaterThanEqualsTo => impl_op!(bool => as_bool >= as_bool),
                LessThan => impl_op!(bool => as_bool < as_bool),
                LessThanEqualsTo => impl_op!(bool => as_bool <= as_bool),
                Ampersand => impl_op!(bool => as_bool & as_bool),
                Pipe => impl_op!(bool => as_bool | as_bool),
                XOr => impl_op!(bool => as_bool ^ as_bool),
                _ => None,
            };
        }

        (Union::Str(..), Union::Str(..), _) => {
            return match op {
                Plus => Some((
                    |_ctx, args| {
                        let s1 = &*args[0].as_immutable_string_ref().unwrap();
                        let s2 = &*args[1].as_immutable_string_ref().unwrap();

                        #[cfg(not(feature = "unchecked"))]
                        _ctx.unwrap()
                            .engine()
                            .throw_on_size((0, 0, s1.len() + s2.len()))?;

                        Ok((s1 + s2).into())
                    },
                    CHECKED_BUILD,
                )),
                Minus => impl_op!(ImmutableString - ImmutableString),
                EqualsTo => impl_op!(ImmutableString == ImmutableString),
                NotEqualsTo => impl_op!(ImmutableString != ImmutableString),
                GreaterThan => impl_op!(ImmutableString > ImmutableString),
                GreaterThanEqualsTo => impl_op!(ImmutableString >= ImmutableString),
                LessThan => impl_op!(ImmutableString < ImmutableString),
                LessThanEqualsTo => impl_op!(ImmutableString <= ImmutableString),
                _ => None,
            };
        }

        (Union::Char(..), Union::Char(..), _) => {
            return match op {
                Plus => Some((
                    |_ctx, args| {
                        let x = args[0].as_char().unwrap();
                        let y = args[1].as_char().unwrap();

                        let mut result = SmartString::new_const();
                        result.push(x);
                        result.push(y);

                        #[cfg(not(feature = "unchecked"))]
                        _ctx.unwrap().engine().throw_on_size((0, 0, result.len()))?;

                        Ok(result.into())
                    },
                    CHECKED_BUILD,
                )),
                EqualsTo => impl_op!(char => as_char == as_char),
                NotEqualsTo => impl_op!(char => as_char != as_char),
                GreaterThan => impl_op!(char => as_char > as_char),
                GreaterThanEqualsTo => impl_op!(char => as_char >= as_char),
                LessThan => impl_op!(char => as_char < as_char),
                LessThanEqualsTo => impl_op!(char => as_char <= as_char),
                _ => None,
            };
        }

        #[cfg(not(feature = "no_index"))]
        (Union::Blob(..), Union::Blob(..), _) => {
            use crate::Blob;

            return match op {
                Plus => Some((
                    |_ctx, args| {
                        let b2 = &*args[1].as_blob_ref().unwrap();
                        if b2.is_empty() {
                            return Ok(args[0].flatten_clone());
                        }
                        let b1 = &*args[0].as_blob_ref().unwrap();
                        if b1.is_empty() {
                            return Ok(args[1].flatten_clone());
                        }

                        #[cfg(not(feature = "unchecked"))]
                        _ctx.unwrap()
                            .engine()
                            .throw_on_size((b1.len() + b2.len(), 0, 0))?;

                        let mut blob = b1.clone();
                        blob.extend(b2);
                        Ok(Dynamic::from_blob(blob))
                    },
                    CHECKED_BUILD,
                )),
                EqualsTo => impl_op!(Blob == Blob),
                NotEqualsTo => impl_op!(Blob != Blob),
                _ => None,
            };
        }

        (Union::Unit(..), Union::Unit(..), _) => {
            return match op {
                EqualsTo => Some((const_true_fn, false)),
                NotEqualsTo | GreaterThan | GreaterThanEqualsTo | LessThan | LessThanEqualsTo => {
                    Some((const_false_fn, false))
                }
                _ => None,
            };
        }

        #[cfg(not(feature = "no_float"))]
        (Union::Float(..), Union::Float(..), _) => {
            impl_float!(as_float, as_float)
        }

        #[cfg(not(feature = "no_float"))]
        (Union::Float(..), Union::Int(..), _) => {
            impl_float!(as_float, as_int)
        }

        #[cfg(not(feature = "no_float"))]
        (Union::Int(..), Union::Float(..), _) => {
            impl_float!(as_int, as_float)
        }

        #[cfg(feature = "decimal")]
        (Union::Decimal(..), Union::Decimal(..), _) => {
            impl_decimal!(as_decimal, as_decimal)
        }
        #[cfg(feature = "decimal")]
        (Union::Decimal(..), Union::Int(..), _) => {
            impl_decimal!(as_decimal, as_int)
        }
        #[cfg(feature = "decimal")]
        (Union::Int(..), Union::Decimal(..), _) => {
            impl_decimal!(as_int, as_decimal)
        }

        // Ranges
        (Union::Int(..), Union::Unit(..), ExclusiveRange) => {
            return Some((
                |_ctx, args| Ok((args[0].as_int().unwrap()..INT::MAX).into()),
                false,
            ))
        }
        (Union::Unit(..), Union::Int(..), ExclusiveRange) => {
            return Some((
                |_ctx, args| Ok((0..args[1].as_int().unwrap()).into()),
                false,
            ))
        }
        (Union::Int(..), Union::Unit(..), InclusiveRange) => {
            return Some((
                |_ctx, args| Ok((args[0].as_int().unwrap()..=INT::MAX).into()),
                false,
            ))
        }
        (Union::Unit(..), Union::Int(..), InclusiveRange) => {
            return Some((
                |_ctx, args| Ok((0..=args[1].as_int().unwrap()).into()),
                false,
            ))
        }

        // char op string
        (Union::Char(..), Union::Str(..), _) => {
            fn get_s1s2(args: &FnCallArgs) -> ([Option<char>; 2], [Option<char>; 2]) {
                let x = args[0].as_char().unwrap();
                let y = &*args[1].as_immutable_string_ref().unwrap();
                let s1 = [Some(x), None];
                let mut y = y.chars();
                let s2 = [y.next(), y.next()];
                (s1, s2)
            }

            return match op {
                Plus => Some((
                    |_ctx, args| {
                        let x = args[0].as_char().unwrap();
                        let y = &*args[1].as_immutable_string_ref().unwrap();

                        let mut result = SmartString::new_const();
                        result.push(x);
                        result.push_str(y);

                        #[cfg(not(feature = "unchecked"))]
                        _ctx.unwrap().engine().throw_on_size((0, 0, result.len()))?;

                        Ok(result.into())
                    },
                    CHECKED_BUILD,
                )),
                EqualsTo => impl_op!(get_s1s2(==)),
                NotEqualsTo => impl_op!(get_s1s2(!=)),
                GreaterThan => impl_op!(get_s1s2(>)),
                GreaterThanEqualsTo => impl_op!(get_s1s2(>=)),
                LessThan => impl_op!(get_s1s2(<)),
                LessThanEqualsTo => impl_op!(get_s1s2(<=)),
                _ => None,
            };
        }
        // string op char
        (Union::Str(..), Union::Char(..), _) => {
            fn get_s1s2(args: &FnCallArgs) -> ([Option<char>; 2], [Option<char>; 2]) {
                let x = &*args[0].as_immutable_string_ref().unwrap();
                let y = args[1].as_char().unwrap();
                let mut x = x.chars();
                let s1 = [x.next(), x.next()];
                let s2 = [Some(y), None];
                (s1, s2)
            }

            return match op {
                Plus => Some((
                    |_ctx, args| {
                        let x = &*args[0].as_immutable_string_ref().unwrap();
                        let y = args[1].as_char().unwrap();
                        let result = x + y;

                        #[cfg(not(feature = "unchecked"))]
                        _ctx.unwrap().engine().throw_on_size((0, 0, result.len()))?;

                        Ok(result.into())
                    },
                    CHECKED_BUILD,
                )),
                Minus => Some((
                    |_, args| {
                        let x = &*args[0].as_immutable_string_ref().unwrap();
                        let y = args[1].as_char().unwrap();
                        Ok((x - y).into())
                    },
                    false,
                )),
                EqualsTo => impl_op!(get_s1s2(==)),
                NotEqualsTo => impl_op!(get_s1s2(!=)),
                GreaterThan => impl_op!(get_s1s2(>)),
                GreaterThanEqualsTo => impl_op!(get_s1s2(>=)),
                LessThan => impl_op!(get_s1s2(<)),
                LessThanEqualsTo => impl_op!(get_s1s2(<=)),
                _ => None,
            };
        }
        // () op string
        (Union::Unit(..), Union::Str(..), _) => {
            return match op {
                Plus => Some((|_, args| Ok(args[1].clone()), false)),
                EqualsTo | GreaterThan | GreaterThanEqualsTo | LessThan | LessThanEqualsTo => {
                    Some((const_false_fn, false))
                }
                NotEqualsTo => Some((const_true_fn, false)),
                _ => None,
            }
        }
        // string op ()
        (Union::Str(..), Union::Unit(..), _) => {
            return match op {
                Plus => Some((|_, args| Ok(args[0].clone()), false)),
                EqualsTo | GreaterThan | GreaterThanEqualsTo | LessThan | LessThanEqualsTo => {
                    Some((const_false_fn, false))
                }
                NotEqualsTo => Some((const_true_fn, false)),
                _ => None,
            };
        }

        // blob
        #[cfg(not(feature = "no_index"))]
        (Union::Blob(..), Union::Char(..), _) => {
            return match op {
                Plus => Some((
                    |_ctx, args| {
                        let mut blob = args[0].as_blob_ref().unwrap().clone();
                        let mut buf = [0_u8; 4];
                        let x = args[1].as_char().unwrap().encode_utf8(&mut buf);

                        #[cfg(not(feature = "unchecked"))]
                        _ctx.unwrap()
                            .engine()
                            .throw_on_size((blob.len() + x.len(), 0, 0))?;

                        blob.extend(x.as_bytes());
                        Ok(Dynamic::from_blob(blob))
                    },
                    CHECKED_BUILD,
                )),
                _ => None,
            }
        }

        _ => (),
    }

    // Check detailed types

    let type1 = x.type_id();
    let type2 = y.type_id();

    if type1 == TypeId::of::<ExclusiveRange>() && type2 == TypeId::of::<ExclusiveRange>() {
        return match op {
            EqualsTo => impl_op!(ExclusiveRange == ExclusiveRange),
            NotEqualsTo => impl_op!(ExclusiveRange != ExclusiveRange),
            _ => None,
        };
    }
    if type1 == TypeId::of::<InclusiveRange>() && type2 == TypeId::of::<InclusiveRange>() {
        return match op {
            EqualsTo => impl_op!(InclusiveRange == InclusiveRange),
            NotEqualsTo => impl_op!(InclusiveRange != InclusiveRange),
            _ => None,
        };
    }

    // Incompatible ranges
    if (type1 == TypeId::of::<ExclusiveRange>() && type2 == TypeId::of::<InclusiveRange>())
        || (type1 == TypeId::of::<InclusiveRange>() && type2 == TypeId::of::<ExclusiveRange>())
    {
        return match op {
            NotEqualsTo => Some((const_true_fn, false)),
            Equals => Some((const_false_fn, false)),
            _ => None,
        };
    }

    // Default comparison operators for different, non-numeric types
    if type1 != type2 {
        return match op {
            NotEqualsTo if !is_numeric(type1) || !is_numeric(type2) => Some((const_true_fn, false)),
            EqualsTo | GreaterThan | GreaterThanEqualsTo | LessThan | LessThanEqualsTo
                if !is_numeric(type1) || !is_numeric(type2) =>
            {
                Some((const_false_fn, false))
            }
            _ => None,
        };
    }

    // Same type but no built-in
    None
}

/// Build in common operator assignment implementations to avoid the cost of calling a registered function.
///
/// The return function is registered as a _method_, so the first parameter cannot be consumed.
#[must_use]
pub fn get_builtin_op_assignment_fn(op: &Token, x: &Dynamic, y: &Dynamic) -> Option<FnBuiltin> {
    macro_rules! impl_op {
        ($x:ty = x $op:tt $yy:ident) => { Some((|_, args| {
            let x = args[0].$yy().unwrap();
            let y = args[1].$yy().unwrap() as $x;
            Ok((*args[0].write_lock::<$x>().unwrap() = x $op y).into())
        }, false)) };
        ($x:ident $op:tt $yy:ident) => { Some((|_, args| {
            let y = args[1].$yy().unwrap() as $x;
            Ok((*args[0].write_lock::<$x>().unwrap() $op y).into())
        }, false)) };
        ($x:ident $op:tt $yy:ident as $yyy:ty) => { Some((|_, args| {
            let y = args[1].$yy().unwrap() as $yyy;
            Ok((*args[0].write_lock::<$x>().unwrap() $op y).into())
        }, false)) };
        ($x:ty => $xx:ident . $func:ident ( $yy:ident as $yyy:ty )) => { Some((|_, args| {
            let x = args[0].$xx().unwrap();
            let y = args[1].$yy().unwrap() as $x;
            Ok((*args[0].write_lock::<$x>().unwrap() = x.$func(y as $yyy)).into())
        }, false)) };
        ($x:ty => Ok($func:ident ( $xx:ident, $yy:ident ))) => { Some((|_, args| {
            let x = args[0].$xx().unwrap();
            let y = args[1].$yy().unwrap() as $x;
            let v: Dynamic = $func(x, y).into();
            Ok((*args[0].write_lock().unwrap() = v).into())
        }, false)) };
        ($x:ty => $func:ident ( $xx:ident, $yy:ident )) => { Some((|_, args| {
            let x = args[0].$xx().unwrap();
            let y = args[1].$yy().unwrap() as $x;
            Ok((*args[0].write_lock().unwrap() = $func(x, y)?).into())
        }, false)) };
        (from $x:ident $op:tt $yy:ident) => { Some((|_, args| {
            let y = <$x>::from(args[1].$yy().unwrap());
            Ok((*args[0].write_lock::<$x>().unwrap() $op y).into())
        }, false)) };
        (from $x:ty => $xx:ident . $func:ident ( $yy:ident )) => { Some((|_, args| {
            let x = args[0].$xx().unwrap();
            let y = <$x>::from(args[1].$yy().unwrap());
            Ok((*args[0].write_lock::<$x>().unwrap() = x.$func(y)).into())
        }, false)) };
        (from $x:ty => Ok($func:ident ( $xx:ident, $yy:ident ))) => { Some((|_, args| {
            let x = args[0].$xx().unwrap();
            let y = <$x>::from(args[1].$yy().unwrap());
            Ok((*args[0].write_lock().unwrap() = $func(x, y).into()).into())
        }, false)) };
        (from $x:ty => $func:ident ( $xx:ident, $yy:ident )) => { Some((|_, args| {
            let x = args[0].$xx().unwrap();
            let y = <$x>::from(args[1].$yy().unwrap());
            Ok((*args[0].write_lock().unwrap() = $func(x, y)?).into())
        }, false)) };
    }

    #[cfg(not(feature = "no_float"))]
    macro_rules! impl_float {
        ($x:ident, $xx:ident, $yy:ident) => {
            return match op {
                PlusAssign      => impl_op!($x += $yy),
                MinusAssign     => impl_op!($x -= $yy),
                MultiplyAssign  => impl_op!($x *= $yy),
                DivideAssign    => impl_op!($x /= $yy),
                ModuloAssign    => impl_op!($x %= $yy),
                PowerOfAssign   => impl_op!($x => $xx.powf($yy as $x)),
                _               => None,
            }
        }
    }

    #[cfg(feature = "decimal")]
    macro_rules! impl_decimal {
        ($x:ident, $xx:ident, $yy:ident) => {
            {
                #[cfg(not(feature = "unchecked"))]
                #[allow(clippy::wildcard_imports)]
                use crate::packages::arithmetic::decimal_functions::builtin::*;

                #[cfg(not(feature = "unchecked"))]
                return match op {
                    PlusAssign      => impl_op!(from $x => add($xx, $yy)),
                    MinusAssign     => impl_op!(from $x => subtract($xx, $yy)),
                    MultiplyAssign  => impl_op!(from $x => multiply($xx, $yy)),
                    DivideAssign    => impl_op!(from $x => divide($xx, $yy)),
                    ModuloAssign    => impl_op!(from $x => modulo($xx, $yy)),
                    PowerOfAssign   => impl_op!(from $x => power($xx, $yy)),
                    _               => None,
                };

                #[cfg(feature = "unchecked")]
                use rust_decimal::MathematicalOps;

                #[cfg(feature = "unchecked")]
                return match op {
                    PlusAssign      => impl_op!(from $x += $yy),
                    MinusAssign     => impl_op!(from $x -= $yy),
                    MultiplyAssign  => impl_op!(from $x *= $yy),
                    DivideAssign    => impl_op!(from $x /= $yy),
                    ModuloAssign    => impl_op!(from $x %= $yy),
                    PowerOfAssign   => impl_op!(from $x => $xx.powd($yy)),
                    _               => None,
                };
            }
        };
    }

    // Check for common patterns
    match (&x.0, &y.0, op) {
        (Union::Int(..), Union::Int(..), _) => {
            #[cfg(not(feature = "unchecked"))]
            #[allow(clippy::wildcard_imports)]
            use crate::packages::arithmetic::arith_basic::INT::functions::*;

            #[cfg(not(feature = "unchecked"))]
            match op {
                PlusAssign => return impl_op!(INT => add(as_int, as_int)),
                MinusAssign => return impl_op!(INT => subtract(as_int, as_int)),
                MultiplyAssign => return impl_op!(INT => multiply(as_int, as_int)),
                DivideAssign => return impl_op!(INT => divide(as_int, as_int)),
                ModuloAssign => return impl_op!(INT => modulo(as_int, as_int)),
                PowerOfAssign => return impl_op!(INT => power(as_int, as_int)),
                RightShiftAssign => return impl_op!(INT => Ok(shift_right(as_int, as_int))),
                LeftShiftAssign => return impl_op!(INT => Ok(shift_left(as_int, as_int))),
                _ => (),
            }

            #[cfg(feature = "unchecked")]
            match op {
                PlusAssign => return impl_op!(INT += as_int),
                MinusAssign => return impl_op!(INT -= as_int),
                MultiplyAssign => return impl_op!(INT *= as_int),
                DivideAssign => return impl_op!(INT /= as_int),
                ModuloAssign => return impl_op!(INT %= as_int),
                PowerOfAssign => return impl_op!(INT => as_int.pow(as_int as u32)),
                RightShiftAssign => {
                    return Some((
                        |_, args| {
                            let x = args[0].as_int().unwrap();
                            let y = args[1].as_int().unwrap();
                            let v = if y < 0 { x << -y } else { x >> y };
                            *args[0].write_lock::<Dynamic>().unwrap() = v.into();
                            Ok(Dynamic::UNIT)
                        },
                        false,
                    ))
                }
                LeftShiftAssign => {
                    return Some((
                        |_, args| {
                            let x = args[0].as_int().unwrap();
                            let y = args[1].as_int().unwrap();
                            let v = if y < 0 { x >> -y } else { x << y };
                            *args[0].write_lock::<Dynamic>().unwrap() = v.into();
                            Ok(Dynamic::UNIT)
                        },
                        false,
                    ))
                }
                _ => (),
            }

            match op {
                AndAssign => impl_op!(INT &= as_int),
                OrAssign => impl_op!(INT |= as_int),
                XOrAssign => impl_op!(INT ^= as_int),
                _ => None,
            }
        }

        (Union::Bool(..), Union::Bool(..), _) => match op {
            AndAssign => impl_op!(bool = x && as_bool),
            OrAssign => impl_op!(bool = x || as_bool),
            XOrAssign => impl_op!(bool = x ^ as_bool),
            _ => None,
        },

        // char += char
        (Union::Char(..), Union::Char(..), PlusAssign) => Some((
            |_, args| {
                let y = args[1].as_char().unwrap();
                let x = &mut *args[0].write_lock::<Dynamic>().unwrap();

                let mut buf = SmartString::new_const();
                buf.push(x.as_char().unwrap());
                buf.push(y);

                *x = buf.into();

                Ok(Dynamic::UNIT)
            },
            false,
        )),

        (Union::Str(..), Union::Str(..), _) => match op {
            PlusAssign => Some((
                |_ctx, args| {
                    let (first, second) = args.split_first_mut().unwrap();
                    let x = &mut *first.as_immutable_string_mut().unwrap();
                    let y = &*second[0].as_immutable_string_ref().unwrap();

                    #[cfg(not(feature = "unchecked"))]
                    if !x.is_empty() && !y.is_empty() {
                        let total_len = x.len() + y.len();
                        _ctx.unwrap().engine().throw_on_size((0, 0, total_len))?;
                    }

                    *x += y;

                    Ok(Dynamic::UNIT)
                },
                CHECKED_BUILD,
            )),
            MinusAssign => Some((
                |_, args| {
                    let (first, second) = args.split_first_mut().unwrap();
                    let x = &mut *first.as_immutable_string_mut().unwrap();
                    let y = &*second[0].as_immutable_string_ref().unwrap();
                    *x -= y;
                    Ok(Dynamic::UNIT)
                },
                false,
            )),
            _ => None,
        },

        // array += array
        #[cfg(not(feature = "no_index"))]
        (Union::Array(..), Union::Array(..), PlusAssign) => {
            #[allow(clippy::wildcard_imports)]
            use crate::packages::array_basic::array_functions::*;

            Some((
                |_ctx, args| {
                    let x = args[1].take().into_array().unwrap();

                    if x.is_empty() {
                        return Ok(Dynamic::UNIT);
                    }

                    #[cfg(not(feature = "unchecked"))]
                    if !args[0].as_array_ref().unwrap().is_empty() {
                        _ctx.unwrap().engine().check_data_size(
                            &*args[0].read_lock().unwrap(),
                            crate::Position::NONE,
                        )?;
                    }

                    let array = &mut *args[0].as_array_mut().unwrap();

                    append(array, x);

                    Ok(Dynamic::UNIT)
                },
                CHECKED_BUILD,
            ))
        }

        // blob += blob
        #[cfg(not(feature = "no_index"))]
        (Union::Blob(..), Union::Blob(..), PlusAssign) => {
            #[allow(clippy::wildcard_imports)]
            use crate::packages::blob_basic::blob_functions::*;

            Some((
                |_ctx, args| {
                    let blob2 = args[1].take().into_blob().unwrap();
                    let blob1 = &mut *args[0].as_blob_mut().unwrap();

                    #[cfg(not(feature = "unchecked"))]
                    _ctx.unwrap()
                        .engine()
                        .throw_on_size((blob1.len() + blob2.len(), 0, 0))?;

                    append(blob1, blob2);

                    Ok(Dynamic::UNIT)
                },
                CHECKED_BUILD,
            ))
        }

        #[cfg(not(feature = "no_float"))]
        (Union::Float(..), Union::Float(..), _) => {
            impl_float!(FLOAT, as_float, as_float)
        }

        #[cfg(not(feature = "no_float"))]
        (Union::Float(..), Union::Int(..), _) => {
            impl_float!(FLOAT, as_float, as_int)
        }

        #[cfg(feature = "decimal")]
        (Union::Decimal(..), Union::Decimal(..), _) => {
            impl_decimal!(Decimal, as_decimal, as_decimal)
        }

        #[cfg(feature = "decimal")]
        (Union::Decimal(..), Union::Int(..), _) => {
            impl_decimal!(Decimal, as_decimal, as_int)
        }

        // string op= char
        (Union::Str(..), Union::Char(..), _) => match op {
            PlusAssign => Some((
                |_ctx, args| {
                    let mut buf = [0_u8; 4];
                    let ch = &*args[1].as_char().unwrap().encode_utf8(&mut buf);
                    let mut x = args[0].as_immutable_string_mut().unwrap();

                    #[cfg(not(feature = "unchecked"))]
                    _ctx.unwrap()
                        .engine()
                        .throw_on_size((0, 0, x.len() + ch.len()))?;

                    *x += ch;

                    Ok(Dynamic::UNIT)
                },
                CHECKED_BUILD,
            )),
            MinusAssign => impl_op!(ImmutableString -= as_char as char),
            _ => None,
        },
        // char += string
        (Union::Char(..), Union::Str(..), PlusAssign) => Some((
            |_ctx, args| {
                let ch = {
                    let s = &*args[1].as_immutable_string_ref().unwrap();

                    if s.is_empty() {
                        return Ok(Dynamic::UNIT);
                    }

                    let mut ch = args[0].as_char().unwrap().to_string();

                    #[cfg(not(feature = "unchecked"))]
                    _ctx.unwrap()
                        .engine()
                        .throw_on_size((0, 0, ch.len() + s.len()))?;

                    ch += s;
                    ch
                };

                *args[0].write_lock::<Dynamic>().unwrap() = ch.into();

                Ok(Dynamic::UNIT)
            },
            CHECKED_BUILD,
        )),

        // array += any
        #[cfg(not(feature = "no_index"))]
        (Union::Array(..), _, PlusAssign) => {
            #[allow(clippy::wildcard_imports)]
            use crate::packages::array_basic::array_functions::*;

            Some((
                |_ctx, args| {
                    {
                        let x = args[1].take();
                        let array = &mut *args[0].as_array_mut().unwrap();
                        push(array, x);
                    }

                    #[cfg(not(feature = "unchecked"))]
                    _ctx.unwrap()
                        .engine()
                        .check_data_size(&*args[0].read_lock().unwrap(), crate::Position::NONE)?;

                    Ok(Dynamic::UNIT)
                },
                CHECKED_BUILD,
            ))
        }

        // blob += int
        #[cfg(not(feature = "no_index"))]
        (Union::Blob(..), Union::Int(..), PlusAssign) => {
            #[allow(clippy::wildcard_imports)]
            use crate::packages::blob_basic::blob_functions::*;

            Some((
                |_ctx, args| {
                    let x = args[1].as_int().unwrap();
                    let blob = &mut *args[0].as_blob_mut().unwrap();

                    #[cfg(not(feature = "unchecked"))]
                    _ctx.unwrap()
                        .engine()
                        .throw_on_size((blob.len() + 1, 0, 0))?;

                    push(blob, x);

                    Ok(Dynamic::UNIT)
                },
                CHECKED_BUILD,
            ))
        }

        // blob += char
        #[cfg(not(feature = "no_index"))]
        (Union::Blob(..), Union::Char(..), PlusAssign) => {
            #[allow(clippy::wildcard_imports)]
            use crate::packages::blob_basic::blob_functions::*;

            Some((
                |_ctx, args| {
                    let x = args[1].as_char().unwrap();
                    let blob = &mut *args[0].as_blob_mut().unwrap();

                    #[cfg(not(feature = "unchecked"))]
                    _ctx.unwrap()
                        .engine()
                        .throw_on_size((blob.len() + 1, 0, 0))?;

                    append_char(blob, x);

                    Ok(Dynamic::UNIT)
                },
                CHECKED_BUILD,
            ))
        }

        // blob += string
        #[cfg(not(feature = "no_index"))]
        (Union::Blob(..), Union::Str(..), PlusAssign) => {
            #[allow(clippy::wildcard_imports)]
            use crate::packages::blob_basic::blob_functions::*;

            Some((
                |_ctx, args| {
                    let (first, second) = args.split_first_mut().unwrap();
                    let blob = &mut *first.as_blob_mut().unwrap();
                    let s = &*second[0].as_immutable_string_ref().unwrap();

                    if s.is_empty() {
                        return Ok(Dynamic::UNIT);
                    }

                    #[cfg(not(feature = "unchecked"))]
                    _ctx.unwrap()
                        .engine()
                        .throw_on_size((blob.len() + s.len(), 0, 0))?;

                    append_str(blob, s);

                    Ok(Dynamic::UNIT)
                },
                CHECKED_BUILD,
            ))
        }

        _ => None,
    }
}
