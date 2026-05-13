extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};
/// Допустимые окончания имен структур для генерации EventOpCode.
const ALLOWED_SUFFIXES: &[&str] = &["Req", "Reply", "Cmd", "Inf", "Err", "Con"];
///
/// ### Автоматическая генерация реализации трейта `EventOpCode`.
/// 
/// Макрос анализирует имя структуры и ожидает, что оно заканчивается на один из
/// стандартизированных суффиксов, определяющих тип сообщения в системе событий
/// (Req, Reply, Cmd, Inf, Err, Con). Суффикс отсекается, а оставшаяся базовая 
/// часть имени используется для связи структуры с соответствующим вариантом 
/// перечисления `OpCode`.
///
/// #### Архитектурные ограничения
/// Макрос строго следит за конвенцией именования. Если имя структуры не содержит 
/// допустимого суффикса или состоит исключительно из него (например, просто `Req`),
/// компиляция будет прервана с указанием на ошибочный идентификатор.
///
/// #### Пример использования
/// ```ignore
/// #[derive(EventOpCode)]
/// pub struct NewProjectReq {
///     pub name: String,
/// }
/// // Макрос развернет этот код в:
/// // impl EventOpCode for NewProjectReq {
/// //     fn op_code(&self) -> OpCode {
/// //         OpCode::NewProject
/// //     }
/// // }
/// ```
#[proc_macro_derive(EventOpCode)]
pub fn derive_event_opcode(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;
    let name_str = name.to_string();
    let mut base_name = None;
    for suffix in ALLOWED_SUFFIXES {
        if name_str.ends_with(suffix) {
            base_name = Some(name_str.trim_end_matches(suffix).to_string());
            break;
        }
    }
    let base_name = match base_name {
        Some(n) if n.is_empty() => {
            let msg = format!(
                "Имя структуры `{}` состоит только из суффикса. Добавьте осмысленное базовое имя (например, Project{}).",
                name_str, name_str
            );
            return syn::Error::new_spanned(name, msg).to_compile_error().into();
        }
        Some(n) => n,
        None => {
            let msg = format!(
                "Архитектурное ограничение: структура `{}` имеет недопустимое окончание.\n\
                 Разрешенные суффиксы: {}.\n\
                 Пример правильного именования: `NewProjectReq` или `UpdateNodeCmd`.",
                name_str,
                ALLOWED_SUFFIXES.join(", ")
            );
            return syn::Error::new_spanned(name, msg).to_compile_error().into();
        }
    };
    let opcode_ident = syn::Ident::new(&base_name, name.span());
    let expanded = quote! {
        impl EventOpCode for #name {
            fn op_code(&self) -> OpCode {
                OpCode::#opcode_ident
            }
        }
    };
    TokenStream::from(expanded)
}
