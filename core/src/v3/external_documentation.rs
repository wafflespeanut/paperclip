use super::v2;

impl From<v2::ExternalDocs> for openapiv3::ExternalDocumentation {
    fn from(v2: v2::ExternalDocs) -> Self {
        openapiv3::ExternalDocumentation {
            description: v2.description,
            url: v2.url,
            extensions: indexmap::IndexMap::new(),
        }
    }
}
