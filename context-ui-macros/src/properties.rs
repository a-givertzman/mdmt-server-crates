use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::shared::{field_value_type, ident_name, named_fields, pascal_case, ui_flags};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let fields = named_fields(&input, "UiContextProperties")?;
    let mut read_arms = Vec::new();
    let mut write_arms = Vec::new();
    for field in &fields.named {
        if ui_flags(field)?.ui_skip {
            continue;
        }
        let Some(field_name) = field.ident.as_ref() else {
            return Err(syn::Error::new_spanned(
                field,
                "UiContextProperties requires named fields",
            ));
        };
        let (_, optional) = field_value_type(&field.ty);
        let pascal = pascal_case(field_name);
        let snake = ident_name(field_name);
        let pattern = if pascal == snake {
            quote!(#pascal)
        } else {
            quote!(#pascal | #snake)
        };
        if optional {
            read_arms.push(quote! {
                #pattern => match &self.#field_name {
                    ::core::option::Option::Some(value) => {
                        crate::domain::UiConvert::encode(conv, (iec_id, value))
                    }
                    ::core::option::Option::None => {
                        ::core::result::Result::Ok(crate::kernel::JsonVal::Null)
                    }
                },
            });
            write_arms.push(quote! {
                #pattern => match val {
                    crate::kernel::JsonVal::Null => {
                        self.#field_name = ::core::option::Option::None;
                        ::core::result::Result::Ok(())
                    }
                    _ => {
                        self.#field_name = ::core::option::Option::Some(
                            crate::domain::UiConvert::decode(conv, (iec_id, val))?
                        );
                        ::core::result::Result::Ok(())
                    }
                },
            });
        } else {
            read_arms.push(quote! {
                #pattern => crate::domain::UiConvert::encode(conv, (iec_id, &self.#field_name)),
            });
            write_arms.push(quote! {
                #pattern => {
                    self.#field_name = crate::domain::UiConvert::decode(conv, (iec_id, val))?;
                    ::core::result::Result::Ok(())
                }
            });
        }
    }
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics crate::domain::UiProperties for #name #type_generics #where_clause {
            fn ui_read(
                &self,
                iec_id: &str,
                field: &str,
                conv: &crate::domain::UiConverter,
            ) -> ::core::result::Result<crate::kernel::JsonVal, ::sal_core::error::Error> {
                match field {
                    #(#read_arms)*
                    _ => ::core::result::Result::Err(
                        ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "ui_read").err(
                            ::std::format!("Unknown field '{field}'")
                        )
                    ),
                }
            }
            fn ui_write(
                &mut self,
                iec_id: &str,
                field: &str,
                val: &crate::kernel::JsonVal,
                conv: &crate::domain::UiConverter,
            ) -> ::core::result::Result<(), ::sal_core::error::Error> {
                match field {
                    #(#write_arms)*
                    _ => ::core::result::Result::Err(
                        ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "ui_write").err(
                            ::std::format!("Unknown field '{field}'")
                        )
                    ),
                }
            }
        }
    })
}
