use super::v2;

impl From<v2::Info> for openapiv3::Info {
    fn from(v2: v2::Info) -> Self {
        openapiv3::Info {
            title: v2.title,
            description: v2.description,
            terms_of_service: None,
            contact: v2.contact.map(|c| c.into()),
            license: v2.license.map(From::from),
            version: v2.version,
            extensions: indexmap::IndexMap::new(),
        }
    }
}
