//! Module which defines the function registration mechanism.

#![allow(non_snake_case)]

use crate::any::{Dynamic, Variant, DynamicWriteLock};
use crate::engine::Engine;
use crate::fn_native::{CallableFunction, FnAny, FnCallArgs, SendSync};
use crate::module::Module;
use crate::parser::FnAccess;
use crate::r#unsafe::unsafe_cast_box;
use crate::result::EvalAltResult;
use crate::utils::ImmutableString;

use crate::stdlib::{
    any::TypeId,
    boxed::Box,
    mem,
    string::{String, ToString},
};

/// Trait to register custom functions with the `Engine`.
pub trait RegisterFn<FN, ARGS, RET> {
    /// Register a custom function with the `Engine`.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, RegisterFn};
    ///
    /// // Normal function
    /// fn add(x: i64, y: i64) -> i64 {
    ///     x + y
    /// }
    ///
    /// let mut engine = Engine::new();
    ///
    /// // You must use the trait rhai::RegisterFn to get this method.
    /// engine.register_fn("add", add);
    ///
    /// assert_eq!(engine.eval::<i64>("add(40, 2)")?, 42);
    ///
    /// // You can also register a closure.
    /// engine.register_fn("sub", |x: i64, y: i64| x - y );
    ///
    /// assert_eq!(engine.eval::<i64>("sub(44, 2)")?, 42);
    /// # Ok(())
    /// # }
    /// ```
    fn register_fn(&mut self, name: &str, f: FN) -> &mut Self;
}

/// Trait to register fallible custom functions returning `Result<Dynamic, Box<EvalAltResult>>` with the `Engine`.
pub trait RegisterResultFn<FN, ARGS> {
    /// Register a custom fallible function with the `Engine`.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Engine, Dynamic, RegisterResultFn, EvalAltResult};
    ///
    /// // Normal function
    /// fn div(x: i64, y: i64) -> Result<Dynamic, Box<EvalAltResult>> {
    ///     if y == 0 {
    ///         // '.into()' automatically converts to 'Box<EvalAltResult::ErrorRuntime>'
    ///         Err("division by zero!".into())
    ///     } else {
    ///         Ok((x / y).into())
    ///     }
    /// }
    ///
    /// let mut engine = Engine::new();
    ///
    /// // You must use the trait rhai::RegisterResultFn to get this method.
    /// engine.register_result_fn("div", div);
    ///
    /// engine.eval::<i64>("div(42, 0)")
    ///         .expect_err("expecting division by zero error!");
    /// ```
    fn register_result_fn(&mut self, name: &str, f: FN) -> &mut Self;
}

// These types are used to build a unique _marker_ tuple type for each combination
// of function parameter types in order to make each trait implementation unique.
// That is because stable Rust currently does not allow distinguishing implementations
// based purely on parameter types of traits (Fn, FnOnce and FnMut).
//
// For example:
//
// `RegisterFn<FN, (Mut<A>, B, Ref<C>), R>`
//
// will have the function prototype constraint to:
//
// `FN: (&mut A, B, &C) -> R`
//
// These types are not actually used anywhere.
pub struct Mut<T>(T);
//pub struct Ref<T>(T);

/// Dereference into DynamicWriteLock
#[inline(always)]
pub fn by_ref<T: Variant + Clone>(data: &mut Dynamic) -> DynamicWriteLock<T> {
    // Directly cast the &mut Dynamic into DynamicWriteLock to access the underlying data.
    data.write_lock::<T>().unwrap()
}

/// Dereference into value.
#[inline(always)]
pub fn by_value<T: Variant + Clone>(data: &mut Dynamic) -> T {
    if TypeId::of::<T>() == TypeId::of::<&str>() {
        // If T is &str, data must be ImmutableString, so map directly to it
        let ref_str = data.as_str().unwrap();
        let ref_T = unsafe { mem::transmute::<_, &T>(&ref_str) };
        ref_T.clone()
    } else if TypeId::of::<T>() == TypeId::of::<String>() {
        // If T is String, data must be ImmutableString, so map directly to it
        *unsafe_cast_box(Box::new(data.as_str().unwrap().to_string())).unwrap()
    } else {
        // We consume the argument and then replace it with () - the argument is not supposed to be used again.
        // This way, we avoid having to clone the argument again, because it is already a clone when passed here.
        mem::take(data).cast::<T>()
    }
}

/// This macro creates a closure wrapping a registered function.
macro_rules! make_func {
	($fn:ident : $map:expr ; $($par:ident => $let:stmt => $convert:expr => $arg:expr),*) => {
//   ^ function pointer
//               ^ result mapping function
//                           ^ function parameter generic type name (A, B, C etc.)
//                                          ^ argument let statement(e.g. let mut A ...)
//                                                       ^ dereferencing function
//                                                                         ^ argument reference expression(like A, *B, &mut C etc)

		Box::new(move |_: &Engine, _: &Module, args: &mut FnCallArgs| {
            // The arguments are assumed to be of the correct number and types!

			let mut _drain = args.iter_mut();
			$($let)*
			$($par = ($convert)(_drain.next().unwrap()); )*

            // Call the function with each parameter value
			let r = $fn($($arg),*);

            // Map the result
            $map(r)
		}) as Box<FnAny>
	};
}

/// To Dynamic mapping function.
#[inline(always)]
pub fn map_dynamic<T: Variant + Clone>(data: T) -> Result<Dynamic, Box<EvalAltResult>> {
    Ok(data.into_dynamic())
}

/// To Dynamic mapping function.
#[inline(always)]
pub fn map_result(
    data: Result<Dynamic, Box<EvalAltResult>>,
) -> Result<Dynamic, Box<EvalAltResult>> {
    data
}

/// Remap `&str` | `String` to `ImmutableString`.
#[inline(always)]
fn map_type_id<T: 'static>() -> TypeId {
    let id = TypeId::of::<T>();

    if id == TypeId::of::<&str>() {
        TypeId::of::<ImmutableString>()
    } else if id == TypeId::of::<String>() {
        TypeId::of::<ImmutableString>()
    } else {
        id
    }
}

macro_rules! def_register {
    () => {
        def_register!(imp from_pure :);
    };
    (imp $abi:ident : $($par:ident => $arg:expr => $mark:ty => $param:ty => $let:stmt => $clone:expr),*) => {
    //   ^ function ABI type
    //                  ^ function parameter generic type name (A, B, C etc.)
//                                    ^ call argument(like A, *B, &mut C etc)
    //                                            ^ function parameter marker type (T, Ref<T> or Mut<T>)
    //                                                         ^ function parameter actual type (T, &T or &mut T)
    //                                                                      ^ argument let statement
        impl<
            $($par: Variant + Clone,)*
            FN: Fn($($param),*) -> RET + SendSync + 'static,
            RET: Variant + Clone
        > RegisterFn<FN, ($($mark,)*), RET> for Engine
        {
            fn register_fn(&mut self, name: &str, f: FN) -> &mut Self {
                self.global_module.set_fn(name, FnAccess::Public,
                    &[$(map_type_id::<$par>()),*],
                    CallableFunction::$abi(make_func!(f : map_dynamic ; $($par => $let => $clone => $arg),*))
                );
                self
            }
        }

        impl<
            $($par: Variant + Clone,)*
            FN: Fn($($param),*) -> Result<Dynamic, Box<EvalAltResult>> + SendSync + 'static,
        > RegisterResultFn<FN, ($($mark,)*)> for Engine
        {
            fn register_result_fn(&mut self, name: &str, f: FN) -> &mut Self {
                self.global_module.set_fn(name, FnAccess::Public,
                    &[$(map_type_id::<$par>()),*],
                    CallableFunction::$abi(make_func!(f : map_result ; $($par => $let => $clone => $arg),*))
                );
                self
            }
        }

        //def_register!(imp_pop $($par => $mark => $param),*);
    };
    ($p0:ident $(, $p:ident)*) => {
        def_register!(imp from_pure   : $p0 => $p0      => $p0      => $p0      => let $p0     => by_value $(, $p => $p => $p => $p => let $p => by_value)*);
        def_register!(imp from_method : $p0 => &mut $p0  => Mut<$p0> => &mut $p0 => let mut $p0 => by_ref   $(, $p => $p => $p => $p => let $p => by_value)*);
        //                ^ CallableFunction
        // handle the first parameter                                              ^ first parameter passed through
        //                                                                                                     ^ others passed by value (by_value)

        // Currently does not support first argument which is a reference, as there will be
        // conflicting implementations since &T: Any and T: Any cannot be distinguished
        //def_register!(imp $p0 => Ref<$p0> => &$p0     => by_ref   $(, $p => $p => $p => by_value)*);

        def_register!($($p),*);
    };
}

def_register!(A, B, C, D, E, F, G, H, J, K, L, M, N, P, Q, R, S, T, U, V);
