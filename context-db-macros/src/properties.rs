use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::shared::{db_flags, ident_name, named_fields, pascal_case};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let fields = named_fields(&input, "DbContextProperties")?;
    let mut patterns = Vec::new();
    let mut read_arms = Vec::new();
    let mut write_arms = Vec::new();
    for field in &fields.named {
        if db_flags(field)?.skip_load {
            continue;
        }
        let Some(field_name) = field.ident.as_ref() else {
            return Err(syn::Error::new_spanned(
                field,
                "DbContextProperties requires named fields",
            ));
        };
        let field_ty = &field.ty;
        let pascal = pascal_case(field_name);
        let snake = ident_name(field_name);
        let pattern = if pascal == snake {
            quote!(#pascal)
        } else {
            quote!(#pascal | #snake)
        };
        patterns.push(pattern.clone());
        read_arms.push(quote! {
            #pattern => ::serde_json::to_value(&self.#field_name).map_err(|err| {
                crate::domain::DbContextAccessError::Err(
                    ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "read_json")
                        .pass_with(iec_id, err.to_string())
                )
            }),
        });
        write_arms.push(quote! {
            #pattern => {
                let parsed = <#field_ty as ::serde::Deserialize>::deserialize(val).map_err(|err| {
                    crate::domain::DbContextAccessError::Err(
                        ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "write_json")
                            .pass_with(iec_id, err.to_string())
                    )
                })?;
                self.#field_name = parsed;
                ::core::result::Result::Ok(())
            }
        });
    }
    let has_field_body = if patterns.is_empty() {
        quote! { false }
    } else {
        quote! {
            match field {
                #(#patterns)|* => true,
                _ => false,
            }
        }
    };
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics crate::domain::DbProperties for #name #type_generics #where_clause {
            fn has_field(&self, field: &str) -> bool {
                #has_field_body
            }
            fn read_json(
                &self,
                iec_id: &str,
                field: &str,
            ) -> ::core::result::Result<crate::kernel::JsonVal, crate::domain::DbContextAccessError> {
                match field {
                    #(#read_arms)*
                    _ => ::core::result::Result::Err(
                        crate::domain::DbContextAccessError::Unknown(
                            ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "read_json").err(
                                ::std::format!("Unknown field '{field}' for iec_id '{iec_id}'")
                            )
                        )
                    ),
                }
            }
            fn write_json(
                &mut self,
                iec_id: &str,
                field: &str,
                val: &crate::kernel::JsonVal,
            ) -> ::core::result::Result<(), crate::domain::DbContextAccessError> {
                match field {
                    #(#write_arms)*
                    _ => ::core::result::Result::Err(
                        crate::domain::DbContextAccessError::Unknown(
                            ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "write_json").err(
                                ::std::format!("Unknown field '{field}' for iec_id '{iec_id}'")
                            )
                        )
                    ),
                }
            }
        }
    })
}
