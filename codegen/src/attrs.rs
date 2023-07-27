use proc_macro2::{Ident, Span, TokenStream};
use syn::{
    parse::{ParseStream, Parser},
    spanned::Spanned,
};

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash)]
pub enum ExportScope {
    #[default]
    PubOnly,
    Prefix(String),
    All,
}

pub trait ExportedParams: Sized {
    fn parse_stream(args: ParseStream) -> syn::Result<Self>;
    fn no_attrs() -> Self;
    fn from_info(info: ExportInfo) -> syn::Result<Self>;
}

#[derive(Debug, Clone)]
pub struct AttrItem {
    pub key: Ident,
    pub value: Option<syn::LitStr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExportInfo {
    pub item_span: Span,
    pub items: Vec<AttrItem>,
}

pub fn parse_attr_items(args: ParseStream) -> syn::Result<ExportInfo> {
    if args.is_empty() {
        return Ok(ExportInfo {
            item_span: args.span(),
            items: Vec::new(),
        });
    }
    let arg_list = args.call(syn::punctuated::Punctuated::parse_separated_nonempty)?;

    parse_punctuated_items(arg_list)
}

pub fn parse_punctuated_items(
    arg_list: syn::punctuated::Punctuated<syn::Expr, syn::Token![,]>,
) -> syn::Result<ExportInfo> {
    let list_span = arg_list.span();

    let mut attrs = Vec::new();

    for arg in arg_list {
        let arg_span = arg.span();
        let (key, value) = match arg {
            syn::Expr::Assign(syn::ExprAssign {
                ref left,
                ref right,
                ..
            }) => {
                let attr_name = match left.as_ref() {
                    syn::Expr::Path(syn::ExprPath {
                        path: attr_path, ..
                    }) => attr_path.get_ident().cloned().ok_or_else(|| {
                        syn::Error::new(attr_path.span(), "expecting attribute name")
                    })?,
                    x => return Err(syn::Error::new(x.span(), "expecting attribute name")),
                };
                let attr_value = match right.as_ref() {
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(string),
                        ..
                    }) => string.clone(),
                    x => return Err(syn::Error::new(x.span(), "expecting string literal")),
                };
                (attr_name, Some(attr_value))
            }
            syn::Expr::Path(syn::ExprPath { path, .. }) => path
                .get_ident()
                .cloned()
                .map(|a| (a, None))
                .ok_or_else(|| syn::Error::new(path.span(), "expecting attribute name"))?,
            x => return Err(syn::Error::new(x.span(), "expecting identifier")),
        };
        attrs.push(AttrItem {
            key,
            value,
            span: arg_span,
        });
    }

    Ok(ExportInfo {
        item_span: list_span,
        items: attrs,
    })
}

pub fn outer_item_attributes<T: ExportedParams>(
    args: TokenStream,
    _attr_name: &str,
) -> syn::Result<T> {
    if args.is_empty() {
        return Ok(T::no_attrs());
    }

    let arg_list = syn::punctuated::Punctuated::parse_separated_nonempty.parse2(args)?;

    T::from_info(parse_punctuated_items(arg_list)?)
}

pub fn inner_item_attributes<T: ExportedParams>(
    attrs: &mut Vec<syn::Attribute>,
    attr_name: &str,
) -> syn::Result<T> {
    // Find the #[rhai_fn] attribute which will turn be read for function parameters.
    if let Some(index) = attrs
        .iter()
        .position(|a| a.path().get_ident().map_or(false, |i| *i == attr_name))
    {
        let rhai_fn_attr = attrs.remove(index);

        // Cannot have more than one #[rhai_fn]
        if let Some(duplicate) = attrs
            .iter()
            .find(|a| a.path().get_ident().map_or(false, |i| *i == attr_name))
        {
            return Err(syn::Error::new(
                duplicate.span(),
                format!("duplicated attribute '{attr_name}'"),
            ));
        }

        rhai_fn_attr.parse_args_with(T::parse_stream)
    } else {
        Ok(T::no_attrs())
    }
}

#[cfg(feature = "metadata")]
pub fn doc_attributes(attrs: &[syn::Attribute]) -> syn::Result<Vec<String>> {
    // Find the #[doc] attribute which will turn be read for function documentation.
    let mut comments = Vec::new();
    let mut buf = String::new();

    for attr in attrs {
        if attr.path().is_ident("doc") {
            attr.parse_nested_meta(|meta| {
                let value = meta.value()?;
                let s = value.parse::<syn::LitStr>()?;

                let mut line = s.value();

                if line.contains('\n') {
                    // Must be a block comment `/** ... */`
                    if !buf.is_empty() {
                        comments.push(buf.clone());
                        buf.clear();
                    }
                    line.insert_str(0, "/**");
                    line.push_str("*/");
                    comments.push(line);
                } else {
                    // Single line - assume it is `///`
                    if !buf.is_empty() {
                        buf.push('\n');
                    }
                    buf.push_str("///");
                    buf.push_str(&line);
                }

                Ok(())
            })?;
        }
    }

    if !buf.is_empty() {
        comments.push(buf);
    }

    Ok(comments)
}

pub fn collect_cfg_attr(attrs: &[syn::Attribute]) -> Vec<syn::Attribute> {
    attrs
        .iter()
        .filter(|&a| a.path().get_ident().map_or(false, |i| *i == "cfg"))
        .cloned()
        .collect()
}
