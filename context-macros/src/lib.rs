extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};
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
    let ast = parse_macro_input!(input as DeriveInput);
    let struct_name = &ast.ident;
    let Data::Struct(data_struct) = &ast.data else {
        panic!("ContextLoad можно применять только к структурам");
    };
    let Fields::Named(fields) = &data_struct.fields else {
        panic!("ContextLoad требует именованных полей");
    };
    let mut field_initializers = Vec::new();
    for field in &fields.named {
        let field_name = field.ident.as_ref().unwrap();
        let field_type = &field.ty;
        // Здесь мы должны вытащить значение iec_id из атрибута #[context(iec_id = "...")]
        // Для примера предполагаем, что мы его распарсили в строковую переменную iec_key_str
        let iec_key_str = extract_iec_id(&field.attrs).expect("Отсутствует атрибут iec_id");
        let is_option = is_option_type(field_type);
        let init_block = if is_option {
            quote! {
                match properties_map.remove(#iec_key_str) {
                    Some(json_val) => match serde_json::from_value(json_val) {
                        Ok(val) => {
                            report.loaded.push(#iec_key_str.to_string());
                            Some(val)
                        },
                        Err(e) => {
                            report.errors.push((#iec_key_str.to_string(), e.to_string()));
                            None
                        }
                    },
                    None => {
                        report.missing_in_db.push(#iec_key_str.to_string());
                        None
                    }
                }
            }
        } else {
            quote! {
                match properties_map.remove(#iec_key_str) {
                    Some(json_val) => match serde_json::from_value(json_val) {
                        Ok(val) => {
                            report.loaded.push(#iec_key_str.to_string());
                            val
                        },
                        Err(e) => {
                            report.errors.push((#iec_key_str.to_string(), e.to_string()));
                            <#field_type>::default()
                        }
                    },
                    None => {
                        report.missing_in_db.push(#iec_key_str.to_string());
                        <#field_type>::default()
                    }
                }
            }
        };
        field_initializers.push(quote! {
            #field_name: #init_block
        });
    }
    let expanded = quote! {
        impl #struct_name {
            /// Инициализирует структуру из коллекции `(Kye, JSON)`.
            /// 
            /// Аргументы:
            /// * `properties` - Любой итератор, отдающий пары IecKey и JSON.
            pub fn from_snapshot(
                properties: impl std::iter::IntoIterator<Item = (String, serde_json::Value)>
            ) -> (Self, LoadReport) {
                let mut properties_map: std::collections::HashMap<_, _> = properties.into_iter().collect();
                let mut report = LoadReport {
                    loaded: Vec::new(),
                    missing_in_db: Vec::new(),
                    unused_in_db: Vec::new(),
                    errors: Vec::new(),
                };
                let instance = Self {
                    #(#field_initializers),*
                };
                report.unused_in_db = properties_map.into_keys().collect();
                (instance, report)
            }
        }
    };
    TokenStream::from(expanded)
}
/// Проверяет, является ли тип `Option<T>`
fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option" || segment.ident == "std::option::Option";
        }
    }
    false
}
/// Извлекает iec_id из атрибутов (заглушка для наглядности)
fn extract_iec_id(attrs: &[syn::Attribute]) -> Option<String> {
    // Здесь должна быть логика поиска #[context(iec_id = "Ship.General...")]
    // ...
    Some("dummy_key".to_string()) 
}
