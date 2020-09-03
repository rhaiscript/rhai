use quote::{quote, quote_spanned};
use syn::{parse::Parser, spanned::Spanned};

pub(crate) fn generated_module_path(
    fn_path: &syn::Path,
) -> syn::punctuated::Punctuated<syn::PathSegment, syn::Token![::]> {
    let mut g = fn_path.clone().segments;
    g.pop();
    let ident = syn::Ident::new(
        &format!("rhai_fn_{}", fn_path.segments.last().unwrap().ident),
        fn_path.span(),
    );
    g.push_value(syn::PathSegment {
        ident,
        arguments: syn::PathArguments::None,
    });
    g
}

type RegisterMacroInput = (syn::Expr, proc_macro2::TokenStream, syn::Path);
pub fn parse_register_macro(
    args: proc_macro::TokenStream,
) -> Result<RegisterMacroInput, syn::Error> {
    let parser = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_separated_nonempty;
    let args = parser.parse(args).unwrap();
    let arg_span = args.span();
    let mut items: Vec<syn::Expr> = args.into_iter().collect();
    if items.len() != 3 {
        return Err(syn::Error::new(
            arg_span,
            "this macro requires three arguments",
        ));
    }
    let export_name = match &items[1] {
        syn::Expr::Lit(litstr) => quote_spanned!(items[1].span()=>
                                                 #litstr.to_string()),
        expr => quote! { #expr },
    };
    let rust_modpath = if let syn::Expr::Path(ref path) = &items[2] {
        path.path.clone()
    } else {
        return Err(syn::Error::new(
            items[2].span(),
            "third argument must be a function name",
        ));
    };
    let module = items.remove(0);
    Ok((module, export_name, rust_modpath))
}
