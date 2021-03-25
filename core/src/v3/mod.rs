#![cfg(feature = "v3")]
//! Conversion traits and helps functions that help converting openapi v2 types to openapi v3.
//! For the OpenAPI v3 types the crate `openapiv3` is used.

mod contact;
mod external_documentation;
mod header;
mod info;
mod license;
mod openapi;
mod operation;
mod parameter;
mod paths;
mod reference;
mod request_body;
mod response;
mod schema;
mod security_scheme;
mod tag;

use super::v2::{models as v2, models::Either};

use parameter::non_body_parameter_to_v3_parameter;
use reference::invalid_referenceor;
use response::OperationEitherResponse;

/// Convert this crates openapi v2 (`DefaultApiRaw`) to `openapiv3::OpenAPI`
pub fn openapiv2_to_v3(v2: v2::DefaultApiRaw) -> openapiv3::OpenAPI {
    openapiv3::OpenAPI::from(v2)
}
