#[cfg(feature = "actix2")]
extern crate actix_web2 as actix_web;
#[cfg(feature = "actix3")]
extern crate actix_web3 as actix_web;
#[cfg(feature = "actix4")]
extern crate actix_web4 as actix_web;

#[cfg(not(feature = "actix4"))]
extern crate actix_service1 as actix_service;
#[cfg(feature = "actix4")]
extern crate actix_service2 as actix_service;

#[cfg(feature = "actix4")]
pub mod web;

#[cfg(not(feature = "actix4"))]
pub mod web3;
#[cfg(not(feature = "actix4"))]
pub use web3 as web;

#[cfg(feature = "actix4")]
pub mod app;

#[cfg(not(feature = "actix4"))]
pub mod app3;
#[cfg(not(feature = "actix4"))]
pub use app3 as app;

pub use self::{
    app::App,
    web::{Resource, Route, Scope},
};
pub use paperclip_macros::{
    api_v2_errors, api_v2_operation, delete, get, post, put, Apiv2Schema, Apiv2Security,
};

use paperclip_core::v2::models::{
    DefaultApiRaw, DefaultOperationRaw, DefaultPathItemRaw, DefaultSchemaRaw, HttpMethod,
    SecurityScheme,
};
use parking_lot::RwLock;

use std::{collections::BTreeMap, sync::Arc};

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
            inner: Some(self),
        }
    }

    fn wrap_api_with_spec(self, spec: DefaultApiRaw) -> Self::Wrapper {
        App {
            spec: Arc::new(RwLock::new(spec)),
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

#[derive(Clone)]
struct SpecHandler(Arc<RwLock<DefaultApiRaw>>);
