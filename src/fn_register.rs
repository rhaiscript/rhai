use std::any::TypeId;

use any::Any;
use engine::{
    Engine,
    EvalAltResult
};

pub trait RegisterFn<FN, ARGS, RET> {
    fn register_fn(&mut self, name: &str, f: FN);
}

pub struct Ref<A>(A);
pub struct Mut<A>(A);

macro_rules! count_args {
    () => {0usize};
    ($head:ident $($tail:ident)*) => {1usize + count_args!($($tail)*)};
}

macro_rules! def_register {
    () => {
        def_register!(imp);
    };
    (imp $($par:ident => $mark:ty => $param:ty => $clone:expr),*) => {
        impl<$($par,)* FN, RET> RegisterFn<FN, ($($mark,)*), RET> for Engine
        where
            $($par: Any + Clone,)*
            FN: Fn($($param),*) -> RET + 'static,
            RET: Any,
        {
            fn register_fn(&mut self, name: &str, f: FN) {
                let fun = move |mut args: Vec<&mut Any>| {
                    // Check for length at the beginning to avoid
                    // per-element bound checks.
                    if args.len() != count_args!($($par)*) {
                        return Err(EvalAltResult::ErrorFunctionArgMismatch);
                    }

                    let mut drain = args.drain(..);
                    $(
                    // Downcast every element, return in case of a type mismatch
                    let $par = ((*drain.next().unwrap()).downcast_mut() as Option<&mut $par>)
                        .ok_or(EvalAltResult::ErrorFunctionArgMismatch)?;
                    )*

                    // Call the user-supplied function using ($clone) to
                    // potentially clone the value, otherwise pass the reference.
                    Ok(Box::new(f($(($clone)($par)),*)) as Box<Any>)
                };
                self.register_fn_raw(name.to_owned(), Some(vec![$(TypeId::of::<$par>()),*]), Box::new(fun));
            }
        }

        //def_register!(imp_pop $($par => $mark => $param),*);
    };
    ($p0:ident $(, $p:ident)*) => {
        def_register!(imp $p0 => $p0 => $p0 => Clone::clone $(, $p => $p => $p => Clone::clone)*);
        def_register!(imp $p0 => Ref<$p0> => &$p0 => |x| { x } $(, $p => $p => $p => Clone::clone)*);
        def_register!(imp $p0 => Mut<$p0> => &mut $p0 => |x| { x } $(, $p => $p => $p => Clone::clone)*);

        def_register!($($p),*);
    };
//    (imp_pop) => {};
//    (imp_pop $head:ident => $head_mark:ty => $head_param:ty $(,$tail:ident => $tail_mark:ty => $tp:ty)*) => {
//        def_register!(imp $($tail => $tail_mark => $tp),*);
//    };
}

#[cfg_attr(rustfmt, rustfmt_skip)]
def_register!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S);
