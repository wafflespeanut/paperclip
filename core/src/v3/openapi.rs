use super::v2;

impl From<v2::DefaultApiRaw> for openapiv3::OpenAPI {
    fn from(v2: v2::DefaultApiRaw) -> Self {
        let mut spec = openapiv3::OpenAPI {
            openapi: "3.0.0".into(),
            tags: v2.tags.iter().cloned().map(From::from).collect(),
            info: v2.info.clone().into(),
            servers: openapi3_server(v2.host, v2.base_path),
            external_docs: v2.external_docs.map(From::from),
            ..Default::default()
        };

        let mut components = openapiv3::Components::default();
        for (name, scheme) in v2.security_definitions {
            components
                .security_schemes
                .insert(name, openapiv3::ReferenceOr::Item(scheme.into()));
        }
        components.responses = v2
            .responses
            .iter()
            .fold(indexmap::IndexMap::new(), |mut i, b| {
                i.insert(b.0.to_string(), b.1.clone().into());
                i
            });
        spec.paths = v2.paths.iter().fold(indexmap::IndexMap::new(), |mut i, b| {
            i.insert(
                b.0.to_string(),
                openapiv3::ReferenceOr::Item(b.1.clone().into()),
            );
            i
        });

        components.schemas = v2
            .definitions
            .iter()
            .fold(indexmap::IndexMap::new(), |mut i, b| {
                i.insert(b.0.to_string(), b.1.clone().into());
                i
            });
        spec.components = Some(components);

        spec
    }
}

fn openapi3_server(_host: Option<String>, base: Option<String>) -> Vec<openapiv3::Server> {
    if let Some(base) = base {
        vec![openapiv3::Server {
            url: base,
            description: None,
            variables: None,
            extensions: indexmap::IndexMap::new(),
        }]
    } else {
        vec![]
    }
}
