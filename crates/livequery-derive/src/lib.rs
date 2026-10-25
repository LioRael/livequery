//! Derive explicit, field-filtered runtime inspection for named Rust structs.
use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

/// Generates inspection for named structs; `#[inspect(skip)]` never serializes a field.
#[proc_macro_derive(Inspect, attributes(inspect))]
pub fn derive_inspect(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input,
            "Inspect requires a named struct; enum fields may implement Serialize",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input,
            "Inspect requires named fields",
        ));
    };
    let crate_path = match proc_macro_crate::crate_name("livequery")
        .map_err(|error| syn::Error::new_spanned(&input.ident, error.to_string()))?
    {
        proc_macro_crate::FoundCrate::Itself => quote!(::livequery),
        proc_macro_crate::FoundCrate::Name(name) => {
            let ident = syn::Ident::new(&name, input.ident.span());
            quote!(::#ident)
        }
    };
    let mut generics = input.generics.clone();
    generics
        .make_where_clause()
        .predicates
        .push(syn::parse_quote!(Self: Send + Sync));
    let mut columns = Vec::new();
    let mut values = Vec::new();
    for field in &fields.named {
        let mut skip = false;
        for attr in &field.attrs {
            if attr.path().is_ident("inspect") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("skip") {
                        skip = true;
                        Ok(())
                    } else {
                        Err(meta.error("expected skip"))
                    }
                })?;
            }
        }
        if skip {
            continue;
        }
        let ident = field.ident.as_ref().expect("named field has an identifier");
        let name = ident.to_string().trim_start_matches("r#").to_owned();
        let ty = &field.ty;
        generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#ty: #crate_path::serde::Serialize));
        columns.push(quote! { #crate_path::Column { name: #name.to_owned(), rust_type: ::std::any::type_name::<#ty>().to_owned() } });
        values.push(quote! { row.insert(#name.to_owned(), #crate_path::value_with_budget(&self.#ident, &mut remaining)?); });
    }
    let name = &input.ident;
    let (imp, ty, clause) = generics.split_for_impl();
    Ok(quote! {
        impl #imp #crate_path::Inspect for #name #ty #clause {
            fn schema() -> Vec<#crate_path::Column> { vec![#(#columns),*] }
            fn inspect(&self) -> Result<#crate_path::Row, #crate_path::Error> {
                self.inspect_bounded(#crate_path::Limits::default().bytes)
            }
            fn inspect_bounded(&self, max_bytes: usize) -> Result<#crate_path::Row, #crate_path::Error> {
                let mut remaining = max_bytes;
                let mut row = #crate_path::Row::new();
                #(#values)*
                Ok(row)
            }
        }
    })
}
