use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    punctuated::Punctuated, spanned::Spanned, Data, DataStruct, DeriveInput, Expr, Field, Fields,
    Lifetime, MetaNameValue, Path, Token, TraitBound,
};

const ATTR: &str = "rhai_type";

const OPTION_NAME: &str = "name";
const OPTION_SKIP: &str = "skip";
const OPTION_GET: &str = "get";
const OPTION_GET_MUT: &str = "get_mut";
const OPTION_SET: &str = "set";
const OPTION_READONLY: &str = "readonly";
const OPTION_EXTRA: &str = "extra";

/// Derive the `CustomType` trait for a struct.
pub fn derive_custom_type_impl(input: DeriveInput) -> TokenStream {
    let type_name = input.ident;
    let mut display_name = quote! { stringify!(#type_name) };
    let mut field_accessors = Vec::new();
    let mut extras = Vec::new();
    let mut errors = Vec::new();

    for attr in input.attrs.iter().filter(|a| a.path().is_ident(ATTR)) {
        let config_list: Result<Punctuated<Expr, Token![,]>, _> =
            attr.parse_args_with(Punctuated::parse_terminated);

        match config_list {
            Ok(list) => {
                for expr in list {
                    match expr {
                        // Key-value
                        Expr::Assign(..) => {
                            let MetaNameValue { path, value, .. } =
                                syn::parse2::<MetaNameValue>(expr.to_token_stream()).unwrap();

                            if path.is_ident(OPTION_NAME) {
                                // Type name
                                display_name = value.to_token_stream();
                            } else if path.is_ident(OPTION_EXTRA) {
                                match syn::parse2::<Path>(value.to_token_stream()) {
                                    Ok(path) => extras.push(path.to_token_stream()),
                                    Err(err) => errors.push(err.into_compile_error()),
                                }
                            } else {
                                let key = path.get_ident().unwrap().to_string();
                                let msg = format!("invalid option: '{key}'");
                                errors.push(syn::Error::new(path.span(), msg).into_compile_error());
                            }
                        }
                        // skip
                        Expr::Path(path) if path.path.is_ident(OPTION_SKIP) => {
                            println!("SKIPPED");
                        }
                        // any other identifier
                        Expr::Path(path) if path.path.get_ident().is_some() => {
                            let key = path.path.get_ident().unwrap().to_string();
                            let msg = format!("invalid option: '{key}'");
                            errors.push(syn::Error::new(path.span(), msg).into_compile_error());
                        }
                        // Error
                        _ => errors.push(
                            syn::Error::new(expr.span(), "expecting identifier")
                                .into_compile_error(),
                        ),
                    }
                }
            }
            Err(err) => errors.push(err.into_compile_error()),
        }
    }

    match input.data {
        // struct Foo { ... }
        Data::Struct(DataStruct {
            fields: Fields::Named(ref f),
            ..
        }) => scan_fields(
            &f.named.iter().collect::<Vec<_>>(),
            &mut field_accessors,
            &mut errors,
        ),

        // struct Foo(...);
        Data::Struct(DataStruct {
            fields: Fields::Unnamed(ref f),
            ..
        }) => scan_fields(
            &f.unnamed.iter().collect::<Vec<_>>(),
            &mut field_accessors,
            &mut errors,
        ),

        // struct Foo;
        Data::Struct(DataStruct {
            fields: Fields::Unit,
            ..
        }) => (),

        // enum ...
        Data::Enum(_) => {
            return syn::Error::new(Span::call_site(), "enums are not yet implemented")
                .into_compile_error()
        }

        // union ...
        Data::Union(_) => {
            return syn::Error::new(Span::call_site(), "unions are not yet supported")
                .into_compile_error()
        }
    };

    let register = {
        let method = {
            quote! { builder.with_name(#display_name) }
        };

        #[cfg(feature = "metadata")]
        {
            let Ok(docs) = crate::attrs::doc_attributes(&input.attrs) else {
                return syn::Error::new(Span::call_site(), "failed to parse doc comments")
                    .into_compile_error();
            };
            // Not sure how to make a Vec<String> a literal, using a string instead.
            let docs = proc_macro2::Literal::string(&docs.join("\n"));
            quote! {  #method.with_comments(&#docs.lines().collect::<Vec<_>>()[..]); }
        }
        #[cfg(not(feature = "metadata"))]
        quote! { #method; }
    };

    let generics = input.generics;
    let mut impl_generics = generics.clone();
    for param in impl_generics.type_params_mut() {
        param.bounds.push(
            TraitBound {
                paren_token: None,
                modifier: syn::TraitBoundModifier::None,
                lifetimes: None,
                path: syn::parse("::core::clone::Clone".parse().unwrap()).unwrap(),
            }
            .into(),
        );

        param
            .bounds
            .push(Lifetime::new("'static", Span::call_site()).into());

        #[cfg(feature = "sync")]
        {
            param.bounds.push(
                TraitBound {
                    paren_token: None,
                    modifier: syn::TraitBoundModifier::None,
                    lifetimes: None,
                    path: syn::parse("Send".parse().unwrap()).unwrap(),
                }
                .into(),
            );
            param.bounds.push(
                TraitBound {
                    paren_token: None,
                    modifier: syn::TraitBoundModifier::None,
                    lifetimes: None,
                    path: syn::parse("Sync".parse().unwrap()).unwrap(),
                }
                .into(),
            );
        }
    }

    quote! {
        impl #impl_generics CustomType for #type_name #generics {
            fn build(mut builder: TypeBuilder<Self>) {
                #(#errors)*
                #register
                #(#field_accessors)*
                #(#extras(&mut builder);)*
            }
        }
    }
}

// Code lifted from: https://stackoverflow.com/questions/55271857/how-can-i-get-the-t-from-an-optiont-when-using-syn
fn extract_type_from_option(ty: &syn::Type) -> Option<&syn::Type> {
    use syn::{GenericArgument, Path, PathArguments, PathSegment};

    fn extract_type_path(ty: &syn::Type) -> Option<&Path> {
        match *ty {
            syn::Type::Path(ref type_path) if type_path.qself.is_none() => Some(&type_path.path),
            _ => None,
        }
    }

    // TODO store (with lazy static) the vec of string
    // TODO maybe optimization, reverse the order of segments
    fn extract_option_segment(path: &Path) -> Option<&PathSegment> {
        let idents_of_path = path.segments.iter().fold(String::new(), |mut acc, v| {
            acc.push_str(&v.ident.to_string());
            acc.push('|');
            acc
        });
        vec!["Option|", "std|option|Option|", "core|option|Option|"]
            .into_iter()
            .find(|s| idents_of_path == *s)
            .and_then(|_| path.segments.last())
    }

    extract_type_path(ty)
        .and_then(|path| extract_option_segment(path))
        .and_then(|path_seg| {
            let type_params = &path_seg.arguments;
            // It should have only on angle-bracketed param ("<String>"):
            match *type_params {
                PathArguments::AngleBracketed(ref params) => params.args.first(),
                _ => None,
            }
        })
        .and_then(|generic_arg| match *generic_arg {
            GenericArgument::Type(ref ty) => Some(ty),
            _ => None,
        })
}

fn scan_fields(fields: &[&Field], accessors: &mut Vec<TokenStream>, errors: &mut Vec<TokenStream>) {
    for (i, &field) in fields.iter().enumerate() {
        let mut map_name = None;
        let mut get_fn = None;
        let mut get_mut_fn = None;
        let mut set_fn = None;
        let mut readonly = false;
        let mut skip = false;

        for attr in field.attrs.iter().filter(|a| a.path().is_ident(ATTR)) {
            let options_list: Result<Punctuated<Expr, Token![,]>, _> =
                attr.parse_args_with(Punctuated::parse_terminated);

            let options = match options_list {
                Ok(list) => list,
                Err(err) => {
                    errors.push(err.into_compile_error());
                    continue;
                }
            };

            for expr in options {
                let ident = match expr {
                    // skip
                    Expr::Path(path) if path.path.is_ident(OPTION_SKIP) => {
                        skip = true;

                        // `skip` cannot be used with any other attributes.
                        if get_fn.is_some()
                            || get_mut_fn.is_some()
                            || set_fn.is_some()
                            || map_name.is_some()
                            || readonly
                        {
                            let msg = format!("cannot use '{OPTION_SKIP}' with other attributes");
                            errors.push(syn::Error::new(path.span(), msg).into_compile_error());
                        }

                        continue;
                    }
                    // readonly
                    Expr::Path(path) if path.path.is_ident(OPTION_READONLY) => {
                        readonly = true;

                        if set_fn.is_some() {
                            let msg = format!("cannot use '{OPTION_READONLY}' with '{OPTION_SET}'");
                            errors
                                .push(syn::Error::new(path.path.span(), msg).into_compile_error());
                        }

                        path.path.get_ident().unwrap().clone()
                    }
                    // Key-value
                    Expr::Assign(..) => {
                        let MetaNameValue { path, value, .. } =
                            syn::parse2::<MetaNameValue>(expr.to_token_stream()).unwrap();

                        if path.is_ident(OPTION_NAME) {
                            // Type name
                            map_name = Some(value.to_token_stream());
                        } else if path.is_ident(OPTION_GET) {
                            match syn::parse2::<Path>(value.to_token_stream()) {
                                Ok(path) => get_fn = Some(path.to_token_stream()),
                                Err(err) => errors.push(err.into_compile_error()),
                            }
                        } else if path.is_ident(OPTION_GET_MUT) {
                            match syn::parse2::<Path>(value.to_token_stream()) {
                                Ok(path) => get_mut_fn = Some(path.to_token_stream()),
                                Err(err) => errors.push(err.into_compile_error()),
                            }
                        } else if path.is_ident(OPTION_SET) {
                            match syn::parse2::<Path>(value.to_token_stream()) {
                                Ok(path) => set_fn = Some(path.to_token_stream()),
                                Err(err) => errors.push(err.into_compile_error()),
                            }
                        } else if path.is_ident(OPTION_SKIP) || path.is_ident(OPTION_READONLY) {
                            let key = path.get_ident().unwrap().to_string();
                            let msg = format!("'{key}' cannot have value");
                            errors.push(syn::Error::new(path.span(), msg).into_compile_error());
                            continue;
                        } else {
                            let key = path.get_ident().unwrap().to_string();
                            let msg = format!("invalid option: '{key}'");
                            errors.push(syn::Error::new(path.span(), msg).into_compile_error());
                            continue;
                        }

                        path.get_ident().unwrap().clone()
                    }
                    // any other identifier
                    Expr::Path(path) if path.path.get_ident().is_some() => {
                        let key = path.path.get_ident().unwrap().to_string();
                        let msg = format!("invalid option: '{key}'");
                        errors.push(syn::Error::new(path.span(), msg).into_compile_error());
                        continue;
                    }

                    // Error
                    _ => {
                        errors.push(
                            syn::Error::new(expr.span(), "expecting identifier")
                                .into_compile_error(),
                        );
                        continue;
                    }
                };

                if skip {
                    let msg = format!("cannot use '{ident}' with '{OPTION_SKIP}'");
                    errors.push(syn::Error::new(attr.path().span(), msg).into_compile_error());
                }
            }
        }

        // If skipped don't do anything.
        if skip {
            continue;
        }

        // No field name - use field0, field1...
        let field_name = if let Some(ref field_name) = field.ident {
            quote! { #field_name }
        } else {
            if map_name.is_none() {
                let name = format!("field{i}");
                map_name = Some(quote! { #name });
            }
            let index = proc_macro2::Literal::usize_unsuffixed(i);
            quote! { #index }
        };

        // Handle `Option` fields
        let option_type = extract_type_from_option(&field.ty);

        // Override functions

        let get_impl = match (get_mut_fn, get_fn) {
            (Some(func), _) => func,
            (None, Some(func)) => quote! { |obj: &mut Self| #func(&*obj) },
            (None, None) => {
                if option_type.is_some() {
                    quote! { |obj: &mut Self| obj.#field_name.clone().map_or(Dynamic::UNIT, Dynamic::from) }
                } else {
                    quote! { |obj: &mut Self| obj.#field_name.clone() }
                }
            }
        };

        let set_impl = set_fn.unwrap_or_else(|| {
            if let Some(typ) = option_type {
                quote! {
                    |obj: &mut Self, val: Dynamic| {
                        if val.is_unit() {
                            obj.#field_name = None;
                            Ok(())
                        } else if let Some(x) = val.read_lock::<#typ>() {
                            obj.#field_name = Some(x.clone());
                            Ok(())
                        } else {
                            Err(Box::new(EvalAltResult::ErrorMismatchDataType(
                                stringify!(#typ).to_string(),
                                val.type_name().to_string(),
                                Position::NONE
                            )))
                        }
                    }
                }
            } else {
                quote! { |obj: &mut Self, val| obj.#field_name = val }
            }
        });

        let name = map_name.unwrap_or_else(|| quote! { stringify!(#field_name) });

        accessors.push({
            let method = if readonly {
                quote! { builder.with_get(#name, #get_impl) }
            } else {
                quote! { builder.with_get_set(#name, #get_impl, #set_impl) }
            };

            #[cfg(feature = "metadata")]
            {
                match crate::attrs::doc_attributes(&field.attrs) {
                    Ok(docs) => {
                        // Not sure how to make a Vec<String> a literal, using a string instead.
                        let docs = proc_macro2::Literal::string(&docs.join("\n"));
                        quote! { #method.and_comments(&#docs.lines().collect::<Vec<_>>()[..]); }
                    }
                    Err(_) => {
                        errors.push(
                            syn::Error::new(
                                Span::call_site(),
                                format!(
                                    "failed to parse doc comments for field {}",
                                    quote! { #name }
                                ),
                            )
                            .into_compile_error(),
                        );
                        continue;
                    }
                }
            }
            #[cfg(not(feature = "metadata"))]
            quote! { #method; }
        });
    }
}
