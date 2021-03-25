use super::{invalid_referenceor, v2};

/// helper function to convert a default raw parameter when we already know it's not part of a body
pub(crate) fn non_body_parameter_to_v3_parameter(
    form_data: bool,
    v2: &v2::DefaultParameterRaw,
) -> Option<openapiv3::Schema> {
    match v2.data_type {
        Some(data_type) => {
            let schema_kind = match data_type {
                v2::DataType::Integer => {
                    openapiv3::SchemaKind::Type(openapiv3::Type::Integer(openapiv3::IntegerType {
                        format: match &v2.format {
                            None => openapiv3::VariantOrUnknownOrEmpty::Empty,
                            Some(format) => match format {
                                v2::DataTypeFormat::Int32 => {
                                    openapiv3::VariantOrUnknownOrEmpty::Item(
                                        openapiv3::IntegerFormat::Int32,
                                    )
                                }
                                v2::DataTypeFormat::Int64 => {
                                    openapiv3::VariantOrUnknownOrEmpty::Item(
                                        openapiv3::IntegerFormat::Int64,
                                    )
                                }
                                other => {
                                    debug_assert!(false, "Invalid data type format: {:?}", other);
                                    openapiv3::VariantOrUnknownOrEmpty::Empty
                                }
                            },
                        },
                        multiple_of: v2.multiple_of.map(|v| v as i64),
                        exclusive_minimum: v2.exclusive_minimum.unwrap_or_default(),
                        exclusive_maximum: v2.exclusive_maximum.unwrap_or_default(),
                        minimum: v2.minimum.map(|v| v as i64),
                        maximum: v2.maximum.map(|v| v as i64),
                        enumeration: v2
                            .enum_
                            .iter()
                            .cloned()
                            .map(|v| serde_json::from_value(v).unwrap_or_default())
                            .collect(),
                    }))
                }
                v2::DataType::Number => {
                    openapiv3::SchemaKind::Type(openapiv3::Type::Number(openapiv3::NumberType {
                        format: match &v2.format {
                            None => openapiv3::VariantOrUnknownOrEmpty::Empty,
                            Some(format) => match format {
                                v2::DataTypeFormat::Float => {
                                    openapiv3::VariantOrUnknownOrEmpty::Item(
                                        openapiv3::NumberFormat::Float {},
                                    )
                                }
                                v2::DataTypeFormat::Double => {
                                    openapiv3::VariantOrUnknownOrEmpty::Item(
                                        openapiv3::NumberFormat::Double {},
                                    )
                                }
                                other => {
                                    debug_assert!(false, "Invalid data type format: {:?}", other);
                                    openapiv3::VariantOrUnknownOrEmpty::Empty
                                }
                            },
                        },
                        multiple_of: v2.multiple_of.map(|v| v as f64),
                        exclusive_minimum: v2.exclusive_minimum.unwrap_or_default(),
                        exclusive_maximum: v2.exclusive_maximum.unwrap_or_default(),
                        minimum: v2.minimum.map(|v| v as f64),
                        maximum: v2.maximum.map(|v| v as f64),
                        enumeration: v2
                            .enum_
                            .iter()
                            .cloned()
                            .map(|v| serde_json::from_value(v).unwrap_or_default())
                            .collect(),
                    }))
                }
                v2::DataType::String => {
                    openapiv3::SchemaKind::Type(openapiv3::Type::String(openapiv3::StringType {
                        format: match &v2.format {
                            None => openapiv3::VariantOrUnknownOrEmpty::Empty,
                            Some(format) => match format {
                                v2::DataTypeFormat::Byte => {
                                    openapiv3::VariantOrUnknownOrEmpty::Item(
                                        openapiv3::StringFormat::Byte,
                                    )
                                }
                                v2::DataTypeFormat::Binary => {
                                    openapiv3::VariantOrUnknownOrEmpty::Item(
                                        openapiv3::StringFormat::Binary,
                                    )
                                }
                                v2::DataTypeFormat::Date => {
                                    openapiv3::VariantOrUnknownOrEmpty::Item(
                                        openapiv3::StringFormat::Date,
                                    )
                                }
                                v2::DataTypeFormat::DateTime => {
                                    openapiv3::VariantOrUnknownOrEmpty::Item(
                                        openapiv3::StringFormat::DateTime,
                                    )
                                }
                                v2::DataTypeFormat::Password => {
                                    openapiv3::VariantOrUnknownOrEmpty::Item(
                                        openapiv3::StringFormat::Password,
                                    )
                                }
                                other => {
                                    debug_assert!(false, "Invalid data type format: {:?}", other);
                                    openapiv3::VariantOrUnknownOrEmpty::Empty
                                }
                            },
                        },
                        pattern: v2.pattern.clone(),
                        enumeration: v2
                            .enum_
                            .iter()
                            .cloned()
                            .map(|v| serde_json::from_value(v).unwrap_or_default())
                            .collect(),
                        min_length: v2.min_length.map(|v| v as usize),
                        max_length: v2.max_length.map(|v| v as usize),
                    }))
                }
                v2::DataType::Boolean => openapiv3::SchemaKind::Type(openapiv3::Type::Boolean {}),
                v2::DataType::Array => {
                    openapiv3::SchemaKind::Type(openapiv3::Type::Array(openapiv3::ArrayType {
                        items: {
                            match &v2.items {
                                Some(items) => items.clone().into(),
                                None => invalid_referenceor("Array with 0 items!".into()),
                            }
                        },
                        min_items: v2.min_items.map(|v| v as usize),
                        max_items: v2.max_items.map(|v| v as usize),
                        unique_items: v2.unique_items,
                    }))
                }
                v2::DataType::Object => {
                    // objects comes from the parameter schema which would not trigger this call
                    return None;
                }
                v2::DataType::File => {
                    if !form_data {
                        // File only usable from formData
                        return None;
                    }
                    openapiv3::SchemaKind::Type(openapiv3::Type::String(openapiv3::StringType {
                        format: openapiv3::VariantOrUnknownOrEmpty::Item(
                            openapiv3::StringFormat::Binary,
                        ),
                        ..Default::default()
                    }))
                }
            };
            let schema_data = if form_data {
                // formData has the description and default values in the schema's properties
                openapiv3::SchemaData {
                    description: v2.description.clone(),
                    default: v2.default.clone(),
                    ..Default::default()
                }
            } else {
                // properties set on the parameter and not on the schema's properties
                openapiv3::SchemaData::default()
            };
            Some(openapiv3::Schema {
                schema_data,
                schema_kind,
            })
        }
        None => None,
    }
}
