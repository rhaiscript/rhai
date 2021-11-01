use std::collections::BTreeMap;

use quote::quote;

use crate::attrs::ExportScope;
use crate::function::{
    flatten_type_groups, print_type, ExportedFn, FnNamespaceAccess, FnSpecialAccess, FN_GET,
    FN_IDX_GET, FN_IDX_SET, FN_SET,
};
use crate::module::Module;

pub type ExportedConst = (String, Box<syn::Type>, syn::Expr);

pub fn generate_body(
    fns: &mut [ExportedFn],
    consts: &[ExportedConst],
    sub_modules: &mut [Module],
    parent_scope: &ExportScope,
) -> proc_macro2::TokenStream {
    let mut set_fn_statements: Vec<syn::Stmt> = Vec::new();
    let mut set_const_statements: Vec<syn::Stmt> = Vec::new();
    let mut add_mod_blocks: Vec<syn::ExprBlock> = Vec::new();
    let mut set_flattened_mod_blocks: Vec<syn::ExprBlock> = Vec::new();
    let str_type_path = syn::parse2::<syn::Path>(quote! { str }).unwrap();
    let string_type_path = syn::parse2::<syn::Path>(quote! { String }).unwrap();

    for (const_name, _, _) in consts {
        let const_literal = syn::LitStr::new(&const_name, proc_macro2::Span::call_site());
        let const_ref = syn::Ident::new(&const_name, proc_macro2::Span::call_site());
        set_const_statements.push(
            syn::parse2::<syn::Stmt>(quote! {
                m.set_var(#const_literal, #const_ref);
            })
            .unwrap(),
        );
    }

    for item_mod in sub_modules {
        item_mod.update_scope(&parent_scope);
        if item_mod.skipped() {
            continue;
        }
        let module_name = item_mod.module_name();
        let exported_name: syn::LitStr = syn::LitStr::new(
            item_mod.exported_name().as_ref(),
            proc_macro2::Span::call_site(),
        );
        let cfg_attrs: Vec<&syn::Attribute> = item_mod
            .attrs()
            .iter()
            .filter(|&a| a.path.get_ident().map(|i| *i == "cfg").unwrap_or(false))
            .collect();
        add_mod_blocks.push(
            syn::parse2::<syn::ExprBlock>(quote! {
                {
                    #(#cfg_attrs)*
                    m.set_sub_module(#exported_name, self::#module_name::rhai_module_generate());
                }
            })
            .unwrap(),
        );
        set_flattened_mod_blocks.push(
            syn::parse2::<syn::ExprBlock>(quote! {
                {
                    #(#cfg_attrs)*
                    self::#module_name::rhai_generate_into_module(m, flatten);
                }
            })
            .unwrap(),
        );
    }

    // NB: these are token streams, because re-parsing messes up "> >" vs ">>"
    let mut gen_fn_tokens: Vec<proc_macro2::TokenStream> = Vec::new();
    for function in fns {
        function.update_scope(&parent_scope);
        if function.skipped() {
            continue;
        }
        let fn_token_name = syn::Ident::new(
            &format!("{}_token", function.name()),
            function.name().span(),
        );
        let reg_names = function.exported_names();

        let fn_input_types: Vec<syn::Expr> = function
            .arg_list()
            .map(|fn_arg| match fn_arg {
                syn::FnArg::Receiver(_) => panic!("internal error: receiver fn outside impl!?"),
                syn::FnArg::Typed(syn::PatType { ref ty, .. }) => {
                    let arg_type = match flatten_type_groups(ty.as_ref()) {
                        syn::Type::Reference(syn::TypeReference {
                            mutability: None,
                            ref elem,
                            ..
                        }) => match flatten_type_groups(elem.as_ref()) {
                            syn::Type::Path(ref p) if p.path == str_type_path => {
                                syn::parse2::<syn::Type>(quote! {
                                ImmutableString })
                                .unwrap()
                            }
                            _ => panic!("internal error: non-string shared reference!?"),
                        },
                        syn::Type::Path(ref p) if p.path == string_type_path => {
                            syn::parse2::<syn::Type>(quote! {
                            ImmutableString })
                            .unwrap()
                        }
                        syn::Type::Reference(syn::TypeReference {
                            mutability: Some(_),
                            ref elem,
                            ..
                        }) => match flatten_type_groups(elem.as_ref()) {
                            syn::Type::Path(ref p) => syn::parse2::<syn::Type>(quote! {
                            #p })
                            .unwrap(),
                            _ => panic!("internal error: invalid mutable reference!?"),
                        },
                        t => t.clone(),
                    };

                    syn::parse2::<syn::Expr>(quote! {
                    TypeId::of::<#arg_type>()})
                    .unwrap()
                }
            })
            .collect();

        for fn_literal in reg_names {
            let mut namespace = FnNamespaceAccess::Internal;

            match function.params().special {
                FnSpecialAccess::None => (),
                FnSpecialAccess::Index(_) | FnSpecialAccess::Property(_) => {
                    let reg_name = fn_literal.value();
                    if reg_name.starts_with(FN_GET)
                        || reg_name.starts_with(FN_SET)
                        || reg_name == FN_IDX_GET
                        || reg_name == FN_IDX_SET
                    {
                        namespace = FnNamespaceAccess::Global;
                    }
                }
            }

            match function.params().namespace {
                FnNamespaceAccess::Unset => (),
                ns => namespace = ns,
            }

            let ns_str = syn::Ident::new(
                match namespace {
                    FnNamespaceAccess::Unset => unreachable!(),
                    FnNamespaceAccess::Global => "Global",
                    FnNamespaceAccess::Internal => "Internal",
                },
                fn_literal.span(),
            );

            #[cfg(feature = "metadata")]
            let param_names = quote! {
                Some(#fn_token_name::PARAM_NAMES)
            };
            #[cfg(not(feature = "metadata"))]
            let param_names = quote! { None };

            set_fn_statements.push(
                syn::parse2::<syn::Stmt>(quote! {
                    m.set_fn(#fn_literal, FnNamespace::#ns_str, FnAccess::Public,
                             #param_names, &[#(#fn_input_types),*], #fn_token_name().into());
                })
                .unwrap(),
            );
        }

        gen_fn_tokens.push(quote! {
            #[allow(non_camel_case_types)]
            pub struct #fn_token_name();
        });
        gen_fn_tokens.push(function.generate_impl(&fn_token_name.to_string()));
    }

    let mut generate_fn_call = syn::parse2::<syn::ItemMod>(quote! {
        pub mod generate_info {
            #[allow(unused_imports)]
            use super::*;

            pub fn rhai_module_generate() -> Module {
                let mut m = Module::new();
                rhai_generate_into_module(&mut m, false);
                m.build_index();
                m
            }
            #[allow(unused_mut)]
            pub fn rhai_generate_into_module(m: &mut Module, flatten: bool) {
                #(#set_fn_statements)*
                #(#set_const_statements)*

                if flatten {
                    #(#set_flattened_mod_blocks)*
                } else {
                    #(#add_mod_blocks)*
                }
            }
        }
    })
    .unwrap();

    let (_, generate_call_content) = generate_fn_call.content.take().unwrap();

    quote! {
        #(#generate_call_content)*
        #(#gen_fn_tokens)*
    }
}

pub fn check_rename_collisions(fns: &[ExportedFn]) -> Result<(), syn::Error> {
    fn make_key(name: impl ToString, item_fn: &ExportedFn) -> String {
        item_fn
            .arg_list()
            .fold(name.to_string(), |mut arg_str, fn_arg| {
                let type_string: String = match fn_arg {
                    syn::FnArg::Receiver(_) => unimplemented!("receiver rhai_fns not implemented"),
                    syn::FnArg::Typed(syn::PatType { ref ty, .. }) => print_type(ty),
                };
                arg_str.push('.');
                arg_str.push_str(&type_string);
                arg_str
            })
    }

    let mut renames = BTreeMap::<String, proc_macro2::Span>::new();
    let mut fn_defs = BTreeMap::<String, proc_macro2::Span>::new();

    for item_fn in fns.iter() {
        if !item_fn.params().name.is_empty() || item_fn.params().special != FnSpecialAccess::None {
            let mut names: Vec<_> = item_fn
                .params()
                .name
                .iter()
                .map(|n| (n.clone(), n.clone()))
                .collect();

            if let Some((s, n, _)) = item_fn.params().special.get_fn_name() {
                names.push((s, n));
            }

            for (name, fn_name) in names {
                let current_span = item_fn.params().span.unwrap();
                let key = make_key(&name, item_fn);
                if let Some(other_span) = renames.insert(key, current_span) {
                    let mut err = syn::Error::new(
                        current_span,
                        format!("duplicate Rhai signature for '{}'", fn_name),
                    );
                    err.combine(syn::Error::new(
                        other_span,
                        format!("duplicated function renamed '{}'", fn_name),
                    ));
                    return Err(err);
                }
            }
        } else {
            let ident = item_fn.name();
            if let Some(other_span) = fn_defs.insert(ident.to_string(), ident.span()) {
                let mut err =
                    syn::Error::new(ident.span(), format!("duplicate function '{}'", ident));
                err.combine(syn::Error::new(
                    other_span,
                    format!("duplicated function '{}'", ident),
                ));
                return Err(err);
            }
            let key = make_key(ident, item_fn);
            if let Some(fn_span) = renames.get(&key) {
                let mut err = syn::Error::new(
                    ident.span(),
                    format!("duplicate Rhai signature for '{}'", ident),
                );
                err.combine(syn::Error::new(
                    *fn_span,
                    format!("duplicated function '{}'", ident),
                ));
                return Err(err);
            }
        }
    }

    Ok(())
}
