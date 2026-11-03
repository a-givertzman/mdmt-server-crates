use std::collections::HashMap;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{DeriveInput, Ident};
use crate::shared::{context_flags, field_value_type, named_fields};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let fields = named_fields(&input, "ContextAccess")?;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(&input.generics,
            "ContextAccess targets the fixed non-generic crate::domain::ContextTransaction; generic state structs are unsupported"));
    }
    let name = &input.ident;
    let mut property_arms = Vec::new();
    let mut generated = Vec::new();
    let mut impls = HashMap::<(&'static str, String), Ident>::new();
    for field in &fields.named {
        let flags = context_flags(field)?;
        let field_name = field.ident.as_ref().ok_or_else(||
            syn::Error::new_spanned(field, "ContextAccess requires named fields"))?;
        if flags.skip { continue; }
        let (value_type, optional) = field_value_type(&field.ty);
        let type_key = value_type.to_token_stream().to_string();
        let mut register = |trait_name: &'static str| -> syn::Result<()> {
            if let Some(previous) = impls.insert((trait_name, type_key.clone()), field_name.clone()) {
                return Err(syn::Error::new_spanned(field, format!(
                    "{trait_name} is ambiguous: fields `{previous}` and `{field_name}` address the same value type `{type_key}`")));
            }
            Ok(())
        };
        if flags.read {
            register("ContextRead")?;
            let body = if optional {
                quote! { ::core::clone::Clone::clone(&self.state.#field_name) }
            } else {
                quote! { ::core::option::Option::Some(::core::clone::Clone::clone(&self.state.#field_name)) }
            };
            generated.push(quote! {
                impl crate::domain::ContextRead<#value_type> for crate::domain::ContextTransaction {
                    fn read(&self) -> ::core::option::Option<#value_type> { #body }
                }
            });
        }
        // Preserve read as the existing shorthand for both read interfaces.
        if flags.read || flags.read_ref {
            register("ContextReadRef")?;
            let body = if optional {
                quote! { self.state.#field_name.as_ref() }
            } else {
                quote! { ::core::option::Option::Some(&self.state.#field_name) }
            };
            generated.push(quote! {
                impl crate::domain::ContextReadRef<#value_type> for crate::domain::ContextTransaction {
                    fn read_ref(&self) -> ::core::option::Option<&#value_type> { #body }
                }
            });
        }
        if flags.write {
            let owner = if optional {
                quote! { self.#field_name.as_ref().ok_or_else(||
                    error.err(::std::format!("Context {prefix} is not initialized")))? }
            } else {
                quote! { &self.#field_name }
            };
            property_arms.push(quote! {
                p if p == <#value_type as crate::domain::IecId>::iec_id() => {
                    let owner: &#value_type = #owner;
                    crate::domain::Properties::properties(owner)?.into_iter()
                        .find(|(key, _)| key == iec_id)
                        .map(|(_, value)| value)
                        .ok_or_else(|| error.err(::std::format!("Unknown property: {iec_id}")))
                }
            });
            register("ContextWrite")?;
            let value = if optional { quote! { ::core::option::Option::Some(value) } } else { quote! { value } };
            generated.push(quote! {
                impl crate::domain::ContextWrite<#value_type> for crate::domain::ContextTransaction {
                    fn write(mut self, value: #value_type) -> ::core::result::Result<Self, ::sal_core::error::Error> {
                        self.state.#field_name = #value;
                        ::core::result::Result::Ok(self)
                    }
                }
            });
        }
    }
    Ok(quote! {
        #(#generated)*
        impl #name {
            /// Domain JSON of one writable property, using ContextProperties.
            pub(crate) fn property_value(&self, iec_id: &str)
                -> ::core::result::Result<crate::kernel::JsonVal, ::sal_core::error::Error>
            {
                let error = ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "property_value");
                let (prefix, _) = iec_id.rsplit_once('.')
                    .ok_or_else(|| error.err(::std::format!("Invalid property key: {iec_id}")))?;
                match prefix {
                    #(#property_arms)*
                    _ => ::core::result::Result::Err(error.err(::std::format!("Unknown property: {iec_id}"))),
                }
            }
        }
    })
}
