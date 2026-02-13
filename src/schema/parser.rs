use std::collections::HashMap;

use super::error::{Result, SchemaError};
use super::json_parser::{JsonValue, parse_json};
use super::types::*;

pub struct AvroParser {
    named_types: HashMap<String, AvroType>,
}

impl AvroParser {
    pub fn new() -> Self {
        Self {
            named_types: HashMap::new(),
        }
    }

    /// Parse JSON text into an Avro schema with position information
    pub fn parse(&mut self, json_text: &str) -> Result<AvroSchema> {
        // Parse JSON with position tracking
        let json = parse_json(json_text)
            .map_err(|e| SchemaError::Custom(format!("JSON parse error: {}", e)))?;

        let root = self.parse_type(&json)?;

        Ok(AvroSchema {
            root,
            named_types: self.named_types.clone(),
        })
    }

    fn parse_type(&mut self, value: &JsonValue) -> Result<AvroType> {
        match value {
            // Primitive type as string: "int", "string", etc.
            JsonValue::String(s, range) => {
                if let Some(primitive) = PrimitiveType::from_str(s) {
                    Ok(AvroType::Primitive(primitive))
                } else {
                    // Must be a type reference
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
                let type_name = obj
                    .get("type")
                    .and_then(|v| v.as_string())
                    .ok_or_else(|| SchemaError::MissingField("type".to_string()))?;

                match type_name {
                    "record" => self.parse_record(obj, value.range()),
                    "enum" => self.parse_enum(obj, value.range()),
                    "array" => self.parse_array(obj),
                    "map" => self.parse_map(obj),
                    "fixed" => self.parse_fixed(obj, value.range()),
                    prim if PrimitiveType::from_str(prim).is_some() => {
                        Ok(AvroType::Primitive(PrimitiveType::from_str(prim).unwrap()))
                    }
                    _ => Err(SchemaError::InvalidPrimitiveType(type_name.to_string())),
                }
            }

            _ => Err(SchemaError::Custom(
                "Schema must be a string, array, or object".to_string(),
            )),
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

        let fields_value = obj
            .get("fields")
            .ok_or_else(|| SchemaError::MissingField("fields".to_string()))?;

        let fields_array = fields_value
            .as_array()
            .ok_or_else(|| SchemaError::InvalidType {
                expected: "array".to_string(),
                found: "other".to_string(),
            })?;

        let mut fields = Vec::new();
        for field_value in fields_array {
            let field_obj = field_value
                .as_object()
                .ok_or_else(|| SchemaError::InvalidType {
                    expected: "object".to_string(),
                    found: "other".to_string(),
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

        let record = RecordSchema {
            type_name: "record".to_string(),
            name: name.clone(),
            namespace,
            doc,
            aliases,
            fields,
            range: Some(record_range),
            name_range,
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
            .ok_or_else(|| SchemaError::MissingField("symbols".to_string()))?
            .iter()
            .map(|v| {
                v.as_string()
                    .map(String::from)
                    .ok_or_else(|| SchemaError::InvalidType {
                        expected: "string".to_string(),
                        found: "other".to_string(),
                    })
            })
            .collect::<Result<Vec<_>>>()?;

        let default = self.get_optional_string(obj, "default");

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
        };

        let avro_type = AvroType::Enum(enum_schema);

        // Store named type
        self.named_types.insert(name, avro_type.clone());

        Ok(avro_type)
    }

    fn parse_array(&mut self, obj: &HashMap<String, JsonValue>) -> Result<AvroType> {
        let items_value = obj
            .get("items")
            .ok_or_else(|| SchemaError::MissingField("items".to_string()))?;
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
        let values_value = obj
            .get("values")
            .ok_or_else(|| SchemaError::MissingField("values".to_string()))?;
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
            .ok_or_else(|| SchemaError::MissingField("size".to_string()))?;

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
        };

        let avro_type = AvroType::Fixed(fixed);

        // Store named type
        self.named_types.insert(name, avro_type.clone());

        Ok(avro_type)
    }

    fn get_required_string(&self, obj: &HashMap<String, JsonValue>, key: &str) -> Result<String> {
        obj.get(key)
            .and_then(|v| v.as_string())
            .map(String::from)
            .ok_or_else(|| SchemaError::MissingField(key.to_string()))
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
}
