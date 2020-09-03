use crate::def_package;
use crate::fn_native::FnPtr;
use crate::plugin::*;

def_package!(crate:BasicFnPackage:"Basic Fn functions.", lib, {
    lib.combine_flatten(exported_module!(fn_ptr_functions));
});

#[export_module]
mod fn_ptr_functions {
    #[inline(always)]
    pub fn name(f: &mut FnPtr) -> ImmutableString {
        f.get_fn_name().clone()
    }

    #[rhai_fn(get = "name")]
    #[inline(always)]
    pub fn name_prop(f: &mut FnPtr) -> ImmutableString {
        name(f)
    }
}
