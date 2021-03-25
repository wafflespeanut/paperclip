use super::v2;

impl From<v2::License> for openapiv3::License {
    fn from(v2: v2::License) -> Self {
        openapiv3::License {
            name: v2.name.unwrap_or_default(),
            url: v2.url,
            extensions: indexmap::IndexMap::new(),
        }
    }
}
