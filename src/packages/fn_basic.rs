use crate::plugin::*;
use crate::{def_package, FnPtr, ImmutableString, NativeCallContext};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

def_package!(crate:BasicFnPackage:"Basic Fn functions.", lib, {
    lib.standard = true;

    combine_with_exported_module!(lib, "FnPtr", fn_ptr_functions);
});

#[export_module]
mod fn_ptr_functions {
    #[rhai_fn(name = "name", get = "name", pure)]
    pub fn name(fn_ptr: &mut FnPtr) -> ImmutableString {
        fn_ptr.fn_name_raw().into()
    }

    #[cfg(not(feature = "no_function"))]
    pub mod functions {
        #[rhai_fn(name = "is_anonymous", get = "is_anonymous", pure)]
        pub fn is_anonymous(fn_ptr: &mut FnPtr) -> bool {
            fn_ptr.is_anonymous()
        }
    }

    #[cfg(not(feature = "no_function"))]
    #[cfg(not(feature = "no_index"))]
    #[cfg(not(feature = "no_object"))]
    pub mod functions_and_maps {
        pub fn get_fn_metadata_list(ctx: NativeCallContext) -> crate::Array {
            collect_fn_metadata(ctx)
        }
    }
}

#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_index"))]
#[cfg(not(feature = "no_object"))]
fn collect_fn_metadata(ctx: NativeCallContext) -> crate::Array {
    use crate::{ast::ScriptFnDef, Array, Identifier, Map};
    use std::collections::BTreeSet;

    // Create a metadata record for a function.
    fn make_metadata(
        dict: &BTreeSet<Identifier>,
        namespace: Option<Identifier>,
        f: &ScriptFnDef,
    ) -> Map {
        const DICT: &str = "key exists";

        let mut map = Map::new();

        if let Some(ns) = namespace {
            map.insert(dict.get("namespace").expect(DICT).clone(), ns.into());
        }
        map.insert(dict.get("name").expect(DICT).clone(), f.name.clone().into());
        map.insert(
            dict.get("access").expect(DICT).clone(),
            match f.access {
                FnAccess::Public => dict.get("public").expect(DICT).clone(),
                FnAccess::Private => dict.get("private").expect(DICT).clone(),
            }
            .into(),
        );
        map.insert(
            dict.get("is_anonymous").expect(DICT).clone(),
            f.name.starts_with(crate::engine::FN_ANONYMOUS).into(),
        );
        map.insert(
            dict.get("params").expect(DICT).clone(),
            f.params
                .iter()
                .cloned()
                .map(Into::<Dynamic>::into)
                .collect::<Array>()
                .into(),
        );

        map
    }

    // Intern strings
    let dict: BTreeSet<Identifier> = [
        "namespace",
        "name",
        "access",
        "public",
        "private",
        "is_anonymous",
        "params",
    ]
    .iter()
    .map(|&s| s.into())
    .collect();

    let mut list = Array::new();

    ctx.iter_namespaces()
        .flat_map(|m| m.iter_script_fn())
        .for_each(|(_, _, _, _, f)| list.push(make_metadata(&dict, None, f).into()));

    #[cfg(not(feature = "no_module"))]
    {
        // Recursively scan modules for script-defined functions.
        fn scan_module(
            list: &mut Array,
            dict: &BTreeSet<Identifier>,
            namespace: Identifier,
            module: &Module,
        ) {
            module.iter_script_fn().for_each(|(_, _, _, _, f)| {
                list.push(make_metadata(dict, Some(namespace.clone()), f).into())
            });
            module.iter_sub_modules().for_each(|(ns, m)| {
                let ns = format!("{}::{}", namespace, ns);
                scan_module(list, dict, ns.into(), m.as_ref())
            });
        }

        ctx.iter_imports_raw()
            .for_each(|(ns, m)| scan_module(&mut list, &dict, ns.clone(), m.as_ref()));
    }

    list
}
