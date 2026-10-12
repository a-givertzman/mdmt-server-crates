use syn::{Data, DeriveInput, Field, Fields, FieldsNamed, GenericArgument, PathArguments, Type};

#[derive(Default)]
pub(crate) struct ContextFlags {
    pub(crate) read: bool,
    pub(crate) read_ref: bool,
    pub(crate) write: bool,
    pub(crate) skip: bool,
    pub(crate) skip_load: bool,
}

pub(crate) fn named_fields<'a>(input: &'a DeriveInput, macro_name: &str) -> syn::Result<&'a FieldsNamed> {
    for attr in &input.attrs {
        if attr.path().is_ident("context") {
            return Err(syn::Error::new_spanned(attr, "context flags are supported only on fields"));
        }
    }
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => Ok(fields),
            _ => Err(syn::Error::new_spanned(&input.ident, format!("{macro_name} requires a struct with named fields"))),
        },
        _ => Err(syn::Error::new_spanned(&input.ident, format!("{macro_name} can only be derived for structs"))),
    }
}

pub(crate) fn context_flags(field: &Field) -> syn::Result<ContextFlags> {
    let mut flags = ContextFlags::default();
    for attr in &field.attrs {
        if !attr.path().is_ident("context") { continue; }
        match &attr.meta {
            syn::Meta::List(list) if !list.tokens.is_empty() => {}
            _ => return Err(syn::Error::new_spanned(attr, "expected a non-empty #[context(...)] flag list")),
        }
        attr.parse_nested_meta(|meta| {
            let slot = if meta.path.is_ident("read") {
                &mut flags.read
            } else if meta.path.is_ident("read_ref") {
                &mut flags.read_ref
            } else if meta.path.is_ident("write") {
                &mut flags.write
            } else if meta.path.is_ident("skip") {
                &mut flags.skip
            } else if meta.path.is_ident("skip_load") {
                &mut flags.skip_load
            } else {
                return Err(meta.error("unknown context flag; expected read, read_ref, write, skip, or skip_load"));
            };
            if *slot { return Err(meta.error("duplicate context flag")); }
            if !meta.input.is_empty() && !meta.input.peek(syn::Token![,]) {
                return Err(meta.error("context flags do not accept values or arguments"));
            }
            *slot = true;
            Ok(())
        })?;
    }
    Ok(flags)
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

/// Recognize standard Option spellings only, not another_module::Option.
/// Imports, shadowing and aliases cannot be resolved by a derive macro.
pub(crate) fn field_value_type(ty: &Type) -> (&Type, bool) {
    let ty = ungroup(ty);
    let Type::Path(path) = ty else { return (ty, false); };
    if path.qself.is_some() { return (ty, false); }
    let segments: Vec<_> = path.path.segments.iter().collect();
    let standard = match segments.as_slice() {
        [option] => option.ident == "Option",
        [root, module, option] => {
            (root.ident == "std" || root.ident == "core")
                && module.ident == "option" && option.ident == "Option"
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
