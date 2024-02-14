//! Module which defines the function registration mechanism.

#![allow(non_snake_case)]
#![allow(unused_imports)]
#![allow(unused_mut)]
#![allow(unused_variables)]

use super::call::FnCallArgs;
use super::function::RhaiFunc;
use super::native::{SendSync, Shared};
use crate::types::dynamic::{DynamicWriteLock, Union, Variant};
use crate::{Dynamic, Identifier, NativeCallContext, RhaiResultOf};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::{type_name, TypeId},
    mem,
};

/// These types are used to build a unique _marker_ tuple type for each combination
/// of function parameter types in order to make each trait implementation unique.
///
/// That is because stable Rust currently does not allow distinguishing implementations
/// based purely on parameter types of traits (`Fn`, `FnOnce` and `FnMut`).
///
/// # Examples
///
/// `RhaiNativeFunc<(Mut<A>, B, Ref<C>), 3, false, R, false>` = `Fn(&mut A, B, &C) -> R`
///
/// `RhaiNativeFunc<(Mut<A>, B, Ref<C>), 3, true,  R, false>` = `Fn(NativeCallContext, &mut A, B, &C) -> R`
///
/// `RhaiNativeFunc<(Mut<A>, B, Ref<C>), 3, false, R, true>`  = `Fn(&mut A, B, &C) -> Result<R, Box<EvalAltResult>>`
///
/// `RhaiNativeFunc<(Mut<A>, B, Ref<C>), 3, true,  R, true>`  = `Fn(NativeCallContext, &mut A, B, &C) -> Result<R, Box<EvalAltResult>>`
///
/// These types are not actually used anywhere.
pub struct Mut<T>(T);
//pub struct Ref<T>(T);

/// Dereference into [`DynamicWriteLock`]
#[inline(always)]
pub fn by_ref<T: Variant + Clone>(data: &mut Dynamic) -> DynamicWriteLock<T> {
    // Directly cast the &mut Dynamic into DynamicWriteLock to access the underlying data.
    data.write_lock::<T>().unwrap()
}

/// Dereference into value.
#[inline(always)]
#[must_use]
pub fn by_value<T: Variant + Clone>(data: &mut Dynamic) -> T {
    if TypeId::of::<T>() == TypeId::of::<&str>() {
        // If T is `&str`, data must be `ImmutableString`, so map directly to it
        *data = data.take().flatten();

        let ref_str = match data.0 {
            Union::Str(ref s, ..) => s.as_str(),
            _ => unreachable!(),
        };
        // SAFETY: We already checked that `T` is `&str`, so it is safe to cast here.
        return unsafe { mem::transmute_copy::<_, T>(&ref_str) };
    }
    if TypeId::of::<T>() == TypeId::of::<String>() {
        // If T is `String`, data must be `ImmutableString`, so map directly to it
        return reify! { data.take().into_string().unwrap() => !!! T };
    }

    // We consume the argument and then replace it with () - the argument is not supposed to be used again.
    // This way, we avoid having to clone the argument again, because it is already a clone when passed here.
    data.take().cast::<T>()
}

/// Trait to register custom Rust functions.
///
/// # Type Parameters
///
/// * `A` - a tuple containing parameter types, with `&mut T` represented by `Mut<T>`.
/// * `N` - a constant generic containing the number of parameters, must be consistent with `ARGS`.
/// * `X` - a constant boolean generic indicating whether there is a `NativeCallContext` parameter.
/// * `R` - return type of the function; if the function returns `Result`, it is the unwrapped inner value type.
/// * `F` - a constant boolean generic indicating whether the function is fallible (i.e. returns `Result<T, Box<EvalAltResult>>`).
pub trait RhaiNativeFunc<A: 'static, const N: usize, const X: bool, R: 'static, const F: bool> {
    /// Convert this function into a [`RhaiFunc`].
    #[must_use]
    fn into_rhai_function(self, is_pure: bool, is_volatile: bool) -> RhaiFunc;
    /// Get the type ID's of this function's parameters.
    #[must_use]
    fn param_types() -> [TypeId; N];
    /// Get the number of parameters for this function.
    #[inline(always)]
    #[must_use]
    fn num_params() -> usize {
        N
    }
    /// Is there a [`NativeCallContext`] parameter for this function?
    #[inline(always)]
    #[must_use]
    fn has_context() -> bool {
        X
    }
    /// _(metadata)_ Get the type names of this function's parameters.
    /// Exported under the `metadata` feature only.
    #[cfg(feature = "metadata")]
    #[must_use]
    fn param_names() -> [&'static str; N];
    /// _(metadata)_ Get the type ID of this function's return value.
    /// Exported under the `metadata` feature only.
    #[cfg(feature = "metadata")]
    #[inline(always)]
    #[must_use]
    fn return_type() -> TypeId {
        if F {
            TypeId::of::<RhaiResultOf<R>>()
        } else {
            TypeId::of::<R>()
        }
    }
    /// _(metadata)_ Get the type name of this function's return value.
    /// Exported under the `metadata` feature only.
    #[cfg(feature = "metadata")]
    #[inline(always)]
    #[must_use]
    fn return_type_name() -> &'static str {
        type_name::<R>()
    }
}

macro_rules! def_register {
    () => {
        def_register!(imp Pure : 0;);
    };
    (imp $abi:ident : $n:expr ; $($par:ident => $arg:expr => $mark:ty => $param:ty => $clone:expr),*) => {
    //   ^ function ABI type
    //                ^ number of parameters
    //                            ^ function parameter generic type name (A, B, C etc.)
    //                                          ^ call argument(like A, *B, &mut C etc)
    //                                                       ^ function parameter marker type (A, Ref<B> or Mut<C>)
    //                                                                   ^ function parameter actual type (A, &B or &mut C)
    //                                                                                ^ parameter access function (by_value or by_ref)

        impl<
            FN: Fn($($param),*) -> RET + SendSync + 'static,
            $($par: Variant + Clone,)*
            RET: Variant + Clone,
        > RhaiNativeFunc<($($mark,)*), $n, false, RET, false> for FN {
            #[inline(always)] fn param_types() -> [TypeId;$n] { [$(TypeId::of::<$par>()),*] }
            #[cfg(feature = "metadata")] #[inline(always)] fn param_names() -> [&'static str;$n] { [$(type_name::<$param>()),*] }
            #[inline(always)] fn into_rhai_function(self, is_pure: bool, is_volatile: bool) -> RhaiFunc {
                RhaiFunc::$abi { func: Shared::new(move |_, args: &mut FnCallArgs| {
                    // The arguments are assumed to be of the correct number and types!
                    let mut drain = args.iter_mut();
                    $(let mut $par = $clone(drain.next().unwrap()); )*

                    // Call the function with each argument value
                    let r = self($($arg),*);

                    // Map the result
                    Ok(Dynamic::from(r))
                }), has_context: false, is_pure, is_volatile }
            }
        }

        impl<
            FN: for<'a> Fn(NativeCallContext<'a>, $($param),*) -> RET + SendSync + 'static,
            $($par: Variant + Clone,)*
            RET: Variant + Clone,
        > RhaiNativeFunc<($($mark,)*), $n, true, RET, false> for FN {
            #[inline(always)] fn param_types() -> [TypeId;$n] { [$(TypeId::of::<$par>()),*] }
            #[cfg(feature = "metadata")] #[inline(always)] fn param_names() -> [&'static str;$n] { [$(type_name::<$param>()),*] }
            #[inline(always)] fn into_rhai_function(self, is_pure: bool, is_volatile: bool) -> RhaiFunc {
                RhaiFunc::$abi { func: Shared::new(move |ctx: Option<NativeCallContext>, args: &mut FnCallArgs| {
                    let ctx = ctx.unwrap();

                    // The arguments are assumed to be of the correct number and types!
                    let mut drain = args.iter_mut();
                    $(let mut $par = $clone(drain.next().unwrap()); )*

                    // Call the function with each argument value
                    let r = self(ctx, $($arg),*);

                    // Map the result
                    Ok(Dynamic::from(r))
                }), has_context: true, is_pure, is_volatile }
            }
        }

        impl<
            FN: Fn($($param),*) -> RhaiResultOf<RET> + SendSync + 'static,
            $($par: Variant + Clone,)*
            RET: Variant + Clone
        > RhaiNativeFunc<($($mark,)*), $n, false, RET, true> for FN {
            #[inline(always)] fn param_types() -> [TypeId;$n] { [$(TypeId::of::<$par>()),*] }
            #[cfg(feature = "metadata")] #[inline(always)] fn param_names() -> [&'static str;$n] { [$(type_name::<$param>()),*] }
            #[cfg(feature = "metadata")] #[inline(always)] fn return_type_name() -> &'static str { type_name::<RhaiResultOf<RET>>() }
            #[inline(always)] fn into_rhai_function(self, is_pure: bool, is_volatile: bool) -> RhaiFunc {
                RhaiFunc::$abi { func: Shared::new(move |_, args: &mut FnCallArgs| {
                    // The arguments are assumed to be of the correct number and types!
                    let mut drain = args.iter_mut();
                    $(let mut $par = $clone(drain.next().unwrap()); )*

                    // Call the function with each argument value
                    self($($arg),*).map(Dynamic::from)
                }), has_context: false, is_pure, is_volatile }
            }
        }

        impl<
            FN: for<'a> Fn(NativeCallContext<'a>, $($param),*) -> RhaiResultOf<RET> + SendSync + 'static,
            $($par: Variant + Clone,)*
            RET: Variant + Clone
        > RhaiNativeFunc<($($mark,)*), $n, true, RET, true> for FN {
            #[inline(always)] fn param_types() -> [TypeId;$n] { [$(TypeId::of::<$par>()),*] }
            #[cfg(feature = "metadata")] #[inline(always)] fn param_names() -> [&'static str;$n] { [$(type_name::<$param>()),*] }
            #[cfg(feature = "metadata")] #[inline(always)] fn return_type_name() -> &'static str { type_name::<RhaiResultOf<RET>>() }
            #[inline(always)] fn into_rhai_function(self, is_pure: bool, is_volatile: bool) -> RhaiFunc {
                RhaiFunc::$abi { func: Shared::new(move |ctx: Option<NativeCallContext>, args: &mut FnCallArgs| {
                    let ctx = ctx.unwrap();

                    // The arguments are assumed to be of the correct number and types!
                    let mut drain = args.iter_mut();
                    $(let mut $par = $clone(drain.next().unwrap()); )*

                    // Call the function with each argument value
                    self(ctx, $($arg),*).map(Dynamic::from)
                }), has_context: true, is_pure, is_volatile }
            }
        }

        //def_register!(imp_pop $($par => $mark => $param),*);
    };
    ($p0:ident:$n0:expr $(, $p:ident: $n:expr)*) => {
        def_register!(imp Pure   : $n0 ; $p0 => $p0      => $p0      => $p0      => by_value $(, $p => $p => $p => $p => by_value)*);
        def_register!(imp Method : $n0 ; $p0 => &mut $p0 => Mut<$p0> => &mut $p0 => by_ref   $(, $p => $p => $p => $p => by_value)*);
        //                ^ RhaiFunc constructor
        //                         ^ number of arguments                            ^ first parameter passed through
        //                                                                                       ^ others passed by value (by_value)

        // Currently does not support first argument which is a reference, as there will be
        // conflicting implementations since &T: Any and T: Any cannot be distinguished
        //def_register!(imp $p0 => Ref<$p0> => &$p0     => by_ref   $(, $p => $p => $p => by_value)*);

        def_register!($($p: $n),*);
    };
}

def_register!(A:20, B:19, C:18, D:17, E:16, F:15, G:14, H:13, J:12, K:11, L:10, M:9, N:8, P:7, Q:6, R:5, S:4, T:3, U:2, V:1);
