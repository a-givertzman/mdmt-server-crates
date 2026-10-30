use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_quote, DeriveInput, Expr, Lit, Meta};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let mut iec_id = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("iec_id") { continue; }
        if iec_id.is_some() {
            return Err(syn::Error::new_spanned(attr, "duplicate iec_id attribute"));
        }
        let Meta::NameValue(value) = &attr.meta else {
            return Err(syn::Error::new_spanned(attr, "expected #[iec_id = \"...\"]"));
        };
        let Expr::Lit(literal) = &value.value else {
            return Err(syn::Error::new_spanned(&value.value, "iec_id must be a string literal"));
        };
        let Lit::Str(key) = &literal.lit else {
            return Err(syn::Error::new_spanned(&literal.lit, "iec_id must be a string literal"));
        };
        if key.value().trim().is_empty() {
            return Err(syn::Error::new_spanned(key, "iec_id must not be empty"));
        }
        iec_id = Some(key);
    }
    let key = iec_id.ok_or_else(||
        syn::Error::new_spanned(name, "ContextProperties requires #[iec_id = \"...\"]"))?;
    let (id_impl_generics, type_generics, id_where_clause) = input.generics.split_for_impl();
    let mut serializable = input.generics.clone();
    serializable.make_where_clause().predicates.push(parse_quote! {
        #name #type_generics: ::serde::Serialize
    });
    let (impl_generics, _, where_clause) = serializable.split_for_impl();
    Ok(quote! {
        impl #id_impl_generics crate::domain::IecId for #name #type_generics #id_where_clause {
            fn iec_id() -> &'static str { #key }
        }
        impl #impl_generics crate::domain::Properties for #name #type_generics #where_clause {
            fn properties(&self) -> ::core::result::Result<
                ::std::vec::Vec<(::std::string::String, crate::kernel::JsonVal)>, ::sal_core::error::Error
            > {
                let value = ::serde_json::to_value(self).map_err(|err| {
                    ::sal_core::error::Error::new("ContextProperties", ::core::stringify!(#name))
                        .pass_with("serialize", err.to_string())
                })?;
+                match value {
+                    ::serde_json::Value::Object(fields) => {
+                        let mut items = ::std::vec::Vec::with_capacity(fields.len());
+                        for (field, value) in fields {
+                            items.push((
+                                ::std::format!("{}.{}", #key, field),
+                                value,
+                            ));
+                        }
+                        ::core::result::Result::Ok(items)
+                    }
+                    // Newtype/tuple structs (например, ProjectId) не дают JSON-объекта.
+                    // Для них сохраняем прежний контракт: один ключ iec_id.
+                    value => ::core::result::Result::Ok(::std::vec![(
+                        ::std::string::String::from(#key),
+                        value,
+                    )]),
+                }
            }
        }
        impl #impl_generics crate::domain::Properties for &#name #type_generics #where_clause {
            fn properties(&self) -> ::core::result::Result<
                ::std::vec::Vec<(::std::string::String, crate::kernel::JsonVal)>, ::sal_core::error::Error
            > {
                <#name #type_generics as crate::domain::Properties>::properties(*self)
            }
        }
    })
}
