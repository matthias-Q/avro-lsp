use async_lsp::lsp_types::{InlayHint, InlayHintLabel, Position};

use crate::schema::{AvroSchema, AvroType, PrimitiveType};

/// Generate inlay hints for all fields in the schema
pub fn generate_inlay_hints(schema: &AvroSchema, _text: &str) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    collect_field_hints(&schema.root, &mut hints);
    hints
}

/// Recursively collect hints from all fields in the schema
fn collect_field_hints(avro_type: &AvroType, hints: &mut Vec<InlayHint>) {
    match avro_type {
        AvroType::Record(record) => {
            // Collect hints for all fields in this record
            for field in &record.fields {
                if let Some(name_range) = field.name_range {
                    let type_hint = format_type_hint(&field.field_type);
                    hints.push(InlayHint {
                        position: Position {
                            line: name_range.end.line,
                            character: name_range.end.character,
                        },
                        label: InlayHintLabel::String(format!(": {}", type_hint)),
                        kind: Some(async_lsp::lsp_types::InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: None,
                        padding_left: None,
                        padding_right: None,
                        data: None,
                    });
                }

                // Recursively collect from nested types
                collect_field_hints(&field.field_type, hints);
            }
        }
        AvroType::Array(array) => {
            collect_field_hints(&array.items, hints);
        }
        AvroType::Map(map) => {
            collect_field_hints(&map.values, hints);
        }
        AvroType::Union(types) => {
            for t in types {
                collect_field_hints(t, hints);
            }
        }
        _ => {
            // Primitive, Enum, Fixed, TypeRef - no nested fields
        }
    }
}

/// Format an Avro type into a human-readable hint string
fn format_type_hint(avro_type: &AvroType) -> String {
    match avro_type {
        AvroType::Primitive(prim) => format_primitive(prim),
        AvroType::PrimitiveObject(prim_obj) => {
            let base = format_primitive(&prim_obj.primitive_type);
            if let Some(logical_type) = &prim_obj.logical_type {
                format!("{} ({})", base, logical_type)
            } else {
                base
            }
        }
        AvroType::Union(types) => {
            let formatted: Vec<String> = types.iter().map(format_type_hint).collect();
            formatted.join(" | ")
        }
        AvroType::Array(array) => {
            format!("array<{}>", format_type_hint(&array.items))
        }
        AvroType::Map(map) => {
            format!("map<{}>", format_type_hint(&map.values))
        }
        AvroType::TypeRef(type_ref) => type_ref.name.clone(),
        AvroType::Record(record) => record.name.clone(),
        AvroType::Enum(enum_schema) => enum_schema.name.clone(),
        AvroType::Fixed(fixed) => fixed.name.clone(),
    }
}

/// Format a primitive type name
fn format_primitive(prim: &PrimitiveType) -> String {
    match prim {
        PrimitiveType::Null => "null".to_string(),
        PrimitiveType::Boolean => "boolean".to_string(),
        PrimitiveType::Int => "int".to_string(),
        PrimitiveType::Long => "long".to_string(),
        PrimitiveType::Float => "float".to_string(),
        PrimitiveType::Double => "double".to_string(),
        PrimitiveType::Bytes => "bytes".to_string(),
        PrimitiveType::String => "string".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        ArraySchema, EnumSchema, Field, FixedSchema, MapSchema, RecordSchema, TypeRefSchema,
    };
    use async_lsp::lsp_types::Range;

    #[test]
    fn test_format_primitive() {
        assert_eq!(
            format_type_hint(&AvroType::Primitive(PrimitiveType::Long)),
            "long"
        );
        assert_eq!(
            format_type_hint(&AvroType::Primitive(PrimitiveType::String)),
            "string"
        );
        assert_eq!(
            format_type_hint(&AvroType::Primitive(PrimitiveType::Boolean)),
            "boolean"
        );
    }

    #[test]
    fn test_format_union() {
        let union = AvroType::Union(vec![
            AvroType::Primitive(PrimitiveType::Null),
            AvroType::Primitive(PrimitiveType::String),
        ]);
        assert_eq!(format_type_hint(&union), "null | string");

        let union3 = AvroType::Union(vec![
            AvroType::Primitive(PrimitiveType::String),
            AvroType::Primitive(PrimitiveType::Long),
            AvroType::Primitive(PrimitiveType::Null),
        ]);
        assert_eq!(format_type_hint(&union3), "string | long | null");
    }

    #[test]
    fn test_format_array() {
        let array = AvroType::Array(ArraySchema {
            type_name: "array".to_string(),
            items: Box::new(AvroType::Primitive(PrimitiveType::String)),
            default: None,
        });
        assert_eq!(format_type_hint(&array), "array<string>");

        // Array with union items
        let array_union = AvroType::Array(ArraySchema {
            type_name: "array".to_string(),
            items: Box::new(AvroType::Union(vec![
                AvroType::Primitive(PrimitiveType::Null),
                AvroType::Primitive(PrimitiveType::Int),
            ])),
            default: None,
        });
        assert_eq!(format_type_hint(&array_union), "array<null | int>");
    }

    #[test]
    fn test_format_map() {
        let map = AvroType::Map(MapSchema {
            type_name: "map".to_string(),
            values: Box::new(AvroType::Primitive(PrimitiveType::String)),
            default: None,
        });
        assert_eq!(format_type_hint(&map), "map<string>");

        // Map with union values
        let map_union = AvroType::Map(MapSchema {
            type_name: "map".to_string(),
            values: Box::new(AvroType::Union(vec![
                AvroType::Primitive(PrimitiveType::Null),
                AvroType::Primitive(PrimitiveType::Long),
            ])),
            default: None,
        });
        assert_eq!(format_type_hint(&map_union), "map<null | long>");
    }

    #[test]
    fn test_format_type_ref() {
        let type_ref = AvroType::TypeRef(TypeRefSchema {
            name: "Address".to_string(),
            range: None,
        });
        assert_eq!(format_type_hint(&type_ref), "Address");
    }

    #[test]
    fn test_format_nested_record() {
        let record = AvroType::Record(RecordSchema {
            type_name: "record".to_string(),
            name: "Address".to_string(),
            namespace: None,
            doc: None,
            aliases: None,
            fields: vec![],
            range: None,
            name_range: None,
            namespace_range: None,
        });
        // Should return just the name, not the full structure
        assert_eq!(format_type_hint(&record), "Address");
    }

    #[test]
    fn test_format_enum() {
        let enum_type = AvroType::Enum(EnumSchema {
            type_name: "enum".to_string(),
            name: "Status".to_string(),
            namespace: None,
            doc: None,
            aliases: None,
            symbols: vec!["ACTIVE".to_string(), "INACTIVE".to_string()],
            default: None,
            range: None,
            name_range: None,
            namespace_range: None,
        });
        assert_eq!(format_type_hint(&enum_type), "Status");
    }

    #[test]
    fn test_format_fixed() {
        let fixed = AvroType::Fixed(FixedSchema {
            type_name: "fixed".to_string(),
            name: "MD5".to_string(),
            namespace: None,
            doc: None,
            aliases: None,
            size: 16,
            logical_type: None,
            precision: None,
            scale: None,
            range: None,
            name_range: None,
            namespace_range: None,
        });
        assert_eq!(format_type_hint(&fixed), "MD5");
    }

    #[test]
    fn test_generate_hints_for_record() {
        use std::collections::HashMap;

        let field1 = Field {
            name: "id".to_string(),
            field_type: Box::new(AvroType::Primitive(PrimitiveType::Long)),
            doc: None,
            default: None,
            order: None,
            aliases: None,
            range: Some(Range {
                start: Position {
                    line: 3,
                    character: 5,
                },
                end: Position {
                    line: 3,
                    character: 30,
                },
            }),
            name_range: Some(Range {
                start: Position {
                    line: 3,
                    character: 13,
                },
                end: Position {
                    line: 3,
                    character: 15,
                },
            }),
            type_range: None,
        };

        let field2 = Field {
            name: "email".to_string(),
            field_type: Box::new(AvroType::Union(vec![
                AvroType::Primitive(PrimitiveType::Null),
                AvroType::Primitive(PrimitiveType::String),
            ])),
            doc: None,
            default: None,
            order: None,
            aliases: None,
            range: Some(Range {
                start: Position {
                    line: 4,
                    character: 5,
                },
                end: Position {
                    line: 4,
                    character: 40,
                },
            }),
            name_range: Some(Range {
                start: Position {
                    line: 4,
                    character: 13,
                },
                end: Position {
                    line: 4,
                    character: 18,
                },
            }),
            type_range: None,
        };

        let record = RecordSchema {
            type_name: "record".to_string(),
            name: "User".to_string(),
            namespace: None,
            doc: None,
            aliases: None,
            fields: vec![field1, field2],
            range: None,
            name_range: None,
            namespace_range: None,
        };

        let schema = AvroSchema {
            root: AvroType::Record(record),
            named_types: HashMap::new(),
        };

        let hints = generate_inlay_hints(&schema, "");

        assert_eq!(hints.len(), 2);

        // Check first hint (id: long)
        assert_eq!(hints[0].position.line, 3);
        assert_eq!(hints[0].position.character, 15);
        if let InlayHintLabel::String(label) = &hints[0].label {
            assert_eq!(label, ": long");
        } else {
            panic!("Expected string label");
        }

        // Check second hint (email: null | string)
        assert_eq!(hints[1].position.line, 4);
        assert_eq!(hints[1].position.character, 18);
        if let InlayHintLabel::String(label) = &hints[1].label {
            assert_eq!(label, ": null | string");
        } else {
            panic!("Expected string label");
        }
    }

    #[test]
    fn test_deeply_nested_types() {
        // array<map<null | string>>
        let nested = AvroType::Array(ArraySchema {
            type_name: "array".to_string(),
            items: Box::new(AvroType::Map(MapSchema {
                type_name: "map".to_string(),
                values: Box::new(AvroType::Union(vec![
                    AvroType::Primitive(PrimitiveType::Null),
                    AvroType::Primitive(PrimitiveType::String),
                ])),
                default: None,
            })),
            default: None,
        });
        assert_eq!(format_type_hint(&nested), "array<map<null | string>>");
    }
}
