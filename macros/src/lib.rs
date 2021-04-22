//! Convenience macros for [paperclip](https://github.com/wafflespeanut/paperclip).
//!
//! You shouldn't need to depend on this, because the stuff here is
//! already exposed by the corresponding crates.

#![recursion_limit = "512"]

extern crate proc_macro;
#[macro_use]
extern crate proc_macro_error;

#[cfg(feature = "actix")]
#[macro_use]
mod actix;
#[cfg(feature = "v2")]
mod core;

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    token::Comma,
    DeriveInput, NestedMeta,
};

/// Converts your struct to support deserializing from an OpenAPI v2
/// [Schema](https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#schemaObject)
/// object ([example](https://paperclip.waffles.space/paperclip/v2/)). This adds the necessary fields (in addition to your own fields) and implements the
/// `Schema` trait for parsing and codegen.
#[cfg(feature = "v2")]
#[proc_macro_attribute]
pub fn api_v2_schema_struct(_attr: TokenStream, input: TokenStream) -> TokenStream {
    self::core::emit_v2_schema_struct(input)
}

/// Marker attribute for indicating that a function is an OpenAPI v2 compatible operation.
#[cfg(feature = "actix")]
#[proc_macro_error]
#[proc_macro_attribute]
pub fn api_v2_operation(attr: TokenStream, input: TokenStream) -> TokenStream {
    self::actix::emit_v2_operation(attr, input)
}

/// Derive attribute for indicating that a type is an OpenAPI v2 compatible definition.
#[cfg(feature = "actix")]
#[proc_macro_error]
#[proc_macro_derive(Apiv2Schema, attributes(openapi))]
pub fn api_v2_schema(input: TokenStream) -> TokenStream {
    self::actix::emit_v2_definition(input)
}

/// Marker attribute for indicating that an object forbids public access to operation (for example AccessToken).
#[cfg(feature = "actix")]
#[proc_macro_error]
#[proc_macro_derive(Apiv2Security, attributes(openapi))]
pub fn api_v2_security(input: TokenStream) -> TokenStream {
    self::actix::emit_v2_security(input)
}

/// Marker attribute for indicating that the marked object can represent non-2xx (error)
/// status codes with optional descriptions.
#[cfg(feature = "actix")]
#[proc_macro_error]
#[proc_macro_attribute]
pub fn api_v2_errors(attrs: TokenStream, input: TokenStream) -> TokenStream {
    self::actix::emit_v2_errors(attrs, input)
}

/// Marker attribute for indicating that the marked object can filter error responses from the
/// the `#[api_v2_errors]` macro.
#[cfg(feature = "actix")]
#[proc_macro_error]
#[proc_macro_attribute]
pub fn api_v2_errors_overlay(attrs: TokenStream, input: TokenStream) -> TokenStream {
    self::actix::emit_v2_errors_overlay(attrs, input)
}

/// Generate an error at the call site and return empty token stream.
#[allow(dead_code)]
fn span_error_with_msg<T: Spanned>(it: &T, msg: &str) -> TokenStream {
    emit_error!(it.span().unwrap(), msg);
    (quote! {}).into()
}

/// Parses this token stream expecting a struct/enum and fails with an error otherwise.
#[allow(dead_code)]
fn expect_struct_or_enum(ts: TokenStream) -> Result<DeriveInput, TokenStream> {
    syn::parse(ts).map_err(|e| {
        emit_error!(
            e.span().unwrap(),
            "expected struct or enum for deriving schema."
        );
        quote!().into()
    })
}

/// Helper struct for parsing proc-macro input attributes.
#[derive(Default)]
struct MacroAttribute(Punctuated<NestedMeta, Comma>);

impl Parse for MacroAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(MacroAttribute(input.call(Punctuated::parse_terminated)?))
    }
}

#[cfg(feature = "actix")]
fn parse_input_attrs(ts: TokenStream) -> MacroAttribute {
    syn::parse(ts)
        .map_err(|e| {
            emit_warning!(
                e.span().unwrap(),
                "cannot parse proc-macro input attributes."
            );
        })
        .ok()
        .unwrap_or_default()
}

#[cfg(feature = "actix")]
rest_methods! {
    Get,    get,
    Post,   post,
    Put,    put,
    Delete, delete,
}
