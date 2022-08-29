//! Module which defines the function registration mechanism.

#![allow(non_snake_case)]

use super::call::FnCallArgs;
use super::callable_function::CallableFunction;
use super::native::{FnAny, SendSync};
use crate::types::dynamic::{DynamicWriteLock, Variant};
use crate::{reify, Dynamic, NativeCallContext, RhaiResultOf};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{any::TypeId, mem};

/// These types are used to build a unique _marker_ tuple type for each combination
/// of function parameter types in order to make each trait implementation unique.
///
/// That is because stable Rust currently does not allow distinguishing implementations
/// based purely on parameter types of traits (`Fn`, `FnOnce` and `FnMut`).
///
/// # Examples
///
/// `RegisterNativeFunction<(Mut<A>, B, Ref<C>), R, ()>` = `Fn(&mut A, B, &C) -> R`
///
/// `RegisterNativeFunction<(Mut<A>, B, Ref<C>), R, NativeCallContext>` = `Fn(NativeCallContext, &mut A, B, &C) -> R`
///
/// `RegisterNativeFunction<(Mut<A>, B, Ref<C>), R, RhaiResultOf<()>>` = `Fn(&mut A, B, &C) -> Result<R, Box<EvalAltResult>>`
///
/// `RegisterNativeFunction<(Mut<A>, B, Ref<C>), R, RhaiResultOf<NativeCallContext>>` = `Fn(NativeCallContext, &mut A, B, &C) -> Result<R, Box<EvalAltResult>>`
///
/// These types are not actually used anywhere.
pub struct Mut<T>(T);
//pub struct Ref<T>(T);

/// Dereference into [`DynamicWriteLock`]
#[inline(always)]
#[must_use]
pub fn by_ref<T: Variant + Clone>(data: &mut Dynamic) -> DynamicWriteLock<T> {
    // Directly cast the &mut Dynamic into DynamicWriteLock to access the underlying data.
    data.write_lock::<T>().expect("checked")
}

/// Dereference into value.
#[inline(always)]
#[must_use]
pub fn by_value<T: Variant + Clone>(data: &mut Dynamic) -> T {
    if TypeId::of::<T>() == TypeId::of::<&str>() {
        // If T is `&str`, data must be `ImmutableString`, so map directly to it
        data.flatten_in_place();
        let ref_str = data.as_str_ref().expect("&str");
        // # Safety
        //
        // We already checked that `T` is `&str`, so it is safe to cast here.
        return unsafe { mem::transmute_copy::<_, T>(&ref_str) };
    }
    if TypeId::of::<T>() == TypeId::of::<String>() {
        // If T is `String`, data must be `ImmutableString`, so map directly to it
        return reify!(mem::take(data).into_string().expect("`ImmutableString`") => T);
    }

    // We consume the argument and then replace it with () - the argument is not supposed to be used again.
    // This way, we avoid having to clone the argument again, because it is already a clone when passed here.
    mem::take(data).cast::<T>()
}

/// Trait to register custom Rust functions.
///
/// # Type Parameters
///
/// * `ARGS` - a tuple containing parameter types, with `&mut T` represented by [`Mut<T>`].
/// * `RET` - return type of the function; if the function returns `Result`, it is the unwrapped inner value type.
pub trait RegisterNativeFunction<ARGS, RET, RESULT> {
    /// Convert this function into a [`CallableFunction`].
    #[must_use]
    fn into_callable_function(self) -> CallableFunction;
    /// Get the type ID's of this function's parameters.
    #[must_use]
    fn param_types() -> Box<[TypeId]>;
    /// _(metadata)_ Get the type names of this function's parameters.
    /// Exported under the `metadata` feature only.
    #[cfg(feature = "metadata")]
    #[must_use]
    fn param_names() -> Box<[&'static str]>;
    /// _(metadata)_ Get the type ID of this function's return value.
    /// Exported under the `metadata` feature only.
    #[cfg(feature = "metadata")]
    #[must_use]
    fn return_type() -> TypeId;
    /// _(metadata)_ Get the type name of this function's return value.
    /// Exported under the `metadata` feature only.
    #[cfg(feature = "metadata")]
    #[must_use]
    fn return_type_name() -> &'static str;
}

const EXPECT_ARGS: &str = "arguments";

macro_rules! check_constant {
    ($ctx:ident, $args:ident) => {
        #[cfg(any(not(feature = "no_object"), not(feature = "no_index")))]
        {
            let args_len = $args.len();

            if args_len > 0 && $args[0].is_read_only() {
                let deny = match $ctx.fn_name() {
                    #[cfg(not(feature = "no_object"))]
                    f if args_len == 2 && f.starts_with(crate::engine::FN_SET) => true,
                    #[cfg(not(feature = "no_index"))]
                    crate::engine::FN_IDX_SET if args_len == 3 => true,
                    _ => false,
                };
                if deny {
                    return Err(crate::ERR::ErrorAssignmentToConstant(
                        String::new(),
                        crate::Position::NONE,
                    )
                    .into());
                }
            }
        }
    };
}

macro_rules! def_register {
    () => {
        def_register!(imp from_pure :);
    };
    (imp $abi:ident : $($par:ident => $arg:expr => $mark:ty => $param:ty => $let:stmt => $clone:expr),*) => {
    //   ^ function ABI type
    //                  ^ function parameter generic type name (A, B, C etc.)
    //                                ^ call argument(like A, *B, &mut C etc)
    //                                             ^ function parameter marker type (A, Ref<B> or Mut<C>)
    //                                                         ^ function parameter actual type (A, &B or &mut C)
    //                                                                      ^ argument let statement

        impl<
            FN: Fn($($param),*) -> RET + SendSync + 'static,
            $($par: Variant + Clone,)*
            RET: Variant + Clone
        > RegisterNativeFunction<($($mark,)*), RET, ()> for FN {
            #[inline(always)] fn param_types() -> Box<[TypeId]> { vec![$(TypeId::of::<$par>()),*].into_boxed_slice() }
            #[cfg(feature = "metadata")] #[inline(always)] fn param_names() -> Box<[&'static str]> { vec![$(std::any::type_name::<$param>()),*].into_boxed_slice() }
            #[cfg(feature = "metadata")] #[inline(always)] fn return_type() -> TypeId { TypeId::of::<RET>() }
            #[cfg(feature = "metadata")] #[inline(always)] fn return_type_name() -> &'static str { std::any::type_name::<RET>() }
            #[inline(always)] fn into_callable_function(self) -> CallableFunction {
                CallableFunction::$abi(Box::new(move |_ctx: NativeCallContext, args: &mut FnCallArgs| {
                    // The arguments are assumed to be of the correct number and types!
                    check_constant!(_ctx, args);

                    let mut _drain = args.iter_mut();
                    $($let $par = ($clone)(_drain.next().expect(EXPECT_ARGS)); )*

                    // Call the function with each argument value
                    let r = self($($arg),*);

                    // Map the result
                    Ok(Dynamic::from(r))
                }) as Box<FnAny>)
            }
        }

        impl<
            FN: for<'a> Fn(NativeCallContext<'a>, $($param),*) -> RET + SendSync + 'static,
            $($par: Variant + Clone,)*
            RET: Variant + Clone
        > RegisterNativeFunction<($($mark,)*), RET, NativeCallContext<'static>> for FN {
            #[inline(always)] fn param_types() -> Box<[TypeId]> { vec![$(TypeId::of::<$par>()),*].into_boxed_slice() }
            #[cfg(feature = "metadata")] #[inline(always)] fn param_names() -> Box<[&'static str]> { vec![$(std::any::type_name::<$param>()),*].into_boxed_slice() }
            #[cfg(feature = "metadata")] #[inline(always)] fn return_type() -> TypeId { TypeId::of::<RET>() }
            #[cfg(feature = "metadata")] #[inline(always)] fn return_type_name() -> &'static str { std::any::type_name::<RET>() }
            #[inline(always)] fn into_callable_function(self) -> CallableFunction {
                CallableFunction::$abi(Box::new(move |ctx: NativeCallContext, args: &mut FnCallArgs| {
                    // The arguments are assumed to be of the correct number and types!
                    check_constant!(ctx, args);

                    let mut _drain = args.iter_mut();
                    $($let $par = ($clone)(_drain.next().expect(EXPECT_ARGS)); )*

                    // Call the function with each argument value
                    let r = self(ctx, $($arg),*);

                    // Map the result
                    Ok(Dynamic::from(r))
                }) as Box<FnAny>)
            }
        }

        impl<
            FN: Fn($($param),*) -> RhaiResultOf<RET> + SendSync + 'static,
            $($par: Variant + Clone,)*
            RET: Variant + Clone
        > RegisterNativeFunction<($($mark,)*), RET, RhaiResultOf<()>> for FN {
            #[inline(always)] fn param_types() -> Box<[TypeId]> { vec![$(TypeId::of::<$par>()),*].into_boxed_slice() }
            #[cfg(feature = "metadata")] #[inline(always)] fn param_names() -> Box<[&'static str]> { vec![$(std::any::type_name::<$param>()),*].into_boxed_slice() }
            #[cfg(feature = "metadata")] #[inline(always)] fn return_type() -> TypeId { TypeId::of::<RhaiResultOf<RET>>() }
            #[cfg(feature = "metadata")] #[inline(always)] fn return_type_name() -> &'static str { std::any::type_name::<RhaiResultOf<RET>>() }
            #[inline(always)] fn into_callable_function(self) -> CallableFunction {
                CallableFunction::$abi(Box::new(move |_ctx: NativeCallContext, args: &mut FnCallArgs| {
                    // The arguments are assumed to be of the correct number and types!
                    check_constant!(_ctx, args);

                    let mut _drain = args.iter_mut();
                    $($let $par = ($clone)(_drain.next().expect(EXPECT_ARGS)); )*

                    // Call the function with each argument value
                    self($($arg),*).map(Dynamic::from)
                }) as Box<FnAny>)
            }
        }

        impl<
            FN: for<'a> Fn(NativeCallContext<'a>, $($param),*) -> RhaiResultOf<RET> + SendSync + 'static,
            $($par: Variant + Clone,)*
            RET: Variant + Clone
        > RegisterNativeFunction<($($mark,)*), RET, RhaiResultOf<NativeCallContext<'static>>> for FN {
            #[inline(always)] fn param_types() -> Box<[TypeId]> { vec![$(TypeId::of::<$par>()),*].into_boxed_slice() }
            #[cfg(feature = "metadata")] #[inline(always)] fn param_names() -> Box<[&'static str]> { vec![$(std::any::type_name::<$param>()),*].into_boxed_slice() }
            #[cfg(feature = "metadata")] #[inline(always)] fn return_type() -> TypeId { TypeId::of::<RhaiResultOf<RET>>() }
            #[cfg(feature = "metadata")] #[inline(always)] fn return_type_name() -> &'static str { std::any::type_name::<RhaiResultOf<RET>>() }
            #[inline(always)] fn into_callable_function(self) -> CallableFunction {
                CallableFunction::$abi(Box::new(move |ctx: NativeCallContext, args: &mut FnCallArgs| {
                    // The arguments are assumed to be of the correct number and types!
                    check_constant!(ctx, args);

                    let mut _drain = args.iter_mut();
                    $($let $par = ($clone)(_drain.next().expect(EXPECT_ARGS)); )*

                    // Call the function with each argument value
                    self(ctx, $($arg),*).map(Dynamic::from)
                }) as Box<FnAny>)
            }
        }

        //def_register!(imp_pop $($par => $mark => $param),*);
    };
    ($p0:ident $(, $p:ident)*) => {
        def_register!(imp from_pure   : $p0 => $p0      => $p0      => $p0      => let $p0     => by_value $(, $p => $p => $p => $p => let $p => by_value)*);
        def_register!(imp from_method : $p0 => &mut $p0 => Mut<$p0> => &mut $p0 => let mut $p0 => by_ref   $(, $p => $p => $p => $p => let $p => by_value)*);
        //                ^ CallableFunction constructor
        //                                                             ^ first parameter passed through
        //                                                                                                     ^ others passed by value (by_value)

        // Currently does not support first argument which is a reference, as there will be
        // conflicting implementations since &T: Any and T: Any cannot be distinguished
        //def_register!(imp $p0 => Ref<$p0> => &$p0     => by_ref   $(, $p => $p => $p => by_value)*);

        def_register!($($p),*);
    };
}

def_register!(A, B, C, D, E, F, G, H, J, K, L, M, N, P, Q, R, S, T, U, V);
