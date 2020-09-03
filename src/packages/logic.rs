use crate::def_package;
use crate::plugin::*;

macro_rules! gen_cmp_functions {
    ($root:ident => $($arg_type:ident),+) => {
        mod $root { $(pub mod $arg_type {
            use super::super::*;

            #[export_module]
            pub mod functions {
                #[rhai_fn(name = "<")]
                #[inline(always)]
                pub fn lt(x: $arg_type, y: $arg_type) -> bool {
                    x < y
                }
                #[rhai_fn(name = "<=")]
                #[inline(always)]
                pub fn lte(x: $arg_type, y: $arg_type) -> bool {
                    x <= y
                }
                #[rhai_fn(name = ">")]
                #[inline(always)]
                pub fn gt(x: $arg_type, y: $arg_type) -> bool {
                    x > y
                }
                #[rhai_fn(name = ">=")]
                #[inline(always)]
                pub fn gte(x: $arg_type, y: $arg_type) -> bool {
                    x >= y
                }
                #[rhai_fn(name = "==")]
                #[inline(always)]
                pub fn eq(x: $arg_type, y: $arg_type) -> bool {
                    x == y
                }
                #[rhai_fn(name = "!=")]
                #[inline(always)]
                pub fn ne(x: $arg_type, y: $arg_type) -> bool {
                    x != y
                }
            }
        })* }
    };
}

macro_rules! reg_functions {
    ($mod_name:ident += $root:ident ; $($arg_type:ident),+) => { $(
        $mod_name.combine_flatten(exported_module!($root::$arg_type::functions));
    )* }
}

def_package!(crate:LogicPackage:"Logical operators.", lib, {
    #[cfg(not(feature = "only_i32"))]
    #[cfg(not(feature = "only_i64"))]
    {
        reg_functions!(lib += numbers; i8, u8, i16, u16, i32, u32, u64);

        #[cfg(not(target_arch = "wasm32"))]
        reg_functions!(lib += num_128; i128, u128);
    }

    #[cfg(not(feature = "no_float"))]
    reg_functions!(lib += float; f32);

    set_exported_fn!(lib, "!", not);
});

// Logic operators
#[export_fn]
#[inline(always)]
fn not(x: bool) -> bool {
    !x
}

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
gen_cmp_functions!(numbers => i8, u8, i16, u16, i32, u32, u64);

#[cfg(not(feature = "only_i32"))]
#[cfg(not(feature = "only_i64"))]
#[cfg(not(target_arch = "wasm32"))]
gen_cmp_functions!(num_128 => i128, u128);

#[cfg(not(feature = "no_float"))]
gen_cmp_functions!(float => f32);
