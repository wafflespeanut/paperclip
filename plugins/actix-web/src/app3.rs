#[cfg(feature = "actix2")]
extern crate actix_web2 as actix_web;
#[cfg(feature = "actix3")]
extern crate actix_web3 as actix_web;

pub use super::web::{Resource, Route, Scope};
pub use paperclip_macros::{
    api_v2_errors, api_v2_operation, delete, get, post, put, Apiv2Schema, Apiv2Security,
};

use super::{
    web::{RouteWrapper, ServiceConfig},
    Mountable, SpecHandler,
};
use actix_service::ServiceFactory;
use actix_web::{
    dev::{HttpServiceFactory, MessageBody, ServiceRequest, ServiceResponse, Transform},
    web::HttpResponse,
    Error,
};
use futures::future::{ok as fut_ok, Ready};
use paperclip_core::v2::models::{DefaultApiRaw, SecurityScheme};
use parking_lot::RwLock;

use std::{fmt::Debug, future::Future, sync::Arc};

/// Wrapper for [`actix_web::App`](https://docs.rs/actix-web/*/actix_web/struct.App.html).
pub struct App<T, B> {
    pub(super) spec: Arc<RwLock<DefaultApiRaw>>,
    pub(super) inner: Option<actix_web::App<T, B>>,
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
            inner: self.inner.take().map(|a| a.wrap_fn(mw)),
        }
    }

    /// Mounts the specification for all operations and definitions
    /// recorded by the wrapper and serves them in the given path
    /// as a JSON.
    pub fn with_json_spec_at(mut self, path: &str) -> Self {
        self.inner = self.inner.take().map(|a| {
            a.service(
                actix_web::web::resource(path)
                    .route(actix_web::web::get().to(SpecHandler(self.spec.clone()))),
            )
        });
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

impl actix_web::dev::Factory<(), Ready<Result<HttpResponse, Error>>, Result<HttpResponse, Error>>
    for SpecHandler
{
    fn call(&self, _: ()) -> Ready<Result<HttpResponse, Error>> {
        fut_ok(HttpResponse::Ok().json(&*self.0.read()))
    }
}
