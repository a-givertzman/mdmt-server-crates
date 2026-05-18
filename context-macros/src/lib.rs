extern crate proc_macro;
use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, DeriveInput, PathArguments, GenericArgument};
use syn::{Data, Fields, Type};

///
/// ### Создает методы безопасного доступа к полям ContextTransacrion
/// 
/// * Добавьте `ContextAccess` в derive `RawContext`
/// * Добавьте атрибут к полю для которого нужен доступ:
///     - **`read`**  =>  impl `ContextRead<T> for ContextTransacrion`
///     - **`read_ref`**  =>  impl `ContextReadRef<T> for ContextTransacrion`
///     - **`write`** =>  impl  `ContextWrite<T> for ContextTransacrion`
///
/// **Пример:**
/// ```ignore
/// #[derive(ContextAccess)]
/// pub struct RawContext {
///     #[context(read, read_ref, write)]
///     pub(super) property: Option<ProperyCtx>,
/// }
/// ```
#[proc_macro_derive(ContextAccess, attributes(context))]
pub fn derive_context_access(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    let syn::Data::Struct(data_struct) = &input.data else {
        panic!("ContextAccess can only be derived for structs");
    };
    let syn::Fields::Named(fields) = &data_struct.fields else {
        panic!("ContextAccess requires named fields");
    };
    let mut generated_impls = Vec::new();
    for field in &fields.named {
        let field_name = field.ident.as_ref().unwrap();
        let field_type = &field.ty;
        let mut gen_read = false;
        let mut gen_write = false;
        for attr in &field.attrs {
            if attr.path().is_ident("context") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("read") {
                        gen_read = true;
                        Ok(())
                    } else if meta.path.is_ident("write") {
                        gen_write = true;
                        Ok(())
                    } else {
                        Err(meta.error(concat!("unsupported context attribute: ", stringify!(#field_name))))
                    }
                });
            }
        }
        let (inner_type, is_option) = extract_option_type(field_type);
        if gen_write {
            let write_impl = if is_option {
                quote! {
                    impl crate::domain::ContextWrite<#inner_type> for crate::domain::ContextTransaction {
                        fn write(mut self, value: #inner_type) -> Result<Self, sal_core::error::Error> {
                            self.state.#field_name = Some(value);
                            Result::Ok(self)
                        }
                    }
                }
            } else {
                quote! {
                    impl crate::domain::ContextWrite<#inner_type> for crate::domain::ContextTransaction {
                        fn write(mut self, value: #inner_type) -> Result<Self, sal_core::error::Error> {
                            self.state.#field_name = value;
                            Result::Ok(self)
                        }
                    }
                }
            };
            generated_impls.push(write_impl);
        }
        if gen_read {
            let read_ref_impl = if is_option {
                quote! {
                    impl crate::domain::ContextReadRef<#inner_type> for crate::domain::ContextTransaction {
                        fn read_ref(&self) -> &#inner_type {
                            self.state.#field_name.as_ref().expect(concat!("Value is None: ", stringify!(#field_name)))
                        }
                    }
                }
            } else {
                quote! {
                    impl crate::domain::ContextReadRef<#inner_type> for crate::domain::ContextTransaction {
                        fn read_ref(&self) -> &#inner_type {
                            &self.state.#field_name
                        }
                    }
                }
            };
            let read_impl = if is_option {
                quote! {
                    impl crate::domain::ContextRead<#inner_type> for crate::domain::ContextTransaction {
                        fn read(&self) -> #inner_type {
                            self.state.#field_name.clone().expect(concat!("Value is None: ", stringify!(#field_name)))
                        }
                    }
                }
            } else {
                quote! {
                    impl crate::domain::ContextRead<#inner_type> for crate::domain::ContextTransaction {
                        fn read(&self) -> #inner_type {
                            self.state.#field_name.clone()
                        }
                    }
                }
            };
            generated_impls.push(read_ref_impl);
            generated_impls.push(read_impl);
        }
    }
    let expanded = quote! {
        #(#generated_impls)*
    };
    TokenStream::from(expanded)
}
//
fn extract_option_type(ty: &syn::Type) -> (&syn::Type, bool) {
    if let syn::Type::Path(type_path) = ty {
        if type_path.path.segments.len() == 1 && type_path.path.segments[0].ident == "Option" {
            if let syn::PathArguments::AngleBracketed(args) = &type_path.path.segments[0].arguments {
                if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                    return (inner_ty, true);
                }
            }
        }
    }
    (ty, false)
}
///
/// ### Макрос создает `impl crate::domain::Properties for Type`
/// 
/// `crate::domain::Properties` сериализует поля данной структуры
/// - Возвращает в виде вектора пар (IEC key, JSON value).
/// - Предназначено для формирования консистентного снимка (Snapshot) и отправки на UI/DB.
#[proc_macro_derive(ContextProperties, attributes(iec_id))]
pub fn derive_context_properties(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let mut iec_id = None;
    for attr in &input.attrs {
        if attr.path().is_ident("iec_id") {
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &nv.value {
                    iec_id = Some(s.value());
                }
            }
        }
    }
    let Some(key) = iec_id else {
        let err = syn::Error::new_spanned(name, "ContextProperties requires #[iec_id = \"...\"] attribute").to_compile_error();
        return TokenStream::from(err);
    };
    let expanded = quote! {
        impl crate::domain::IecId for #name {
            fn iec_id() -> &'static str {
                #key
            }
        }
        impl crate::domain::Properties for #name {
            ///
            /// ### Сериализует поля данной структуры
            /// - Возвращает в виде вектора пар (IEC key, JSON value).
            /// - Предназначено для формирования консистентного снимка (Snapshot) и отправки на UI/DB.
            fn properties(&self) -> std::vec::Vec<(&'static str, std::string::String)> {
                let json_string = serde_json::to_string(self)
                    .expect(concat!("Failed to serialize property for type ", stringify!(#name)));
                std::vec![(#key, json_string)]
            }
        }
        impl crate::domain::Properties for &#name {
            ///
            /// ### Сериализует поля данной структуры
            /// - Возвращает в виде вектора пар (IEC key, JSON value).
            /// - Предназначено для формирования консистентного снимка (Snapshot) и отправки на UI/DB.
            fn properties(&self) -> std::vec::Vec<(&'static str, std::string::String)> {
                let json_string = serde_json::to_string(self)
                    .expect(concat!("Failed to serialize property for type ", stringify!(#name)));
                std::vec![(#key, json_string)]
            }
        }
    };
    TokenStream::from(expanded)
}

///
/// ### Макрос для инициализации  `RawContext` из коллекции пар `Key-Value`
/// 
/// * Добавьте `ContextLoad` в derive `RawContext`
/// * Инициализируйте `RawContext` из пар `Key-Value` загруженных `Snapshot`
/// ```ignore
/// RawContext.from_snapshot(
///     Snapshot.fetch(...)
/// )
/// ```
#[proc_macro_derive(ContextLoad, attributes(context))]
pub fn derive_context_load(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let report_name = format_ident!("{}LoadReport", struct_name);
    let Data::Struct(data_struct) = &input.data else {
        panic!("ContextLoad can only be derived for structs");
    };
    let Fields::Named(fields) = &data_struct.fields else {
        panic!("ContextLoad requires named fields");
    };
    let mut field_inits = Vec::new();
    for field in &fields.named {
        let field_name = &field.ident;
        let ty = &field.ty;
        // Проверяем, есть ли атрибут #[context(skip)]
        let skip = field.attrs.iter().any(|a| a.path().is_ident("context") && a.parse_args::<syn::Ident>().map_or(false, |i| i == "skip"));
        if skip {
            field_inits.push(quote! {
                #field_name: Default::default()
            });
            continue;
        }
        let (is_option, target_ty) = extract_option_inner(ty).map_or((false, ty), |inner| (true, inner));
        let success_val = if is_option { quote!(Some(parsed)) } else { quote!(parsed) };
        let fallback_val = if is_option { quote!(None) } else { quote!(Default::default()) };
        field_inits.push(quote! {
            #field_name: {
                let key = <#target_ty as IecKey>::iec_id();
                match props_map.remove(key) {
                    Some(val) => {
                        match serde_json::from_value(val) {
                            Ok(parsed) => {
                                report.loaded.push(key.to_string());
                                #success_val
                            },
                            Err(e) => {
                                report.errors.push((key.to_string(), e.to_string()));
                                #fallback_val
                            }
                        }
                    },
                    None => {
                        report.missing_in_db.push(key.to_string());
                        #fallback_val
                    }
                }
            }
        });
    }
    let expanded = quote! {
        /// Отчет о результатах загрузки состояния из БД.
        pub struct #report_name {
            pub loaded: Vec<String>,
            pub missing_in_db: Vec<String>,
            pub unused_in_db: Vec<String>,
            pub errors: Vec<(String, String)>,
        }
        impl #struct_name {
            /// Инициализирует структуру на основе сырых данных.
            pub fn from_snapshot(
                properties: impl std::iter::IntoIterator<Item = (String, serde_json::Value)>
            ) -> (Self, #report_name) {
                let mut props_map: std::collections::HashMap<String, serde_json::Value> = properties.into_iter().collect();
                let mut report = #report_name {
                    loaded: Vec::new(),
                    missing_in_db: Vec::new(),
                    unused_in_db: Vec::new(),
                    errors: Vec::new(),
                };
                let instance = Self {
                    #(#field_inits),*
                };
                report.unused_in_db = props_map.into_keys().collect();
                (instance, report)
            }
        }
    };
    TokenStream::from(expanded)
}
/// Вспомогательная функция для извлечения T из Option<T>
fn extract_option_inner(ty: &Type) -> Option<&Type> {
    if let Type::Path(ty_path) = ty {
        if let Some(segment) = ty_path.path.segments.last() {
            if segment.ident == "Option" || segment.ident == "std::option::Option" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}
