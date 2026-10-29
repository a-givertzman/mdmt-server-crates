use heck::ToUpperCamelCase;
use syn::ext::IdentExt;
use syn::punctuated::Punctuated;
use syn::{
    Data, DeriveInput, Field, Fields, FieldsNamed, GenericArgument, Ident, Meta, PathArguments, Token, Type,
};

/// Флаги `#[context(...)]`, значимые для UI-слоя.
pub(crate) struct UiFlags {
    pub(crate) ui_skip: bool,
}

/// Возвращает именованные поля, отвергая enum/union/tuple-struct
/// и `#[context]` на самой структуре.
pub(crate) fn named_fields<'a>(input: &'a DeriveInput, macro_name: &str) -> syn::Result<&'a FieldsNamed> {
    for attr in &input.attrs {
        if attr.path().is_ident("context") {
            return Err(syn::Error::new_spanned(
                attr,
                "context flags are supported only on fields",
            ));
        }
    }
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => Ok(fields),
            _ => Err(syn::Error::new_spanned(
                &input.ident,
                format!("{macro_name} requires a struct with named fields"),
            )),
        },
        _ => Err(syn::Error::new_spanned(
            &input.ident,
            format!("{macro_name} can only be derived for structs"),
        )),
    }
}

/// Разбирает `#[context(...)]` поля.
///
/// - `ui_skip` исключает поле из генерации;
/// - `read`, `read_ref`, `write`, `skip`, `skip_load` принадлежат соседним derive и игнорируются;
/// - любой другой флаг игнорируется;
/// - повтор любого флага — ошибка;
/// - `#[context]` и пустой `#[context()]` — ошибка.
pub(crate) fn ui_flags(field: &Field) -> syn::Result<UiFlags> {
    let mut ui_skip = false;
    let mut seen: Vec<String> = Vec::new();
    for attr in &field.attrs {
        if !attr.path().is_ident("context") {
            continue;
        }
        let list = match &attr.meta {
            Meta::List(list) if !list.tokens.is_empty() => list,
            _ => {
                return Err(syn::Error::new_spanned(
                    attr,
                    "expected a non-empty #[context(...)] flag list",
                ))
            }
        };
        let flags = list
            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .map_err(|error| syn::Error::new_spanned(attr, error))?;
        for flag in flags {
            let name = match &flag {
                Meta::Path(path) => path.get_ident(),
                Meta::List(list) => list.path.get_ident(),
                Meta::NameValue(value) => value.path.get_ident(),
            };
            let Some(name) = name else { continue };
            let name = name.unraw().to_string();
            if seen.iter().any(|seen| seen == &name) {
                return Err(syn::Error::new_spanned(
                    &flag,
                    format!("duplicate context flag `{name}`"),
                ));
            }
            seen.push(name.clone());
            if name == "ui_skip" {
                if !matches!(flag, Meta::Path(_)) {
                    return Err(syn::Error::new_spanned(
                        &flag,
                        "context flags do not accept values or arguments",
                    ));
                }
                ui_skip = true;
            }
        }
    }
    Ok(UiFlags { ui_skip })
}

/// Снимает `r#` с raw-идентификатора и возвращает обычное написание.
pub(crate) fn ident_name(ident: &Ident) -> String {
    ident.unraw().to_string()
}

/// PascalCase-написание имени поля.
///
/// Эквивалентно `heck::AsUpperCamelCase`, который использует
/// `mdmt-server::domain::to_pascal_case`, и совпадает с
/// `serde(rename_all = "PascalCase")`.
pub(crate) fn pascal_case(ident: &Ident) -> String {
    ident_name(ident).to_upper_camel_case()
}

fn ungroup(mut ty: &Type) -> &Type {
    loop {
        ty = match ty {
            Type::Group(group) => &group.elem,
            Type::Paren(paren) => &paren.elem,
            _ => return ty,
        };
    }
}

/// Распознаёт только стандартные написания `Option`.
///
/// Импорты, shadowing и псевдонимы derive-макрос разрешить не может,
/// поэтому принимаются ровно `Option<T>`, `std::option::Option<T>`
/// и `core::option::Option<T>`.
pub(crate) fn field_value_type(ty: &Type) -> (&Type, bool) {
    let ty = ungroup(ty);
    let Type::Path(path) = ty else { return (ty, false) };
    if path.qself.is_some() {
        return (ty, false);
    }
    let segments: Vec<_> = path.path.segments.iter().collect();
    let standard = match segments.as_slice() {
        [option] => option.ident == "Option",
        [root, module, option] => {
            (root.ident == "std" || root.ident == "core")
                && module.ident == "option"
                && option.ident == "Option"
                && matches!(root.arguments, PathArguments::None)
                && matches!(module.arguments, PathArguments::None)
        }
        _ => false,
    };
    if standard {
        if let Some(last) = segments.last() {
            if let PathArguments::AngleBracketed(args) = &last.arguments {
                if args.args.len() == 1 {
                    if let Some(GenericArgument::Type(inner)) = args.args.first() {
                        return (ungroup(inner), true);
                    }
                }
            }
        }
    }
    (ty, false)
}
