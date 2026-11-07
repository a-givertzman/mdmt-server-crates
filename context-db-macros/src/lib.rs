//! Макрос для автоматизации записи и чтения стандартных  (database) JSON значений в контекст (`RawContext`)
use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

mod access;
mod properties;
mod shared;

/// ### Генерирует методы `db_read` / `db_write` для `RawContext`.
///
/// Структура обязана иметь именованные поля и не иметь generic-параметров.
/// Каждое поле без `#[context(skip_load)]` даёт ветку `match` по
/// `<ТипПоля as IecId>::iec_id()`.
#[proc_macro_derive(DbContextAccess, attributes(context))]
pub fn derive_db_context_access(input: TokenStream) -> TokenStream {
    finish(access::expand(parse_macro_input!(input as DeriveInput)))
}

/// ### Генерирует `impl DbProperties` для элемента контекста.
///
/// Структура обязана иметь именованные поля.
/// Каждое поле без `#[context(skip_load)]` даёт ветку `match` по PascalCase
/// и snake_case написаниям имени поля.
#[proc_macro_derive(DbContextProperties, attributes(context))]
pub fn derive_db_context_properties(input: TokenStream) -> TokenStream {
    finish(properties::expand(parse_macro_input!(input as DeriveInput)))
}

fn finish(result: syn::Result<proc_macro2::TokenStream>) -> TokenStream {
    match result {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
