use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::shared::{db_flags, field_value_type, named_fields};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let fields = named_fields(&input, "DbContextAccess")?;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "DbContextAccess targets the fixed non-generic raw context; generic state structs are unsupported",
        ));
    }
    let name = &input.ident;
    let mut read_arms = Vec::new();
    let mut write_arms = Vec::new();
    let mut type_checks = TokenStream::new();
    for field in &fields.named {
        if db_flags(field)?.skip_load {
            continue;
        }
        let Some(field_name) = field.ident.as_ref() else {
            return Err(syn::Error::new_spanned(
                field,
                "DbContextAccess requires named fields",
            ));
        };
        let (value_type, optional) = field_value_type(&field.ty);
        type_checks.extend(quote! {
            if prefix == <#value_type as crate::domain::IecId>::iec_id() {
                __matches += 1;
            }
        });
        let read_body = if optional {
            quote! {
                match self.#field_name.as_ref() {
                    ::core::option::Option::Some(ctx) => {
                        crate::domain::DbProperties::read_json(ctx, iec_id, field)
                    }
                    ::core::option::Option::None => {
                        if !crate::domain::DbProperties::has_field(
                            &<#value_type as ::core::default::Default>::default(),
                            field,
                        ) {
                            return ::core::result::Result::Err(
                                crate::domain::DbContextAccessError::Unknown(
                                    ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "db_read").err(
                                        ::std::format!("Unknown field '{field}' for iec_id '{iec_id}'")
                                    )
                                )
                            );
                        }
                        ::core::result::Result::Ok(crate::kernel::JsonVal::Null)
                    }
                }
            }
        } else {
            quote! {
                crate::domain::DbProperties::read_json(&self.#field_name, iec_id, field)
            }
        };
        read_arms.push(quote! {
            p if p == <#value_type as crate::domain::IecId>::iec_id() => { #read_body }
        });
        let write_body = if optional {
            quote! {
                match self.#field_name.as_mut() {
                    ::core::option::Option::Some(ctx) => {
                        crate::domain::DbProperties::write_json(ctx, iec_id, field, val)
                    }
                    ::core::option::Option::None => {
                        let mut candidate = <#value_type as ::core::default::Default>::default();
                        if !crate::domain::DbProperties::has_field(&candidate, field) {
                            ::core::result::Result::Err(
                                crate::domain::DbContextAccessError::Unknown(
                                    ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "db_write").err(
                                        ::std::format!("Unknown field '{field}' for iec_id '{iec_id}'")
                                    )
                                )
                            )
                        } else {
                            match crate::domain::DbProperties::write_json(&mut candidate, iec_id, field, val) {
                                ::core::result::Result::Ok(()) => {
                                    self.#field_name = ::core::option::Option::Some(candidate);
                                    ::core::result::Result::Ok(())
                                }
                                ::core::result::Result::Err(err) => ::core::result::Result::Err(err),
                            }
                        }
                    }
                }
            }
        } else {
            quote! {
                crate::domain::DbProperties::write_json(&mut self.#field_name, iec_id, field, val)
            }
        };
        write_arms.push(quote! {
            p if p == <#value_type as crate::domain::IecId>::iec_id() => { #write_body }
        });
    }
    let write_val_guard = if write_arms.is_empty() {
        quote! { let _ = val; }
    } else {
        quote! {}
    };
    Ok(quote! {
        impl #name {
            /// ### Читает из контекста одно поле элемента по `iec_id` в формате domain-JSON.
            /// - `iec_id` - полный IEC-идентификатор значения в формате `<ctx_prefix>.<field>`
            pub fn db_read(
                &self,
                iec_id: impl ::core::convert::AsRef<str>,
            ) -> ::core::result::Result<crate::kernel::JsonVal, crate::domain::DbContextAccessError> {
                let iec_id = iec_id.as_ref();
                let (prefix, field) = match iec_id.rsplit_once('.') {
                    ::core::option::Option::Some(split) => split,
                    ::core::option::Option::None => {
                        return ::core::result::Result::Err(
                            crate::domain::DbContextAccessError::Unknown(
                                ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "db_read").err(
                                    ::std::format!("Wrong iec_id format '{iec_id}'. expected '<ctx_prefix>.<field>'")
                                )
                            )
                        )
                    }
                };
                let mut __matches = 0usize;
                #type_checks
                if __matches > 1 {
                    return ::core::result::Result::Err(
                        crate::domain::DbContextAccessError::Duplicated(
                            ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "db_read").err(
                                ::std::format!("duplicated iec_id '{iec_id}'")
                            )
                        )
                    );
                }
                match prefix {
                    #(#read_arms)*
                    _ => ::core::result::Result::Err(
                        crate::domain::DbContextAccessError::Unknown(
                            ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "db_read").err(
                                ::std::format!("iec_id '{iec_id}' not found")
                            )
                        )
                    ),
                }
            }
            /// ### Записывает в контекст одно поле элемента по `iec_id` в формате domain-JSON.
            /// - `iec_id` - полный IEC-идентификатор значения в формате `<ctx_prefix>.<field>`
            /// - `val` - значение в формате domain-JSON
            pub fn db_write(
                &mut self,
                iec_id: impl ::core::convert::AsRef<str>,
                val: &crate::kernel::JsonVal,
            ) -> ::core::result::Result<(), crate::domain::DbContextAccessError> {
                #write_val_guard
                let iec_id = iec_id.as_ref();
                let (prefix, field) = match iec_id.rsplit_once('.') {
                    ::core::option::Option::Some(split) => split,
                    ::core::option::Option::None => {
                        return ::core::result::Result::Err(
                            crate::domain::DbContextAccessError::Unknown(
                                ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "db_write").err(
                                    ::std::format!("Wrong iec_id format '{iec_id}'. expected '<ctx_prefix>.<field>'")
                                )
                            )
                        )
                    }
                };
                let mut __matches = 0usize;
                #type_checks
                let __duplicated = __matches > 1;
                let __result = match prefix {
                    #(#write_arms)*
                    _ => ::core::result::Result::Err(
                        crate::domain::DbContextAccessError::Unknown(
                            ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "db_write").err(
                                ::std::format!("iec_id '{iec_id}' not found")
                            )
                        )
                    ),
                };
                if __duplicated && __result.is_ok() {
                    return ::core::result::Result::Err(
                        crate::domain::DbContextAccessError::Duplicated(
                            ::sal_core::error::Error::new(::core::any::type_name::<Self>(), "db_write").err(
                                ::std::format!("duplicated iec_id '{iec_id}'")
                            )
                        )
                    );
                }
                __result
            }
        }
    })
}
