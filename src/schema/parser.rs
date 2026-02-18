use std::collections::HashMap;

use super::error::{Result, SchemaError};
use super::json_parser::{parse_json, JsonValue};
use super::types::*;

pub struct AvroParser {
    named_types: HashMap<String, AvroType>,
    errors: Vec<SchemaError>,
}

impl AvroParser {
    pub fn new() -> Self {
        Self {
            named_types: HashMap::new(),
            errors: Vec::new(),
        }
    }

    /// Parse JSON text into an Avro schema with position information
    pub fn parse(&mut self, json_text: &str) -> Result<AvroSchema> {
        // Parse JSON with position tracking
        let json = parse_json(json_text).map_err(|e| SchemaError::Custom {
            message: format!("JSON parse error: {}", e),
            range: None,
        })?;

        let root = self.parse_type(&json)?;

        Ok(AvroSchema {
            root,
            named_types: self.named_types.clone(),
            parse_errors: self.errors.clone(),
        })
    }

    fn parse_type(&mut self, value: &JsonValue) -> Result<AvroType> {
        match value {
            // Primitive type as string: "int", "string", etc.
            JsonValue::String(s, range) => {
                if let Some(primitive) = PrimitiveType::parse(s) {
                    Ok(AvroType::Primitive(primitive))
                } else if s.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                    // Looks like a primitive type but isn't valid - use error recovery
                    let error = SchemaError::InvalidPrimitiveType {
                        type_name: s.clone(),
                        range: Some(*range),
                        suggested: suggest_primitive_type(s),
                    };

                    // Collect error for later reporting
                    self.errors.push(error);

                    // Return Invalid type node to continue parsing
                    Ok(AvroType::Invalid(InvalidTypeSchema {
                        type_name: s.clone(),
                        range: Some(*range),
                    }))
                } else {
                    // Must be a type reference (contains dots, uppercase, etc.)
                    Ok(AvroType::TypeRef(TypeRefSchema {
                        name: s.clone(),
                        range: Some(*range),
                    }))
                }
            }

            // Union type as array: ["null", "string"]
            JsonValue::Array(arr, _range) => {
                let types: Result<Vec<_>> = arr.iter().map(|v| self.parse_type(v)).collect();
                Ok(AvroType::Union(types?))
            }

            // Complex type as object
            JsonValue::Object(obj, _range) => {
                let type_name = obj.get("type").and_then(|v| v.as_string()).ok_or_else(|| {
                    SchemaError::MissingField {
                        field: "type".to_string(),
                    }
                })?;

                match type_name {
                    "record" => self.parse_record(obj, value.range()),
                    "enum" => self.parse_enum(obj, value.range()),
                    "array" => self.parse_array(obj),
                    "map" => self.parse_map(obj),
                    "fixed" => self.parse_fixed(obj, value.range()),
                    prim if PrimitiveType::parse(prim).is_some() => {
                        // Check if this primitive has logicalType or precision/scale attributes
                        if obj.contains_key("logicalType")
                            || obj.contains_key("precision")
                            || obj.contains_key("scale")
                        {
                            self.parse_primitive_object(obj, value.range())
                        } else {
                            Ok(AvroType::Primitive(PrimitiveType::parse(prim).unwrap()))
                        }
                    }
                    _ => {
                        // Invalid primitive type - use error recovery instead of failing
                        let error = SchemaError::InvalidPrimitiveType {
                            type_name: type_name.to_string(),
                            range: obj.get("type").map(|v| v.range()),
                            suggested: suggest_primitive_type(type_name),
                        };

                        // Collect error for later reporting
                        self.errors.push(error);

                        // Return Invalid type node to continue parsing
                        Ok(AvroType::Invalid(InvalidTypeSchema {
                            type_name: type_name.to_string(),
                            range: obj.get("type").map(|v| v.range()),
                        }))
                    }
                }
            }

            _ => Err(SchemaError::Custom {
                message: "Schema must be a string, array, or object".to_string(),
                range: None,
            }),
        }
    }

    fn parse_record(
        &mut self,
        obj: &HashMap<String, JsonValue>,
        record_range: async_lsp::lsp_types::Range,
    ) -> Result<AvroType> {
        let name = self.get_required_string(obj, "name")?;
        let namespace = self.get_optional_string(obj, "namespace");
        let doc = self.get_optional_string(obj, "doc");
        let aliases = self.get_optional_string_array(obj, "aliases");

        // Get name range
        let name_range = obj.get("name").map(|v| v.range()).or(Some(record_range));

        let fields_value = obj.get("fields").ok_or_else(|| SchemaError::MissingField {
            field: "fields".to_string(),
        })?;

        let fields_array = fields_value
            .as_array()
            .ok_or_else(|| SchemaError::InvalidType {
                expected: "array".to_string(),
                found: "other".to_string(),
                range: Some(fields_value.range()),
            })?;

        let mut fields = Vec::new();
        for field_value in fields_array {
            let field_obj = field_value
                .as_object()
                .ok_or_else(|| SchemaError::InvalidType {
                    expected: "object".to_string(),
                    found: "other".to_string(),
                    range: Some(field_value.range()),
                })?;

            let field_name = self.get_required_string(field_obj, "name")?;
            let field_type_value =
                field_obj
                    .get("type")
                    .ok_or_else(|| SchemaError::MissingFieldWithContext {
                        field: "type".to_string(),
                        context: format!("field '{}'", field_name),
                        range: Some(field_value.range()),
                    })?;
            let field_type = self.parse_type(field_type_value)?;

            // Get position ranges
            let field_range = Some(field_value.range());
            let name_range = field_obj.get("name").map(|v| v.range());
            let type_range = Some(field_type_value.range());

            fields.push(Field {
                name: field_name,
                field_type: Box::new(field_type),
                doc: self.get_optional_string(field_obj, "doc"),
                default: field_obj
                    .get("default")
                    .and_then(|v| self.json_value_to_serde(v)),
                order: self.get_optional_string(field_obj, "order"),
                aliases: self.get_optional_string_array(field_obj, "aliases"),
                range: field_range,
                name_range,
                type_range,
            });
        }

        // Get namespace range if namespace exists
        let namespace_range = if namespace.is_some() {
            obj.get("namespace").map(|v| v.range())
        } else {
            None
        };

        let record = RecordSchema {
            type_name: "record".to_string(),
            name: name.clone(),
            namespace,
            doc,
            aliases,
            fields,
            range: Some(record_range),
            name_range,
            namespace_range,
        };

        let avro_type = AvroType::Record(record);

        // Store named type
        self.named_types.insert(name, avro_type.clone());

        Ok(avro_type)
    }

    fn parse_enum(
        &mut self,
        obj: &HashMap<String, JsonValue>,
        enum_range: async_lsp::lsp_types::Range,
    ) -> Result<AvroType> {
        let name = self.get_required_string(obj, "name")?;
        let namespace = self.get_optional_string(obj, "namespace");
        let doc = self.get_optional_string(obj, "doc");
        let aliases = self.get_optional_string_array(obj, "aliases");

        // Get name range
        let name_range = obj.get("name").map(|v| v.range()).or(Some(enum_range));

        let symbols = obj
            .get("symbols")
            .and_then(|v| v.as_array())
            .ok_or_else(|| SchemaError::MissingField {
                field: "symbols".to_string(),
            })?
            .iter()
            .map(|v| {
                v.as_string()
                    .map(String::from)
                    .ok_or_else(|| SchemaError::InvalidType {
                        expected: "string".to_string(),
                        found: "other".to_string(),
                        range: Some(v.range()),
                    })
            })
            .collect::<Result<Vec<_>>>()?;

        let default = self.get_optional_string(obj, "default");

        // Get namespace range if namespace exists
        let namespace_range = if namespace.is_some() {
            obj.get("namespace").map(|v| v.range())
        } else {
            None
        };

        let enum_schema = EnumSchema {
            type_name: "enum".to_string(),
            name: name.clone(),
            namespace,
            doc,
            aliases,
            symbols,
            default,
            range: Some(enum_range),
            name_range,
            namespace_range,
        };

        let avro_type = AvroType::Enum(enum_schema);

        // Store named type
        self.named_types.insert(name, avro_type.clone());

        Ok(avro_type)
    }

    fn parse_array(&mut self, obj: &HashMap<String, JsonValue>) -> Result<AvroType> {
        let items_value = obj.get("items").ok_or_else(|| SchemaError::MissingField {
            field: "items".to_string(),
        })?;
        let items = self.parse_type(items_value)?;

        Ok(AvroType::Array(ArraySchema {
            type_name: "array".to_string(),
            items: Box::new(items),
            default: obj.get("default").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| self.json_value_to_serde(v))
                    .collect()
            }),
        }))
    }

    fn parse_map(&mut self, obj: &HashMap<String, JsonValue>) -> Result<AvroType> {
        let values_value = obj.get("values").ok_or_else(|| SchemaError::MissingField {
            field: "values".to_string(),
        })?;
        let values = self.parse_type(values_value)?;

        Ok(AvroType::Map(MapSchema {
            type_name: "map".to_string(),
            values: Box::new(values),
            default: obj.get("default").and_then(|v| v.as_object()).map(|m| {
                m.iter()
                    .filter_map(|(k, v)| self.json_value_to_serde(v).map(|val| (k.clone(), val)))
                    .collect()
            }),
        }))
    }

    fn parse_fixed(
        &mut self,
        obj: &HashMap<String, JsonValue>,
        fixed_range: async_lsp::lsp_types::Range,
    ) -> Result<AvroType> {
        let name = self.get_required_string(obj, "name")?;
        let namespace = self.get_optional_string(obj, "namespace");
        let doc = self.get_optional_string(obj, "doc");
        let aliases = self.get_optional_string_array(obj, "aliases");

        // Get name range
        let name_range = obj.get("name").map(|v| v.range()).or(Some(fixed_range));

        let size = obj
            .get("size")
            .and_then(|v| match v {
                JsonValue::Number(n, _) => Some(*n as usize),
                _ => None,
            })
            .ok_or_else(|| SchemaError::MissingField {
                field: "size".to_string(),
            })?;

        // Parse logical type and related attributes
        let logical_type = obj
            .get("logicalType")
            .and_then(|v| v.as_string())
            .map(String::from);
        let precision = obj.get("precision").and_then(|v| match v {
            JsonValue::Number(n, _) => Some(*n as usize),
            _ => None,
        });
        let scale = obj.get("scale").and_then(|v| match v {
            JsonValue::Number(n, _) => Some(*n as usize),
            _ => None,
        });

        // Get namespace range if namespace exists
        let namespace_range = if namespace.is_some() {
            obj.get("namespace").map(|v| v.range())
        } else {
            None
        };

        let fixed = FixedSchema {
            type_name: "fixed".to_string(),
            name: name.clone(),
            namespace,
            doc,
            aliases,
            size,
            logical_type,
            precision,
            scale,
            range: Some(fixed_range),
            name_range,
            namespace_range,
        };

        let avro_type = AvroType::Fixed(fixed);

        // Store named type
        self.named_types.insert(name, avro_type.clone());

        Ok(avro_type)
    }

    fn parse_primitive_object(
        &mut self,
        obj: &HashMap<String, JsonValue>,
        range: async_lsp::lsp_types::Range,
    ) -> Result<AvroType> {
        let type_name = self.get_required_string(obj, "type")?;

        // Verify it's actually a primitive type
        let primitive_type =
            PrimitiveType::parse(&type_name).ok_or_else(|| SchemaError::InvalidPrimitiveType {
                type_name: type_name.clone(),
                range: obj.get("type").map(|v| v.range()),
                suggested: suggest_primitive_type(&type_name),
            })?;

        // Capture range for type_name value
        let type_name_range = obj.get("type").map(|v| v.range());

        // Parse logical type and capture its range
        let (logical_type, logical_type_range) = obj
            .get("logicalType")
            .and_then(|v| match v {
                JsonValue::String(s, range) => Some((s.clone(), *range)),
                _ => None,
            })
            .map(|(s, r)| (Some(s), Some(r)))
            .unwrap_or((None, None));

        let precision = obj.get("precision").and_then(|v| match v {
            JsonValue::Number(n, _) => Some(*n as usize),
            _ => None,
        });
        let scale = obj.get("scale").and_then(|v| match v {
            JsonValue::Number(n, _) => Some(*n as usize),
            _ => None,
        });

        Ok(AvroType::PrimitiveObject(PrimitiveSchema {
            type_name,
            primitive_type,
            logical_type,
            precision,
            scale,
            range: Some(range),
            name_range: None,
            namespace_range: None,
            type_name_range,
            logical_type_range,
        }))
    }

    fn get_required_string(&self, obj: &HashMap<String, JsonValue>, key: &str) -> Result<String> {
        obj.get(key)
            .and_then(|v| v.as_string())
            .map(String::from)
            .ok_or_else(|| SchemaError::MissingField {
                field: key.to_string(),
            })
    }

    fn get_optional_string(&self, obj: &HashMap<String, JsonValue>, key: &str) -> Option<String> {
        obj.get(key).and_then(|v| v.as_string()).map(String::from)
    }

    fn get_optional_string_array(
        &self,
        obj: &HashMap<String, JsonValue>,
        key: &str,
    ) -> Option<Vec<String>> {
        obj.get(key).and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_string().map(String::from))
                    .collect()
            })
        })
    }

    /// Convert our JsonValue to serde_json::Value for default values
    fn json_value_to_serde(&self, value: &JsonValue) -> Option<serde_json::Value> {
        match value {
            JsonValue::Null(_) => Some(serde_json::Value::Null),
            JsonValue::Bool(b, _) => Some(serde_json::Value::Bool(*b)),
            JsonValue::Number(n, _) => {
                serde_json::Number::from_f64(*n).map(serde_json::Value::Number)
            }
            JsonValue::String(s, _) => Some(serde_json::Value::String(s.clone())),
            JsonValue::Array(arr, _) => {
                let vals: Option<Vec<_>> =
                    arr.iter().map(|v| self.json_value_to_serde(v)).collect();
                vals.map(serde_json::Value::Array)
            }
            JsonValue::Object(obj, _) => {
                let map: Option<serde_json::Map<String, serde_json::Value>> = obj
                    .iter()
                    .map(|(k, v)| self.json_value_to_serde(v).map(|val| (k.clone(), val)))
                    .collect();
                map.map(serde_json::Value::Object)
            }
        }
    }
}

/// Calculate Levenshtein distance between two strings
#[allow(clippy::needless_range_loop)]
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.chars().count();
    let len2 = s2.chars().count();

    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    for i in 0..=len1 {
        matrix[i][0] = i;
    }
    for j in 0..=len2 {
        matrix[0][j] = j;
    }

    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();

    for i in 1..=len1 {
        for j in 1..=len2 {
            let cost = if s1_chars[i - 1] == s2_chars[j - 1] {
                0
            } else {
                1
            };
            matrix[i][j] = std::cmp::min(
                std::cmp::min(matrix[i - 1][j] + 1, matrix[i][j - 1] + 1),
                matrix[i - 1][j - 1] + cost,
            );
        }
    }

    matrix[len1][len2]
}

/// Suggest the closest valid primitive type for an invalid type name
fn suggest_primitive_type(invalid_type: &str) -> Option<String> {
    const PRIMITIVE_TYPES: &[&str] = &[
        "null", "boolean", "int", "long", "float", "double", "bytes", "string",
    ];

    let invalid_lower = invalid_type.to_lowercase();

    // Find the primitive with the smallest Levenshtein distance
    let mut best_match: Option<(&str, usize)> = None;

    for &prim in PRIMITIVE_TYPES {
        let distance = levenshtein_distance(&invalid_lower, prim);

        // Only suggest if distance is reasonable (≤ 3 edits)
        if distance <= 3 {
            match best_match {
                None => best_match = Some((prim, distance)),
                Some((_, best_dist)) if distance < best_dist => {
                    best_match = Some((prim, distance));
                }
                _ => {}
            }
        }
    }

    best_match.map(|(prim, _)| prim.to_string())
}

impl Default for AvroParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_primitive_string() {
        let mut parser = AvroParser::new();
        let schema = parser.parse(r#""string""#).unwrap();
        assert_eq!(schema.root, AvroType::Primitive(PrimitiveType::String));
    }

    #[test]
    fn test_parse_simple_record() {
        let mut parser = AvroParser::new();
        let json = r#"
        {
            "type": "record",
            "name": "User",
            "fields": [
                {"name": "name", "type": "string"},
                {"name": "age", "type": "int"}
            ]
        }
        "#;
        let schema = parser.parse(json).unwrap();
        if let AvroType::Record(record) = schema.root {
            assert_eq!(record.name, "User");
            assert_eq!(record.fields.len(), 2);
            // Check that positions are tracked
            assert!(record.range.is_some());
            assert!(record.fields[0].range.is_some());
            assert!(record.fields[0].type_range.is_some());
        } else {
            panic!("Expected record type");
        }
    }

    #[test]
    fn test_parse_union() {
        let mut parser = AvroParser::new();
        let json = r#"["null", "string"]"#;
        let schema = parser.parse(json).unwrap();
        if let AvroType::Union(types) = schema.root {
            assert_eq!(types.len(), 2);
        } else {
            panic!("Expected union type");
        }
    }

    #[test]
    fn test_parse_record_with_union_field() {
        let mut parser = AvroParser::new();
        let json = r#"{
            "type": "record",
            "name": "Response",
            "namespace": "com.example",
            "fields": [
                {"name": "data", "type": ["null", "string"], "default": null}
            ]
        }"#;
        let schema = parser.parse(json).unwrap();

        if let AvroType::Record(record) = &schema.root {
            assert_eq!(record.name, "Response");
            assert_eq!(record.fields.len(), 1);

            // Check the field type - should be Union
            if let AvroType::Union(types) = &*record.fields[0].field_type {
                assert_eq!(types.len(), 2);
            } else {
                panic!(
                    "Expected union type for field, got: {:?}",
                    record.fields[0].field_type
                );
            }
        } else {
            panic!("Expected record type");
        }
    }

    #[test]
    fn test_error_recovery_invalid_primitive() {
        let mut parser = AvroParser::new();
        let json = r#"{
            "type": "record",
            "name": "TestRecord",
            "fields": [
                {"name": "flag", "type": "boolena"}
            ]
        }"#;
        let schema = parser.parse(json).unwrap();

        // Schema should parse successfully despite invalid type
        assert!(!schema.parse_errors.is_empty(), "Expected parse errors");
        assert_eq!(
            schema.parse_errors.len(),
            1,
            "Expected exactly 1 parse error"
        );

        // Check the error details
        match &schema.parse_errors[0] {
            SchemaError::InvalidPrimitiveType {
                type_name,
                suggested,
                ..
            } => {
                assert_eq!(type_name, "boolena");
                assert_eq!(suggested.as_deref(), Some("boolean"));
            }
            _ => panic!(
                "Expected InvalidPrimitiveType error, got: {:?}",
                schema.parse_errors[0]
            ),
        }

        // Check that the record structure is preserved
        if let AvroType::Record(record) = &schema.root {
            assert_eq!(record.name, "TestRecord");
            assert_eq!(record.fields.len(), 1);
            assert_eq!(record.fields[0].name, "flag");

            // Check that the invalid type is represented as Invalid
            if let AvroType::Invalid(invalid) = &*record.fields[0].field_type {
                assert_eq!(invalid.type_name, "boolena");
                assert!(invalid.range.is_some());
            } else {
                panic!(
                    "Expected Invalid type for field, got: {:?}",
                    record.fields[0].field_type
                );
            }
        } else {
            panic!("Expected record type");
        }
    }

    #[test]
    fn test_error_recovery_multiple_invalid_types() {
        let mut parser = AvroParser::new();
        let json = r#"{
            "type": "record",
            "name": "TestRecord",
            "fields": [
                {"name": "field1", "type": "boolena"},
                {"name": "field2", "type": "integr"},
                {"name": "field3", "type": "string"}
            ]
        }"#;
        let schema = parser.parse(json).unwrap();

        // Should have 2 errors (boolena and integr)
        assert_eq!(schema.parse_errors.len(), 2, "Expected 2 parse errors");

        // Check that valid field is still parsed correctly
        if let AvroType::Record(record) = &schema.root {
            assert_eq!(record.fields.len(), 3);

            // First field should be Invalid
            assert!(matches!(
                &*record.fields[0].field_type,
                AvroType::Invalid(_)
            ));

            // Second field should be Invalid
            assert!(matches!(
                &*record.fields[1].field_type,
                AvroType::Invalid(_)
            ));

            // Third field should be valid String
            assert!(matches!(
                &*record.fields[2].field_type,
                AvroType::Primitive(PrimitiveType::String)
            ));
        } else {
            panic!("Expected record type");
        }
    }

    #[test]
    fn test_no_errors_for_valid_schema() {
        let mut parser = AvroParser::new();
        let json = r#"{
            "type": "record",
            "name": "ValidRecord",
            "fields": [
                {"name": "name", "type": "string"},
                {"name": "age", "type": "int"},
                {"name": "active", "type": "boolean"}
            ]
        }"#;
        let schema = parser.parse(json).unwrap();

        // Valid schema should have no parse errors
        assert!(
            schema.parse_errors.is_empty(),
            "Expected no parse errors for valid schema"
        );
    }
}
