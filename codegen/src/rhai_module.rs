use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};

use std::collections::BTreeMap;

use crate::attrs::ExportScope;
use crate::function::{
    print_type, ExportedFn, FnNamespaceAccess, FnSpecialAccess, FN_GET, FN_IDX_GET, FN_IDX_SET,
    FN_SET,
};
use crate::module::Module;

#[derive(Debug)]
pub struct ExportedConst {
    pub name: String,
    pub typ: Box<syn::Type>,
    pub expr: syn::Expr,
    pub cfg_attrs: Vec<syn::Attribute>,
}

#[derive(Debug)]
pub struct ExportedType {
    pub name: String,
    pub typ: Box<syn::Type>,
    pub cfg_attrs: Vec<syn::Attribute>,
    #[cfg(feature = "metadata")]
    pub comments: Vec<String>,
}

pub fn generate_body(
    doc: &str,
    fns: &mut [ExportedFn],
    consts: &[ExportedConst],
    custom_types: &[ExportedType],
    sub_modules: &mut [Module],
    parent_scope: &ExportScope,
) -> TokenStream {
    let mut set_fn_statements = Vec::new();
    let mut set_const_statements = Vec::new();
    let mut add_mod_blocks = Vec::new();
    let mut set_flattened_mod_blocks = Vec::new();

    for ExportedConst {
        name: const_name,
        cfg_attrs,
        ..
    } in consts
    {
        let const_literal = syn::LitStr::new(const_name, Span::call_site());
        let const_ref = syn::Ident::new(const_name, Span::call_site());

        let cfg_attrs: Vec<_> = cfg_attrs
            .iter()
            .map(syn::Attribute::to_token_stream)
            .collect();

        set_const_statements.push(
            syn::parse2::<syn::Stmt>(quote! {
                #(#cfg_attrs)*
                m.set_var(#const_literal, #const_ref);
            })
            .unwrap(),
        );
    }

    for ExportedType {
        name,
        typ,
        cfg_attrs,
        #[cfg(feature = "metadata")]
        comments,
        ..
    } in custom_types
    {
        let const_literal = syn::LitStr::new(name, Span::call_site());

        let cfg_attrs: Vec<_> = cfg_attrs
            .iter()
            .map(syn::Attribute::to_token_stream)
            .collect();

        #[cfg(feature = "metadata")]
        let comments = comments
            .iter()
            .map(|s| syn::LitStr::new(s, Span::call_site()))
            .collect::<Vec<_>>();
        #[cfg(not(feature = "metadata"))]
        let comments = Vec::<syn::LitStr>::new();

        set_const_statements.push(if comments.is_empty() {
            syn::parse2::<syn::Stmt>(quote! {
                #(#cfg_attrs)*
                m.set_custom_type::<#typ>(#const_literal);
            })
            .unwrap()
        } else {
            syn::parse2::<syn::Stmt>(quote! {
                #(#cfg_attrs)*
                m.set_custom_type_with_comments::<#typ>(#const_literal, &[#(#comments),*]);
            })
            .unwrap()
        });
    }

    for item_mod in sub_modules {
        item_mod.update_scope(parent_scope);
        if item_mod.skipped() {
            continue;
        }
        let module_name = item_mod.module_name();
        let exported_name = syn::LitStr::new(item_mod.exported_name().as_ref(), Span::call_site());
        let cfg_attrs = crate::attrs::collect_cfg_attr(item_mod.attrs());
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
    let mut gen_fn_tokens = Vec::new();

    for function in fns {
        function.update_scope(parent_scope);
        if function.skipped() {
            continue;
        }
        let fn_token_name = syn::Ident::new(
            &format!("{}_token", function.name()),
            function.name().span(),
        );
        let reg_names = function.exported_names();

        let cfg_attrs: Vec<_> = function
            .cfg_attrs()
            .iter()
            .map(syn::Attribute::to_token_stream)
            .collect();

        for fn_literal in reg_names {
            let mut namespace = FnNamespaceAccess::Internal;

            match function.params().special {
                FnSpecialAccess::None => (),
                FnSpecialAccess::Index(..) | FnSpecialAccess::Property(..) => {
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
                    FnNamespaceAccess::Unset => unreachable!("`namespace` should be set"),
                    FnNamespaceAccess::Global => "Global",
                    FnNamespaceAccess::Internal => "Internal",
                },
                fn_literal.span(),
            );

            let mut tokens = quote! {
                #(#cfg_attrs)*
                FuncRegistration::new(#fn_literal).with_namespace(FnNamespace::#ns_str)
            };
            #[cfg(feature = "metadata")]
            {
                tokens.extend(quote! {
                    .with_params_info(#fn_token_name::PARAM_NAMES)
                });

                let comments = function
                    .comments()
                    .iter()
                    .map(|s| syn::LitStr::new(s, Span::call_site()))
                    .collect::<Vec<_>>();

                if !comments.is_empty() {
                    tokens.extend(quote! {
                        .with_comments(&[#(#comments),*])
                    });
                }
            }

            tokens.extend(quote! {
                .set_into_module_raw(m, &#fn_token_name::param_types(), #fn_token_name().into());
            });

            set_fn_statements.push(syn::parse2::<syn::Stmt>(tokens).unwrap());
        }

        gen_fn_tokens.push(quote! {
            #(#cfg_attrs)*
            #[allow(non_camel_case_types)]
            #[doc(hidden)]
            pub struct #fn_token_name();
        });

        gen_fn_tokens.push(function.generate_impl(&fn_token_name.to_string()));
    }

    let module_docs = if doc.is_empty() {
        None
    } else {
        Some(
            syn::parse2::<syn::Stmt>(quote! {
                m.set_doc(#doc);
            })
            .unwrap(),
        )
    };

    let mut generate_fn_call = syn::parse2::<syn::ItemMod>(quote! {
        pub mod generate_info {
            #[allow(unused_imports)]
            use super::*;

            #[doc(hidden)]
            pub fn rhai_module_generate() -> Module {
                let mut m = Module::new();
                #module_docs
                rhai_generate_into_module(&mut m, false);
                m.build_index();
                m
            }
            #[allow(unused_mut)]
            #[doc(hidden)]
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

    let (.., generate_call_content) = generate_fn_call.content.take().unwrap();

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
                    syn::FnArg::Receiver(..) => unimplemented!("receiver rhai_fns not implemented"),
                    syn::FnArg::Typed(syn::PatType { ref ty, .. }) => print_type(ty),
                };
                arg_str.push('.');
                arg_str.push_str(&type_string);
                arg_str
            })
    }

    let mut renames = BTreeMap::new();
    let mut fn_defs = BTreeMap::new();

    for item_fn in fns {
        if !item_fn.params().name.is_empty() || item_fn.params().special != FnSpecialAccess::None {
            let mut names: Vec<_> = item_fn
                .params()
                .name
                .iter()
                .map(|n| (n.clone(), n.clone()))
                .collect();

            if let Some((s, n, ..)) = item_fn.params().special.get_fn_name() {
                names.push((s, n));
            }

            for (name, fn_name) in names {
                let current_span = item_fn.params().span.unwrap();
                let key = make_key(name, item_fn);
                if let Some(other_span) = renames.insert(key, current_span) {
                    let mut err = syn::Error::new(
                        current_span,
                        format!("duplicate Rhai signature for '{fn_name}'"),
                    );
                    err.combine(syn::Error::new(
                        other_span,
                        format!("duplicated function renamed '{fn_name}'"),
                    ));
                    return Err(err);
                }
            }
        } else {
            let ident = item_fn.name();
            if let Some(other_span) = fn_defs.insert(ident.to_string(), ident.span()) {
                let mut err =
                    syn::Error::new(ident.span(), format!("duplicate function '{ident}'"));
                err.combine(syn::Error::new(
                    other_span,
                    format!("duplicated function '{ident}'"),
                ));
                return Err(err);
            }
            let key = make_key(ident, item_fn);
            if let Some(fn_span) = renames.get(&key) {
                let mut err = syn::Error::new(
                    ident.span(),
                    format!("duplicate Rhai signature for '{ident}'"),
                );
                err.combine(syn::Error::new(
                    *fn_span,
                    format!("duplicated function '{ident}'"),
                ));
                return Err(err);
            }
        }
    }

    Ok(())
}
