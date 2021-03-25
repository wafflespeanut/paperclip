use super::v2;

impl From<v2::Tag> for openapiv3::Tag {
    fn from(v2: v2::Tag) -> Self {
        openapiv3::Tag {
            name: v2.name,
            description: v2.description,
            external_docs: v2.external_docs.map(openapiv3::ExternalDocumentation::from),
            extensions: indexmap::IndexMap::new(),
        }
    }
}
