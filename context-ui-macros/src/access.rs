use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::shared::{field_value_type, named_fields, ui_flags};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let fields = named_fields(&input, "UiContextAccess")?;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "UiContextAccess targets the fixed non-generic raw context; generic state structs are unsupported",
        ));
    }
    let mut read_arms = Vec::new();
    let mut write_arms = Vec::new();
    for field in &fields.named {
        if ui_flags(field)?.ui_skip {
            continue;
        }
        let Some(field_name) = field.ident.as_ref() else {
            return Err(syn::Error::new_spanned(
                field,
                "UiContextAccess requires named fields",
            ));
        };
        let (value_type, optional) = field_value_type(&field.ty);
        if optional {
            read_arms.push(quote! {
                p if p == <#value_type as crate::domain::IecId>::iec_id() => match &self.#field_name {
                    ::core::option::Option::Some(ctx) => {
                        crate::domain::UiProperties::ui_read(ctx, iec_id, field, conv)
                    }
                    ::core::option::Option::None => {
                        ::core::result::Result::Ok(crate::kernel::JsonVal::Null)
                    }
                },
            });
            write_arms.push(quote! {
                p if p == <#value_type as crate::domain::IecId>::iec_id() => match self.#field_name.as_mut() {
                    ::core::option::Option::Some(ctx) => {
                        crate::domain::UiProperties::ui_write(ctx, iec_id, field, val, conv)
                    }
                    ::core::option::Option::None => ::core::result::Result::Err(
                        ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "ui_write").err(
                            ::std::format!(
                                "Context iec_id {iec_id}, field {} is not initialized.",
                                <#value_type as crate::domain::IecId>::iec_id()
                            )
                        )
                    ),
                },
            });
        } else {
            read_arms.push(quote! {
                p if p == <#value_type as crate::domain::IecId>::iec_id() => {
                    crate::domain::UiProperties::ui_read(&self.#field_name, iec_id, field, conv)
                }
            });
            write_arms.push(quote! {
                p if p == <#value_type as crate::domain::IecId>::iec_id() => {
                    crate::domain::UiProperties::ui_write(&mut self.#field_name, iec_id, field, val, conv)
                }
            });
        }
    }
    Ok(quote! {
        impl #name {
            /// ### Возвращает свойство из контекста в Ui-формате.
            /// - `iec_id` - Указывает на поле контекста запрошенного свойства.
            /// - `conv` - Конвертор доменного типа в Ui-json и обратно.
            pub fn ui_read(
                &self,
                iec_id: impl ::core::convert::AsRef<str>,
                conv: &crate::domain::UiConverter,
            ) -> ::core::result::Result<crate::kernel::JsonVal, ::sal_core::error::Error> {
                let iec_id = iec_id.as_ref();
                let (prefix, field) = match iec_id.rsplit_once('.') {
                    ::core::option::Option::Some(split) => split,
                    ::core::option::Option::None => {
                        return ::core::result::Result::Err(
                            ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "ui_read").err(
                                ::std::format!("Wrong iec_id format '{iec_id}'. expected '<ctx_prefix>.<field>'")
                            )
                        )
                    }
                };
                match prefix {
                    #(#read_arms)*
                    _ => ::core::result::Result::Err(
                        ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "ui_read").err(
                            ::std::format!("Unknown iec_id: '{iec_id}'")
                        )
                    ),
                }
            }
            /// ### Пишет свойство в Ui-формате в контекст.
            /// - `iec_id` - Указывает на поле контекста, куда будет записано значение.
            /// - `val` - Значение в Ui-формате.
            /// - `conv` - Конвертор Ui-json в доменный тип и обратно.
            pub fn ui_write(
                &mut self,
                iec_id: impl ::core::convert::AsRef<str>,
                val: &crate::kernel::JsonVal,
                conv: &crate::domain::UiConverter,
            ) -> ::core::result::Result<(), ::sal_core::error::Error> {
                let iec_id = iec_id.as_ref();
                let (prefix, field) = match iec_id.rsplit_once('.') {
                    ::core::option::Option::Some(split) => split,
                    ::core::option::Option::None => {
                        return ::core::result::Result::Err(
                            ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "ui_write").err(
                                ::std::format!("Wrong iec_id format '{iec_id}'. expected '<ctx_prefix>.<field>'")
                            )
                        )
                    }
                };
                match prefix {
                    #(#write_arms)*
                    _ => ::core::result::Result::Err(
                        ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "ui_write").err(
                            ::std::format!("Unknown iec_id: '{iec_id}'")
                        )
                    ),
                }
            }
        }
    })
}
