//! Derive macros for the UI access layer of the MDMT context.
//!
//! Контракт потребителя описан в `ui-context-access-spec.md`.
use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

mod access;
mod properties;
mod shared;

/// ### Генерирует `ui_read` / `ui_write` как inherent-методы сырого контекста.
///
/// Структура обязана иметь именованные поля и не иметь generic-параметров.
/// Каждое поле без `#[context(ui_skip)]` даёт ветку `match` по
/// `<ТипПоля as IecId>::iec_id()`.
#[proc_macro_derive(UiContextAccess, attributes(context))]
pub fn derive_ui_context_access(input: TokenStream) -> TokenStream {
    finish(access::expand(parse_macro_input!(input as DeriveInput)))
}

/// ### Генерирует `impl UiProperties` для элемента контекста.
///
/// Структура обязана иметь именованные поля.
/// Каждое поле без `#[context(ui_skip)]` даёт ветку `match` по PascalCase
/// и snake_case написаниям имени поля.
#[proc_macro_derive(UiContextProperties, attributes(context))]
pub fn derive_ui_context_properties(input: TokenStream) -> TokenStream {
    finish(properties::expand(parse_macro_input!(input as DeriveInput)))
}

fn finish(result: syn::Result<proc_macro2::TokenStream>) -> TokenStream {
    match result {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
