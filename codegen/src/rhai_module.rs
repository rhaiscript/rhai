use std::collections::HashMap;

use quote::{quote, ToTokens};

use crate::function::ExportedFn;
use crate::module::Module;

pub(crate) type ExportedConst = (String, syn::Expr);

pub(crate) fn generate_body(
    fns: &[ExportedFn],
    consts: &[ExportedConst],
    submodules: &[Module],
) -> proc_macro2::TokenStream {
    let mut set_fn_stmts: Vec<syn::Stmt> = Vec::new();
    let mut set_const_stmts: Vec<syn::Stmt> = Vec::new();
    let mut add_mod_blocks: Vec<syn::ExprBlock> = Vec::new();
    let str_type_path = syn::parse2::<syn::Path>(quote! { str }).unwrap();

    for (const_name, const_expr) in consts {
        let const_literal = syn::LitStr::new(&const_name, proc_macro2::Span::call_site());
        set_const_stmts.push(
            syn::parse2::<syn::Stmt>(quote! {
                m.set_var(#const_literal, #const_expr);
            })
            .unwrap(),
        );
    }

    for itemmod in submodules {
        if itemmod.skipped() {
            continue;
        }
        let module_name: &syn::Ident = itemmod.module_name().unwrap();
        let exported_name: syn::LitStr = if let Some(name) = itemmod.exported_name() {
            syn::LitStr::new(&name, proc_macro2::Span::call_site())
        } else {
            syn::LitStr::new(&module_name.to_string(), proc_macro2::Span::call_site())
        };
        let cfg_attrs: Vec<&syn::Attribute> = itemmod
            .attrs()
            .unwrap()
            .iter()
            .filter(|&a| a.path.get_ident().map(|i| *i == "cfg").unwrap_or(false))
            .collect();
        add_mod_blocks.push(
            syn::parse2::<syn::ExprBlock>(quote! {
                #(#cfg_attrs)* {
                    m.set_sub_module(#exported_name, self::#module_name::rhai_module_generate());
                }
            })
            .unwrap(),
        );
    }

    // NB: these are token streams, because reparsing messes up "> >" vs ">>"
    let mut gen_fn_tokens: Vec<proc_macro2::TokenStream> = Vec::new();
    for function in fns {
        if function.skipped() {
            continue;
        }
        let fn_token_name = syn::Ident::new(
            &format!("{}_token", function.name().to_string()),
            function.name().span(),
        );
        let reg_name = function
            .params()
            .name
            .clone()
            .unwrap_or_else(|| function.name().to_string());
        let fn_literal = syn::LitStr::new(&reg_name, proc_macro2::Span::call_site());
        let fn_input_types: Vec<syn::Expr> = function
            .arg_list()
            .map(|fnarg| match fnarg {
                syn::FnArg::Receiver(_) => panic!("internal error: receiver fn outside impl!?"),
                syn::FnArg::Typed(syn::PatType { ref ty, .. }) => {
                    let arg_type = match ty.as_ref() {
                        syn::Type::Reference(syn::TypeReference {
                            mutability: None,
                            ref elem,
                            ..
                        }) => match elem.as_ref() {
                            syn::Type::Path(ref p) if p.path == str_type_path => {
                                syn::parse2::<syn::Type>(quote! {
                                ImmutableString })
                                .unwrap()
                            }
                            _ => panic!("internal error: non-string shared reference!?"),
                        },
                        syn::Type::Reference(syn::TypeReference {
                            mutability: Some(_),
                            ref elem,
                            ..
                        }) => match elem.as_ref() {
                            syn::Type::Path(ref p) => syn::parse2::<syn::Type>(quote! {
                            #p })
                            .unwrap(),
                            _ => panic!("internal error: non-string shared reference!?"),
                        },
                        t => t.clone(),
                    };
                    syn::parse2::<syn::Expr>(quote! {
                    core::any::TypeId::of::<#arg_type>()})
                    .unwrap()
                }
            })
            .collect();

        set_fn_stmts.push(
            syn::parse2::<syn::Stmt>(quote! {
                m.set_fn(#fn_literal, FnAccess::Public, &[#(#fn_input_types),*],
                         CallableFunction::from_plugin(#fn_token_name()));
            })
            .unwrap(),
        );

        gen_fn_tokens.push(quote! {
            #[allow(non_camel_case_types)]
            struct #fn_token_name();
        });
        gen_fn_tokens.push(function.generate_impl(&fn_token_name.to_string()));
        gen_fn_tokens.push(function.generate_callable(&fn_token_name.to_string()));
        gen_fn_tokens.push(function.generate_input_types(&fn_token_name.to_string()));
    }

    let mut generate_fncall = syn::parse2::<syn::ItemMod>(quote! {
        pub mod generate_info {
            #[allow(unused_imports)]
            use super::*;
            #[allow(unused_mut)]
            pub fn rhai_module_generate() -> Module {
                let mut m = Module::new();
                #(#set_fn_stmts)*
                #(#set_const_stmts)*
                #(#add_mod_blocks)*
                m
            }
        }
    })
    .unwrap();

    let (_, generate_call_content) = generate_fncall.content.take().unwrap();

    quote! {
        #(#generate_call_content)*
        #(#gen_fn_tokens)*
    }
}

pub(crate) fn check_rename_collisions(fns: &Vec<ExportedFn>) -> Result<(), syn::Error> {
    let mut renames = HashMap::<String, proc_macro2::Span>::new();
    let mut names = HashMap::<String, proc_macro2::Span>::new();
    for itemfn in fns.iter() {
        if let Some(ref name) = itemfn.params().name {
            let current_span = itemfn.params().span.as_ref().unwrap();
            let key = itemfn.arg_list().fold(name.clone(), |mut argstr, fnarg| {
                let type_string: String = match fnarg {
                    syn::FnArg::Receiver(_) => unimplemented!("receiver rhai_fns not implemented"),
                    syn::FnArg::Typed(syn::PatType { ref ty, .. }) => {
                        ty.as_ref().to_token_stream().to_string()
                    }
                };
                argstr.push('.');
                argstr.push_str(&type_string);
                argstr
            });
            if let Some(other_span) = renames.insert(key, *current_span) {
                let mut err = syn::Error::new(
                    *current_span,
                    format!("duplicate Rhai signature for '{}'", &name),
                );
                err.combine(syn::Error::new(
                    other_span,
                    format!("duplicated function renamed '{}'", &name),
                ));
                return Err(err);
            }
        } else {
            let ident = itemfn.name();
            if let Some(other_span) = names.insert(ident.to_string(), ident.span()) {
                let mut err = syn::Error::new(
                    ident.span(),
                    format!("duplicate function '{}'", ident.to_string()),
                );
                err.combine(syn::Error::new(
                    other_span,
                    format!("duplicated function '{}'", ident.to_string()),
                ));
                return Err(err);
            }
        }
    }
    for (new_name, attr_span) in renames.drain() {
        let new_name = new_name.split('.').next().unwrap();
        if let Some(fn_span) = names.get(new_name) {
            let mut err = syn::Error::new(
                attr_span,
                format!("duplicate Rhai signature for '{}'", &new_name),
            );
            err.combine(syn::Error::new(
                *fn_span,
                format!("duplicated function '{}'", &new_name),
            ));
            return Err(err);
        }
    }
    Ok(())
}
