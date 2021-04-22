//! Paperclip is a OpenAPI code generator for efficient type-safe
//! compile-time checked HTTP APIs in Rust.
//!
//! See the [website](https://paperclip.waffles.space) for detailed
//! documentation and examples.

#[cfg_attr(feature = "codegen", macro_use)]
#[cfg(feature = "codegen")]
extern crate log;

mod error;
#[cfg(feature = "v2")]
pub mod v2;

pub use error::{PaperClipError, PaperClipResult};
pub use paperclip_core::util;
#[cfg(feature = "v2")]
pub use paperclip_macros::api_v2_schema_struct as api_v2_schema;

#[cfg(feature = "actix-base")]
pub mod actix {
    //! Plugin types, traits and macros for actix-web framework.

    pub use paperclip_actix::{
        api_v2_errors, api_v2_errors_overlay, api_v2_operation, delete, get, post, put, web,
        Apiv2Schema, Apiv2Security, App, Mountable, OpenApiExt,
    };
    pub use paperclip_core::v2::{
        AcceptedJson, CreatedJson, NoContent, OperationModifier, ResponderWrapper, ResponseWrapper,
    };
}
