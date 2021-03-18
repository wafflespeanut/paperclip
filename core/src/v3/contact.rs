use super::v2;

impl From<v2::Contact> for openapiv3::Contact {
    fn from(v2: v2::Contact) -> Self {
        openapiv3::Contact {
            name: v2.name,
            url: v2.url,
            email: v2.email,
            extensions: indexmap::IndexMap::new(),
        }
    }
}
