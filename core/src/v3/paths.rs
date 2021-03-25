use super::v2;
use std::collections::BTreeMap;

impl From<v2::DefaultPathItemRaw> for openapiv3::PathItem {
    fn from(v2: v2::DefaultPathItemRaw) -> Self {
        let methods = v2
            .methods
            .iter()
            .map(|(k, v)| (*k, v.clone().into()))
            .collect::<BTreeMap<v2::HttpMethod, openapiv3::Operation>>();

        openapiv3::PathItem {
            get: methods.get(&v2::HttpMethod::Get).cloned(),
            put: methods.get(&v2::HttpMethod::Put).cloned(),
            post: methods.get(&v2::HttpMethod::Post).cloned(),
            delete: methods.get(&v2::HttpMethod::Delete).cloned(),
            options: methods.get(&v2::HttpMethod::Options).cloned(),
            head: methods.get(&v2::HttpMethod::Head).cloned(),
            patch: methods.get(&v2::HttpMethod::Patch).cloned(),
            trace: None,
            servers: vec![],
            parameters: {
                openapiv3::Operation::from(v2::DefaultOperationRaw {
                    parameters: v2.parameters,
                    ..Default::default()
                })
                .parameters
            },
            extensions: indexmap::IndexMap::new(),
        }
    }
}
