use crate::plugin::*;
use crate::{def_package, FnPtr, ImmutableString};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

def_package! {
    /// Package of basic function pointer utilities.
    pub BasicFnPackage(lib) {
        lib.set_standard_lib(true);

        combine_with_exported_manifest!(lib, "FnPtr", fn_ptr_functions);
    }
}

#[export_module(manifest)]
mod fn_ptr_functions {
    /// Return the name of the function.
    ///
    /// # Example
    ///
    /// ```rhai
    /// fn double(x) { x * 2 }
    ///
    /// let f = Fn("double");
    ///
    /// print(f.name);      // prints "double"
    /// ```
    #[rhai_fn(name = "name", get = "name", pure)]
    pub fn name(fn_ptr: &mut FnPtr) -> ImmutableString {
        fn_ptr.fn_name_raw().clone()
    }

    /// Return `true` if the function is an anonymous function.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let f = |x| x * 2;
    ///
    /// print(f.is_anonymous);      // prints true
    /// ```
    #[cfg(not(feature = "no_function"))]
    #[rhai_fn(name = "is_anonymous", get = "is_anonymous", pure)]
    pub fn is_anonymous(fn_ptr: &mut FnPtr) -> bool {
        fn_ptr.is_anonymous()
    }
}
