use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::DeriveInput;
use crate::shared::{context_flags, field_value_type, named_fields};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let fields = named_fields(&input, "ContextLoad")?;
    let name = &input.ident;
    let visibility = &input.vis;
    let raw_name = name.to_string();
    let report_name = format_ident!("{}LoadReport", raw_name.trim_start_matches("r#"));
    let mut key_checks = Vec::new();
    let mut mutations = Vec::new();
    let mut bounds = Vec::new();
    for field in &fields.named {
        let flags = context_flags(field)?;
        if flags.skip_load { continue; }
        let field_name = field.ident.as_ref().ok_or_else(||
            syn::Error::new_spanned(field, "ContextLoad requires named fields"))?;
        let (value_type, optional) = field_value_type(&field.ty);
        bounds.push(quote! { #value_type: crate::domain::IecId + ::serde::de::DeserializeOwned });
        key_checks.push(quote! {
            {
                let key = <#value_type as crate::domain::IecId>::iec_id();
                if !field_keys.insert(key) { duplicate_field_keys.insert(key); }
            }
        });
        let assignment = if optional { quote! { ::core::option::Option::Some(parsed) } } else { quote! { parsed } };
        mutations.push(quote! {
            {
                let key = <#value_type as crate::domain::IecId>::iec_id();
                if !invalid_keys.contains(key) {
                    match props_map.remove(key) {
                        ::core::option::Option::Some(value) => {
                            match ::serde_json::from_value::<#value_type>(value) {
                                ::core::result::Result::Ok(parsed) => {
                                    self.#field_name = #assignment;
                                    report.loaded.push(::std::string::String::from(key));
                                }
                                ::core::result::Result::Err(error) => {
                                    report.errors.push((::std::string::String::from(key), error.to_string()));
                                }
                            }
                        }
                        ::core::option::Option::None => {
                            report.missing_in_db.push(::std::string::String::from(key));
                        }
                    }
                }
            }
        });
    }
    let method_where = if bounds.is_empty() { quote! {} } else { quote! { where #(#bounds,)* } };
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        /// Applied values, missing/unused keys, and recoverable load errors.
        #[derive(Debug, Default)]
        #visibility struct #report_name {
            pub loaded: ::std::vec::Vec<::std::string::String>,
            pub missing_in_db: ::std::vec::Vec<::std::string::String>,
            pub unused_in_db: ::std::vec::Vec<::std::string::String>,
            /// Deserialization errors and duplicate-key errors (key, message).
            pub errors: ::std::vec::Vec<(::std::string::String, ::std::string::String)>,
        }
        impl #report_name {
            /// Missing and unused keys are not errors.
            pub fn is_clean(&self) -> bool { self.errors.is_empty() }
        }
        impl #impl_generics #name #type_generics #where_clause {
            pub fn with_snapshot(
                mut self,
                properties: impl ::std::iter::IntoIterator<Item = (::std::string::String, ::serde_json::Value)>
            ) -> (Self, #report_name)
            #method_where
            {
                let mut report = #report_name::default();
                let mut props_map = ::std::collections::HashMap::<::std::string::String, ::serde_json::Value>::new();
                let mut duplicate_input_keys = ::std::collections::BTreeSet::<::std::string::String>::new();
                for (key, value) in properties {
                    match props_map.entry(key) {
                        ::std::collections::hash_map::Entry::Vacant(entry) => { entry.insert(value); }
                        ::std::collections::hash_map::Entry::Occupied(entry) => {
                            duplicate_input_keys.insert(entry.key().clone());
                        }
                    }
                }
                let mut field_keys = ::std::collections::BTreeSet::<&'static str>::new();
                let mut duplicate_field_keys = ::std::collections::BTreeSet::<&'static str>::new();
                #(#key_checks)*
                let mut invalid_keys = ::std::collections::BTreeSet::<::std::string::String>::new();
                for key in duplicate_input_keys {
                    props_map.remove(key.as_str());
                    report.errors.push((key.clone(), ::std::string::String::from("duplicate input IEC key")));
                    invalid_keys.insert(key);
                }
                for key in duplicate_field_keys {
                    props_map.remove(key);
                    report.errors.push((::std::string::String::from(key), ::std::string::String::from("duplicate context field IEC key")));
                    invalid_keys.insert(::std::string::String::from(key));
                }
                #(#mutations)*
                report.unused_in_db = props_map.into_keys().collect();
                report.unused_in_db.sort_unstable();
                (self, report)
            }
        }
    })
}
