#[cfg(feature = "actix2")]
extern crate actix_web2 as actix_web;
#[cfg(feature = "actix3")]
extern crate actix_web3 as actix_web;

pub mod web;

pub use self::web::{Resource, Route, Scope};
pub use paperclip_macros::{api_v2_errors, api_v2_operation, Apiv2Schema, Apiv2Security};

use self::web::{RouteWrapper, ServiceConfig};
use actix_service::ServiceFactory;
use actix_web::dev::{HttpServiceFactory, MessageBody, ServiceRequest, ServiceResponse, Transform};
use actix_web::{web::HttpResponse, Error};
use futures::future::{ok as fut_ok, Ready};
use paperclip_core::v2::models::{
    DefaultApiRaw, DefaultOperationRaw, DefaultPathItemRaw, DefaultSchemaRaw, HttpMethod,
    SecurityScheme,
};
use parking_lot::RwLock;

#[cfg(feature = "swagger-ui")]
use tinytemplate::TinyTemplate;

#[cfg(feature = "swagger-ui")]
use serde::Serialize;

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;

#[cfg(feature = "swagger-ui")]
static TEMPLATE: &str = include_str!("../../templates/swagger-ui.html");

/// Wrapper for [`actix_web::App`](https://docs.rs/actix-web/*/actix_web/struct.App.html).
pub struct App<T, B> {
    spec: Arc<RwLock<DefaultApiRaw>>,
    spec_path: Option<String>,
    inner: Option<actix_web::App<T, B>>,
}

/// Extension trait for actix-web applications.
pub trait OpenApiExt<T, B> {
    type Wrapper;

    /// Consumes this app and produces its wrapper to start tracking
    /// paths and their corresponding operations.
    fn wrap_api(self) -> Self::Wrapper;

    /// Same as `wrap_api` initializing with provided specification
    /// defaults. Useful for defining Api properties outside of definitions and
    /// paths.
    fn wrap_api_with_spec(self, spec: DefaultApiRaw) -> Self::Wrapper;
}

impl<T, B> OpenApiExt<T, B> for actix_web::App<T, B> {
    type Wrapper = App<T, B>;

    fn wrap_api(self) -> Self::Wrapper {
        App {
            spec: Arc::new(RwLock::new(DefaultApiRaw::default())),
            spec_path: None,
            inner: Some(self),
        }
    }

    fn wrap_api_with_spec(self, spec: DefaultApiRaw) -> Self::Wrapper {
        App {
            spec: Arc::new(RwLock::new(spec)),
            spec_path: None,
            inner: Some(self),
        }
    }
}

/// Indicates that this thingmabob has a path and a bunch of definitions and operations.
pub trait Mountable {
    /// Where this thing gets mounted.
    fn path(&self) -> &str;

    /// Map of HTTP methods and the associated API operations.
    fn operations(&mut self) -> BTreeMap<HttpMethod, DefaultOperationRaw>;

    /// The definitions recorded by this object.
    fn definitions(&mut self) -> BTreeMap<String, DefaultSchemaRaw>;

    /// The security definitions recorded by this object.
    fn security_definitions(&mut self) -> BTreeMap<String, SecurityScheme>;

    /// Updates the given map of operations with operations tracked by this object.
    ///
    /// **NOTE:** Overriding implementations must ensure that the `PathItem`
    /// is normalized before updating the input map.
    fn update_operations(&mut self, map: &mut BTreeMap<String, DefaultPathItemRaw>) {
        let op_map = map
            .entry(self.path().into())
            .or_insert_with(Default::default);
        op_map.methods.extend(self.operations().into_iter());
    }
}

impl<T, B> App<T, B>
where
    B: MessageBody,
    T: ServiceFactory<
        Config = (),
        Request = ServiceRequest,
        Response = ServiceResponse<B>,
        Error = Error,
        InitError = (),
    >,
{
    /// Proxy for [`actix_web::App::data`](https://docs.rs/actix-web/*/actix_web/struct.App.html#method.data).
    ///
    /// **NOTE:** This doesn't affect spec generation.
    pub fn data<U: 'static>(mut self, data: U) -> Self {
        self.inner = self.inner.take().map(|a| a.data(data));
        self
    }

    /// Proxy for [`actix_web::App::data_factory`](https://docs.rs/actix-web/*/actix_web/struct.App.html#method.data_factory).
    ///
    /// **NOTE:** This doesn't affect spec generation.
    pub fn data_factory<F, Out, D, E>(mut self, data: F) -> Self
    where
        F: Fn() -> Out + 'static,
        Out: Future<Output = Result<D, E>> + 'static,
        D: 'static,
        E: Debug,
    {
        self.inner = self.inner.take().map(|a| a.data_factory(data));
        self
    }

    /// Proxy for [`actix_web::App::app_data`](https://docs.rs/actix-web/*/actix_web/struct.App.html#method.app_data).
    ///
    /// **NOTE:** This doesn't affect spec generation.
    pub fn app_data<U: 'static>(mut self, data: U) -> Self {
        self.inner = self.inner.take().map(|a| a.app_data(data));
        self
    }

    /// Wrapper for [`actix_web::App::configure`](https://docs.rs/actix-web/*/actix_web/struct.App.html#method.configure).
    pub fn configure<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut ServiceConfig),
    {
        self.inner = self.inner.take().map(|s| {
            s.configure(|c| {
                let mut cfg = ServiceConfig::from(c);
                f(&mut cfg);
                self.update_from_mountable(&mut cfg);
            })
        });
        self
    }

    /// Wrapper for [`actix_web::App::route`](https://docs.rs/actix-web/*/actix_web/struct.App.html#method.route).
    pub fn route(mut self, path: &str, route: Route) -> Self {
        let mut w = RouteWrapper::from(path, route);
        self.update_from_mountable(&mut w);
        self.inner = self.inner.take().map(|a| a.route(path, w.inner));
        self
    }

    /// Wrapper for [`actix_web::App::service`](https://docs.rs/actix-web/*/actix_web/struct.App.html#method.service).
    pub fn service<F>(mut self, mut factory: F) -> Self
    where
        F: Mountable + HttpServiceFactory + 'static,
    {
        self.update_from_mountable(&mut factory);
        self.inner = self.inner.take().map(|a| a.service(factory));
        self
    }

    /// Proxy for [`actix_web::App::default_service`](https://docs.rs/actix-web/*/actix_web/struct.App.html#method.default_service).
    ///
    /// **NOTE:** This doesn't affect spec generation.
    pub fn default_service<F, U>(mut self, f: F) -> Self
    where
        F: actix_service::IntoServiceFactory<U>,
        U: ServiceFactory<
                Config = (),
                Request = ServiceRequest,
                Response = ServiceResponse,
                Error = Error,
                InitError = (),
            > + 'static,
        U::InitError: Debug,
    {
        self.inner = self.inner.take().map(|a| a.default_service(f));
        self
    }

    /// Proxy for [`actix_web::App::external_resource`](https://docs.rs/actix-web/*/actix_web/struct.App.html#method.external_resource).
    ///
    /// **NOTE:** This doesn't affect spec generation.
    pub fn external_resource<N, U>(mut self, name: N, url: U) -> Self
    where
        N: AsRef<str>,
        U: AsRef<str>,
    {
        self.inner = self.inner.take().map(|a| a.external_resource(name, url));
        self
    }

    /// Proxy for [`actix_web::web::App::wrap`](https://docs.rs/actix-web/*/actix_web/struct.App.html#method.wrap).
    ///
    /// **NOTE:** This doesn't affect spec generation.
    pub fn wrap<M, B1>(
        mut self,
        mw: M,
    ) -> App<
        impl ServiceFactory<
            Config = (),
            Request = ServiceRequest,
            Response = ServiceResponse<B1>,
            Error = Error,
            InitError = (),
        >,
        B1,
    >
    where
        M: Transform<
            T::Service,
            Request = ServiceRequest,
            Response = ServiceResponse<B1>,
            Error = Error,
            InitError = (),
        >,
        B1: MessageBody,
    {
        App {
            spec: self.spec,
            spec_path: self.spec_path,
            inner: self.inner.take().map(|a| a.wrap(mw)),
        }
    }

    /// Proxy for [`actix_web::web::App::wrap_fn`](https://docs.rs/actix-web/*/actix_web/struct.App.html#method.wrap_fn).
    ///
    /// **NOTE:** This doesn't affect spec generation.
    pub fn wrap_fn<B1, F, R>(
        mut self,
        mw: F,
    ) -> App<
        impl ServiceFactory<
            Config = (),
            Request = ServiceRequest,
            Response = ServiceResponse<B1>,
            Error = Error,
            InitError = (),
        >,
        B1,
    >
    where
        B1: MessageBody,
        F: FnMut(ServiceRequest, &mut T::Service) -> R + Clone,
        R: Future<Output = Result<ServiceResponse<B1>, Error>>,
    {
        App {
            spec: self.spec,
            spec_path: self.spec_path,
            inner: self.inner.take().map(|a| a.wrap_fn(mw)),
        }
    }

    /// Mounts the specification for all operations and definitions
    /// recorded by the wrapper and serves them in the given path
    /// as a JSON.
    pub fn with_json_spec_at(mut self, path: &str) -> Self {
        self.spec_path = Some(path.to_owned());
        self.inner = self.inner.take().map(|a| {
            a.service(
                actix_web::web::resource(path)
                    .route(actix_web::web::get().to(SpecHandler(self.spec.clone()))),
            )
        });
        self
    }

    /// Exposes the previously built JSON specification with Swagger UI at the given path
    ///
    /// **NOTE:** you **MUST** call with_json_spec_at before calling this function
    #[cfg(feature = "swagger-ui")]
    pub fn with_swagger_ui_at(mut self, path: &str) -> Self {
        self.inner = match self.spec_path.clone() {
            Some(spec_path) => self.inner.take().map(|a| {
                let mut tt = TinyTemplate::new();
                let context = SwaggerUIContext {
                    api_spec_url: spec_path,
                };
                // This is safe as we are using a given file as input so no errors can occur
                let _ = tt.add_template("swagger-ui", TEMPLATE);
                let rendered = tt.render("swagger-ui", &context);
                match rendered {
                    Ok(r) => {
                        a.service(
                            actix_web::web::resource(path.to_owned())
                                .route(actix_web::web::get().to(SwaggerUIHandler(r))),
                        )
                    },
                    Err(_) => {
                        a.service(
                            actix_web::web::resource(path.to_owned()).route(
                            actix_web::web::get().to(|| {
                                HttpResponse::InternalServerError().body(
                                    "An error occurred while rendering Swagger-UI",
                                )
                            }),
                        ))
                    }
                }

            }),
            None => self.inner.take().map(|a| {
                a.service(actix_web::web::resource(path.to_owned()).route(
                    actix_web::web::get().to(|| {
                        HttpResponse::NotFound().body(
                            "JSON specification not found.\nYou have to run with_json_spec_at(path) first.",
                        )
                    }),
                ))
            }),
        };
        self
    }

    /// Calls the given function with `App` and JSON `Value` representing your API
    /// specification **built until now**.
    ///
    /// **NOTE:** Unlike `with_json_spec_at`, this only has the API spec built until
    /// this function call. Any route handler added after this call won't affect the
    /// spec. So, it's important to call this function after adding all route handlers.
    pub fn with_raw_json_spec<F>(self, mut call: F) -> Self
    where
        F: FnMut(Self, serde_json::Value) -> Self,
    {
        let spec = serde_json::to_value(&*self.spec.read()).expect("generating json spec");
        call(self, spec)
    }

    /// Builds and returns the `actix_web::App`.
    pub fn build(self) -> actix_web::App<T, B> {
        self.inner.expect("missing app?")
    }

    /// Updates the underlying spec with definitions and operations from the given factory.
    fn update_from_mountable<F>(&mut self, factory: &mut F)
    where
        F: Mountable,
    {
        let mut api = self.spec.write();
        api.definitions.extend(factory.definitions().into_iter());
        SecurityScheme::append_map(
            factory.security_definitions(),
            &mut api.security_definitions,
        );
        factory.update_operations(&mut api.paths);
        if cfg!(feature = "normalize") {
            for map in api.paths.values_mut() {
                map.normalize();
            }
        }
    }
}

#[derive(Clone)]
struct SpecHandler(Arc<RwLock<DefaultApiRaw>>);

impl actix_web::dev::Factory<(), Ready<Result<HttpResponse, Error>>, Result<HttpResponse, Error>>
    for SpecHandler
{
    fn call(&self, _: ()) -> Ready<Result<HttpResponse, Error>> {
        fut_ok(HttpResponse::Ok().json(&*self.0.read()))
    }
}

#[cfg(feature = "swagger-ui")]
#[derive(Clone)]
struct SwaggerUIHandler(String);

#[cfg(feature = "swagger-ui")]
#[derive(Serialize)]
struct SwaggerUIContext {
    api_spec_url: String,
}

#[cfg(feature = "swagger-ui")]
impl actix_web::dev::Factory<(), Ready<Result<HttpResponse, Error>>, Result<HttpResponse, Error>>
    for SwaggerUIHandler
{
    fn call(&self, _: ()) -> Ready<Result<HttpResponse, Error>> {
        fut_ok(
            HttpResponse::Ok()
                .content_type("text/html")
                .body(self.0.to_owned()),
        )
    }
}
