use quote::{quote, ToTokens};
use syn::{parse::Parse, parse::ParseStream};

use crate::function::ExportedFn;
use crate::rhai_module::ExportedConst;

#[cfg(no_std)]
use alloc::vec as new_vec;
#[cfg(not(no_std))]
use std::vec as new_vec;

#[cfg(no_std)]
use core::mem;
#[cfg(not(no_std))]
use std::mem;

use std::borrow::Cow;

use crate::attrs::{AttrItem, ExportInfo, ExportedParams};
use crate::function::{ExportedFnParams};

#[derive(Debug, Default)]
pub(crate) struct ExportedModParams {
    pub name: Option<String>,
    pub skip: bool,
}

impl Parse for ExportedModParams {
    fn parse(args: ParseStream) -> syn::Result<Self> {
        if args.is_empty() {
            return Ok(ExportedModParams::default());
        }

        let info = crate::attrs::parse_attr_items(args)?;

        Self::from_info(info)
    }
}

impl ExportedParams for ExportedModParams {
    fn parse_stream(args: ParseStream) -> syn::Result<Self> {
        Self::parse(args)
    }

    fn no_attrs() -> Self {
        Default::default()
    }

    fn from_info(info: ExportInfo) -> syn::Result<Self> {
        let ExportInfo { items: attrs, .. } = info;
        let mut name = None;
        let mut skip = false;
        for attr in attrs {
            let AttrItem { key, value } = attr;
            match (key.to_string().as_ref(), value) {
                ("name", Some(s)) => name = Some(s.value()),
                ("name", None) => return Err(syn::Error::new(key.span(), "requires value")),
                ("skip", None) => skip = true,
                ("skip", Some(s)) => return Err(syn::Error::new(s.span(), "extraneous value")),
                (attr, _) => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown attribute '{}'", attr),
                    ))
                }
            }
        }

        Ok(ExportedModParams {
            name,
            skip,
            ..Default::default()
        })
    }
}

#[derive(Debug)]
pub(crate) struct Module {
    mod_all: Option<syn::ItemMod>,
    fns: Vec<ExportedFn>,
    consts: Vec<ExportedConst>,
    submodules: Vec<Module>,
    params: ExportedModParams,
}

impl Parse for Module {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut mod_all: syn::ItemMod = input.parse()?;
        let fns: Vec<_>;
        let mut consts: Vec<_> = new_vec![];
        let mut submodules: Vec<_> = Vec::new();
        if let Some((_, ref mut content)) = mod_all.content {
            // Gather and parse functions.
            fns = content
                .iter_mut()
                .filter_map(|item| match item {
                    syn::Item::Fn(f) => Some(f),
                    _ => None,
                })
                .try_fold(Vec::new(), |mut vec, itemfn| {
                    // #[cfg] attributes are not allowed on functions
                    crate::attrs::deny_cfg_attr(&itemfn.attrs)?;

                    let mut params: ExportedFnParams =
                        match crate::attrs::inner_item_attributes(&mut itemfn.attrs, "rhai_fn") {
                            Ok(p) => p,
                            Err(e) => return Err(e),
                        };
                    params.skip = if let syn::Visibility::Public(_) = itemfn.vis {
                        params.skip
                    } else {
                        true
                    };
                    syn::parse2::<ExportedFn>(itemfn.to_token_stream())
                        .and_then(|mut f| {
                            f.set_params(params)?;
                            Ok(f)
                        })
                        .map(|f| vec.push(f))
                        .map(|_| vec)
                })?;
            // Gather and parse constants definitions.
            for item in content.iter() {
                match item {
                    syn::Item::Const(syn::ItemConst {
                        vis,
                        ref expr,
                        ident,
                        attrs,
                        ..
                    }) => {
                        // #[cfg] attributes are not allowed on const declarations
                        crate::attrs::deny_cfg_attr(&attrs)?;
                        if let syn::Visibility::Public(_) = vis {
                            consts.push((ident.to_string(), expr.as_ref().clone()));
                        }
                    }
                    _ => {}
                }
            }
            // Gather and parse submodule definitions.
            //
            // They are actually removed from the module's body, because they will need
            // re-generating later when generated code is added.
            submodules.reserve(content.len() - fns.len() - consts.len());
            let mut i = 0;
            while i < content.len() {
                if let syn::Item::Mod(_) = &content[i] {
                    let mut itemmod = match content.remove(i) {
                        syn::Item::Mod(m) => m,
                        _ => unreachable!(),
                    };
                    let mut params: ExportedModParams =
                        match crate::attrs::inner_item_attributes(&mut itemmod.attrs, "rhai_mod") {
                            Ok(p) => p,
                            Err(e) => return Err(e),
                        };
                    params.skip = if let syn::Visibility::Public(_) = itemmod.vis {
                        params.skip
                    } else {
                        true
                    };
                    let module =
                        syn::parse2::<Module>(itemmod.to_token_stream()).map(|mut f| {
                            f.params = params;
                            f
                        })?;
                    submodules.push(module);
                } else {
                    i += 1;
                }
            }
        } else {
            fns = new_vec![];
        }
        Ok(Module {
            mod_all: Some(mod_all),
            fns,
            consts,
            submodules,
            params: ExportedModParams::default(),
        })
    }
}

impl Module {
    pub fn attrs(&self) -> Option<&Vec<syn::Attribute>> {
        self.mod_all.as_ref().map(|m| &m.attrs)
    }

    pub fn module_name(&self) -> Option<&syn::Ident> {
        self.mod_all.as_ref().map(|m| &m.ident)
    }

    pub fn exported_name(&self) -> Option<Cow<str>> {
        if let Some(ref s) = self.params.name {
            Some(Cow::Borrowed(s))
        } else {
            self.module_name().map(|m| Cow::Owned(m.to_string()))
        }
    }

    pub fn skipped(&self) -> bool {
        self.params.skip
    }

    pub fn generate(self) -> proc_macro2::TokenStream {
        match self.generate_inner() {
            Ok(tokens) => tokens,
            Err(e) => e.to_compile_error(),
        }
    }

    fn generate_inner(self) -> Result<proc_macro2::TokenStream, syn::Error> {
        // Check for collisions if the "name" attribute was used on inner functions.
        crate::rhai_module::check_rename_collisions(&self.fns)?;

        // Extract the current structure of the module.
        let Module {
            mod_all,
            fns,
            consts,
            mut submodules,
            params,
            ..
        } = self;
        let mut mod_all = mod_all.unwrap();
        let mod_name = mod_all.ident.clone();
        let (_, orig_content) = mod_all.content.take().unwrap();
        let mod_attrs = mem::replace(&mut mod_all.attrs, Vec::with_capacity(0));

        if !params.skip {
            // Generate new module items.
            //
            // This is done before inner module recursive generation, because that is destructive.
            let mod_gen = crate::rhai_module::generate_body(&fns, &consts, &submodules);

            // NB: submodules must have their new items for exporting generated in depth-first order
            // to avoid issues caused by re-parsing them
            let inner_modules: Vec<proc_macro2::TokenStream> = submodules.drain(..)
                .try_fold::<Vec<proc_macro2::TokenStream>, _,
                            Result<Vec<proc_macro2::TokenStream>, syn::Error>>(
                    Vec::new(), |mut acc, m| { acc.push(m.generate_inner()?); Ok(acc) })?;

            // Regenerate the module with the new content added.
            Ok(quote! {
                #(#mod_attrs)*
                pub mod #mod_name {
                    #(#orig_content)*
                    #(#inner_modules)*
                    #mod_gen
                }
            })
        } else {
            // Regenerate the original module as-is.
            Ok(quote! {
                #(#mod_attrs)*
                pub mod #mod_name {
                    #(#orig_content)*
                }
            })
        }
    }

    pub fn name(&self) -> Option<&syn::Ident> {
        self.mod_all.as_ref().map(|m| &m.ident)
    }

    pub fn content(&self) -> Option<&Vec<syn::Item>> {
        match self.mod_all {
            Some(syn::ItemMod {
                content: Some((_, ref vec)),
                ..
            }) => Some(vec),
            _ => None,
        }
    }
}

#[cfg(test)]
mod module_tests {
    use super::Module;

    use proc_macro2::TokenStream;
    use quote::quote;

    #[test]
    fn empty_module() {
        let input_tokens: TokenStream = quote! {
            pub mod empty { }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.fns.is_empty());
        assert!(item_mod.consts.is_empty());
    }

    #[test]
    fn one_factory_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.consts.is_empty());
        assert_eq!(item_mod.fns.len(), 1);
        assert_eq!(item_mod.fns[0].name().to_string(), "get_mystic_number");
        assert_eq!(item_mod.fns[0].arg_count(), 0);
        assert_eq!(
            item_mod.fns[0].return_type().unwrap(),
            &syn::parse2::<syn::Type>(quote! { INT }).unwrap()
        );
    }

    #[test]
    fn one_single_arg_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn add_one_to(x: INT) -> INT {
                    x + 1
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.consts.is_empty());
        assert_eq!(item_mod.fns.len(), 1);
        assert_eq!(item_mod.fns[0].name().to_string(), "add_one_to");
        assert_eq!(item_mod.fns[0].arg_count(), 1);
        assert_eq!(
            item_mod.fns[0].arg_list().next().unwrap(),
            &syn::parse2::<syn::FnArg>(quote! { x: INT }).unwrap()
        );
        assert_eq!(
            item_mod.fns[0].return_type().unwrap(),
            &syn::parse2::<syn::Type>(quote! { INT }).unwrap()
        );
    }

    #[test]
    fn one_double_arg_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn add_together(x: INT, y: INT) -> INT {
                    x + y
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        let mut args = item_mod.fns[0].arg_list();
        assert!(item_mod.consts.is_empty());
        assert_eq!(item_mod.fns.len(), 1);
        assert_eq!(item_mod.fns[0].name().to_string(), "add_together");
        assert_eq!(item_mod.fns[0].arg_count(), 2);
        assert_eq!(
            args.next().unwrap(),
            &syn::parse2::<syn::FnArg>(quote! { x: INT }).unwrap()
        );
        assert_eq!(
            args.next().unwrap(),
            &syn::parse2::<syn::FnArg>(quote! { y: INT }).unwrap()
        );
        assert!(args.next().is_none());
        assert_eq!(
            item_mod.fns[0].return_type().unwrap(),
            &syn::parse2::<syn::Type>(quote! { INT }).unwrap()
        );
    }

    #[test]
    fn one_constant_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                pub mod it_is {
                    pub const MYSTIC_NUMBER: INT = 42;
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.fns.is_empty());
        assert!(item_mod.consts.is_empty());
        assert_eq!(item_mod.submodules.len(), 1);
        assert_eq!(&item_mod.submodules[0].consts[0].0, "MYSTIC_NUMBER");
        assert_eq!(
            item_mod.submodules[0].consts[0].1,
            syn::parse2::<syn::Expr>(quote! { 42 }).unwrap()
        );
    }

    #[test]
    fn one_skipped_fn_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub mod skip_this {
                    #[rhai_fn(skip)]
                    pub fn get_mystic_number() -> INT {
                        42
                    }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.fns.is_empty());
        assert!(item_mod.consts.is_empty());
        assert_eq!(item_mod.submodules.len(), 1);
        assert_eq!(item_mod.submodules[0].fns.len(), 1);
        assert!(item_mod.submodules[0].fns[0].skipped());
        assert!(item_mod.submodules[0].consts.is_empty());
        assert!(item_mod.submodules[0].submodules.is_empty());
    }

    #[test]
    fn one_skipped_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[rhai_mod(skip)]
                pub mod skip_this {
                    pub fn get_mystic_number() -> INT {
                        42
                    }
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.fns.is_empty());
        assert!(item_mod.consts.is_empty());
        assert_eq!(item_mod.submodules.len(), 1);
        assert!(item_mod.submodules[0].skipped());
    }

    #[test]
    fn one_constant_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                pub const MYSTIC_NUMBER: INT = 42;
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.fns.is_empty());
        assert_eq!(item_mod.consts.len(), 1);
        assert_eq!(&item_mod.consts[0].0, "MYSTIC_NUMBER");
        assert_eq!(
            item_mod.consts[0].1,
            syn::parse2::<syn::Expr>(quote! { 42 }).unwrap()
        );
    }

    #[test]
    fn one_private_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_eq!(item_mod.fns.len(), 1);
        assert!(item_mod.fns[0].skipped());
        assert!(item_mod.consts.is_empty());
    }

    #[test]
    fn one_skipped_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[rhai_fn(skip)]
                pub fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_eq!(item_mod.fns.len(), 1);
        assert!(item_mod.fns[0].skipped());
        assert!(item_mod.consts.is_empty());
    }

    #[test]
    fn one_private_constant_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                const MYSTIC_NUMBER: INT = 42;
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert!(item_mod.fns.is_empty());
        assert!(item_mod.consts.is_empty());
    }
}

#[cfg(test)]
mod generate_tests {
    use super::Module;

    use proc_macro2::TokenStream;
    use quote::quote;

    fn assert_streams_eq(actual: TokenStream, expected: TokenStream) {
        let actual = actual.to_string();
        let expected = expected.to_string();
        if &actual != &expected {
            let mut counter = 0;
            let iter = actual
                .chars()
                .zip(expected.chars())
                .inspect(|_| counter += 1)
                .skip_while(|(a, e)| *a == *e);
            let (_actual_diff, _expected_diff) = {
                let mut actual_diff = String::new();
                let mut expected_diff = String::new();
                for (a, e) in iter.take(50) {
                    actual_diff.push(a);
                    expected_diff.push(e);
                }
                (actual_diff, expected_diff)
            };
            eprintln!("actual != expected, diverge at char {}", counter);
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn empty_module() {
        let input_tokens: TokenStream = quote! {
            pub mod empty { }
        };

        let expected_tokens = quote! {
            pub mod empty {
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_factory_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let expected_tokens = quote! {
            pub mod one_fn {
                pub fn get_mystic_number() -> INT {
                    42
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m.set_fn("get_mystic_number", FnAccess::Public, &[],
                             CallableFunction::from_plugin(get_mystic_number_token()));
                    m
                }
                #[allow(non_camel_case_types)]
                struct get_mystic_number_token();
                impl PluginFunction for get_mystic_number_token {
                    fn call(&self,
                            args: &mut [&mut Dynamic], pos: Position
                    ) -> Result<Dynamic, Box<EvalAltResult>> {
                        debug_assert_eq!(args.len(), 0usize,
                                            "wrong arg count: {} != {}", args.len(), 0usize);
                        Ok(Dynamic::from(get_mystic_number()))
                    }

                    fn is_method_call(&self) -> bool { false }
                    fn is_varadic(&self) -> bool { false }
                    fn clone_boxed(&self) -> Box<dyn PluginFunction> {
                        Box::new(get_mystic_number_token())
                    }
                    fn input_types(&self) -> Box<[TypeId]> {
                        new_vec![].into_boxed_slice()
                    }
                }
                pub fn get_mystic_number_token_callable() -> CallableFunction {
                    CallableFunction::from_plugin(get_mystic_number_token())
                }
                pub fn get_mystic_number_token_input_types() -> Box<[TypeId]> {
                    get_mystic_number_token().input_types()
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_single_arg_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn add_one_to(x: INT) -> INT {
                    x + 1
                }
            }
        };

        let expected_tokens = quote! {
            pub mod one_fn {
                pub fn add_one_to(x: INT) -> INT {
                    x + 1
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m.set_fn("add_one_to", FnAccess::Public, &[core::any::TypeId::of::<INT>()],
                             CallableFunction::from_plugin(add_one_to_token()));
                    m
                }
                #[allow(non_camel_case_types)]
                struct add_one_to_token();
                impl PluginFunction for add_one_to_token {
                    fn call(&self,
                            args: &mut [&mut Dynamic], pos: Position
                    ) -> Result<Dynamic, Box<EvalAltResult>> {
                        debug_assert_eq!(args.len(), 1usize,
                                            "wrong arg count: {} != {}", args.len(), 1usize);
                        let arg0 = mem::take(args[0usize]).clone().cast::<INT>();
                        Ok(Dynamic::from(add_one_to(arg0)))
                    }

                    fn is_method_call(&self) -> bool { false }
                    fn is_varadic(&self) -> bool { false }
                    fn clone_boxed(&self) -> Box<dyn PluginFunction> {
                        Box::new(add_one_to_token())
                    }
                    fn input_types(&self) -> Box<[TypeId]> {
                        new_vec![TypeId::of::<INT>()].into_boxed_slice()
                    }
                }
                pub fn add_one_to_token_callable() -> CallableFunction {
                    CallableFunction::from_plugin(add_one_to_token())
                }
                pub fn add_one_to_token_input_types() -> Box<[TypeId]> {
                    add_one_to_token().input_types()
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn two_fn_overload_module() {
        let input_tokens: TokenStream = quote! {
            pub mod two_fns {
                #[rhai_fn(name = "add_n")]
                pub fn add_one_to(x: INT) -> INT {
                    x + 1
                }

                #[rhai_fn(name = "add_n")]
                pub fn add_n_to(x: INT, y: INT) -> INT {
                    x + y
                }
            }
        };

        let expected_tokens = quote! {
            pub mod two_fns {
                pub fn add_one_to(x: INT) -> INT {
                    x + 1
                }

                pub fn add_n_to(x: INT, y: INT) -> INT {
                    x + y
                }

                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m.set_fn("add_n", FnAccess::Public, &[core::any::TypeId::of::<INT>()],
                             CallableFunction::from_plugin(add_one_to_token()));
                    m.set_fn("add_n", FnAccess::Public, &[core::any::TypeId::of::<INT>(),
                                                          core::any::TypeId::of::<INT>()],
                             CallableFunction::from_plugin(add_n_to_token()));
                    m
                }

                #[allow(non_camel_case_types)]
                struct add_one_to_token();
                impl PluginFunction for add_one_to_token {
                    fn call(&self,
                            args: &mut [&mut Dynamic], pos: Position
                    ) -> Result<Dynamic, Box<EvalAltResult>> {
                        debug_assert_eq!(args.len(), 1usize,
                                            "wrong arg count: {} != {}", args.len(), 1usize);
                        let arg0 = mem::take(args[0usize]).clone().cast::<INT>();
                        Ok(Dynamic::from(add_one_to(arg0)))
                    }

                    fn is_method_call(&self) -> bool { false }
                    fn is_varadic(&self) -> bool { false }
                    fn clone_boxed(&self) -> Box<dyn PluginFunction> {
                        Box::new(add_one_to_token())
                    }
                    fn input_types(&self) -> Box<[TypeId]> {
                        new_vec![TypeId::of::<INT>()].into_boxed_slice()
                    }
                }
                pub fn add_one_to_token_callable() -> CallableFunction {
                    CallableFunction::from_plugin(add_one_to_token())
                }
                pub fn add_one_to_token_input_types() -> Box<[TypeId]> {
                    add_one_to_token().input_types()
                }

                #[allow(non_camel_case_types)]
                struct add_n_to_token();
                impl PluginFunction for add_n_to_token {
                    fn call(&self,
                            args: &mut [&mut Dynamic], pos: Position
                    ) -> Result<Dynamic, Box<EvalAltResult>> {
                        debug_assert_eq!(args.len(), 2usize,
                                            "wrong arg count: {} != {}", args.len(), 2usize);
                        let arg0 = mem::take(args[0usize]).clone().cast::<INT>();
                        let arg1 = mem::take(args[1usize]).clone().cast::<INT>();
                        Ok(Dynamic::from(add_n_to(arg0, arg1)))
                    }

                    fn is_method_call(&self) -> bool { false }
                    fn is_varadic(&self) -> bool { false }
                    fn clone_boxed(&self) -> Box<dyn PluginFunction> {
                        Box::new(add_n_to_token())
                    }
                    fn input_types(&self) -> Box<[TypeId]> {
                        new_vec![TypeId::of::<INT>(),
                                 TypeId::of::<INT>()].into_boxed_slice()
                    }
                }
                pub fn add_n_to_token_callable() -> CallableFunction {
                    CallableFunction::from_plugin(add_n_to_token())
                }
                pub fn add_n_to_token_input_types() -> Box<[TypeId]> {
                    add_n_to_token().input_types()
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_double_arg_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn add_together(x: INT, y: INT) -> INT {
                    x + y
                }
            }
        };

        let expected_tokens = quote! {
            pub mod one_fn {
                pub fn add_together(x: INT, y: INT) -> INT {
                    x + y
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m.set_fn("add_together", FnAccess::Public, &[core::any::TypeId::of::<INT>(),
                                                                 core::any::TypeId::of::<INT>()],
                             CallableFunction::from_plugin(add_together_token()));
                    m
                }
                #[allow(non_camel_case_types)]
                struct add_together_token();
                impl PluginFunction for add_together_token {
                    fn call(&self,
                            args: &mut [&mut Dynamic], pos: Position
                    ) -> Result<Dynamic, Box<EvalAltResult>> {
                        debug_assert_eq!(args.len(), 2usize,
                                            "wrong arg count: {} != {}", args.len(), 2usize);
                        let arg0 = mem::take(args[0usize]).clone().cast::<INT>();
                        let arg1 = mem::take(args[1usize]).clone().cast::<INT>();
                        Ok(Dynamic::from(add_together(arg0, arg1)))
                    }

                    fn is_method_call(&self) -> bool { false }
                    fn is_varadic(&self) -> bool { false }
                    fn clone_boxed(&self) -> Box<dyn PluginFunction> {
                        Box::new(add_together_token())
                    }
                    fn input_types(&self) -> Box<[TypeId]> {
                        new_vec![TypeId::of::<INT>(),
                             TypeId::of::<INT>()].into_boxed_slice()
                    }
                }
                pub fn add_together_token_callable() -> CallableFunction {
                    CallableFunction::from_plugin(add_together_token())
                }
                pub fn add_together_token_input_types() -> Box<[TypeId]> {
                    add_together_token().input_types()
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_constant_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                pub const MYSTIC_NUMBER: INT = 42;
            }
        };

        let expected_tokens = quote! {
            pub mod one_constant {
                pub const MYSTIC_NUMBER: INT = 42;
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m.set_var("MYSTIC_NUMBER", 42);
                    m
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_constant_module_imports_preserved() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                pub use rhai::INT;
                pub const MYSTIC_NUMBER: INT = 42;
            }
        };

        let expected_tokens = quote! {
            pub mod one_constant {
                pub use rhai::INT;
                pub const MYSTIC_NUMBER: INT = 42;
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m.set_var("MYSTIC_NUMBER", 42);
                    m
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_private_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let expected_tokens = quote! {
            pub mod one_fn {
                fn get_mystic_number() -> INT {
                    42
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_skipped_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[rhai_fn(skip)]
                pub fn get_mystic_number() -> INT {
                    42
                }
            }
        };

        let expected_tokens = quote! {
            pub mod one_fn {
                pub fn get_mystic_number() -> INT {
                    42
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_skipped_submodule() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub fn get_mystic_number() -> INT {
                    42
                }
                #[rhai_mod(skip)]
                pub mod inner_secrets {
                    pub const SECRET_NUMBER: INT = 86;
                }
            }
        };

        let expected_tokens = quote! {
            pub mod one_fn {
                pub fn get_mystic_number() -> INT {
                    42
                }
                pub mod inner_secrets {
                    pub const SECRET_NUMBER: INT = 86;
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m.set_fn("get_mystic_number", FnAccess::Public, &[],
                             CallableFunction::from_plugin(get_mystic_number_token()));
                    m
                }
                #[allow(non_camel_case_types)]
                struct get_mystic_number_token();
                impl PluginFunction for get_mystic_number_token {
                    fn call(&self,
                            args: &mut [&mut Dynamic], pos: Position
                    ) -> Result<Dynamic, Box<EvalAltResult>> {
                        debug_assert_eq!(args.len(), 0usize,
                                            "wrong arg count: {} != {}", args.len(), 0usize);
                        Ok(Dynamic::from(get_mystic_number()))
                    }

                    fn is_method_call(&self) -> bool { false }
                    fn is_varadic(&self) -> bool { false }
                    fn clone_boxed(&self) -> Box<dyn PluginFunction> {
                        Box::new(get_mystic_number_token())
                    }
                    fn input_types(&self) -> Box<[TypeId]> {
                        new_vec![].into_boxed_slice()
                    }
                }
                pub fn get_mystic_number_token_callable() -> CallableFunction {
                    CallableFunction::from_plugin(get_mystic_number_token())
                }
                pub fn get_mystic_number_token_input_types() -> Box<[TypeId]> {
                    get_mystic_number_token().input_types()
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_private_constant_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                const MYSTIC_NUMBER: INT = 42;
            }
        };

        let expected_tokens = quote! {
            pub mod one_constant {
                const MYSTIC_NUMBER: INT = 42;
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_str_arg_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod str_fn {
                pub fn print_out_to(x: &str) {
                    x + 1
                }
            }
        };

        let expected_tokens = quote! {
            pub mod str_fn {
                pub fn print_out_to(x: &str) {
                    x + 1
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m.set_fn("print_out_to", FnAccess::Public,
                             &[core::any::TypeId::of::<ImmutableString>()],
                             CallableFunction::from_plugin(print_out_to_token()));
                    m
                }
                #[allow(non_camel_case_types)]
                struct print_out_to_token();
                impl PluginFunction for print_out_to_token {
                    fn call(&self,
                            args: &mut [&mut Dynamic], pos: Position
                    ) -> Result<Dynamic, Box<EvalAltResult>> {
                        debug_assert_eq!(args.len(), 1usize,
                                            "wrong arg count: {} != {}", args.len(), 1usize);
                        let arg0 = mem::take(args[0usize]).clone().cast::<ImmutableString>();
                        Ok(Dynamic::from(print_out_to(&arg0)))
                    }

                    fn is_method_call(&self) -> bool { false }
                    fn is_varadic(&self) -> bool { false }
                    fn clone_boxed(&self) -> Box<dyn PluginFunction> {
                        Box::new(print_out_to_token())
                    }
                    fn input_types(&self) -> Box<[TypeId]> {
                        new_vec![TypeId::of::<ImmutableString>()].into_boxed_slice()
                    }
                }
                pub fn print_out_to_token_callable() -> CallableFunction {
                    CallableFunction::from_plugin(print_out_to_token())
                }
                pub fn print_out_to_token_input_types() -> Box<[TypeId]> {
                    print_out_to_token().input_types()
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_mut_ref_fn_module() {
        let input_tokens: TokenStream = quote! {
            pub mod ref_fn {
                pub fn increment(x: &mut FLOAT) {
                    *x += 1.0 as FLOAT;
                }
            }
        };

        let expected_tokens = quote! {
            pub mod ref_fn {
                pub fn increment(x: &mut FLOAT) {
                    *x += 1.0 as FLOAT;
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m.set_fn("increment", FnAccess::Public,
                             &[core::any::TypeId::of::<FLOAT>()],
                             CallableFunction::from_plugin(increment_token()));
                    m
                }
                #[allow(non_camel_case_types)]
                struct increment_token();
                impl PluginFunction for increment_token {
                    fn call(&self,
                            args: &mut [&mut Dynamic], pos: Position
                    ) -> Result<Dynamic, Box<EvalAltResult>> {
                        debug_assert_eq!(args.len(), 1usize,
                                            "wrong arg count: {} != {}", args.len(), 1usize);
                        let arg0: &mut _ = &mut args[0usize].write_lock::<FLOAT>().unwrap();
                        Ok(Dynamic::from(increment(arg0)))
                    }

                    fn is_method_call(&self) -> bool { true }
                    fn is_varadic(&self) -> bool { false }
                    fn clone_boxed(&self) -> Box<dyn PluginFunction> {
                        Box::new(increment_token())
                    }
                    fn input_types(&self) -> Box<[TypeId]> {
                        new_vec![TypeId::of::<FLOAT>()].into_boxed_slice()
                    }
                }
                pub fn increment_token_callable() -> CallableFunction {
                    CallableFunction::from_plugin(increment_token())
                }
                pub fn increment_token_input_types() -> Box<[TypeId]> {
                    increment_token().input_types()
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_fn_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                pub mod it_is {
                    pub fn increment(x: &mut FLOAT) {
                        *x += 1.0 as FLOAT;
                    }
                }
            }
        };

        let expected_tokens = quote! {
            pub mod one_fn {
                pub mod it_is {
                    pub fn increment(x: &mut FLOAT) {
                        *x += 1.0 as FLOAT;
                    }
                    #[allow(unused_imports)]
                    use super::*;
                    #[allow(unused_mut)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        m.set_fn("increment", FnAccess::Public,
                                 &[core::any::TypeId::of::<FLOAT>()],
                                 CallableFunction::from_plugin(increment_token()));
                        m
                    }
                    #[allow(non_camel_case_types)]
                    struct increment_token();
                    impl PluginFunction for increment_token {
                        fn call(&self,
                                args: &mut [&mut Dynamic], pos: Position
                        ) -> Result<Dynamic, Box<EvalAltResult>> {
                            debug_assert_eq!(args.len(), 1usize,
                                                "wrong arg count: {} != {}", args.len(), 1usize);
                            let arg0: &mut _ = &mut args[0usize].write_lock::<FLOAT>().unwrap();
                            Ok(Dynamic::from(increment(arg0)))
                        }

                        fn is_method_call(&self) -> bool { true }
                        fn is_varadic(&self) -> bool { false }
                        fn clone_boxed(&self) -> Box<dyn PluginFunction> {
                            Box::new(increment_token())
                        }
                        fn input_types(&self) -> Box<[TypeId]> {
                            new_vec![TypeId::of::<FLOAT>()].into_boxed_slice()
                        }
                    }
                    pub fn increment_token_callable() -> CallableFunction {
                        CallableFunction::from_plugin(increment_token())
                    }
                    pub fn increment_token_input_types() -> Box<[TypeId]> {
                        increment_token().input_types()
                    }
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    { m.set_sub_module("it_is", self::it_is::rhai_module_generate()); }
                    m
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_fn_with_cfg_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_fn {
                #[cfg(not(feature = "no_float"))]
                pub mod it_is {
                    pub fn increment(x: &mut FLOAT) {
                        *x += 1.0 as FLOAT;
                    }
                }
            }
        };

        let expected_tokens = quote! {
            pub mod one_fn {
                #[cfg(not(feature = "no_float"))]
                pub mod it_is {
                    pub fn increment(x: &mut FLOAT) {
                        *x += 1.0 as FLOAT;
                    }
                    #[allow(unused_imports)]
                    use super::*;
                    #[allow(unused_mut)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        m.set_fn("increment", FnAccess::Public,
                                 &[core::any::TypeId::of::<FLOAT>()],
                                 CallableFunction::from_plugin(increment_token()));
                        m
                    }
                    #[allow(non_camel_case_types)]
                    struct increment_token();
                    impl PluginFunction for increment_token {
                        fn call(&self,
                                args: &mut [&mut Dynamic], pos: Position
                        ) -> Result<Dynamic, Box<EvalAltResult>> {
                            debug_assert_eq!(args.len(), 1usize,
                                                "wrong arg count: {} != {}", args.len(), 1usize);
                            let arg0: &mut _ = &mut args[0usize].write_lock::<FLOAT>().unwrap();
                            Ok(Dynamic::from(increment(arg0)))
                        }

                        fn is_method_call(&self) -> bool { true }
                        fn is_varadic(&self) -> bool { false }
                        fn clone_boxed(&self) -> Box<dyn PluginFunction> {
                            Box::new(increment_token())
                        }
                        fn input_types(&self) -> Box<[TypeId]> {
                            new_vec![TypeId::of::<FLOAT>()].into_boxed_slice()
                        }
                    }
                    pub fn increment_token_callable() -> CallableFunction {
                        CallableFunction::from_plugin(increment_token())
                    }
                    pub fn increment_token_input_types() -> Box<[TypeId]> {
                        increment_token().input_types()
                    }
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    #[cfg(not(feature = "no_float"))] {
                        m.set_sub_module("it_is", self::it_is::rhai_module_generate());
                    }
                    m
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn one_constant_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod one_constant {
                pub mod it_is {
                    pub const MYSTIC_NUMBER: INT = 42;
                }
            }
        };

        let expected_tokens = quote! {
            pub mod one_constant {
                pub mod it_is {
                    pub const MYSTIC_NUMBER: INT = 42;
                    #[allow(unused_imports)]
                    use super::*;
                    #[allow(unused_mut)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        m.set_var("MYSTIC_NUMBER", 42);
                        m
                    }
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    { m.set_sub_module("it_is", self::it_is::rhai_module_generate()); }
                    m
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn dual_constant_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod two_constants {
                pub mod first_is {
                    pub const MYSTIC_NUMBER: INT = 42;
                }
                pub mod second_is {
                    pub const SPECIAL_CPU_NUMBER: INT = 68000;
                }
            }
        };

        let expected_tokens = quote! {
            pub mod two_constants {
                pub mod first_is {
                    pub const MYSTIC_NUMBER: INT = 42;
                    #[allow(unused_imports)]
                    use super::*;
                    #[allow(unused_mut)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        m.set_var("MYSTIC_NUMBER", 42);
                        m
                    }
                }
                pub mod second_is {
                    pub const SPECIAL_CPU_NUMBER: INT = 68000;
                    #[allow(unused_imports)]
                    use super::*;
                    #[allow(unused_mut)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        m.set_var("SPECIAL_CPU_NUMBER", 68000);
                        m
                    }
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    { m.set_sub_module("first_is", self::first_is::rhai_module_generate()); }
                    { m.set_sub_module("second_is", self::second_is::rhai_module_generate()); }
                    m
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }

    #[test]
    fn deep_tree_nested_module() {
        let input_tokens: TokenStream = quote! {
            pub mod heap_root {
                pub const VALUE: INT = 100;
                pub mod left {
                    pub const VALUE: INT = 19;
                    pub mod left {
                        pub const VALUE: INT = 17;
                        pub mod left {
                            pub const VALUE: INT = 2;
                        }
                        pub mod right {
                            pub const VALUE: INT = 7;
                        }
                    }
                    pub mod right {
                        pub const VALUE: INT = 3;
                    }
                }
                pub mod right {
                    pub const VALUE: INT = 36;
                    pub mod left {
                        pub const VALUE: INT = 25;
                    }
                    pub mod right {
                        pub const VALUE: INT = 1;
                    }
                }
            }
        };

        let expected_tokens = quote! {
            pub mod heap_root {
                pub const VALUE: INT = 100;
                pub mod left {
                    pub const VALUE: INT = 19;
                    pub mod left {
                        pub const VALUE: INT = 17;
                        pub mod left {
                            pub const VALUE: INT = 2;
                            #[allow(unused_imports)]
                            use super::*;
                            #[allow(unused_mut)]
                            pub fn rhai_module_generate() -> Module {
                                let mut m = Module::new();
                                m.set_var("VALUE", 2);
                                m
                            }
                        }
                        pub mod right {
                            pub const VALUE: INT = 7;
                            #[allow(unused_imports)]
                            use super::*;
                            #[allow(unused_mut)]
                            pub fn rhai_module_generate() -> Module {
                                let mut m = Module::new();
                                m.set_var("VALUE", 7);
                                m
                            }
                        }
                        #[allow(unused_imports)]
                        use super::*;
                        #[allow(unused_mut)]
                        pub fn rhai_module_generate() -> Module {
                            let mut m = Module::new();
                            m.set_var("VALUE", 17);
                            { m.set_sub_module("left", self::left::rhai_module_generate()); }
                            { m.set_sub_module("right", self::right::rhai_module_generate()); }
                            m
                        }
                    }
                    pub mod right {
                        pub const VALUE: INT = 3;
                        #[allow(unused_imports)]
                        use super::*;
                        #[allow(unused_mut)]
                        pub fn rhai_module_generate() -> Module {
                            let mut m = Module::new();
                            m.set_var("VALUE", 3);
                            m
                        }
                    }
                    #[allow(unused_imports)]
                    use super::*;
                    #[allow(unused_mut)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        m.set_var("VALUE", 19);
                        { m.set_sub_module("left", self::left::rhai_module_generate()); }
                        { m.set_sub_module("right", self::right::rhai_module_generate()); }
                        m
                    }
                }
                pub mod right {
                    pub const VALUE: INT = 36;
                    pub mod left {
                        pub const VALUE: INT = 25;
                        #[allow(unused_imports)]
                        use super::*;
                        #[allow(unused_mut)]
                        pub fn rhai_module_generate() -> Module {
                            let mut m = Module::new();
                            m.set_var("VALUE", 25);
                            m
                        }
                    }
                    pub mod right {
                        pub const VALUE: INT = 1;
                        #[allow(unused_imports)]
                        use super::*;
                        #[allow(unused_mut)]
                        pub fn rhai_module_generate() -> Module {
                            let mut m = Module::new();
                            m.set_var("VALUE", 1);
                            m
                        }
                    }
                    #[allow(unused_imports)]
                    use super::*;
                    #[allow(unused_mut)]
                    pub fn rhai_module_generate() -> Module {
                        let mut m = Module::new();
                        m.set_var("VALUE", 36);
                        { m.set_sub_module("left", self::left::rhai_module_generate()); }
                        { m.set_sub_module("right", self::right::rhai_module_generate()); }
                        m
                    }
                }
                #[allow(unused_imports)]
                use super::*;
                #[allow(unused_mut)]
                pub fn rhai_module_generate() -> Module {
                    let mut m = Module::new();
                    m.set_var("VALUE", 100);
                    { m.set_sub_module("left", self::left::rhai_module_generate()); }
                    { m.set_sub_module("right", self::right::rhai_module_generate()); }
                    m
                }
            }
        };

        let item_mod = syn::parse2::<Module>(input_tokens).unwrap();
        assert_streams_eq(item_mod.generate(), expected_tokens);
    }
}
