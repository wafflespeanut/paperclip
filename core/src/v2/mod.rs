//! Core types and traits associated with the
//! [OpenAPI v2 specification](https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md).

#[cfg(feature = "actix4")]
mod actix;

#[cfg(any(feature = "actix2", feature = "actix3"))]
mod actix3;
#[cfg(any(feature = "actix2", feature = "actix3"))]
use actix3 as actix;

mod extensions;
pub mod models;
#[cfg(feature = "codegen")]
mod resolver;
pub mod schema;

#[cfg(feature = "actix-base")]
pub use self::actix::{
    AcceptedJson, CreatedJson, NoContent, OperationModifier, ResponderWrapper, ResponseWrapper,
};

pub use self::{
    models::{DefaultSchema, ResolvableApi},
    schema::Schema,
};
pub use paperclip_macros::*;

#[cfg(feature = "codegen")]
use self::resolver::Resolver;
#[cfg(feature = "codegen")]
use crate::error::ValidationError;

#[cfg(feature = "codegen")]
impl<S: Schema + Default> ResolvableApi<S> {
    /// Consumes this API schema, resolves the references and returns
    /// the resolved schema.
    ///
    /// This walks recursively, collects the referenced schema objects,
    /// substitutes the referenced IDs with the pointer to schema objects
    /// and returns the resolved object or an error if it encountered one.
    pub fn resolve(self) -> Result<ResolvableApi<S>, ValidationError> {
        let mut resolver = Resolver::from((
            self.definitions,
            self.paths,
            self.parameters,
            self.responses,
        ));
        resolver.resolve()?;
        Ok(ResolvableApi {
            swagger: self.swagger,
            info: self.info,
            definitions: resolver.defs,
            paths: resolver.paths,
            base_path: self.base_path,
            host: self.host,
            schemes: self.schemes,
            consumes: self.consumes,
            produces: self.produces,
            coders: self.coders,
            support_crates: self.support_crates,
            parameters: resolver.params,
            responses: resolver.resp,
            spec_format: self.spec_format,
            external_docs: self.external_docs,
            security: self.security,
            security_definitions: self.security_definitions,
            tags: self.tags,
        })
    }
}
