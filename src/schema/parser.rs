use serde_json::Value;
use std::collections::HashMap;

use super::error::{Result, SchemaError};
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

    /// Parse JSON text into an Avro schema
    pub fn parse(&mut self, json_text: &str) -> Result<AvroSchema> {
        let value: Value = serde_json::from_str(json_text)?;
        let root = self.parse_type(&value)?;

        Ok(AvroSchema {
            root,
            named_types: self.named_types.clone(),
        })
    }

    fn parse_type(&mut self, value: &Value) -> Result<AvroType> {
        match value {
            // Primitive type as string: "int", "string", etc.
            Value::String(s) => {
                if let Some(primitive) = PrimitiveType::from_str(s) {
                    Ok(AvroType::Primitive(primitive))
                } else {
                    // Must be a type reference
                    Ok(AvroType::TypeRef(s.clone()))
                }
            }

            // Union type as array: ["null", "string"]
            Value::Array(arr) => {
                let types: Result<Vec<_>> = arr.iter().map(|v| self.parse_type(v)).collect();
                Ok(AvroType::Union(types?))
            }

            // Complex type as object
            Value::Object(obj) => {
                let type_name = obj
                    .get("type")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SchemaError::MissingField("type".to_string()))?;

                match type_name {
                    "record" => self.parse_record(obj),
                    "enum" => self.parse_enum(obj),
                    "array" => self.parse_array(obj),
                    "map" => self.parse_map(obj),
                    "fixed" => self.parse_fixed(obj),
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

    fn parse_record(&mut self, obj: &serde_json::Map<String, Value>) -> Result<AvroType> {
        let name = self.get_required_string(obj, "name")?;
        let namespace = self.get_optional_string(obj, "namespace");
        let doc = self.get_optional_string(obj, "doc");
        let aliases = self.get_optional_string_array(obj, "aliases");

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
            let field_type_value = field_obj
                .get("type")
                .ok_or_else(|| SchemaError::MissingField("type".to_string()))?;
            let field_type = self.parse_type(field_type_value)?;

            fields.push(Field {
                name: field_name,
                field_type: Box::new(field_type),
                doc: self.get_optional_string(field_obj, "doc"),
                default: field_obj.get("default").cloned(),
                order: self.get_optional_string(field_obj, "order"),
                aliases: self.get_optional_string_array(field_obj, "aliases"),
            });
        }

        let record = RecordSchema {
            type_name: "record".to_string(),
            name: name.clone(),
            namespace,
            doc,
            aliases,
            fields,
        };

        let avro_type = AvroType::Record(record);

        // Store named type
        self.named_types.insert(name, avro_type.clone());

        Ok(avro_type)
    }

    fn parse_enum(&mut self, obj: &serde_json::Map<String, Value>) -> Result<AvroType> {
        let name = self.get_required_string(obj, "name")?;
        let namespace = self.get_optional_string(obj, "namespace");
        let doc = self.get_optional_string(obj, "doc");
        let aliases = self.get_optional_string_array(obj, "aliases");

        let symbols = obj
            .get("symbols")
            .and_then(|v| v.as_array())
            .ok_or_else(|| SchemaError::MissingField("symbols".to_string()))?
            .iter()
            .map(|v| {
                v.as_str()
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
        };

        let avro_type = AvroType::Enum(enum_schema);

        // Store named type
        self.named_types.insert(name, avro_type.clone());

        Ok(avro_type)
    }

    fn parse_array(&mut self, obj: &serde_json::Map<String, Value>) -> Result<AvroType> {
        let items_value = obj
            .get("items")
            .ok_or_else(|| SchemaError::MissingField("items".to_string()))?;
        let items = self.parse_type(items_value)?;

        Ok(AvroType::Array(ArraySchema {
            type_name: "array".to_string(),
            items: Box::new(items),
            default: obj.get("default").and_then(|v| v.as_array().cloned()),
        }))
    }

    fn parse_map(&mut self, obj: &serde_json::Map<String, Value>) -> Result<AvroType> {
        let values_value = obj
            .get("values")
            .ok_or_else(|| SchemaError::MissingField("values".to_string()))?;
        let values = self.parse_type(values_value)?;

        Ok(AvroType::Map(MapSchema {
            type_name: "map".to_string(),
            values: Box::new(values),
            default: obj
                .get("default")
                .and_then(|v| v.as_object())
                .map(|m| m.clone().into_iter().collect()),
        }))
    }

    fn parse_fixed(&mut self, obj: &serde_json::Map<String, Value>) -> Result<AvroType> {
        let name = self.get_required_string(obj, "name")?;
        let namespace = self.get_optional_string(obj, "namespace");
        let aliases = self.get_optional_string_array(obj, "aliases");

        let size =
            obj.get("size")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| SchemaError::MissingField("size".to_string()))? as usize;

        let fixed = FixedSchema {
            type_name: "fixed".to_string(),
            name: name.clone(),
            namespace,
            aliases,
            size,
        };

        let avro_type = AvroType::Fixed(fixed);

        // Store named type
        self.named_types.insert(name, avro_type.clone());

        Ok(avro_type)
    }

    fn get_required_string(
        &self,
        obj: &serde_json::Map<String, Value>,
        key: &str,
    ) -> Result<String> {
        obj.get(key)
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| SchemaError::MissingField(key.to_string()))
    }

    fn get_optional_string(
        &self,
        obj: &serde_json::Map<String, Value>,
        key: &str,
    ) -> Option<String> {
        obj.get(key).and_then(|v| v.as_str()).map(String::from)
    }

    fn get_optional_string_array(
        &self,
        obj: &serde_json::Map<String, Value>,
        key: &str,
    ) -> Option<Vec<String>> {
        obj.get(key).and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
        })
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

        // Debug print to see what we got
        eprintln!("Parsed schema: {:#?}", schema.root);

        if let AvroType::Record(record) = &schema.root {
            assert_eq!(record.name, "Response");
            assert_eq!(record.fields.len(), 1);

            // Check the field type - should be Union
            if let AvroType::Union(types) = &*record.fields[0].field_type {
                eprintln!("Union types: {:#?}", types);
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
