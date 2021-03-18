//! Models used by OpenAPI v2.

pub use super::extensions::{
    Coder, Coders, MediaRange, JSON_CODER, JSON_MIME, YAML_CODER, YAML_MIME,
};

use super::schema::Schema;
use crate::error::ValidationError;
use once_cell::sync::Lazy;
use paperclip_macros::api_v2_schema_struct;
use regex::{Captures, Regex};

#[cfg(feature = "actix-base")]
use actix_web::http::Method;

use parking_lot::RwLock;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Display},
    ops::{Deref, DerefMut},
    sync::Arc,
};

/// Regex that can be used for fetching templated path parameters.
static PATH_TEMPLATE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{(.*?)\}").expect("path template regex"));

// Headers that have special meaning in OpenAPI. These cannot be used in header parameter.
// Ensure that they're all lowercase for case insensitive check.
const SPECIAL_HEADERS: &[&str] = &["content-type", "accept", "authorization"];

/// OpenAPI version.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum Version {
    #[serde(rename = "2.0")]
    V2,
}

/// Supported data types.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    Integer,
    Number,
    String,
    Boolean,
    Array,
    Object,
    File,
}

impl DataType {
    /// Checks if this is a primitive type.
    #[inline]
    pub fn is_primitive(self) -> bool {
        std::matches!(
            self,
            DataType::Integer | DataType::Number | DataType::String | DataType::Boolean
        )
    }
}

/// Supported data type formats.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DataTypeFormat {
    Int32,
    Int64,
    Float,
    Double,
    Byte,
    Binary,
    Date,
    #[serde(rename = "date-time")]
    DateTime,
    Password,
    Uuid,
    #[serde(other)]
    Other,
}

impl ToString for DataTypeFormat {
    fn to_string(&self) -> String {
        match self {
            DataTypeFormat::Int32 => "int32",
            DataTypeFormat::Int64 => "int64",
            DataTypeFormat::Float => "float",
            DataTypeFormat::Double => "double",
            DataTypeFormat::Byte => "byte",
            DataTypeFormat::Binary => "binary",
            DataTypeFormat::Date => "data",
            DataTypeFormat::DateTime => "datetime",
            DataTypeFormat::Password => "password",
            DataTypeFormat::Uuid => "uuid",
            // would be nice if Other was Other(String)
            DataTypeFormat::Other => "other",
        }
        .to_string()
    }
}

/// OpenAPI v2 spec which can be traversed and resolved for codegen.
pub type ResolvableApi<S> = Api<ResolvableParameter<S>, ResolvableResponse<S>, Resolvable<S>>;

/// OpenAPI v2 spec with defaults.
pub type DefaultApiRaw = Api<DefaultParameterRaw, DefaultResponseRaw, DefaultSchemaRaw>;

/// OpenAPI v2 (swagger) spec generic over parameter and schema.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#swagger-object
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Api<P, R, S> {
    pub swagger: Version,
    #[serde(default = "BTreeMap::new")]
    pub definitions: BTreeMap<String, S>,
    pub paths: BTreeMap<String, PathItem<P, R>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(rename = "basePath", skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub consumes: BTreeSet<MediaRange>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub produces: BTreeSet<MediaRange>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub schemes: BTreeSet<OperationProtocol>,
    #[serde(default = "BTreeMap::new", skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, P>,
    #[serde(default = "BTreeMap::new", skip_serializing_if = "BTreeMap::is_empty")]
    pub responses: BTreeMap<String, R>,
    #[serde(
        default,
        rename = "securityDefinitions",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub security_definitions: BTreeMap<String, SecurityScheme>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<BTreeMap<String, BTreeSet<String>>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
    #[serde(rename = "externalDocs", skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocs>,
    /// Extension for custom coders to be used for decoding API objects.
    ///
    /// An example for JSON would be:
    /// ```yaml
    /// x-rust-coders:
    ///   application/json:
    ///     encoder_path: serde_json::to_writer
    ///     decoder_path: serde_json::from_reader
    ///     any_value: serde_json::Value
    ///     error_path: serde_json::Error
    /// ```
    /// **NOTE:** JSON and YAML encodings are already supported.
    #[serde(
        default,
        rename = "x-rust-coders",
        skip_serializing_if = "<Coders as Deref>::Target::is_empty"
    )]
    pub coders: Coders,
    /// Additional crates that need to be added to the manifest.
    ///
    /// The key is the LHS of a dependency, which is the crate name.
    /// The value is the RHS of a crate's requirements as it would appear
    /// in the manifest. Note that the caller must add proper escaping
    /// wherever required.
    ///
    /// For example, the following are all valid:
    /// - `my_crate: "0.7"`
    /// - `my_crate: "{ git = \"git://foo.bar/repo\" }"`
    /// - `my_crate: "{ version = \"0.9\", features = [\"booya\"] }"`
    #[serde(
        default,
        rename = "x-rust-dependencies",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub support_crates: BTreeMap<String, String>,
    /// This field is set manually, because we don't know the format in which
    /// the spec was provided and we need to use this as the fallback encoding.
    #[serde(skip)]
    pub spec_format: SpecFormat,
    pub info: Info,
}

/// The format used by spec (JSON/YAML).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecFormat {
    Json,
    Yaml,
}

impl SpecFormat {
    /// The en/decoder used for this format.
    pub fn coder(self) -> Arc<Coder> {
        match self {
            SpecFormat::Json => JSON_CODER.clone(),
            SpecFormat::Yaml => YAML_CODER.clone(),
        }
    }

    /// The mime for this format.
    pub fn mime(self) -> &'static MediaRange {
        match self {
            SpecFormat::Json => &*JSON_MIME,
            SpecFormat::Yaml => &*YAML_MIME,
        }
    }
}

impl<P, R, S> Api<P, R, S> {
    /// Gets the parameters from the given path template and calls
    /// the given function with the parameter names.
    pub fn path_parameters_map(
        path: &str,
        mut f: impl FnMut(&str) -> Cow<'static, str>,
    ) -> Cow<'_, str> {
        PATH_TEMPLATE_REGEX.replace_all(path, |c: &Captures| f(&c[1]))
    }
}

use crate as paperclip; // hack for proc macro

/// Default schema if your schema doesn't have any custom fields.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#schemaObject
#[api_v2_schema_struct]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DefaultSchema;

/// Info object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#infoObject
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Info {
    pub version: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<Contact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,
}

/// Contact object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#contactObject
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Contact {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// License object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#licenseObject
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct License {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Security Scheme object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#security-scheme-object
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SecurityScheme {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(rename = "in", skip_serializing_if = "Option::is_none")]
    pub in_: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow: Option<String>,
    #[serde(rename = "authorizationUrl", skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,
    #[serde(rename = "tokenUrl", skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scopes: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl SecurityScheme {
    /// Adds or updates this definition to the map of security definitions.
    pub fn update_definitions(mut self, name: &str, map: &mut BTreeMap<String, SecurityScheme>) {
        if let Some(existing) = map.get_mut(name) {
            existing.name = existing.name.take().or(self.name);
            if !self.type_.is_empty() {
                existing.type_ = self.type_;
            }
            existing.in_ = existing.in_.take().or(self.in_);
            existing.flow = existing.flow.take().or(self.flow);
            existing.auth_url = existing.auth_url.take().or(self.auth_url);
            existing.token_url = existing.token_url.take().or(self.token_url);
            existing.scopes.append(&mut self.scopes);
            existing.description = existing.description.take().or(self.description);
            return;
        }

        map.insert(name.into(), self);
    }

    /// Appends one map to the other whilst merging individual scheme properties.
    pub fn append_map(
        old: BTreeMap<String, SecurityScheme>,
        new: &mut BTreeMap<String, SecurityScheme>,
    ) {
        for (name, def) in old {
            def.update_definitions(&name, new);
        }
    }
}

/// Tag object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#tag-object
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "externalDocs")]
    pub external_docs: Option<ExternalDocs>,
}

/// External Documentation object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#external-documentation-object
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExternalDocs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub url: String,
}

/// Path item that can be traversed and resolved for codegen.
pub type ResolvablePathItem<S> = PathItem<ResolvableParameter<S>, ResolvableResponse<S>>;

/// Path item with default parameter and response.
pub type DefaultPathItemRaw = PathItem<DefaultParameterRaw, DefaultResponseRaw>;

/// Path item object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#pathItemObject
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PathItem<P, R> {
    #[serde(flatten, default = "BTreeMap::default")]
    pub methods: BTreeMap<HttpMethod, Operation<P, R>>,
    #[serde(default = "Vec::default", skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Either<Reference, P>>,
}

impl<S> PathItem<Parameter<S>, Response<S>> {
    /// Normalizes this operation map.
    /// - Collects and removes parameters shared across operations
    /// and adds them to the list global to this map.
    pub fn normalize(&mut self) {
        // We're using `Option<BTreeSet>` over `BTreeSet` because we need to
        // differentiate between the first operation that we use for initial
        // value of the set and  an operation that doesn't have any parameters.
        let mut shared_params = None;
        for op in self.methods.values() {
            let params = op
                .parameters
                .iter()
                .map(|p| p.name.clone())
                .collect::<BTreeSet<_>>();
            if let Some(p) = shared_params.take() {
                shared_params = Some(&p & &params); // set intersection
            } else {
                shared_params = Some(params);
            }
        }

        let shared_params = match shared_params {
            Some(p) => p,
            None => return,
        };

        // FIXME: A parameter defined at path level could be overridden at
        // the operation level with a different type. We shouldn't remove such
        // path-level parameters.
        for name in &shared_params {
            for op in self.methods.values_mut() {
                let idx = op
                    .parameters
                    .iter()
                    .position(|p| p.name == name.as_str())
                    .expect("collected parameter missing?");
                let p = op.parameters.swap_remove(idx);
                if self
                    .parameters
                    .iter()
                    .find(|p| p.name == name.as_str())
                    .is_none()
                {
                    self.parameters.push(p);
                }
            }
        }
    }
}

/// Parameter that can be traversed and resolved for codegen.
pub type ResolvableParameter<S> = Arc<RwLock<Parameter<Resolvable<S>>>>;

/// Parameter with the default raw schema.
pub type DefaultParameterRaw = Parameter<DefaultSchemaRaw>;

/// Request parameter object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#parameterObject
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Parameter<S> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "in")]
    pub in_: ParameterIn,
    pub name: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<S>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub data_type: Option<DataType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<DataTypeFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Items>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_format: Option<CollectionFormat>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_empty_value: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f32>,
    #[serde(rename = "exclusiveMaximum", skip_serializing_if = "Option::is_none")]
    pub exclusive_maximum: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f32>,
    #[serde(rename = "exclusiveMinimum", skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum: Option<bool>,
    #[serde(rename = "maxLength", skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(rename = "minLength", skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(rename = "maxItems", skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u32>,
    #[serde(rename = "minItems", skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u32>,
    #[serde(default, rename = "uniqueItems", skip_serializing_if = "is_false")]
    pub unique_items: bool,
    #[serde(rename = "multipleOf", skip_serializing_if = "Option::is_none")]
    pub multiple_of: Option<f32>,
    #[serde(default, rename = "enum", skip_serializing_if = "Vec::is_empty")]
    pub enum_: Vec<serde_json::Value>,
}

/// Items object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#itemsObject
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Items {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub data_type: Option<DataType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<DataTypeFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Items>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_format: Option<CollectionFormat>,
    #[serde(default, rename = "enum", skip_serializing_if = "Vec::is_empty")]
    pub enum_: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f32>,
    #[serde(rename = "exclusiveMaximum", skip_serializing_if = "Option::is_none")]
    pub exclusive_maximum: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f32>,
    #[serde(rename = "exclusiveMinimum", skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum: Option<bool>,
    #[serde(rename = "maxLength", skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(rename = "minLength", skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(rename = "maxItems", skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u32>,
    #[serde(rename = "minItems", skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u32>,
    #[serde(rename = "uniqueItems", skip_serializing_if = "Option::is_none")]
    pub unique_items: Option<bool>,
    #[serde(rename = "multipleOf", skip_serializing_if = "Option::is_none")]
    pub multiple_of: Option<f32>,
}

impl<S> Parameter<Resolvable<S>>
where
    S: Schema,
{
    /// Checks the validity of this parameter using the relative URL
    /// path it's associated with.
    pub fn check(&self, path: &str) -> Result<(), ValidationError> {
        if self.in_ == ParameterIn::Body {
            // Body parameter must specify a schema.
            if self.schema.is_none() {
                return Err(ValidationError::MissingSchemaForBodyParameter(
                    self.name.clone(),
                    path.into(),
                ));
            }

            return Ok(());
        } else if self.in_ == ParameterIn::Header {
            // Some headers aren't allowed.
            let lower = self.name.to_lowercase();
            if SPECIAL_HEADERS.iter().any(|&h| lower == h) {
                return Err(ValidationError::InvalidHeader(
                    self.name.clone(),
                    path.into(),
                ));
            }
        }

        // Non-body parameters must be primitives or an array - they can't have objects.
        let mut is_invalid = false;
        match self.data_type {
            Some(dt) if dt.is_primitive() => (),
            Some(DataType::Array) => {
                let mut inner = self.items.as_ref();
                loop {
                    let dt = inner.as_ref().and_then(|s| s.data_type);
                    match dt {
                        Some(ty) if ty.is_primitive() => break,
                        Some(DataType::Array) => {
                            inner = inner.as_ref().and_then(|s| s.items.as_deref());
                        }
                        None => {
                            return Err(ValidationError::InvalidParameterType(
                                self.name.clone(),
                                path.into(),
                                dt,
                                self.in_,
                            ));
                        }
                        _ => {
                            is_invalid = true;
                            break;
                        }
                    }
                }
            }
            // If "file" is specified, then it must be `formData` parameter.
            Some(DataType::File) => {
                // FIXME: Check against `consumes` and `produces` fields.
                if self.in_ != ParameterIn::FormData {
                    is_invalid = true;
                }
            }
            _ => is_invalid = true,
        }

        if is_invalid {
            return Err(ValidationError::InvalidParameterType(
                self.name.clone(),
                path.into(),
                self.data_type,
                self.in_,
            ));
        }

        Ok(())
    }
}

/// The location of the parameter.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum ParameterIn {
    Query,
    Header,
    Path,
    FormData,
    Body,
}

/// Possible formats for array values in parameter.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum CollectionFormat {
    Csv,
    Ssv,
    Tsv,
    Pipes,
    Multi,
}

/// Operation that can be traversed and resolved for codegen.
pub type ResolvableOperation<S> = Operation<ResolvableParameter<S>, ResolvableResponse<S>>;

/// Operation with default raw parameter and response.
pub type DefaultOperationRaw = Operation<DefaultParameterRaw, DefaultResponseRaw>;

/// Operation object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#operationObject
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation<P, R> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    // *NOTE:* `consumes` and `produces` are optional, because
    // local media ranges can be used to override global media ranges
    // (including setting it to empty), so we cannot go for an empty set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumes: Option<BTreeSet<MediaRange>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub produces: Option<BTreeSet<MediaRange>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<BTreeMap<String, Vec<String>>>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub schemes: BTreeSet<OperationProtocol>,
    // FIXME: Validate using `http::status::StatusCode::from_u16`
    pub responses: BTreeMap<String, Either<Reference, R>>,
    #[serde(default = "Vec::default", skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Either<Reference, P>>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl<S> Operation<Parameter<S>, Response<S>> {
    /// Overwrites the names of parameters in this operation using the
    /// given path template.
    pub fn set_parameter_names_from_path_template(&mut self, path: &str) {
        let mut names = vec![];
        Api::<(), (), ()>::path_parameters_map(path, |p| {
            names.push(p.to_owned());
            ":".into()
        });

        for p in self
            .parameters
            .iter_mut()
            .filter(|p| p.in_ == ParameterIn::Path)
            .rev()
        {
            if let Some(n) = names.pop() {
                p.name = n;
            } else {
                break;
            }
        }
    }
}

/// Reference object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#referenceObject
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Reference {
    #[serde(rename = "$ref")]
    pub reference: String,
}

/// The protocol used for an operation.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum OperationProtocol {
    Http,
    Https,
    Ws,
    Wss,
}

/// Response that can be traversed and resolved for codegen.
pub type ResolvableResponse<S> = Arc<RwLock<Response<Resolvable<S>>>>;

/// Response with the default raw schema.
pub type DefaultResponseRaw = Response<DefaultSchemaRaw>;

/// Response object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#responseObject
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Response<S> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<S>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, Header>,
}

/// Header object.
///
/// https://github.com/OAI/OpenAPI-Specification/blob/master/versions/2.0.md#headerObject
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Header {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub data_type: Option<DataType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<DataTypeFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Items>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_format: Option<CollectionFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, rename = "enum", skip_serializing_if = "Vec::is_empty")]
    pub enum_: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f32>,
    #[serde(rename = "exclusiveMaximum", skip_serializing_if = "Option::is_none")]
    pub exclusive_maximum: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f32>,
    #[serde(rename = "exclusiveMinimum", skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum: Option<bool>,
    #[serde(rename = "maxLength", skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(rename = "minLength", skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(rename = "maxItems", skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u32>,
    #[serde(rename = "minItems", skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u32>,
    #[serde(rename = "uniqueItems", skip_serializing_if = "Option::is_none")]
    pub unique_items: Option<bool>,
    #[serde(rename = "multipleOf", skip_serializing_if = "Option::is_none")]
    pub multiple_of: Option<f32>,
}

/// The HTTP method used for an operation.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum HttpMethod {
    Get,
    Put,
    Post,
    Delete,
    Options,
    Head,
    Patch,
}

impl HttpMethod {
    /// Whether this method allows body in requests.
    pub fn allows_body(self) -> bool {
        std::matches!(self, HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch)
    }
}

/* Helpers */

/// `Either` from "either" crate. We can't use that crate because
/// we don't want the enum to be tagged during de/serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Either<L, R> {
    /// Get a readable reference to the right variant (if it exists).
    pub fn right(&self) -> Option<&R> {
        match self {
            Either::Left(_) => None,
            Either::Right(r) => Some(r),
        }
    }

    /// Get a mutable reference to the right variant (if it exists).
    pub fn right_mut(&mut self) -> Option<&mut R> {
        match self {
            Either::Left(_) => None,
            Either::Right(r) => Some(r),
        }
    }

    /// Get a readable reference to the left variant (if it exists).
    pub fn left(&self) -> Option<&L> {
        match self {
            Either::Left(l) => Some(l),
            Either::Right(_) => None,
        }
    }

    /// Get a mutable reference to the left variant (if it exists).
    pub fn left_mut(&mut self) -> Option<&mut L> {
        match self {
            Either::Left(l) => Some(l),
            Either::Right(_) => None,
        }
    }
}

/// Wrapper for schema. This uses `Arc<RwLock<S>>` for interior
/// mutability and differentiates raw schema from resolved schema
/// (i.e., the one where `$ref` references point to the actual schema).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Resolvable<S> {
    Raw(Arc<RwLock<S>>),
    #[serde(skip)]
    Resolved {
        new: Arc<RwLock<S>>,
        old: Arc<RwLock<S>>,
    },
}

impl<S> Resolvable<S>
where
    S: Schema,
{
    /// Fetch the description for this schema.
    pub fn get_description(&self) -> Option<String> {
        match *self {
            Resolvable::Raw(ref s) => s.read().description().map(String::from),
            // We don't want parameters/fields to describe the actual refrenced object.
            Resolvable::Resolved { ref old, .. } => old.read().description().map(String::from),
        }
    }
}

/* Common trait impls */

impl Default for SpecFormat {
    fn default() -> Self {
        SpecFormat::Json
    }
}

#[cfg(feature = "actix-base")]
impl From<&Method> for HttpMethod {
    fn from(method: &Method) -> HttpMethod {
        match method.as_str() {
            "PUT" => HttpMethod::Put,
            "POST" => HttpMethod::Post,
            "DELETE" => HttpMethod::Delete,
            "OPTIONS" => HttpMethod::Options,
            "HEAD" => HttpMethod::Head,
            "PATCH" => HttpMethod::Patch,
            _ => HttpMethod::Get,
        }
    }
}

impl<T> Deref for Either<Reference, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match *self {
            Either::Left(_) => panic!("unable to deref because reference is not resolved."),
            Either::Right(ref r) => r,
        }
    }
}

impl<T> DerefMut for Either<Reference, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match *self {
            Either::Left(_) => panic!("unable to deref because reference is not resolved."),
            Either::Right(ref mut r) => r,
        }
    }
}

impl<S> Deref for Resolvable<S> {
    type Target = Arc<RwLock<S>>;

    fn deref(&self) -> &Self::Target {
        match *self {
            Resolvable::Raw(ref s) => s,
            Resolvable::Resolved { ref new, .. } => new,
        }
    }
}

impl<S> DerefMut for Resolvable<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match *self {
            Resolvable::Raw(ref mut s) => s,
            Resolvable::Resolved { ref mut new, .. } => new,
        }
    }
}

impl<S: Default> Default for Resolvable<S> {
    fn default() -> Self {
        Resolvable::from(S::default())
    }
}

impl<S> From<S> for Resolvable<S> {
    fn from(t: S) -> Self {
        Resolvable::Raw(Arc::new(RwLock::new(t)))
    }
}

impl<S> Clone for Resolvable<S> {
    fn clone(&self) -> Self {
        match *self {
            Resolvable::Raw(ref s) => Resolvable::Raw(s.clone()),
            Resolvable::Resolved { ref new, ref old } => Resolvable::Resolved {
                new: new.clone(),
                old: old.clone(),
            },
        }
    }
}

impl Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Default for Version {
    fn default() -> Self {
        Version::V2
    }
}

impl Default for CollectionFormat {
    fn default() -> Self {
        CollectionFormat::Csv
    }
}

/// **NOTE:** This is just a stub. This is usually set explicitly.
impl Default for ParameterIn {
    fn default() -> Self {
        ParameterIn::Body
    }
}

/* Serde helpers */

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(val: &bool) -> bool {
    !*val
}
