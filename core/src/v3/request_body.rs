use super::{invalid_referenceor, non_body_parameter_to_v3_parameter, v2, Either};

impl From<v2::DefaultParameterRaw>
    for Either<openapiv3::Parameter, Either<openapiv3::RequestBody, Option<openapiv3::Schema>>>
{
    fn from(v2: v2::DefaultParameterRaw) -> Self {
        let parameter_data = |schema: Option<openapiv3::Schema>| openapiv3::ParameterData {
            name: v2.name.clone(),
            description: v2.description.clone(),
            required: v2.required,
            deprecated: None,
            format: match &schema {
                Some(schema) => openapiv3::ParameterSchemaOrContent::Schema(
                    openapiv3::ReferenceOr::Item(schema.clone()),
                ),
                None => openapiv3::ParameterSchemaOrContent::Schema(invalid_referenceor(
                    "No Schema found".into(),
                )),
            },
            example: None,
            examples: indexmap::IndexMap::new(),
            extensions: indexmap::IndexMap::new(),
        };

        match v2.in_ {
            v2::ParameterIn::Query => Either::Left(openapiv3::Parameter::Query {
                parameter_data: parameter_data(non_body_parameter_to_v3_parameter(false, &v2)),
                allow_reserved: false,
                style: Default::default(),
                allow_empty_value: None,
            }),
            v2::ParameterIn::Header => Either::Left(openapiv3::Parameter::Header {
                parameter_data: parameter_data(non_body_parameter_to_v3_parameter(false, &v2)),
                style: Default::default(),
            }),
            v2::ParameterIn::Path => Either::Left(openapiv3::Parameter::Path {
                parameter_data: parameter_data(non_body_parameter_to_v3_parameter(false, &v2)),
                style: Default::default(),
            }),
            v2::ParameterIn::FormData => {
                Either::Right(Either::Right(non_body_parameter_to_v3_parameter(true, &v2)))
            }
            v2::ParameterIn::Body => Either::Right(Either::Left(openapiv3::RequestBody {
                description: v2.description,
                content: {
                    let media = openapiv3::MediaType {
                        schema: v2.schema.map(|s| s.into()),
                        example: None,
                        examples: indexmap::IndexMap::new(),
                        encoding: indexmap::IndexMap::new(),
                    };
                    let mut map = indexmap::IndexMap::new();
                    map.insert(v2::SpecFormat::Json.mime().0.to_string(), media);
                    map
                },
                required: v2.required,
                extensions: indexmap::IndexMap::new(),
            })),
        }
    }
}
