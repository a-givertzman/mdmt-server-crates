use quote::quote;
use syn::{parse_quote, DeriveInput};
use crate::{access, load, properties};

fn access_error(input: DeriveInput) -> String {
    access::expand(input).unwrap_err().to_string()
}

#[test]
fn invalid_flags_are_reported_instead_of_silently_ignored() {
    for attribute in [
        quote!(#[context(unknown)]),
        quote!(#[context(read = true)]),
        quote!(#[context(read_ref(extra))]),
        quote!(#[context(read, read)]),
        quote!(#[context(read)] #[context(read)]),
        quote!(#[context()]),
        quote!(#[context]),
    ] {
        let input: DeriveInput = syn::parse2(quote! {
            struct RawContext { #attribute value: Option<Value> }
        }).unwrap();
        assert!(access::expand(input.clone()).is_err());
        assert!(load::expand(input).is_err());
    }
}

#[test]
fn skipped_fields_still_validate_their_attributes() {
    assert!(access::expand(parse_quote! {
        struct RawContext { #[context(skip, typo)] value: Value }
    }).is_err());
    assert!(load::expand(parse_quote! {
        struct RawContext { #[context(skip_load, typo)] value: Value }
    }).is_err());
}

#[test]
fn access_requires_a_named_non_generic_struct() {
    for input in [
        parse_quote!(enum RawContext { A }),
        parse_quote!(struct RawContext(Value);),
        parse_quote!(struct RawContext;),
        parse_quote!(union RawContext { value: u32 }),
        parse_quote!(struct RawContext<T> { value: T }),
        parse_quote!(#[context(read)] struct RawContext { value: Value }),
    ] {
        assert!(access::expand(input).is_err());
    }
}

#[test]
fn load_rejects_non_named_structs_without_a_macro_panic() {
    assert!(load::expand(parse_quote!(enum State { A })).is_err());
    assert!(load::expand(parse_quote!(struct State(u32);)).is_err());
}

#[test]
fn conflicting_access_is_diagnosed_per_trait_and_value_type() {
    let message = access_error(parse_quote! {
        struct RawContext {
            #[context(read)] first: Value,
            #[context(read_ref)] second: std::option::Option<Value>,
        }
    });
    assert!(message.contains("ContextReadRef"));
    assert!(message.contains("first") && message.contains("second"));
    let message = access_error(parse_quote! {
        struct RawContext {
            #[context(write)] first: Value,
            #[context(write)] second: Option<Value>,
        }
    });
    assert!(message.contains("ContextWrite"));
}

#[test]
fn different_traits_for_same_type_do_not_conflict() {
    // Reading one field and writing another is explicit and unambiguous.
    let tokens = access::expand(parse_quote! {
        struct RawContext {
            #[context(read)] first: Value,
            #[context(write)] second: Value,
        }
    }).unwrap();
    syn::parse2::<syn::File>(tokens).unwrap();
}

#[test]
fn skip_is_independent_of_flag_order() {
    for input in [
        parse_quote!(struct RawContext { #[context(skip, read, read_ref, write)] value: Value }),
        parse_quote!(struct RawContext { #[context(read, read_ref, write, skip)] value: Value }),
    ] {
        assert!(access::expand(input).unwrap().is_empty());
    }
}

#[test]
fn iec_id_is_validated_strictly() {
    for input in [
        parse_quote!(struct Property;),
        parse_quote!(#[iec_id = ""] struct Property;),
        parse_quote!(#[iec_id = "  "] struct Property;),
        parse_quote!(#[iec_id = 12] struct Property;),
        parse_quote!(#[iec_id("key")] struct Property;),
        parse_quote!(#[iec_id = "key"] #[iec_id = "other"] struct Property;),
    ] {
        assert!(properties::expand(input).is_err());
    }
}

#[test]
fn emitted_generic_impls_are_valid_rust_syntax() {
    let property = properties::expand(parse_quote! {
        #[iec_id = "Generic"] struct Property<'a, T, const N: usize>
        where T: Clone { value: &'a T, bytes: [u8; N] }
    }).unwrap();
    syn::parse2::<syn::File>(property).unwrap();
    let context = load::expand(parse_quote! {
        struct Context<'a, T, const N: usize> where T: Clone {
            value: Option<T>,
            #[context(skip_load)] cache: &'a [u8; N],
        }
    }).unwrap();
    syn::parse2::<syn::File>(context).unwrap();
}

#[test]
fn raw_struct_identifiers_produce_valid_report_names() {
    let tokens = load::expand(parse_quote! { struct r#type { #[context(skip_load)] value: usize } }).unwrap();
    syn::parse2::<syn::File>(tokens).unwrap();
}
