use quote::{quote, ToTokens};
use syn::{parse::Parse, parse::ParseStream};

#[cfg(no_std)]
use core::mem;
#[cfg(not(no_std))]
use std::mem;

use std::borrow::Cow;

use crate::attrs::{AttrItem, ExportInfo, ExportScope, ExportedParams};
use crate::function::ExportedFn;
use crate::rhai_module::{ExportedConst, ExportedType};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Default)]
pub struct ExportedModParams {
    pub name: String,
    skip: bool,
    pub scope: ExportScope,
}

impl Parse for ExportedModParams {
    fn parse(args: ParseStream) -> syn::Result<Self> {
        if args.is_empty() {
            return Ok(<_>::default());
        }

        Self::from_info(crate::attrs::parse_attr_items(args)?)
    }
}

impl ExportedParams for ExportedModParams {
    fn parse_stream(args: ParseStream) -> syn::Result<Self> {
        Self::parse(args)
    }

    fn no_attrs() -> Self {
        Self::default()
    }

    fn from_info(info: ExportInfo) -> syn::Result<Self> {
        let ExportInfo { items: attrs, .. } = info;
        let mut name = String::new();
        let mut skip = false;
        let mut scope = None;
        for attr in attrs {
            let AttrItem { key, value, .. } = attr;
            match (key.to_string().as_ref(), value) {
                ("name", Some(s)) => {
                    let new_name = s.value();
                    if name == new_name {
                        return Err(syn::Error::new(key.span(), "conflicting name"));
                    }
                    name = new_name;
                }
                ("name", None) => return Err(syn::Error::new(key.span(), "requires value")),

                ("skip", None) => skip = true,
                ("skip", Some(s)) => return Err(syn::Error::new(s.span(), "extraneous value")),

                ("export_prefix", Some(_)) | ("export_all", None) if scope.is_some() => {
                    return Err(syn::Error::new(key.span(), "duplicate export scope"));
                }
                ("export_prefix", Some(s)) => scope = Some(ExportScope::Prefix(s.value())),
                ("export_prefix", None) => {
                    return Err(syn::Error::new(key.span(), "requires value"))
                }
                ("export_all", None) => scope = Some(ExportScope::All),
                ("export_all", Some(s)) => {
                    return Err(syn::Error::new(s.span(), "extraneous value"))
                }
                (attr, ..) => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown attribute '{attr}'"),
                    ))
                }
            }
        }

        let scope = scope.unwrap_or_default();

        Ok(ExportedModParams { name, skip, scope })
    }
}

#[derive(Debug)]
pub struct Module {
    mod_all: syn::ItemMod,
    consts: Vec<ExportedConst>,
    custom_types: Vec<ExportedType>,
    fns: Vec<ExportedFn>,
    sub_modules: Vec<Module>,
    params: ExportedModParams,
}

impl Module {
    pub fn set_params(&mut self, params: ExportedModParams) -> syn::Result<()> {
        self.params = params;
        Ok(())
    }
}

impl Parse for Module {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut mod_all: syn::ItemMod = input.parse()?;

        let fns: Vec<_>;
        let mut consts = Vec::new();
        let mut custom_types = Vec::new();
        let mut sub_modules = Vec::new();

        if let Some((.., ref mut content)) = mod_all.content {
            // Gather and parse functions.
            fns = content
                .iter_mut()
                .filter_map(|item| match item {
                    syn::Item::Fn(f) => Some(f),
                    _ => None,
                })
                .try_fold(Vec::new(), |mut vec, item_fn| -> syn::Result<_> {
                    let params =
                        crate::attrs::inner_item_attributes(&mut item_fn.attrs, "rhai_fn")?;

                    let f =
                        syn::parse2(item_fn.to_token_stream()).and_then(|mut f: ExportedFn| {
                            f.set_params(params)?;
                            f.set_cfg_attrs(crate::attrs::collect_cfg_attr(&item_fn.attrs));

                            #[cfg(feature = "metadata")]
                            f.set_comments(crate::attrs::doc_attributes(&item_fn.attrs)?);
                            Ok(f)
                        })?;

                    vec.push(f);
                    Ok(vec)
                })?;
            // Gather and parse constants definitions.
            for item in &*content {
                if let syn::Item::Const(syn::ItemConst {
                    vis: syn::Visibility::Public(..),
                    ref expr,
                    ident,
                    attrs,
                    ty,
                    ..
                }) = item
                {
                    consts.push(ExportedConst {
                        name: ident.to_string(),
                        typ: ty.clone(),
                        expr: expr.as_ref().clone(),
                        cfg_attrs: crate::attrs::collect_cfg_attr(attrs),
                    })
                }
            }
            // Gather and parse type definitions.
            for item in &*content {
                if let syn::Item::Type(syn::ItemType {
                    vis: syn::Visibility::Public(..),
                    ident,
                    attrs,
                    ty,
                    ..
                }) = item
                {
                    custom_types.push(ExportedType {
                        name: ident.to_string(),
                        typ: ty.clone(),
                        cfg_attrs: crate::attrs::collect_cfg_attr(attrs),
                        #[cfg(feature = "metadata")]
                        comments: crate::attrs::doc_attributes(attrs)?,
                    })
                }
            }
            // Gather and parse sub-module definitions.
            //
            // They are actually removed from the module's body, because they will need
            // re-generating later when generated code is added.
            sub_modules.reserve(content.len() - fns.len() - consts.len());
            let mut i = 0;
            while i < content.len() {
                match content[i] {
                    syn::Item::Mod(..) => {
                        let mut item_mod = match content.remove(i) {
                            syn::Item::Mod(m) => m,
                            _ => unreachable!(),
                        };
                        let params: ExportedModParams =
                            crate::attrs::inner_item_attributes(&mut item_mod.attrs, "rhai_mod")?;
                        let module = syn::parse2::<Module>(item_mod.to_token_stream()).and_then(
                            |mut m| {
                                m.set_params(params)?;
                                Ok(m)
                            },
                        )?;
                        sub_modules.push(module);
                    }
                    _ => i += 1,
                }
            }
        } else {
            fns = Vec::new();
        }
        Ok(Module {
            mod_all,
            fns,
            consts,
            custom_types,
            sub_modules,
            params: <_>::default(),
        })
    }
}

impl Module {
    pub fn attrs(&self) -> &[syn::Attribute] {
        &self.mod_all.attrs
    }

    pub fn module_name(&self) -> &syn::Ident {
        &self.mod_all.ident
    }

    pub fn exported_name(&self) -> Cow<str> {
        if !self.params.name.is_empty() {
            (&self.params.name).into()
        } else {
            self.module_name().to_string().into()
        }
    }

    pub fn update_scope(&mut self, parent_scope: &ExportScope) {
        let keep = match (self.params.skip, parent_scope) {
            (true, ..) => false,
            (.., ExportScope::PubOnly) => matches!(self.mod_all.vis, syn::Visibility::Public(..)),
            (.., ExportScope::Prefix(s)) => self.mod_all.ident.to_string().starts_with(s),
            (.., ExportScope::All) => true,
        };
        self.params.skip = !keep;
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
            mut mod_all,
            mut fns,
            consts,
            custom_types,
            mut sub_modules,
            params,
            ..
        } = self;
        let mod_vis = mod_all.vis;
        let mod_name = mod_all.ident.clone();
        let (.., orig_content) = mod_all.content.take().unwrap();
        let mod_attrs = mem::take(&mut mod_all.attrs);

        #[cfg(feature = "metadata")]
        let mod_doc = crate::attrs::doc_attributes(&mod_attrs)?.join("\n");
        #[cfg(not(feature = "metadata"))]
        let mod_doc = String::new();

        if !params.skip {
            // Generate new module items.
            //
            // This is done before inner module recursive generation, because that is destructive.
            let mod_gen = crate::rhai_module::generate_body(
                &mod_doc,
                &mut fns,
                &consts,
                &custom_types,
                &mut sub_modules,
                &params.scope,
            );

            // NB: sub-modules must have their new items for exporting generated in depth-first order
            // to avoid issues caused by re-parsing them
            let inner_modules = sub_modules
                .into_iter()
                .try_fold::<_, _, Result<_, syn::Error>>(Vec::new(), |mut acc, m| {
                    acc.push(m.generate_inner()?);
                    Ok(acc)
                })?;

            // Regenerate the module with the new content added.
            Ok(quote! {
                #(#mod_attrs)*
                #[allow(clippy::needless_pass_by_value, clippy::needless_pass_by_ref_mut)]
                #mod_vis mod #mod_name {
                    #(#orig_content)*
                    #(#inner_modules)*
                    #mod_gen
                }
            })
        } else {
            // Regenerate the original module as-is.
            Ok(quote! {
                #(#mod_attrs)*
                #mod_vis mod #mod_name {
                    #(#orig_content)*
                }
            })
        }
    }

    #[allow(dead_code)]
    pub fn name(&self) -> &syn::Ident {
        &self.mod_all.ident
    }

    #[allow(dead_code)]
    pub fn consts(&self) -> &[ExportedConst] {
        &self.consts
    }

    #[allow(dead_code)]
    pub fn custom_types(&self) -> &[ExportedType] {
        &self.custom_types
    }

    #[allow(dead_code)]
    pub fn fns(&self) -> &[ExportedFn] {
        &self.fns
    }

    #[allow(dead_code)]
    pub fn sub_modules(&self) -> &[Module] {
        &self.sub_modules
    }

    #[allow(dead_code)]
    pub fn content(&self) -> Option<&[syn::Item]> {
        match self.mod_all {
            syn::ItemMod {
                content: Some((.., ref vec)),
                ..
            } => Some(vec),
            _ => None,
        }
    }
}
