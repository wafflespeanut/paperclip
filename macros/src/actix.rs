//! Convenience macros for the [actix-web](https://github.com/wafflespeanut/paperclip/tree/master/plugins/actix-web)
//! OpenAPI plugin (exposed by paperclip with `actix` feature).

use heck::*;
use http::StatusCode;
use lazy_static::lazy_static;
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use strum_macros::EnumString;
use syn::{
    parse_macro_input,
    punctuated::{Pair, Punctuated},
    spanned::Spanned,
    Attribute, Data, DataEnum, DeriveInput, Field, Fields, FieldsNamed, FieldsUnnamed, FnArg,
    Generics, Ident, ItemFn, Lit, Meta, MetaList, MetaNameValue, NestedMeta, Path, PathArguments,
    ReturnType, Token, TraitBound, Type, TypeTraitObject,
};

use std::collections::HashMap;

const SCHEMA_MACRO_ATTR: &str = "openapi";

lazy_static! {
    static ref EMPTY_SCHEMA_HELP: String = format!(
        "you can mark the struct with #[{}(empty)] to ignore this warning.",
        SCHEMA_MACRO_ATTR
    );
}

/// Actual parser and emitter for `api_v2_operation` macro.
pub fn emit_v2_operation(attrs: TokenStream, input: TokenStream) -> TokenStream {
    let default_span = proc_macro2::Span::call_site();
    let mut item_ast: ItemFn = match syn::parse(input) {
        Ok(s) => s,
        Err(e) => {
            emit_error!(e.span().unwrap(), "operation must be a function.");
            return quote!().into();
        }
    };

    // Unit struct
    let s_name = format!("paperclip_{}", item_ast.sig.ident);
    let unit_struct = Ident::new(&s_name, default_span);
    let generics = &item_ast.sig.generics;
    let mut generics_call = quote!();
    let mut struct_definition = quote!(struct #unit_struct;);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    if !generics.params.is_empty() {
        let turbofish = ty_generics.as_turbofish();
        let generics_params = extract_generics_params(&item_ast);
        generics_call = quote!(#turbofish { p: std::marker::PhantomData });
        struct_definition = quote!(struct #unit_struct #ty_generics { p: std::marker::PhantomData<(#generics_params)> } )
    }

    // Get rid of async prefix. In the end, we'll have them all as `impl Future` thingies.
    if item_ast.sig.asyncness.is_some() {
        item_ast.sig.asyncness = None;
    }

    let mut wrapper = quote!(paperclip::actix::ResponseWrapper<actix_web::HttpResponse, #unit_struct #ty_generics>);
    let mut is_impl_trait = false;
    let mut is_responder = false;
    match &mut item_ast.sig.output {
        rt @ ReturnType::Default => {
            // Not particularly useful, but let's deal with it anyway
            *rt = ReturnType::Type(
                Token![->](default_span),
                Box::new(syn::parse2(wrapper.clone()).expect("parsing empty type")),
            );
        }
        ReturnType::Type(_, ty) => {
            let t = quote!(#ty).to_string();
            if let Type::ImplTrait(_) = &**ty {
                is_impl_trait = true;
            }

            if t == "impl Responder" {
                // `impl Responder` is a special case because we have to add another wrapper.
                // FIXME: Better way to deal with this?
                is_responder = true;
                *ty = Box::new(
                    syn::parse2(quote!(
                        impl std::future::Future<Output=paperclip::actix::ResponderWrapper<#ty>>
                    ))
                    .expect("parsing impl trait"),
                );
            } else if !is_impl_trait {
                // Any handler that's not returning an impl trait should return an `impl Future`
                *ty = Box::new(
                    syn::parse2(quote!(impl std::future::Future<Output=#ty>))
                        .expect("parsing impl trait"),
                );
            }

            if let Type::ImplTrait(imp) = &**ty {
                let obj = TypeTraitObject {
                    dyn_token: Some(Token![dyn](default_span)),
                    bounds: imp.bounds.clone(),
                };
                *ty = Box::new(
                    syn::parse2(quote!(#ty + paperclip::v2::schema::Apiv2Operation))
                        .expect("parsing impl trait"),
                );

                if !is_responder {
                    // NOTE: We're only using the box "type" to generate the operation data, we're not boxing
                    // the handlers at runtime.
                    wrapper = quote!(paperclip::actix::ResponseWrapper<Box<#obj + std::marker::Unpin>, #unit_struct #ty_generics>);
                }
            }
        }
    }

    let block = item_ast.block;
    // We need a function because devs should be able to use "return" keyword along the way.
    let wrapped_fn_call = if is_responder {
        quote!(paperclip::util::ready(paperclip::actix::ResponderWrapper((move || #block)())))
    } else if is_impl_trait {
        quote!((move || #block)())
    } else {
        quote!((move || async move #block)())
    };

    item_ast.block = Box::new(
        syn::parse2(quote!(
            {
                let f = #wrapped_fn_call;
                paperclip::actix::ResponseWrapper {
                    0: f,
                    1: #unit_struct #generics_call,
                }
            }
        ))
        .expect("parsing wrapped block"),
    );

    // Initialize operation parameters from macro attributes
    let (mut op_params, mut op_values) = parse_operation_attrs(attrs);

    // Optionally extract summary and description from doc comments
    if op_params.iter().find(|i| *i == "summary").is_none() {
        let (summary, description) = extract_fn_documentation(&item_ast);
        if let Some(summary) = summary {
            op_params.push(Ident::new("summary", item_ast.span()));
            op_values.push(summary)
        }
        if let Some(description) = description {
            op_params.push(Ident::new("description", item_ast.span()));
            op_values.push(description)
        }
    }

    let modifiers = extract_fn_arguments_types(&item_ast);

    quote!(
        #struct_definition

        #item_ast

        impl #impl_generics paperclip::v2::schema::Apiv2Operation for #unit_struct #ty_generics #where_clause {
            fn operation() -> paperclip::v2::models::DefaultOperationRaw {
                use paperclip::actix::OperationModifier;
                let mut op = paperclip::v2::models::DefaultOperationRaw {
                    #(
                        #op_params: #op_values,
                    )*
                    .. Default::default()
                };
                #(
                    <#modifiers>::update_parameter(&mut op);
                    <#modifiers>::update_security(&mut op);
                )*
                <<#wrapper as std::future::Future>::Output>::update_response(&mut op);
                op
            }

            #[allow(unused_mut)]
            fn security_definitions() -> std::collections::BTreeMap<String, paperclip::v2::models::SecurityScheme> {
                use paperclip::actix::OperationModifier;
                let mut map = Default::default();
                #(
                    <#modifiers>::update_security_definitions(&mut map);
                )*
                map
            }

            fn definitions() -> std::collections::BTreeMap<String, paperclip::v2::models::DefaultSchemaRaw> {
                use paperclip::actix::OperationModifier;
                let mut map = std::collections::BTreeMap::new();
                #(
                    <#modifiers>::update_definitions(&mut map);
                )*
                <<#wrapper as std::future::Future>::Output>::update_definitions(&mut map);
                map
            }
        }
    )
    .into()
}

/// Extract punctuated generic parameters from fn definition
fn extract_generics_params(item_ast: &ItemFn) -> Punctuated<Ident, syn::token::Comma> {
    item_ast
        .sig
        .generics
        .params
        .pairs()
        .filter_map(|pair| match pair {
            Pair::Punctuated(syn::GenericParam::Type(gen), punct) => {
                Some(Pair::new(gen.ident.clone(), Some(*punct)))
            }
            Pair::End(syn::GenericParam::Type(gen)) => Some(Pair::new(gen.ident.clone(), None)),
            _ => None,
        })
        .collect()
}

/// Extract function arguments
fn extract_fn_arguments_types(item_ast: &ItemFn) -> Vec<Type> {
    item_ast
        .sig
        .inputs
        .iter()
        .filter_map(|inp| match inp {
            FnArg::Receiver(_) => None,
            FnArg::Typed(ref t) => Some(*t.ty.clone()),
        })
        .collect()
}

/// Parse macro attrs, matching to Operation fields
/// Returning operation attribute identifier and value initialization arrays
/// Note: Array likes initialized from string "val1, val2, val3", where "val1"
/// would parse into destination item
fn parse_operation_attrs(attrs: TokenStream) -> (Vec<Ident>, Vec<proc_macro2::TokenStream>) {
    let attrs = crate::parse_input_attrs(attrs);
    let mut params = Vec::new();
    let mut values = Vec::new();
    for attr in attrs.0 {
        if let NestedMeta::Meta(Meta::NameValue(MetaNameValue { path, lit, .. })) = &attr {
            if let Some(ident) = path.get_ident() {
                match ident.to_string().as_str() {
                    "summary" | "description" | "operation_id" => {
                        if let Lit::Str(val) = lit {
                            params.push(ident.clone());
                            values.push(quote!(Some(#val.to_string())));
                        } else {
                            emit_error!(lit.span(), "Expected string literal: {:?}", lit)
                        }
                    }
                    "consumes" | "produces" => {
                        if let Lit::Str(mimes) = lit {
                            let mut mime_types = Vec::new();
                            for val in mimes.value().split(',') {
                                let val = val.trim();
                                if let Err(err) = val.parse::<mime::Mime>() {
                                    emit_error!(
                                        lit.span(),
                                        "Value {} does not parse as mime type: {}",
                                        val,
                                        err
                                    );
                                } else {
                                    mime_types.push(quote!(paperclip::v2::models::MediaRange(#val.parse().unwrap())));
                                }
                            }
                            if !mime_types.is_empty() {
                                params.push(ident.clone());
                                values.push(quote!({
                                    let mut tmp = std::collections::BTreeSet::new();
                                    #(
                                        tmp.insert(#mime_types);
                                    )*
                                    Some(tmp)
                                }));
                            }
                        } else {
                            emit_error!(
                                lit.span(),
                                "Expected comma separated values in string literal: {:?}",
                                lit
                            )
                        }
                    }
                    x => emit_error!(ident.span(), "Unknown attribute {}", x),
                }
            } else {
                emit_error!(
                    path.span(),
                    "Expected single identifier, got path {:?}",
                    path
                )
            }
        } else if let NestedMeta::Meta(Meta::List(MetaList { path, nested, .. })) = &attr {
            if let Some(ident) = path.get_ident() {
                match ident.to_string().as_str() {
                    "tags" => {
                        let mut tags = Vec::new();
                        for meta in nested.pairs().map(|pair| pair.into_value()) {
                            if let NestedMeta::Meta(Meta::Path(Path { segments, .. })) = meta {
                                tags.push(segments[0].ident.to_string());
                            } else if let NestedMeta::Lit(Lit::Str(lit)) = meta {
                                tags.push(lit.value());
                            } else {
                                emit_error!(
                                    meta.span(),
                                    "Expected comma separated list of tags idents: {:?}",
                                    meta
                                )
                            }
                        }
                        if !tags.is_empty() {
                            params.push(ident.clone());
                            values.push(quote!(vec![ #( #tags.to_string() ),* ]));
                        }
                    }
                    x => emit_error!(ident.span(), "Unknown list ident {}", x),
                }
            }
        } else {
            emit_error!(attr.span(), "Not supported attribute type {:?}", attr)
        }
    }
    (params, values)
}

/// Extracts summary from top line doc comment and description from the rest
fn extract_fn_documentation(
    item_ast: &ItemFn,
) -> (
    Option<proc_macro2::TokenStream>,
    Option<proc_macro2::TokenStream>,
) {
    let docs = extract_documentation(&item_ast.attrs);
    let lines = docs.lines();
    let mut before_empty = true;
    let (summary, description): (Vec<_>, Vec<_>) = lines.partition(|line| {
        if line.trim().is_empty() {
            before_empty = false
        };
        before_empty
    });
    let none_if_empty = |text: &str| {
        if text.is_empty() {
            None
        } else {
            Some(quote!(Some(#text.to_string())))
        }
    };
    let summary = none_if_empty(summary.into_iter().collect::<String>().trim());
    let description = none_if_empty(description.join("\n").trim());
    (summary, description)
}

/// Actual parser and emitter for `api_v2_errors` macro.
pub fn emit_v2_errors(attrs: TokenStream, input: TokenStream) -> TokenStream {
    let item_ast = match crate::expect_struct_or_enum(input) {
        Ok(i) => i,
        Err(ts) => return ts,
    };

    let name = &item_ast.ident;
    let attrs = crate::parse_input_attrs(attrs);
    let generics = item_ast.generics.clone();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Convert macro attributes to tuples in form of (u16, &str)
    let error_codes = attrs
        .0
        .iter()
        // Pair code attrs with description attrs; save attr itself to properly span error messages at later stage
        .fold(Vec::new(), |mut list: Vec<(Option<u16>, Option<String>, _)>, attr| {
            let span = attr.span().unwrap();
            match attr {
                // Read named attribute.
                NestedMeta::Meta(Meta::NameValue(name_value)) => {
                    let attr_name = name_value.path.get_ident().map(|ident| ident.to_string());
                    let attr_value = &name_value.lit;

                    match (attr_name.as_deref(), attr_value) {
                        // "code" attribute adds new element to list
                        (Some("code"), Lit::Int(attr_value)) => {
                            let status_code = attr_value.base10_parse::<u16>()
                                .map_err(|_| emit_error!(span, "Invalid u16 in code argument")).ok();
                            list.push((status_code, None, attr));
                        },
                        // "description" attribute updates last element in list
                        (Some("description"), Lit::Str(attr_value)) =>
                            if let Some(last_value) = list.last_mut() {
                                if last_value.1.is_some() {
                                    emit_warning!(span, "This attribute overwrites previous description");
                                }
                                last_value.1 = Some(attr_value.value());
                            } else {
                                emit_error!(span, "Attribute 'description' can be only placed after prior 'code' argument");
                            },
                        _ => emit_error!(span, "Invalid macro attribute. Should be plain u16, 'code = u16' or 'description = str'")
                    }
                },
                // Read plain status code as attribute.
                NestedMeta::Lit(Lit::Int(attr_value)) => {
                    let status_code = attr_value.base10_parse::<u16>()
                    .map_err(|_| emit_error!(span, "Invalid u16 in code argument")).ok();
                    list.push((status_code, None, attr));
                },
                _ => emit_error!(span, "This macro supports only named attributes - 'code' (u16) or 'description' (str)")
            }

            list
        })
        .iter()
        // Map code-message pairs into bits of code, filter empty codes out
        .filter_map(|triple| {
            let (code, description) = match triple {
                (Some(code), Some(description), _) => (code, description.to_owned()),
                (Some(code), None, attr) => {
                    let span = attr.span().unwrap();
                    let description = StatusCode::from_u16(*code)
                        .map_err(|_| {
                            emit_warning!(span, format!("Invalid status code {}", code));
                            String::new()
                        })
                        .map(|s| s.canonical_reason()
                            .map(str::to_string)
                            .unwrap_or_else(|| {
                                emit_warning!(span, format!("Status code {} doesn't have a canonical name", code));
                                String::new()
                            })
                        )
                        .unwrap_or_else(|_| String::new());
                    (code, description)
                },
                (None, _, _) => return None,
            };

            Some(quote! {
                (#code, #description),
            })
        })
        .fold(proc_macro2::TokenStream::new(), |mut stream, tokens| {
            stream.extend(tokens);
            stream
        });

    let gen = quote! {
        #item_ast

        impl #impl_generics paperclip::v2::schema::Apiv2Errors for #name #ty_generics #where_clause {
            const ERROR_MAP: &'static [(u16, &'static str)] = &[
                #error_codes
            ];
        }
    };

    gen.into()
}

/// Actual parser and emitter for `api_v2_schema` macro.
pub fn emit_v2_definition(input: TokenStream) -> TokenStream {
    let item_ast = match crate::expect_struct_or_enum(input) {
        Ok(i) => i,
        Err(ts) => return ts,
    };

    if let Some(empty) = check_empty_schema(&item_ast) {
        return empty;
    }

    let docs = extract_documentation(&item_ast.attrs);
    let docs = docs.trim();

    let props = SerdeProps::from_item_attrs(&item_ast.attrs);
    let name = &item_ast.ident;

    // Add `Apiv2Schema` bound for impl if the type is generic.
    let mut generics = item_ast.generics.clone();
    let bound = syn::parse2::<TraitBound>(quote!(paperclip::v2::schema::Apiv2Schema))
        .expect("expected to parse trait bound");
    generics.type_params_mut().for_each(|param| {
        param.bounds.push(bound.clone().into());
    });

    let opt_impl = add_optional_impl(&name, &generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // FIXME: Use attr path segments to find flattening, skipping, etc.
    let mut props_gen = quote! {};

    match &item_ast.data {
        Data::Struct(ref s) => {
            props_gen.extend(quote!(
                schema.data_type = Some(DataType::Object);
            ));
            match &s.fields {
                Fields::Named(ref f) => {
                    handle_field_struct(f, &item_ast.attrs, &props, &mut props_gen)
                }
                Fields::Unnamed(ref f) => {
                    handle_unnamed_field_struct(f, &item_ast.attrs, &mut props_gen)
                }
                Fields::Unit => {
                    emit_warning!(
                        s.struct_token.span().unwrap(),
                        "unit structs do not have any fields and hence will have empty schema.";
                        help = "{}", &*EMPTY_SCHEMA_HELP;
                    );
                }
            }
        }
        Data::Enum(ref e) => handle_enum(e, &props, &mut props_gen),
        Data::Union(ref u) => emit_error!(
            u.union_token.span().unwrap(),
            "unions are unsupported for deriving schema"
        ),
    };

    let schema_name = name.to_string();
    let props_gen_empty = props_gen.is_empty();
    let gen = quote! {
        impl #impl_generics paperclip::v2::schema::Apiv2Schema for #name #ty_generics #where_clause {
            const NAME: Option<&'static str> = Some(#schema_name);

            const DESCRIPTION: &'static str = #docs;

            fn raw_schema() -> paperclip::v2::models::DefaultSchemaRaw {
                use paperclip::v2::models::{DataType, DataTypeFormat, DefaultSchemaRaw};
                use paperclip::v2::schema::TypedData;

                let mut schema = DefaultSchemaRaw {
                    name: Some(#schema_name.into()), // Add name for later use.
                    .. Default::default()
                };
                #props_gen
                // props_gen may override the schema for unnamed structs with 1 element
                // as it replaces the struct type with inner type.
                // make sure we set the name properly if props_gen is not empty
                if !#props_gen_empty {
                    schema.name = Some(#schema_name.into());
                }
                schema
            }
        }

        #opt_impl
    };

    gen.into()
}

/// Actual parser and emitter for `Apiv2Security` derive macro.
pub fn emit_v2_security(input: TokenStream) -> TokenStream {
    let item_ast = match crate::expect_struct_or_enum(input) {
        Ok(i) => i,
        Err(ts) => return ts,
    };

    if let Some(empty) = check_empty_schema(&item_ast) {
        return empty;
    }

    let name = &item_ast.ident;
    // Add `Apiv2Schema` bound for impl if the type is generic.
    let mut generics = item_ast.generics.clone();
    let bound = syn::parse2::<TraitBound>(quote!(paperclip::v2::schema::Apiv2Schema))
        .expect("expected to parse trait bound");
    generics.type_params_mut().for_each(|param| {
        param.bounds.push(bound.clone().into());
    });

    let opt_impl = add_optional_impl(&name, &generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let mut security_attrs = HashMap::new();
    let mut scopes = Vec::new();

    let valid_attrs = vec![
        "alias",
        "description",
        "name",
        "in",
        "flow",
        "auth_url",
        "token_url",
        "parent",
    ];
    let invalid_attr_msg = format!("Invalid macro attribute. Should be bare security type [\"apiKey\", \"oauth2\"] or named attribute {:?}", valid_attrs);

    // Read security params from openapi attr.
    for nested in extract_openapi_attrs(&item_ast.attrs) {
        for nested_attr in nested {
            let span = nested_attr.span().unwrap();
            match &nested_attr {
                // Read bare attribute.
                NestedMeta::Meta(Meta::Path(attr_path)) => {
                    if let Some(type_) = attr_path.get_ident() {
                        if security_attrs
                            .insert("type".to_string(), type_.to_string())
                            .is_some()
                        {
                            emit_warning!(span, "Auth type defined multiple times.");
                        }
                    }
                }
                // Read named attribute.
                NestedMeta::Meta(Meta::NameValue(name_value)) => {
                    let attr_name = name_value.path.get_ident().map(|id| id.to_string());
                    let attr_value = &name_value.lit;

                    if let Some(attr_name) = attr_name {
                        if valid_attrs.contains(&attr_name.as_str()) {
                            if let Lit::Str(attr_value) = attr_value {
                                if security_attrs
                                    .insert(attr_name.clone(), attr_value.value())
                                    .is_some()
                                {
                                    emit_warning!(
                                        span,
                                        "Attribute {} defined multiple times.",
                                        attr_name
                                    );
                                }
                            } else {
                                emit_warning!(
                                    span,
                                    "Invalid value for named attribute: {}",
                                    attr_name
                                );
                            }
                        } else {
                            emit_warning!(span, invalid_attr_msg);
                        }
                    } else {
                        emit_error!(span, invalid_attr_msg);
                    }
                }
                // Read scopes attribute
                NestedMeta::Meta(Meta::List(list_attr)) => {
                    match list_attr
                        .path
                        .get_ident()
                        .map(|id| id.to_string())
                        .as_deref()
                    {
                        Some("scopes") => {
                            for nested in &list_attr.nested {
                                match nested {
                                    NestedMeta::Lit(Lit::Str(value)) => {
                                        scopes.push(value.value().to_string())
                                    }
                                    _ => emit_error!(
                                        nested.span().unwrap(),
                                        "Invalid list attribute value"
                                    ),
                                }
                            }
                        }
                        Some(path) => emit_error!(span, "Invalid list attribute: {}", path),
                        _ => emit_error!(span, "Invalid list attribute"),
                    }
                }
                _ => {
                    emit_error!(span, invalid_attr_msg);
                }
            }
        }
    }

    fn quote_option(value: Option<&String>) -> proc_macro2::TokenStream {
        if let Some(value) = value {
            quote! { Some(#value.to_string()) }
        } else {
            quote! { None }
        }
    }

    let scopes_stream = scopes
        .iter()
        .fold(proc_macro2::TokenStream::new(), |mut stream, scope| {
            stream.extend(quote! {
                oauth2_scopes.insert(#scope.to_string(), #scope.to_string());
            });
            stream
        });

    let (security_def, security_def_name) = match (
        security_attrs.get("type"),
        security_attrs.get("parent"),
    ) {
        (Some(type_), None) => {
            let alias = security_attrs.get("alias").unwrap_or(type_);
            let quoted_description = quote_option(security_attrs.get("description"));
            let quoted_name = quote_option(security_attrs.get("name"));
            let quoted_in = quote_option(security_attrs.get("in"));
            let quoted_flow = quote_option(security_attrs.get("flow"));
            let quoted_auth_url = quote_option(security_attrs.get("auth_url"));
            let quoted_token_url = quote_option(security_attrs.get("token_url"));

            (
                Some(quote! {
                    Some(paperclip::v2::models::SecurityScheme {
                        type_: #type_.to_string(),
                        name: #quoted_name,
                        in_: #quoted_in,
                        flow: #quoted_flow,
                        auth_url: #quoted_auth_url,
                        token_url: #quoted_token_url,
                        scopes: std::collections::BTreeMap::new(),
                        description: #quoted_description,
                    })
                }),
                Some(quote!(Some(#alias))),
            )
        }
        (None, Some(parent)) => {
            let parent_ident = Ident::new(parent, proc_macro2::Span::call_site());
            // Child of security definition (Scopes will be glued to parent definition).
            (
                Some(quote! {
                    let mut oauth2_scopes = std::collections::BTreeMap::new();
                    #scopes_stream
                    let mut scheme = #parent_ident::security_scheme()
                        .expect("empty schema. did you derive `Apiv2Security` for parent struct?");
                    scheme.scopes = oauth2_scopes;
                    Some(scheme)
                }),
                Some(quote!(<#parent_ident as paperclip::v2::schema::Apiv2Schema>::NAME)),
            )
        }
        (Some(_), Some(_)) => {
            emit_error!(
                item_ast.span().unwrap(),
                "Can't define new security type and use parent attribute together."
            );
            (None, None)
        }
        (None, None) => {
            emit_error!(
                item_ast.span().unwrap(),
                "Invalid attributes. Expected security type or parent defined."
            );
            (None, None)
        }
    };

    let gen = if let (Some(def_block), Some(def_name)) = (security_def, security_def_name) {
        quote! {
            impl #impl_generics paperclip::v2::schema::Apiv2Schema for #name #ty_generics #where_clause {
                const NAME: Option<&'static str> = #def_name;

                fn security_scheme() -> Option<paperclip::v2::models::SecurityScheme> {
                    #def_block
                }
            }

            #opt_impl
        }
    } else {
        quote! {}
    };

    gen.into()
}

#[cfg(feature = "nightly")]
fn add_optional_impl(_: &Ident, _: &Generics) -> proc_macro2::TokenStream {
    // Empty impl for "nightly" feature because specialization helps us there.
    quote!()
}

#[cfg(not(feature = "nightly"))]
fn add_optional_impl(name: &Ident, generics: &Generics) -> proc_macro2::TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    quote! {
        impl #impl_generics paperclip::actix::OperationModifier for #name #ty_generics #where_clause {}
    }
}

fn get_field_type(field: &Field) -> Option<proc_macro2::TokenStream> {
    match field.ty {
        Type::Path(_) | Type::Reference(_) => Some(address_type_for_fn_call(&field.ty)),
        _ => {
            emit_warning!(
                field.ty.span().unwrap(),
                "unsupported field type will be ignored."
            );
            None
        }
    }
}

/// Generates code for a tuple struct with fields.
fn handle_unnamed_field_struct(
    fields: &FieldsUnnamed,
    struct_attr: &[Attribute],
    props_gen: &mut proc_macro2::TokenStream,
) {
    if fields.unnamed.len() == 1 {
        let field = fields.unnamed.iter().next().unwrap();

        if let Some(ty_ref) = get_field_type(&field) {
            let docs = extract_documentation(struct_attr);
            let docs = docs.trim();

            if SerdeSkip::exists(&field.attrs) {
                props_gen.extend(quote!({
                    let mut s: DefaultSchemaRaw = Default::default();
                    if !#docs.is_empty() {
                        s.description = Some(#docs.to_string());
                    }
                    schema = s;
                }));
            } else {
                props_gen.extend(quote!({
                    let mut s = #ty_ref::raw_schema();
                    if !#docs.is_empty() {
                        s.description = Some(#docs.to_string());
                    }
                    schema = s;
                }));
            }
        }
    } else {
        for (inner_field_id, field) in (&fields.unnamed).into_iter().enumerate() {
            if SerdeSkip::exists(&field.attrs) {
                continue;
            }

            let ty_ref = get_field_type(&field);

            let docs = extract_documentation(&field.attrs);
            let docs = docs.trim();

            let mut gen = if !SerdeFlatten::exists(&field.attrs) {
                // this is really not what we'd want to do because that's not how the
                // deserialized struct will be like, ideally we want an actual tuple
                // this type should therefore not be used for anything else than `Path`
                quote!({
                    let mut s = #ty_ref::raw_schema();
                    if !#docs.is_empty() {
                        s.description = Some(#docs.to_string());
                    }
                    schema.properties.insert(#inner_field_id.to_string(), s.into());
                })
            } else {
                quote!({
                    let s = #ty_ref::raw_schema();
                    schema.properties.extend(s.properties);
                })
            };

            gen.extend(quote! {
                if #ty_ref::REQUIRED {
                    schema.required.insert(#inner_field_id.to_string());
                }
            });

            props_gen.extend(gen);
        }
    }
}

/// Checks for `api_v2_empty` attributes and removes them.
fn extract_openapi_attrs(
    field_attrs: &'_ [Attribute],
) -> impl Iterator<Item = Punctuated<syn::NestedMeta, syn::token::Comma>> + '_ {
    field_attrs.iter().filter_map(|a| match a.parse_meta() {
        Ok(Meta::List(list)) if list.path.is_ident("openapi") => Some(list.nested),
        _ => None,
    })
}

/// Checks for `api_v2_empty` attributes and removes them.
fn extract_documentation(attrs: &[Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|a| match a.parse_meta() {
            Ok(Meta::NameValue(mnv)) if mnv.path.is_ident("doc") => match &mnv.lit {
                Lit::Str(s) => Some(s.value()),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// Checks if an empty schema has been requested and generate if needed.
fn check_empty_schema(item_ast: &DeriveInput) -> Option<TokenStream> {
    let needs_empty_schema = extract_openapi_attrs(&item_ast.attrs).any(|nested| {
        nested.len() == 1
            && match &nested[0] {
                NestedMeta::Meta(Meta::Path(path)) => path.is_ident("empty"),
                _ => false,
            }
    });

    if needs_empty_schema {
        let name = &item_ast.ident;
        let generics = item_ast.generics.clone();
        let opt_impl = add_optional_impl(&name, &generics);
        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
        return Some(quote!(
            impl #impl_generics paperclip::v2::schema::Apiv2Schema for #name #ty_generics #where_clause {}

            #opt_impl
        ).into());
    }

    None
}

/// Generates code for a struct with fields.
fn handle_field_struct(
    fields: &FieldsNamed,
    struct_attr: &[Attribute],
    serde: &SerdeProps,
    props_gen: &mut proc_macro2::TokenStream,
) {
    let docs = extract_documentation(struct_attr);
    let docs = docs.trim();

    props_gen.extend(quote!({
        if !#docs.is_empty() {
            schema.description = Some(#docs.to_string());
        }
    }));
    for field in &fields.named {
        let mut field_name = field
            .ident
            .as_ref()
            .expect("missing field name?")
            .to_string();

        if SerdeSkip::exists(&field.attrs) {
            continue;
        }

        if let Some(renamed) = SerdeRename::from_field_attrs(&field.attrs) {
            field_name = renamed;
        } else if let Some(prop) = serde.rename {
            field_name = prop.rename(&field_name);
        }

        let ty_ref = get_field_type(&field);

        let docs = extract_documentation(&field.attrs);
        let docs = docs.trim();

        let mut gen = if !SerdeFlatten::exists(&field.attrs) {
            quote!({
                let mut s = #ty_ref::raw_schema();
                if !#docs.is_empty() {
                    s.description = Some(#docs.to_string());
                }
                schema.properties.insert(#field_name.into(), s.into());
            })
        } else {
            quote!({
                let s = #ty_ref::raw_schema();
                schema.properties.extend(s.properties);
            })
        };

        gen.extend(quote! {
            if #ty_ref::REQUIRED {
                schema.required.insert(#field_name.into());
            }
        });

        props_gen.extend(gen);
    }
}

/// Generates code for an enum (if supported).
fn handle_enum(e: &DataEnum, serde: &SerdeProps, props_gen: &mut proc_macro2::TokenStream) {
    props_gen.extend(quote!(
        schema.data_type = Some(DataType::String);
    ));

    for var in &e.variants {
        let mut name = var.ident.to_string();
        match &var.fields {
            Fields::Unit => (),
            Fields::Named(ref f) => {
                emit_warning!(
                    f.span().unwrap(),
                    "skipping enum variant with named fields in schema."
                );
                continue;
            }
            Fields::Unnamed(ref f) => {
                emit_warning!(f.span().unwrap(), "skipping tuple enum variant in schema.");
                continue;
            }
        }

        if SerdeSkip::exists(&var.attrs) {
            continue;
        }

        if let Some(renamed) = SerdeRename::from_field_attrs(&var.attrs) {
            name = renamed;
        } else if let Some(prop) = serde.rename {
            name = prop.rename(&name);
        }

        props_gen.extend(quote!(
            schema.enum_.push(serde_json::json!(#name));
        ));
    }
}

/// An associated function of a generic type, say, a vector cannot be called
/// like `Vec::foo` as it doesn't have a default type. We should instead call
/// `Vec::<T>::foo`. Something similar applies to `str`. This function takes
/// care of that special treatment.
fn address_type_for_fn_call(old_ty: &Type) -> proc_macro2::TokenStream {
    if let Type::Reference(_) = old_ty {
        return quote!(<(#old_ty)>);
    }

    let mut ty = old_ty.clone();
    if let Type::Path(ref mut p) = &mut ty {
        p.path.segments.pairs_mut().for_each(|mut pair| {
            let is_empty = pair.value().arguments.is_empty();
            let args = &mut pair.value_mut().arguments;
            match args {
                PathArguments::AngleBracketed(ref mut brack_args) if !is_empty => {
                    brack_args.colon2_token = Some(Token![::](proc_macro2::Span::call_site()));
                }
                _ => (),
            }
        });
    }

    quote!(#ty)
}

/* Serde attributes */

/// Supported renaming options in serde (https://serde.rs/variant-attrs.html).
#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString)]
enum SerdeRename {
    #[strum(serialize = "lowercase")]
    Lower,
    #[strum(serialize = "UPPERCASE")]
    Upper,
    #[strum(serialize = "PascalCase")]
    Pascal,
    #[strum(serialize = "camelCase")]
    Camel,
    #[strum(serialize = "snake_case")]
    Snake,
    #[strum(serialize = "SCREAMING_SNAKE_CASE")]
    ScreamingSnake,
    #[strum(serialize = "kebab-case")]
    Kebab,
    #[strum(serialize = "SCREAMING-KEBAB-CASE")]
    ScreamingKebab,
}

impl SerdeRename {
    /// Traverses the field attributes and returns the renamed value from the first matching
    /// `#[serde(rename = "...")]` pattern.
    fn from_field_attrs(field_attrs: &[Attribute]) -> Option<String> {
        for meta in field_attrs.iter().filter_map(|a| a.parse_meta().ok()) {
            let inner_meta = match meta {
                Meta::List(ref l)
                    if l.path
                        .segments
                        .last()
                        .map(|p| p.ident == "serde")
                        .unwrap_or(false) =>
                {
                    &l.nested
                }
                _ => continue,
            };

            for meta in inner_meta {
                let rename = match meta {
                    NestedMeta::Meta(Meta::NameValue(ref v))
                        if v.path
                            .segments
                            .last()
                            .map(|p| p.ident == "rename")
                            .unwrap_or(false) =>
                    {
                        &v.lit
                    }
                    _ => continue,
                };

                if let Lit::Str(ref s) = rename {
                    return Some(s.value());
                }
            }
        }

        None
    }

    /// Renames the given value using the current option.
    fn rename(self, name: &str) -> String {
        match self {
            SerdeRename::Lower => name.to_lowercase(),
            SerdeRename::Upper => name.to_uppercase(),
            SerdeRename::Pascal => name.to_camel_case(),
            SerdeRename::Camel => name.to_mixed_case(),
            SerdeRename::Snake => name.to_snek_case(),
            SerdeRename::ScreamingSnake => name.to_snek_case().to_uppercase(),
            SerdeRename::Kebab => name.to_kebab_case(),
            SerdeRename::ScreamingKebab => name.to_kebab_case().to_uppercase(),
        }
    }
}

/// Serde skip (https://serde.rs/variant-attrs.html)
/// Never serialize or deserialize this variant.
/// There are other variants available (skip_serializing,skip_deserializing) though it's not clear
/// how this should be handled since we use the same Schema for Ser/DeSer
struct SerdeSkip;

impl SerdeSkip {
    /// Traverses the field attributes and returns whether the field should be skipped or not
    /// dependent on finding the `#[serde(skip]` attribute.
    fn exists(field_attrs: &[Attribute]) -> bool {
        for meta in field_attrs.iter().filter_map(|a| a.parse_meta().ok()) {
            let inner_meta = match meta {
                Meta::List(ref l)
                    if l.path
                        .segments
                        .last()
                        .map(|p| p.ident == "serde")
                        .unwrap_or(false) =>
                {
                    &l.nested
                }
                _ => continue,
            };
            for meta in inner_meta {
                if let NestedMeta::Meta(Meta::Path(path)) = meta {
                    if path.segments.iter().any(|s| s.ident == "skip") {
                        return true;
                    }
                }
            }
        }

        false
    }
}

#[derive(Clone, Debug, Default)]
struct SerdeProps {
    rename: Option<SerdeRename>,
}

impl SerdeProps {
    /// Traverses the serde attributes in the given item attributes and returns
    /// the applicable properties.
    fn from_item_attrs(item_attrs: &[Attribute]) -> Self {
        let mut props = Self::default();
        for meta in item_attrs.iter().filter_map(|a| a.parse_meta().ok()) {
            let inner_meta = match meta {
                Meta::List(ref l)
                    if l.path
                        .segments
                        .last()
                        .map(|p| p.ident == "serde")
                        .unwrap_or(false) =>
                {
                    &l.nested
                }
                _ => continue,
            };

            for meta in inner_meta {
                let global_rename = match meta {
                    NestedMeta::Meta(Meta::NameValue(ref v))
                        if v.path
                            .segments
                            .last()
                            .map(|p| p.ident == "rename_all")
                            .unwrap_or(false) =>
                    {
                        &v.lit
                    }
                    _ => continue,
                };

                if let Lit::Str(ref s) = global_rename {
                    props.rename = s.value().parse().ok();
                }
            }
        }

        props
    }
}

/// Supported flattening of embedded struct (https://serde.rs/variant-attrs.html).
struct SerdeFlatten;

impl SerdeFlatten {
    /// Traverses the field attributes and returns true if there is `#[serde(flatten)]`.
    fn exists(field_attrs: &[Attribute]) -> bool {
        for meta in field_attrs.iter().filter_map(|a| a.parse_meta().ok()) {
            let inner_meta = match meta {
                Meta::List(ref l)
                    if l.path
                        .segments
                        .last()
                        .map(|p| p.ident == "serde")
                        .unwrap_or(false) =>
                {
                    &l.nested
                }
                _ => continue,
            };

            for meta in inner_meta {
                if let NestedMeta::Meta(Meta::Path(syn::Path { segments, .. })) = meta {
                    if segments.iter().any(|p| p.ident == "flatten") {
                        return true;
                    }
                }
            }
        }

        false
    }
}

macro_rules! doc_comment {
    ($x:expr; $($tt:tt)*) => {
        #[doc = $x]
        $($tt)*
    };
}

#[cfg(feature = "actix")]
impl super::Method {
    fn handler_uri(attr: TokenStream) -> TokenStream {
        let attr = parse_macro_input!(attr as syn::AttributeArgs);
        attr.first().into_token_stream().into()
    }
    fn handler_name(item: TokenStream) -> syn::Result<syn::Ident> {
        let handler: ItemFn = syn::parse(item)?;
        Ok(handler.sig.ident)
    }
    pub(crate) fn generate(
        &self,
        attr: TokenStream,
        item: TokenStream,
    ) -> syn::Result<proc_macro2::TokenStream> {
        let uri: proc_macro2::TokenStream = Self::handler_uri(attr).into();
        let handler_name = Self::handler_name(item.clone())?;
        let handler_fn: proc_macro2::TokenStream = item.into();
        let method: proc_macro2::TokenStream = self.method().parse()?;
        let variant: proc_macro2::TokenStream = self.variant().parse()?;
        let handler_name_str = handler_name.to_string();

        Ok(quote! {
            #[allow(non_camel_case_types, missing_docs)]
            pub struct #handler_name;

            impl #handler_name {
                fn resource() -> paperclip::actix::web::Resource {
                    #handler_fn
                    paperclip::actix::web::Resource::new(#uri)
                        .name(#handler_name_str)
                        .guard(actix_web::guard::#variant())
                        .route(paperclip::actix::web::#method().to(#handler_name))
                }
            }

            impl actix_web::dev::HttpServiceFactory for #handler_name {
                fn register(self, config: &mut actix_web::dev::AppService) {
                    Self::resource().register(config);
                }
            }

            impl paperclip::actix::Mountable for #handler_name {
                fn path(&self) -> &str {
                    #uri
                }

                fn operations(
                    &mut self,
                ) -> std::collections::BTreeMap<
                    paperclip::v2::models::HttpMethod,
                    paperclip::v2::models::DefaultOperationRaw,
                > {
                    Self::resource().operations()
                }

                fn definitions(
                    &mut self,
                ) -> std::collections::BTreeMap<
                    String,
                    paperclip::v2::models::DefaultSchemaRaw,
                > {
                    Self::resource().definitions()
                }

                fn security_definitions(
                    &mut self,
                ) -> std::collections::BTreeMap<String, paperclip::v2::models::SecurityScheme>
                {
                    Self::resource().security_definitions()
                }
            }
        })
    }
}

#[macro_use]
macro_rules! rest_methods {
    (
        $($variant:ident, $method:ident, )+
    ) => {
        /// All available Rest methods
        #[derive(Debug, PartialEq, Eq, Hash)]
        pub(crate) enum Method {
            $(
                $variant,
            )+
        }

        impl Method {
            fn method(&self) -> &'static str {
                match self {
                    $(Self::$variant => stringify!($method),)+
                }
            }
            fn variant(&self) -> &'static str {
                match self {
                    $(Self::$variant => stringify!($variant),)+
                }
            }
        }

        $(doc_comment! {
            concat!("
Creates route handler with `paperclip::actix::web::Resource", "`.
In order to control the output type and status codes the return value/response must implement the
trait actix_web::Responder.

# Syntax
```text
#[", stringify!($method), r#"("path"[, attributes])]
```

# Attributes
- `"path"` - Raw literal string with path for which to register handler.

# Example

/// use paperclip::actix::web::Json;
/// use paperclip_macros::"#, stringify!($method), ";
/// #[", stringify!($method), r#"("/")]
/// async fn example() {
/// }
"#);
            #[cfg(feature = "actix")]
            #[proc_macro_attribute]
            pub fn $method(attr: TokenStream, item: TokenStream) -> TokenStream {
                match Method::$variant.generate(attr, item) {
                    Ok(v) => v.into(),
                    Err(e) => e.to_compile_error().into(),
                }
            }
        })+
    };
}
