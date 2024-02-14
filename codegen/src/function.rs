use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned, ToTokens};
use syn::{
    parse::{Parse, ParseStream},
    spanned::Spanned,
};

#[cfg(no_std)]
use alloc::format;
#[cfg(not(no_std))]
use std::format;

use std::borrow::Cow;

use crate::attrs::{ExportInfo, ExportScope, ExportedParams};

#[derive(Clone, Debug, Eq, PartialEq, Copy, Default, Hash)]
pub enum FnNamespaceAccess {
    #[default]
    Unset,
    Global,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash)]
pub enum Index {
    Get,
    Set,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Property {
    Get(syn::Ident),
    Set(syn::Ident),
}

#[derive(Clone, Debug, Eq, PartialEq, Default, Hash)]
pub enum FnSpecialAccess {
    #[default]
    None,
    Index(Index),
    Property(Property),
}

impl FnSpecialAccess {
    pub fn get_fn_name(&self) -> Option<(String, String, Span)> {
        match self {
            FnSpecialAccess::None => None,
            FnSpecialAccess::Property(Property::Get(ref g)) => {
                Some((format!("{FN_GET}{g}"), g.to_string(), g.span()))
            }
            FnSpecialAccess::Property(Property::Set(ref s)) => {
                Some((format!("{FN_SET}{s}"), s.to_string(), s.span()))
            }
            FnSpecialAccess::Index(Index::Get) => Some((
                FN_IDX_GET.to_string(),
                "index_get".to_string(),
                Span::call_site(),
            )),
            FnSpecialAccess::Index(Index::Set) => Some((
                FN_IDX_SET.to_string(),
                "index_set".to_string(),
                Span::call_site(),
            )),
        }
    }
}

pub fn flatten_type_groups(ty: &syn::Type) -> &syn::Type {
    match ty {
        syn::Type::Group(syn::TypeGroup { ref elem, .. })
        | syn::Type::Paren(syn::TypeParen { ref elem, .. }) => flatten_type_groups(elem.as_ref()),
        _ => ty,
    }
}

pub fn print_type(ty: &syn::Type) -> String {
    ty.to_token_stream()
        .to_string()
        .replace(" , ", ", ")
        .replace("& ", "&")
        .replace(" :: ", "::")
        .replace(" ( ", "(")
        .replace(" ) ", ")")
        .replace(" < ", "<")
        .replace(" > ", ">")
}

#[derive(Debug, Default)]
pub struct ExportedFnParams {
    pub name: Vec<String>,
    pub return_raw: Option<Span>,
    pub pure: Option<Span>,
    pub volatile: Option<Span>,
    pub skip: bool,
    pub special: FnSpecialAccess,
    pub namespace: FnNamespaceAccess,
    pub span: Option<Span>,
}

pub const FN_GET: &str = "get$";
pub const FN_SET: &str = "set$";
pub const FN_IDX_GET: &str = "index$get$";
pub const FN_IDX_SET: &str = "index$set$";

impl Parse for ExportedFnParams {
    fn parse(args: ParseStream) -> syn::Result<Self> {
        if args.is_empty() {
            return Ok(<_>::default());
        }

        let info = crate::attrs::parse_attr_items(args)?;
        Self::from_info(info)
    }
}

impl ExportedParams for ExportedFnParams {
    fn parse_stream(args: ParseStream) -> syn::Result<Self> {
        Self::parse(args)
    }

    fn no_attrs() -> Self {
        Self::default()
    }

    fn from_info(info: crate::attrs::ExportInfo) -> syn::Result<Self> {
        let ExportInfo {
            item_span: span,
            items: attrs,
        } = info;
        let mut name = Vec::new();
        let mut return_raw = None;
        let mut pure = None;
        let mut volatile = None;
        let mut skip = false;
        let mut namespace = FnNamespaceAccess::Unset;
        let mut special = FnSpecialAccess::None;
        for attr in attrs {
            let crate::attrs::AttrItem {
                key,
                value,
                span: item_span,
            } = attr;
            match (key.to_string().as_ref(), value) {
                ("get", None) | ("set", None) | ("name", None) => {
                    return Err(syn::Error::new(key.span(), "requires value"))
                }
                ("name", Some(s)) if s.value() == FN_IDX_GET => {
                    return Err(syn::Error::new(
                        item_span,
                        "use attribute 'index_get' instead",
                    ))
                }
                ("name", Some(s)) if s.value() == FN_IDX_SET => {
                    return Err(syn::Error::new(
                        item_span,
                        "use attribute 'index_set' instead",
                    ))
                }
                ("name", Some(s)) if s.value().starts_with(FN_GET) => {
                    return Err(syn::Error::new(
                        item_span,
                        format!(
                            "use attribute 'getter = \"{}\"' instead",
                            &s.value()[FN_GET.len()..]
                        ),
                    ))
                }
                ("name", Some(s)) if s.value().starts_with(FN_SET) => {
                    return Err(syn::Error::new(
                        item_span,
                        format!(
                            "use attribute 'setter = \"{}\"' instead",
                            &s.value()[FN_SET.len()..]
                        ),
                    ))
                }
                ("name", Some(s)) => name.push(s.value()),

                ("index_get", Some(s))
                | ("index_set", Some(s))
                | ("return_raw", Some(s))
                | ("pure", Some(s))
                | ("skip", Some(s))
                | ("global", Some(s))
                | ("internal", Some(s)) => {
                    return Err(syn::Error::new(s.span(), "extraneous value"))
                }

                ("pure", None) => pure = Some(item_span),
                ("volatile", None) => volatile = Some(item_span),
                ("return_raw", None) => return_raw = Some(item_span),
                ("skip", None) => skip = true,
                ("global", None) => match namespace {
                    FnNamespaceAccess::Unset => namespace = FnNamespaceAccess::Global,
                    FnNamespaceAccess::Global => (),
                    FnNamespaceAccess::Internal => {
                        return Err(syn::Error::new(
                            key.span(),
                            "namespace is already set to 'internal'",
                        ))
                    }
                },
                ("internal", None) => match namespace {
                    FnNamespaceAccess::Unset => namespace = FnNamespaceAccess::Internal,
                    FnNamespaceAccess::Internal => (),
                    FnNamespaceAccess::Global => {
                        return Err(syn::Error::new(
                            key.span(),
                            "namespace is already set to 'global'",
                        ))
                    }
                },

                ("get", Some(s)) => {
                    special = match special {
                        FnSpecialAccess::None => FnSpecialAccess::Property(Property::Get(
                            syn::Ident::new(&s.value(), s.span()),
                        )),
                        _ => return Err(syn::Error::new(item_span.span(), "conflicting getter")),
                    }
                }
                ("set", Some(s)) => {
                    special = match special {
                        FnSpecialAccess::None => FnSpecialAccess::Property(Property::Set(
                            syn::Ident::new(&s.value(), s.span()),
                        )),
                        _ => return Err(syn::Error::new(item_span.span(), "conflicting setter")),
                    }
                }
                ("index_get", None) => {
                    special = match special {
                        FnSpecialAccess::None => FnSpecialAccess::Index(Index::Get),
                        _ => {
                            return Err(syn::Error::new(item_span.span(), "conflicting index_get"))
                        }
                    }
                }
                ("index_set", None) => {
                    special = match special {
                        FnSpecialAccess::None => FnSpecialAccess::Index(Index::Set),
                        _ => {
                            return Err(syn::Error::new(item_span.span(), "conflicting index_set"))
                        }
                    }
                }

                (attr, ..) => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown attribute '{attr}'"),
                    ))
                }
            }
        }

        Ok(ExportedFnParams {
            name,
            return_raw,
            pure,
            volatile,
            skip,
            special,
            namespace,
            span: Some(span),
        })
    }
}

#[derive(Debug)]
pub struct ExportedFn {
    entire_span: Span,
    signature: syn::Signature,
    visibility: syn::Visibility,
    pass_context: bool,
    mut_receiver: bool,
    params: ExportedFnParams,
    cfg_attrs: Vec<syn::Attribute>,
    #[cfg(feature = "metadata")]
    comments: Vec<String>,
}

impl Parse for ExportedFn {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let fn_all: syn::ItemFn = input.parse()?;
        let entire_span = fn_all.span();
        let str_type_path = syn::parse2::<syn::Path>(quote! { str }).unwrap();

        let context_type_path1 = syn::parse2::<syn::Path>(quote! { NativeCallContext }).unwrap();
        let context_type_path2 =
            syn::parse2::<syn::Path>(quote! { rhai::NativeCallContext }).unwrap();
        let mut pass_context = false;

        let cfg_attrs = crate::attrs::collect_cfg_attr(&fn_all.attrs);

        let visibility = fn_all.vis;

        // Determine if the function requires a call context
        if let Some(syn::FnArg::Typed(syn::PatType { ref ty, .. })) = fn_all.sig.inputs.first() {
            match flatten_type_groups(ty.as_ref()) {
                syn::Type::Path(p)
                    if p.path == context_type_path1 || p.path == context_type_path2 =>
                {
                    pass_context = true;
                }
                _ => (),
            }
        }

        let skip_slots = usize::from(pass_context);

        // Determine whether function generates a special calling convention for a mutable receiver.
        let mut_receiver = match fn_all.sig.inputs.iter().nth(skip_slots) {
            Some(syn::FnArg::Receiver(syn::Receiver {
                reference: Some(_), ..
            })) => true,
            Some(syn::FnArg::Typed(syn::PatType { ref ty, .. })) => {
                match flatten_type_groups(ty.as_ref()) {
                    syn::Type::Reference(syn::TypeReference {
                        mutability: Some(_),
                        ..
                    }) => true,
                    syn::Type::Reference(syn::TypeReference {
                        mutability: None,
                        ref elem,
                        ..
                    }) => match flatten_type_groups(elem.as_ref()) {
                        syn::Type::Path(ref p) if p.path == str_type_path => false,
                        _ => {
                            return Err(syn::Error::new(
                                ty.span(),
                                "references from Rhai in this position must be mutable",
                            ))
                        }
                    },
                    _ => false,
                }
            }
            _ => false,
        };

        // All arguments after the first must be moved except for &str.
        for arg in fn_all.sig.inputs.iter().skip(skip_slots + 1) {
            let ty = match arg {
                syn::FnArg::Typed(syn::PatType { ref ty, .. }) => ty,
                _ => unreachable!("receiver argument outside of first position!?"),
            };
            let is_ok = match flatten_type_groups(ty.as_ref()) {
                syn::Type::Reference(syn::TypeReference {
                    mutability: Some(_),
                    ..
                }) => false,
                syn::Type::Reference(syn::TypeReference {
                    mutability: None,
                    ref elem,
                    ..
                }) => {
                    matches!(flatten_type_groups(elem.as_ref()), syn::Type::Path(ref p) if p.path == str_type_path)
                }
                syn::Type::Verbatim(..) => false,
                _ => true,
            };
            if !is_ok {
                return Err(syn::Error::new(
                    ty.span(),
                    "function parameters other than the first one cannot be passed by reference",
                ));
            }
        }

        // Check return type.
        if let syn::ReturnType::Type(.., ref ret_type) = fn_all.sig.output {
            match flatten_type_groups(ret_type.as_ref()) {
                syn::Type::Ptr(..) => {
                    return Err(syn::Error::new(
                        fn_all.sig.output.span(),
                        "Rhai functions cannot return pointers",
                    ))
                }
                syn::Type::Reference(..) => {
                    return Err(syn::Error::new(
                        fn_all.sig.output.span(),
                        "Rhai functions cannot return references",
                    ))
                }
                _ => (),
            }
        }
        Ok(ExportedFn {
            entire_span,
            signature: fn_all.sig,
            visibility,
            pass_context,
            mut_receiver,
            params: <_>::default(),
            cfg_attrs,
            #[cfg(feature = "metadata")]
            comments: Vec::new(),
        })
    }
}

impl ExportedFn {
    #![allow(unused)]

    pub fn params(&self) -> &ExportedFnParams {
        &self.params
    }

    pub fn cfg_attrs(&self) -> &[syn::Attribute] {
        &self.cfg_attrs
    }

    pub fn update_scope(&mut self, parent_scope: &ExportScope) {
        let keep = match (self.params.skip, parent_scope) {
            (true, ..) => false,
            (.., ExportScope::PubOnly) => self.is_public(),
            (.., ExportScope::Prefix(s)) => self.name().to_string().starts_with(s),
            (.., ExportScope::All) => true,
        };
        self.params.skip = !keep;
    }

    pub fn skipped(&self) -> bool {
        self.params.skip
    }

    pub fn pass_context(&self) -> bool {
        self.pass_context
    }

    pub fn signature(&self) -> &syn::Signature {
        &self.signature
    }

    pub fn mutable_receiver(&self) -> bool {
        self.mut_receiver
    }

    pub fn is_public(&self) -> bool {
        !matches!(self.visibility, syn::Visibility::Inherited)
    }

    pub fn span(&self) -> &Span {
        &self.entire_span
    }

    pub fn name(&self) -> &syn::Ident {
        &self.signature.ident
    }

    pub fn exported_names(&self) -> Vec<syn::LitStr> {
        let mut literals: Vec<_> = self
            .params
            .name
            .iter()
            .map(|s| syn::LitStr::new(s, Span::call_site()))
            .collect();

        if let Some((s, _, span)) = self.params.special.get_fn_name() {
            literals.push(syn::LitStr::new(&s, span));
        }

        if literals.is_empty() {
            literals.push(syn::LitStr::new(
                &self.signature.ident.to_string(),
                self.signature.ident.span(),
            ));
        }

        literals
    }

    pub fn exported_name(&self) -> Cow<str> {
        self.params
            .name
            .last()
            .map_or_else(|| self.signature.ident.to_string().into(), |s| s.into())
    }

    pub fn arg_list(&self) -> impl Iterator<Item = &syn::FnArg> {
        let skip = usize::from(self.pass_context);
        self.signature.inputs.iter().skip(skip)
    }

    pub fn arg_count(&self) -> usize {
        let skip = usize::from(self.pass_context);
        self.signature.inputs.len() - skip
    }

    pub fn return_type(&self) -> Option<&syn::Type> {
        match self.signature.output {
            syn::ReturnType::Type(.., ref ret_type) => Some(flatten_type_groups(ret_type)),
            _ => None,
        }
    }

    #[cfg(feature = "metadata")]
    pub fn comments(&self) -> &[String] {
        &self.comments
    }

    #[cfg(feature = "metadata")]
    pub fn set_comments(&mut self, comments: Vec<String>) {
        self.comments = comments
    }

    pub fn set_cfg_attrs(&mut self, cfg_attrs: Vec<syn::Attribute>) {
        self.cfg_attrs = cfg_attrs
    }

    pub fn set_params(&mut self, mut params: ExportedFnParams) -> syn::Result<()> {
        // Several issues are checked here to avoid issues with diagnostics caused by raising them later.
        //
        // 1a. Do not allow non-returning raw functions.
        //
        if params.return_raw.is_some() && self.return_type().is_none() {
            return Err(syn::Error::new(
                params.return_raw.unwrap(),
                "functions marked with 'return_raw' must return Result<T, Box<EvalAltResult>>",
            ));
        }

        // 1b. Do not allow non-method pure functions.
        //
        if params.pure.is_some() && !self.mutable_receiver() {
            return Err(syn::Error::new(
                params.pure.unwrap(),
                "'pure' is not necessary on functions without a &mut first parameter",
            ));
        }

        match params.special {
            // 2a. Property getters must take only the subject as an argument.
            FnSpecialAccess::Property(Property::Get(..)) if self.arg_count() != 1 => {
                return Err(syn::Error::new(
                    self.signature.inputs.span(),
                    "property getter requires exactly 1 parameter",
                ))
            }
            // 2b. Property getters must return a value.
            FnSpecialAccess::Property(Property::Get(..)) if self.return_type().is_none() => {
                return Err(syn::Error::new(
                    self.signature.span(),
                    "property getter must return a value",
                ))
            }
            // 3a. Property setters must take the subject and a new value as arguments.
            FnSpecialAccess::Property(Property::Set(..)) if self.arg_count() != 2 => {
                return Err(syn::Error::new(
                    self.signature.inputs.span(),
                    "property setter requires exactly 2 parameters",
                ))
            }
            // 3b. Non-raw property setters must return nothing.
            FnSpecialAccess::Property(Property::Set(..))
                if params.return_raw.is_none() && self.return_type().is_some() =>
            {
                return Err(syn::Error::new(
                    self.signature.output.span(),
                    "property setter cannot return any value",
                ))
            }
            // 4a. Index getters must take the subject and the accessed "index" as arguments.
            FnSpecialAccess::Index(Index::Get) if self.arg_count() != 2 => {
                return Err(syn::Error::new(
                    self.signature.inputs.span(),
                    "index getter requires exactly 2 parameters",
                ))
            }
            // 4b. Index getters must return a value.
            FnSpecialAccess::Index(Index::Get) if self.return_type().is_none() => {
                return Err(syn::Error::new(
                    self.signature.span(),
                    "index getter must return a value",
                ))
            }
            // 5a. Index setters must take the subject, "index", and new value as arguments.
            FnSpecialAccess::Index(Index::Set) if self.arg_count() != 3 => {
                return Err(syn::Error::new(
                    self.signature.inputs.span(),
                    "index setter requires exactly 3 parameters",
                ))
            }
            // 5b. Non-raw index setters must return nothing.
            FnSpecialAccess::Index(Index::Set)
                if params.return_raw.is_none() && self.return_type().is_some() =>
            {
                return Err(syn::Error::new(
                    self.signature.output.span(),
                    "index setter cannot return any value",
                ))
            }
            _ => (),
        }

        self.params = params;
        Ok(())
    }

    pub fn generate(self) -> TokenStream {
        let name: syn::Ident =
            syn::Ident::new(&format!("rhai_fn_{}", self.name()), self.name().span());
        let impl_block = self.generate_impl("Token");
        let dyn_result_fn_block = self.generate_dynamic_fn();
        let vis = self.visibility;
        quote! {
            #[automatically_derived]
            #vis mod #name {
                use super::*;
                #[doc(hidden)]
                pub struct Token();
                #impl_block
                #dyn_result_fn_block
            }
        }
    }

    pub fn generate_dynamic_fn(&self) -> TokenStream {
        let name = self.name().clone();

        let mut dynamic_signature = self.signature.clone();
        dynamic_signature.ident = syn::Ident::new("dynamic_result_fn", Span::call_site());
        dynamic_signature.output = syn::parse2::<syn::ReturnType>(quote! {
            -> RhaiResult
        })
        .unwrap();
        let arguments: Vec<_> = dynamic_signature
            .inputs
            .iter()
            .filter_map(|fn_arg| match fn_arg {
                syn::FnArg::Typed(syn::PatType { ref pat, .. }) => match pat.as_ref() {
                    syn::Pat::Ident(ref ident) => Some(ident.ident.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect();

        let return_span = self
            .return_type()
            .map(|r| r.span())
            .unwrap_or_else(Span::call_site)
            .resolved_at(Span::call_site());
        if self.params.return_raw.is_some() {
            quote_spanned! { return_span =>
                #[allow(unused)]
                #[doc(hidden)]
                #[inline(always)]
                pub #dynamic_signature {
                    #name(#(#arguments),*).map(Dynamic::from)
                }
            }
        } else {
            quote_spanned! { return_span =>
                #[allow(unused)]
                #[doc(hidden)]
                #[inline(always)]
                pub #dynamic_signature {
                    Ok(Dynamic::from(#name(#(#arguments),*)))
                }
            }
        }
    }

    pub fn generate_impl(&self, on_type_name: &str) -> TokenStream {
        let sig_name = self.name().clone();
        let arg_count = self.arg_count();
        let is_method_call = self.mutable_receiver();
        let is_pure = !self.mutable_receiver() || self.params().pure.is_some();
        let is_volatile = self.params().volatile.is_some();
        let pass_context = self.pass_context;

        let mut unpack_statements = Vec::new();
        let mut unpack_exprs = Vec::new();
        #[cfg(feature = "metadata")]
        let mut input_type_names = Vec::new();
        let mut input_type_exprs = Vec::new();

        let return_type = self
            .return_type()
            .map(print_type)
            .unwrap_or_else(|| "()".to_string());

        let skip_first_arg;

        if self.pass_context {
            unpack_exprs.push(syn::parse2::<syn::Expr>(quote! { context.unwrap() }).unwrap());
        }

        // Handle the first argument separately if the function has a "method like" receiver
        if is_method_call {
            skip_first_arg = true;
            let first_arg = self.arg_list().next().unwrap();
            let var = syn::Ident::new("arg0", Span::call_site());
            match first_arg {
                syn::FnArg::Typed(syn::PatType { pat, ty, .. }) => {
                    #[cfg(feature = "metadata")]
                    let arg_name = format!("{}: {}", pat.to_token_stream(), print_type(ty));
                    let arg_type = match flatten_type_groups(ty.as_ref()) {
                        syn::Type::Reference(syn::TypeReference { ref elem, .. }) => elem.as_ref(),
                        p => p,
                    };
                    let downcast_span = quote_spanned!(arg_type.span() =>
                        &mut args[0usize].write_lock::<#arg_type>().unwrap()
                    );
                    unpack_statements.push(
                        syn::parse2::<syn::Stmt>(quote! {
                            let #var = #downcast_span;
                        })
                        .unwrap(),
                    );
                    #[cfg(feature = "metadata")]
                    input_type_names.push(arg_name);
                    input_type_exprs.push(
                        syn::parse2::<syn::Expr>(quote_spanned!(arg_type.span() =>
                            TypeId::of::<#arg_type>()
                        ))
                        .unwrap(),
                    );
                }
                syn::FnArg::Receiver(..) => todo!("true self parameters not implemented yet"),
            }
            unpack_exprs.push(syn::parse2::<syn::Expr>(quote! { #var }).unwrap());
        } else {
            skip_first_arg = false;
        }

        // Handle the rest of the arguments, which all are passed by value.
        //
        // The only exception is strings, which need to be downcast to ImmutableString to enable a
        // zero-copy conversion to &str by reference, or a cloned String.
        let str_type_path = syn::parse2::<syn::Path>(quote! { str }).unwrap();
        let string_type_path = syn::parse2::<syn::Path>(quote! { String }).unwrap();
        for (i, arg) in self.arg_list().enumerate().skip(skip_first_arg as usize) {
            let var = syn::Ident::new(&format!("arg{i}"), Span::call_site());
            let is_string;
            let is_ref;
            match arg {
                syn::FnArg::Typed(syn::PatType { pat, ty, .. }) => {
                    #[cfg(feature = "metadata")]
                    let arg_name = format!("{}: {}", pat.to_token_stream(), print_type(ty));
                    let arg_type = ty.as_ref();
                    let downcast_span = match flatten_type_groups(arg_type) {
                        syn::Type::Reference(syn::TypeReference {
                            mutability: None,
                            ref elem,
                            ..
                        }) => match flatten_type_groups(elem.as_ref()) {
                            syn::Type::Path(ref p) if p.path == str_type_path => {
                                is_string = true;
                                is_ref = true;
                                quote_spanned!(arg_type.span().resolved_at(Span::call_site()) =>
                                    mem::take(args[#i]).into_immutable_string().unwrap()
                                )
                            }
                            _ => unreachable!("why wasn't this found earlier!?"),
                        },
                        syn::Type::Path(ref p) if p.path == string_type_path => {
                            is_string = true;
                            is_ref = false;
                            quote_spanned!(arg_type.span().resolved_at(Span::call_site()) =>
                                mem::take(args[#i]).into_string().unwrap()
                            )
                        }
                        _ => {
                            is_string = false;
                            is_ref = false;
                            quote_spanned!(arg_type.span().resolved_at(Span::call_site()) =>
                                mem::take(args[#i]).cast::<#arg_type>()
                            )
                        }
                    };

                    unpack_statements.push(
                        syn::parse2::<syn::Stmt>(quote! {
                            let #var = #downcast_span;
                        })
                        .unwrap(),
                    );
                    #[cfg(feature = "metadata")]
                    input_type_names.push(arg_name);
                    if !is_string {
                        input_type_exprs.push(
                            syn::parse2::<syn::Expr>(quote_spanned!(arg_type.span() =>
                                TypeId::of::<#arg_type>()
                            ))
                            .unwrap(),
                        );
                    } else {
                        input_type_exprs.push(
                            syn::parse2::<syn::Expr>(quote_spanned!(arg_type.span() =>
                                TypeId::of::<ImmutableString>()
                            ))
                            .unwrap(),
                        );
                    }
                }
                syn::FnArg::Receiver(..) => unreachable!("how did this happen!?"),
            }
            if !is_ref {
                unpack_exprs.push(syn::parse2::<syn::Expr>(quote! { #var }).unwrap());
            } else {
                unpack_exprs.push(syn::parse2::<syn::Expr>(quote! { &#var }).unwrap());
            }
        }

        // In method calls, the first argument will need to be mutably borrowed. Because Rust marks
        // that as needing to borrow the entire array, all of the previous argument unpacking via
        // clone needs to happen first.
        if is_method_call {
            let arg0 = unpack_statements.remove(0);
            unpack_statements.push(arg0);
        }

        // Handle "raw returns", aka cases where the result is a dynamic or an error.
        //
        // This allows skipping the Dynamic::from wrap.
        let return_span = self
            .return_type()
            .map(|r| r.span())
            .unwrap_or_else(Span::call_site)
            .resolved_at(Span::call_site());
        let return_expr = if self.params.return_raw.is_none() {
            quote_spanned! { return_span =>
                Ok(Dynamic::from(#sig_name(#(#unpack_exprs),*)))
            }
        } else {
            quote_spanned! { return_span =>
                #sig_name(#(#unpack_exprs),*).map(Dynamic::from)
            }
        };

        let type_name = syn::Ident::new(on_type_name, Span::call_site());

        #[cfg(feature = "metadata")]
        let param_names = quote! {
            pub const PARAM_NAMES: &'static [&'static str] = &[#(#input_type_names,)* #return_type];
        };
        #[cfg(not(feature = "metadata"))]
        let param_names = quote! {};

        let cfg_attrs: Vec<_> = self
            .cfg_attrs()
            .iter()
            .map(syn::Attribute::to_token_stream)
            .collect();

        quote! {
            #(#cfg_attrs)*
            #[doc(hidden)]
            impl #type_name {
                #param_names
                #[inline(always)] pub fn param_types() -> [TypeId; #arg_count] { [#(#input_type_exprs),*] }
            }
            #(#cfg_attrs)*
            impl PluginFunc for #type_name {
                #[inline(always)]
                fn call(&self, context: Option<NativeCallContext>, args: &mut [&mut Dynamic]) -> RhaiResult {
                    #(#unpack_statements)*
                    #return_expr
                }

                #[inline(always)] fn is_method_call(&self) -> bool { #is_method_call }
                #[inline(always)] fn is_pure(&self) -> bool { #is_pure }
                #[inline(always)] fn is_volatile(&self) -> bool { #is_volatile }
                #[inline(always)] fn has_context(&self) -> bool { #pass_context }
            }
        }
    }
}
