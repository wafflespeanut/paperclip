#[cfg(feature = "actix-multipart")]
use super::schema::TypedData;
use super::{
    models::{
        DefaultOperationRaw, DefaultResponseRaw, DefaultSchemaRaw, Either, Items, Parameter,
        ParameterIn, Response, SecurityScheme,
    },
    schema::{Apiv2Errors, Apiv2Operation, Apiv2Schema},
};
use crate::util::{ready, Ready};
use actix_web::{
    http::StatusCode,
    web::{Bytes, Data, Form, Json, Path, Payload, Query},
    Error, HttpRequest, HttpResponse, Responder,
};
use pin_project::pin_project;

use serde::Serialize;
#[cfg(feature = "serde_qs")]
use serde_qs::actix::QsQuery;

use std::{
    collections::BTreeMap,
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Actix-specific trait for indicating that this entity can modify an operation
/// and/or update the global map of definitions.
pub trait OperationModifier: Apiv2Schema + Sized {
    /// Update the parameters list in the given operation (if needed).
    fn update_parameter(_op: &mut DefaultOperationRaw) {}

    /// Update the responses map in the given operation (if needed).
    fn update_response(_op: &mut DefaultOperationRaw) {}

    /// Update the definitions map (if needed).
    fn update_definitions(map: &mut BTreeMap<String, DefaultSchemaRaw>) {
        update_definitions_from_schema_type::<Self>(map);
    }

    /// Update the security map in the given operation (if needed).
    fn update_security(op: &mut DefaultOperationRaw) {
        update_security::<Self>(op);
    }

    /// Update the security definition map (if needed).
    fn update_security_definitions(map: &mut BTreeMap<String, SecurityScheme>) {
        update_security_definitions::<Self>(map);
    }
}

/// All schema types default to updating the definitions map.
#[cfg(feature = "nightly")]
impl<T> OperationModifier for T
where
    T: Apiv2Schema,
{
    default fn update_parameter(_op: &mut DefaultOperationRaw) {}

    default fn update_response(_op: &mut DefaultOperationRaw) {}

    default fn update_definitions(map: &mut BTreeMap<String, DefaultSchemaRaw>) {
        update_definitions_from_schema_type::<Self>(map);
    }

    default fn update_security(op: &mut DefaultOperationRaw) {
        update_security::<Self>(op);
    }

    default fn update_security_definitions(map: &mut BTreeMap<String, SecurityScheme>) {
        update_security_definitions::<Self>(map);
    }
}

impl<T> OperationModifier for Option<T>
where
    T: OperationModifier,
{
    fn update_parameter(op: &mut DefaultOperationRaw) {
        T::update_parameter(op);
    }

    fn update_response(op: &mut DefaultOperationRaw) {
        T::update_response(op);
    }

    fn update_definitions(map: &mut BTreeMap<String, DefaultSchemaRaw>) {
        T::update_definitions(map);
    }

    fn update_security(op: &mut DefaultOperationRaw) {
        T::update_security(op);
    }

    fn update_security_definitions(map: &mut BTreeMap<String, SecurityScheme>) {
        T::update_security_definitions(map);
    }
}

#[cfg(feature = "nightly")]
impl<T, E> OperationModifier for Result<T, E>
where
    T: OperationModifier,
{
    default fn update_parameter(op: &mut DefaultOperationRaw) {
        T::update_parameter(op);
    }

    default fn update_response(op: &mut DefaultOperationRaw) {
        T::update_response(op);
    }

    default fn update_definitions(map: &mut BTreeMap<String, DefaultSchemaRaw>) {
        T::update_definitions(map);
    }

    default fn update_security_definitions(map: &mut BTreeMap<String, SecurityScheme>) {
        T::update_security_definitions(map);
    }
}

impl<T, E> OperationModifier for Result<T, E>
where
    T: OperationModifier,
    E: Apiv2Errors,
{
    fn update_parameter(op: &mut DefaultOperationRaw) {
        T::update_parameter(op);
    }

    fn update_response(op: &mut DefaultOperationRaw) {
        T::update_response(op);
        update_error_definitions_from_schema_type::<E>(op);
    }

    fn update_definitions(map: &mut BTreeMap<String, DefaultSchemaRaw>) {
        T::update_definitions(map);
    }

    fn update_security_definitions(map: &mut BTreeMap<String, SecurityScheme>) {
        T::update_security_definitions(map);
    }
}

// We don't know what we should do with these abstractions
// as they could be anything.
impl<T> Apiv2Schema for Data<T> {}
#[cfg(not(feature = "nightly"))]
impl<T> OperationModifier for Data<T> {}

macro_rules! impl_empty({ $($ty:ty),+ } => {
    $(
        impl Apiv2Schema for $ty {}
        #[cfg(not(feature = "nightly"))]
        impl OperationModifier for $ty {}
    )+
});

impl Apiv2Operation for HttpResponse {
    fn operation() -> DefaultOperationRaw {
        Default::default()
    }

    fn security_definitions() -> BTreeMap<String, SecurityScheme> {
        Default::default()
    }

    fn definitions() -> BTreeMap<String, DefaultSchemaRaw> {
        Default::default()
    }
}

impl_empty!(HttpRequest, HttpResponse, Bytes, Payload);

#[cfg(not(feature = "nightly"))]
mod manual_impl {
    use super::OperationModifier;

    impl<'a> OperationModifier for &'a str {}
    impl<'a, T: OperationModifier> OperationModifier for &'a [T] {}

    macro_rules! impl_simple({ $ty:ty } => {
        impl OperationModifier for $ty {}
    });

    impl_simple!(char);
    impl_simple!(String);
    impl_simple!(bool);
    impl_simple!(f32);
    impl_simple!(f64);
    impl_simple!(i8);
    impl_simple!(i16);
    impl_simple!(i32);
    impl_simple!(u8);
    impl_simple!(u16);
    impl_simple!(u32);
    impl_simple!(i64);
    impl_simple!(i128);
    impl_simple!(isize);
    impl_simple!(u64);
    impl_simple!(u128);
    impl_simple!(usize);
    #[cfg(feature = "chrono")]
    impl_simple!(chrono::NaiveDateTime);
    #[cfg(feature = "rust_decimal")]
    impl_simple!(rust_decimal::Decimal);
    #[cfg(feature = "uuid")]
    impl_simple!(uuid::Uuid);
}

#[cfg(feature = "chrono")]
impl<T: chrono::TimeZone> OperationModifier for chrono::DateTime<T> {}

// Other extractors

#[cfg(feature = "nightly")]
impl<T> Apiv2Schema for Json<T> {
    default const NAME: Option<&'static str> = None;

    default fn raw_schema() -> DefaultSchemaRaw {
        Default::default()
    }
}

/// JSON needs specialization because it updates the global definitions.
impl<T: Apiv2Schema> Apiv2Schema for Json<T> {
    const NAME: Option<&'static str> = T::NAME;

    fn raw_schema() -> DefaultSchemaRaw {
        T::raw_schema()
    }
}

impl<T> OperationModifier for Json<T>
where
    T: Apiv2Schema,
{
    fn update_parameter(op: &mut DefaultOperationRaw) {
        op.parameters.push(Either::Right(Parameter {
            description: None,
            in_: ParameterIn::Body,
            name: "body".into(),
            required: true,
            schema: Some({
                let mut def = T::schema_with_ref();
                def.retain_ref();
                def
            }),
            ..Default::default()
        }));
    }

    fn update_response(op: &mut DefaultOperationRaw) {
        op.responses.insert(
            "200".into(),
            Either::Right(Response {
                // TODO: Support configuring other 2xx codes using macro attribute.
                description: Some("OK".into()),
                schema: Some({
                    let mut def = T::schema_with_ref();
                    def.retain_ref();
                    def
                }),
                ..Default::default()
            }),
        );
    }
}

#[cfg(feature = "actix-multipart")]
impl OperationModifier for actix_multipart::Multipart {
    fn update_parameter(op: &mut DefaultOperationRaw) {
        op.parameters.push(Either::Right(Parameter {
            description: None,
            in_: ParameterIn::FormData,
            name: "file_data".into(),
            required: true,
            data_type: Some(<actix_multipart::Multipart as TypedData>::data_type()),
            format: <actix_multipart::Multipart as TypedData>::format(),
            ..Default::default()
        }));
    }
}

#[cfg(feature = "actix-session")]
impl OperationModifier for actix_session::Session {
    fn update_definitions(_map: &mut BTreeMap<String, DefaultSchemaRaw>) {}
}

#[cfg(feature = "actix-files")]
impl OperationModifier for actix_files::NamedFile {
    fn update_definitions(_map: &mut BTreeMap<String, DefaultSchemaRaw>) {}
}

macro_rules! impl_param_extractor ({ $ty:ty => $container:ident } => {
    #[cfg(feature = "nightly")]
    impl<T> Apiv2Schema for $ty {
        default const NAME: Option<&'static str> = None;

        default fn raw_schema() -> DefaultSchemaRaw {
            Default::default()
        }
    }

    #[cfg(not(feature = "nightly"))]
    impl<T: Apiv2Schema> Apiv2Schema for $ty {}

    impl<T: Apiv2Schema> OperationModifier for $ty {
        fn update_parameter(op: &mut DefaultOperationRaw) {
            let def = T::raw_schema();
            // If there aren't any properties and if it's a path parameter,
            // then add a parameter whose name will be overridden later.
            if def.properties.is_empty() && ParameterIn::$container == ParameterIn::Path {
                op.parameters.push(Either::Right(Parameter {
                    name: String::new(),
                    in_: ParameterIn::Path,
                    required: true,
                    data_type: def.data_type,
                    format: def.format,
                    enum_: def.enum_,
                    description: def.description,
                    ..Default::default()
                }));
            }
            for (k, v) in def.properties {
                op.parameters.push(Either::Right(Parameter {
                    in_: ParameterIn::$container,
                    required: def.required.contains(&k),
                    data_type: v.data_type,
                    format: v.format,
                    enum_: v.enum_,
                    description: v.description,
                    collection_format: None, // this defaults to csv
                    items: v.items.as_deref().map(map_schema_to_items),
                    name: k,
                    ..Default::default()
                }));
            }
        }

        // These don't require updating definitions, as we use them only
        // to get their properties.
        fn update_definitions(_map: &mut BTreeMap<String, DefaultSchemaRaw>) {}
    }
});

fn map_schema_to_items(schema: &DefaultSchemaRaw) -> Items {
    Items {
        data_type: schema.data_type,
        format: schema.format.clone(),
        collection_format: None, // this defaults to csv
        enum_: schema.enum_.clone(),
        items: schema
            .items
            .as_deref()
            .map(|schema| Box::new(map_schema_to_items(schema))),
        ..Default::default() // range fields are not emitted
    }
}

/// `formData` can refer to the global definitions.
#[cfg(feature = "nightly")]
impl<T: Apiv2Schema> Apiv2Schema for Form<T> {
    const NAME: Option<&'static str> = T::NAME;

    fn raw_schema() -> DefaultSchemaRaw {
        T::raw_schema()
    }
}

impl_param_extractor!(Path<T> => Path);
impl_param_extractor!(Query<T> => Query);
impl_param_extractor!(Form<T> => FormData);
#[cfg(feature = "serde_qs")]
impl_param_extractor!(QsQuery<T> => Query);

macro_rules! impl_path_tuple ({ $($ty:ident),+ } => {
    #[cfg(feature = "nightly")]
    impl<$($ty,)+> Apiv2Schema for Path<($($ty,)+)> {}

    #[cfg(not(feature = "nightly"))]
    impl<$($ty: Apiv2Schema,)+> Apiv2Schema for Path<($($ty,)+)> {}

    impl<$($ty,)+> OperationModifier for Path<($($ty,)+)>
        where $($ty: Apiv2Schema,)+
    {
        fn update_parameter(op: &mut DefaultOperationRaw) {
            $(
                let def = $ty::raw_schema();
                if def.properties.is_empty() {
                    op.parameters.push(Either::Right(Parameter {
                        // NOTE: We're setting empty name, because we don't know
                        // the name in this context. We'll get it when we add services.
                        name: String::new(),
                        in_: ParameterIn::Path,
                        required: true,
                        data_type: def.data_type,
                        format: def.format,
                        enum_: def.enum_,
                        description: def.description,
                        ..Default::default()
                    }));
                }
                for (k, v) in def.properties {
                    op.parameters.push(Either::Right(Parameter {
                        in_: ParameterIn::Path,
                        required: def.required.contains(&k),
                        data_type: v.data_type,
                        format: v.format,
                        enum_: v.enum_,
                        description: v.description,
                        collection_format: None, // this defaults to csv
                        items: v.items.as_deref().map(map_schema_to_items),
                        name: k,
                        ..Default::default()
                    }));
                }
            )+
        }
    }
});

impl_path_tuple!(A);
impl_path_tuple!(A, B);
impl_path_tuple!(A, B, C);
impl_path_tuple!(A, B, C, D);
impl_path_tuple!(A, B, C, D, E);

/// Wrapper for wrapping over `impl Responder` thingies (to avoid breakage).
pub struct ResponderWrapper<T>(pub T);

#[cfg(feature = "nightly")]
impl<T: Responder> Apiv2Schema for ResponderWrapper<T> {
    default const NAME: Option<&'static str> = None;

    default fn raw_schema() -> DefaultSchemaRaw {
        DefaultSchemaRaw::default()
    }
}

#[cfg(not(feature = "nightly"))]
impl<T: Responder> Apiv2Schema for ResponderWrapper<T> {}

#[cfg(not(feature = "nightly"))]
impl<T: Responder> OperationModifier for ResponderWrapper<T> {}

impl<T: Responder> Responder for ResponderWrapper<T> {
    type Error = T::Error;
    type Future = T::Future;

    #[inline]
    fn respond_to(self, req: &HttpRequest) -> Self::Future {
        self.0.respond_to(req)
    }
}

/// Wrapper for all response types from handlers. This holds the actual value
/// returned by the handler and a unit struct (autogenerated by the plugin) which
/// is used for generating operation information.
#[pin_project]
pub struct ResponseWrapper<T, H>(#[pin] pub T, pub H);

impl<T: Responder, H> Responder for ResponseWrapper<T, H> {
    type Error = <T as Responder>::Error;
    type Future = <T as Responder>::Future;

    #[inline]
    fn respond_to(self, req: &HttpRequest) -> Self::Future {
        self.0.respond_to(req)
    }
}

impl<F, T, H> Future for ResponseWrapper<F, H>
where
    F: Future<Output = T>,
    T: OperationModifier + Responder,
    H: Apiv2Operation,
{
    type Output = T;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().project();
        this.0.poll(ctx)
    }
}

impl<F, T, H> Apiv2Operation for ResponseWrapper<F, H>
where
    F: Future<Output = T>,
    T: OperationModifier + Responder,
    H: Apiv2Operation,
{
    fn operation() -> DefaultOperationRaw {
        H::operation()
    }

    fn security_definitions() -> BTreeMap<String, SecurityScheme> {
        H::security_definitions()
    }

    fn definitions() -> BTreeMap<String, DefaultSchemaRaw> {
        H::definitions()
    }
}

/// Given the schema type, recursively update the map of definitions.
fn update_definitions_from_schema_type<T>(map: &mut BTreeMap<String, DefaultSchemaRaw>)
where
    T: Apiv2Schema,
{
    let mut schema = T::schema_with_ref();
    loop {
        if let Some(s) = schema.items {
            schema = *s;
            continue;
        } else if let Some(Either::Right(s)) = schema.extra_props {
            schema = *s;
            continue;
        } else if let Some(n) = schema.name.take() {
            schema.remove_refs();
            map.insert(n, schema);
        }

        break;
    }
}

/// Given a schema type that represents an error, add the responses
/// representing those errors.
fn update_error_definitions_from_schema_type<T>(op: &mut DefaultOperationRaw)
where
    T: Apiv2Errors,
{
    for (status, def_name) in T::ERROR_MAP {
        let response = DefaultResponseRaw {
            description: Some((*def_name).to_string()),
            ..Default::default()
        };
        op.responses
            .insert(status.to_string(), Either::Right(response));
    }
}

/// Add security requirements to operation.
fn update_security<T>(op: &mut DefaultOperationRaw)
where
    T: Apiv2Schema,
{
    if let (Some(name), Some(scheme)) = (T::NAME, T::security_scheme()) {
        let mut security_map = BTreeMap::new();
        let scopes = scheme.scopes.keys().map(String::clone).collect();
        security_map.insert(name.into(), scopes);
        op.security.push(security_map);
    }
}

/// Merge security scheme into existing security definitions or add new.
fn update_security_definitions<T>(map: &mut BTreeMap<String, SecurityScheme>)
where
    T: Apiv2Schema,
{
    if let (Some(name), Some(new)) = (T::NAME, T::security_scheme()) {
        new.update_definitions(name, map);
    }
}

macro_rules! json_with_status {
    ($name:ident => $status:expr) => {
        pub struct $name<T: Serialize + Apiv2Schema>(pub T);

        impl<T> fmt::Debug for $name<T>
        where
            T: fmt::Debug + Serialize + Apiv2Schema,
        {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let status: StatusCode = $status;
                let status_str = status.canonical_reason().unwrap_or(status.as_str());
                write!(f, "{} Json: {:?}", status_str, self.0)
            }
        }

        impl<T> fmt::Display for $name<T>
        where
            T: fmt::Display + Serialize + Apiv2Schema,
        {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl<T> Responder for $name<T>
        where
            T: Serialize + Apiv2Schema,
        {
            type Error = Error;
            type Future = Ready<Result<HttpResponse, Error>>;

            fn respond_to(self, _: &HttpRequest) -> Self::Future {
                let status: StatusCode = $status;
                let body = match serde_json::to_string(&self.0) {
                    Ok(body) => body,
                    Err(e) => return ready(Err(e.into())),
                };

                ready(Ok(HttpResponse::build(status)
                    .content_type("application/json")
                    .body(body)))
            }
        }

        impl<T> Apiv2Schema for $name<T>
        where
            T: Serialize + Apiv2Schema,
        {
            const NAME: Option<&'static str> = T::NAME;

            fn raw_schema() -> DefaultSchemaRaw {
                T::raw_schema()
            }
        }

        impl<T> OperationModifier for $name<T>
        where
            T: Serialize + Apiv2Schema,
        {
            fn update_response(op: &mut DefaultOperationRaw) {
                let status: StatusCode = $status;
                op.responses.insert(
                    status.as_str().into(),
                    Either::Right(Response {
                        description: status.canonical_reason().map(ToString::to_string),
                        schema: Some({
                            let mut def = T::schema_with_ref();
                            def.retain_ref();
                            def
                        }),
                        ..Default::default()
                    }),
                );
            }
        }
    };
}

json_with_status!(CreatedJson => StatusCode::CREATED);
json_with_status!(AcceptedJson => StatusCode::ACCEPTED);

#[derive(Debug)]
pub struct NoContent;

impl fmt::Display for NoContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("No Content")
    }
}

impl Responder for NoContent {
    type Error = Error;
    type Future = Ready<Result<HttpResponse, Error>>;

    fn respond_to(self, _: &HttpRequest) -> Self::Future {
        ready(Ok(HttpResponse::build(StatusCode::NO_CONTENT)
            .content_type("application/json")
            .finish()))
    }
}

impl Apiv2Schema for NoContent {}

impl OperationModifier for NoContent {
    fn update_response(op: &mut DefaultOperationRaw) {
        let status = StatusCode::NO_CONTENT;
        op.responses.insert(
            status.as_str().into(),
            Either::Right(Response {
                description: status.canonical_reason().map(ToString::to_string),
                schema: None,
                ..Default::default()
            }),
        );
    }
}
