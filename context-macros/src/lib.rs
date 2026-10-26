//! Derive macros for MDMT context. See README.md for consumer contracts.
use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

mod access;
mod load;
mod properties;
mod shared;

/// Generate typed access to crate::domain::ContextTransaction::state.
///
/// read generates both ContextRead<T> and ContextReadRef<T>.
/// read_ref generates only reference access, without requiring Clone.
/// write generates ContextWrite<T>. skip suppresses all access to a field.
/// skip_load is understood but affects only ContextLoad.
/// Reads return Option and never panic on an absent optional value.
#[proc_macro_derive(ContextAccess, attributes(context))]
pub fn derive_context_access(input: TokenStream) -> TokenStream {
    finish(access::expand(parse_macro_input!(input as DeriveInput)))
}

/// Generate IecId and Properties in crate::domain.
/// Requires one non-empty #[iec_id = "..."] on the type.
/// Properties returns Result<Vec<(String, String)>, serde_json::Error>.
#[proc_macro_derive(ContextProperties, attributes(iec_id))]
pub fn derive_context_properties(input: TokenStream) -> TokenStream {
    finish(properties::expand(parse_macro_input!(input as DeriveInput)))
}

/// Generate with_snapshot(self, properties) -> (Self, TypeLoadReport).
/// Missing or invalid values preserve existing fields. skip_load excludes a
/// field and its trait bounds. Duplicate IEC keys are reported, never applied.
#[proc_macro_derive(ContextLoad, attributes(context))]
pub fn derive_context_load(input: TokenStream) -> TokenStream {
    finish(load::expand(parse_macro_input!(input as DeriveInput)))
}

fn finish(result: syn::Result<proc_macro2::TokenStream>) -> TokenStream {
    match result {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[cfg(test)]
mod tests;
