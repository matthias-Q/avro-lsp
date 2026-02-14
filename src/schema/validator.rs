use regex::Regex;
use std::collections::{HashMap, HashSet};

use super::error::{Result, SchemaError};
use super::types::*;

/// External type resolver for cross-file type checking
pub trait TypeResolver {
    /// Check if a type name exists (either locally or in workspace)
    fn type_exists(&self, name: &str) -> bool;
}

/// Default resolver that only checks local types
struct LocalTypeResolver<'a> {
    named_types: &'a HashMap<String, AvroType>,
}

impl<'a> TypeResolver for LocalTypeResolver<'a> {
    fn type_exists(&self, name: &str) -> bool {
        self.named_types.contains_key(name)
    }
}

pub struct AvroValidator {
    name_regex: Regex,
}

impl AvroValidator {
    pub fn new() -> Self {
        Self {
            name_regex: Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap(),
        }
    }

    /// Validate an Avro schema according to specification rules
    pub fn validate(&self, schema: &AvroSchema) -> Result<()> {
        self.validate_type(&schema.root, &schema.named_types)?;
        Ok(())
    }

    /// Validate with a custom type resolver (for workspace-aware validation)
    pub fn validate_with_resolver(
        &self,
        schema: &AvroSchema,
        resolver: &dyn TypeResolver,
    ) -> Result<()> {
        self.validate_type_with_resolver(&schema.root, &schema.named_types, resolver)?;
        Ok(())
    }

    fn validate_type(
        &self,
        avro_type: &AvroType,
        named_types: &HashMap<String, AvroType>,
    ) -> Result<()> {
        let resolver = LocalTypeResolver { named_types };
        self.validate_type_with_resolver(avro_type, named_types, &resolver)
    }

    fn validate_type_with_resolver(
        &self,
        avro_type: &AvroType,
        named_types: &HashMap<String, AvroType>,
        resolver: &dyn TypeResolver,
    ) -> Result<()> {
        match avro_type {
            AvroType::Primitive(_) => Ok(()),
            AvroType::PrimitiveObject(primitive) => {
                self.validate_primitive_with_logical_type(primitive)
            }
            AvroType::Record(record) => {
                self.validate_record_with_resolver(record, named_types, resolver)
            }
            AvroType::Enum(enum_schema) => self.validate_enum(enum_schema),
            AvroType::Array(array) => {
                self.validate_type_with_resolver(&array.items, named_types, resolver)
            }
            AvroType::Map(map) => {
                self.validate_type_with_resolver(&map.values, named_types, resolver)
            }
            AvroType::Union(types) => {
                self.validate_union_with_resolver(types, named_types, resolver)
            }
            AvroType::Fixed(fixed) => self.validate_fixed(fixed),
            AvroType::TypeRef(type_ref) => {
                self.validate_type_reference_with_resolver(&type_ref.name, named_types, resolver)
            }
            AvroType::Invalid(_) => {
                // Invalid types are already marked as errors during parsing
                // Skip validation as the error is already collected
                Ok(())
            }
        }
    }

    #[allow(dead_code)] // Internal helper, kept for backward compatibility
    fn validate_record(
        &self,
        record: &RecordSchema,
        named_types: &HashMap<String, AvroType>,
    ) -> Result<()> {
        let resolver = LocalTypeResolver { named_types };
        self.validate_record_with_resolver(record, named_types, &resolver)
    }

    fn validate_record_with_resolver(
        &self,
        record: &RecordSchema,
        named_types: &HashMap<String, AvroType>,
        resolver: &dyn TypeResolver,
    ) -> Result<()> {
        // Validate name
        self.validate_name_with_range(&record.name, record.name_range)?;

        // Validate namespace if present
        if let Some(namespace) = &record.namespace {
            self.validate_namespace_with_range(namespace, record.namespace_range)?;
        }

        // Validate fields
        if record.fields.is_empty() {
            return Err(SchemaError::Custom {
                message: "Record must have at least one field".to_string(),
                range: record.range,
            });
        }

        for field in &record.fields {
            self.validate_name_with_range(&field.name, field.name_range)?;
            self.validate_type_with_resolver(&field.field_type, named_types, resolver)?;

            // Validate default value if present
            if let Some(default_value) = &field.default {
                self.validate_default_value(
                    default_value,
                    &field.field_type,
                    named_types,
                    field.range,
                )?;
            }
        }

        Ok(())
    }

    fn validate_enum(&self, enum_schema: &EnumSchema) -> Result<()> {
        // Validate name
        self.validate_name_with_range(&enum_schema.name, enum_schema.name_range)?;

        // Validate namespace if present
        if let Some(namespace) = &enum_schema.namespace {
            self.validate_namespace_with_range(namespace, enum_schema.namespace_range)?;
        }

        // Validate symbols
        if enum_schema.symbols.is_empty() {
            return Err(SchemaError::Custom {
                message: "Enum must have at least one symbol".to_string(),
                range: enum_schema.range,
            });
        }

        // Check for duplicate symbols
        let mut seen = HashSet::new();
        for symbol in &enum_schema.symbols {
            self.validate_name(symbol)?;
            if !seen.insert(symbol) {
                return Err(SchemaError::DuplicateSymbol {
                    symbol: symbol.clone(),
                    first_occurrence: None,
                    duplicate_occurrence: None,
                });
            }
        }

        // Validate default if present
        if let Some(default) = &enum_schema.default
            && !enum_schema.symbols.contains(default)
        {
            return Err(SchemaError::Custom {
                message: format!("Default value '{}' is not in symbols list", default),
                range: enum_schema.range,
            });
        }

        Ok(())
    }

    fn validate_fixed(&self, fixed: &FixedSchema) -> Result<()> {
        // Validate name
        self.validate_name_with_range(&fixed.name, fixed.name_range)?;

        // Validate namespace if present
        if let Some(namespace) = &fixed.namespace {
            self.validate_namespace_with_range(namespace, fixed.namespace_range)?;
        }

        // Size must be positive
        if fixed.size == 0 {
            return Err(SchemaError::Custom {
                message: "Fixed size must be greater than 0".to_string(),
                range: fixed.range,
            });
        }

        // Validate logical type if present
        if let Some(logical_type) = &fixed.logical_type {
            self.validate_logical_type_for_fixed(logical_type, fixed)?;
        }

        Ok(())
    }

    fn validate_logical_type_for_fixed(
        &self,
        logical_type: &str,
        fixed: &FixedSchema,
    ) -> Result<()> {
        match logical_type {
            "decimal" => {
                // Decimal requires precision
                if fixed.precision.is_none() {
                    return Err(SchemaError::Custom {
                        message: "Decimal logical type requires 'precision' attribute".to_string(),
                        range: fixed.range,
                    });
                }
                // Scale is optional but must be <= precision if present
                if let (Some(precision), Some(scale)) = (fixed.precision, fixed.scale)
                    && scale > precision
                {
                    return Err(SchemaError::Custom {
                        message: format!(
                            "Decimal scale ({}) cannot be greater than precision ({})",
                            scale, precision
                        ),
                        range: fixed.range,
                    });
                }
            }
            "duration" => {
                // Duration must be exactly 12 bytes
                if fixed.size != 12 {
                    return Err(SchemaError::Custom {
                        message: "Duration logical type requires fixed size of 12 bytes"
                            .to_string(),
                        range: fixed.range,
                    });
                }
            }
            _ => {
                return Err(SchemaError::Custom {
                    message: format!("Unknown logical type '{}' for fixed type", logical_type),
                    range: fixed.range,
                });
            }
        }
        Ok(())
    }

    fn validate_primitive_with_logical_type(&self, primitive: &PrimitiveSchema) -> Result<()> {
        if let Some(logical_type) = &primitive.logical_type {
            // Validate logical type based on primitive type
            match (primitive.primitive_type, logical_type.as_str()) {
                // int can have: date, time-millis
                (PrimitiveType::Int, "date") => Ok(()),
                (PrimitiveType::Int, "time-millis") => Ok(()),

                // long can have: time-micros, timestamp-millis, timestamp-micros,
                // local-timestamp-millis, local-timestamp-micros
                (PrimitiveType::Long, "time-micros") => Ok(()),
                (PrimitiveType::Long, "timestamp-millis") => Ok(()),
                (PrimitiveType::Long, "timestamp-micros") => Ok(()),
                (PrimitiveType::Long, "local-timestamp-millis") => Ok(()),
                (PrimitiveType::Long, "local-timestamp-micros") => Ok(()),

                // string can have: uuid
                (PrimitiveType::String, "uuid") => Ok(()),

                // bytes can have: decimal (with precision/scale)
                (PrimitiveType::Bytes, "decimal") => {
                    // Decimal requires precision
                    if primitive.precision.is_none() {
                        return Err(SchemaError::Custom {
                            message: "Decimal logical type requires 'precision' attribute"
                                .to_string(),
                            range: primitive.range,
                        });
                    }
                    // Scale is optional but must be <= precision if present
                    if let (Some(precision), Some(scale)) = (primitive.precision, primitive.scale)
                        && scale > precision
                    {
                        return Err(SchemaError::Custom {
                            message: format!(
                                "Decimal scale ({}) cannot be greater than precision ({})",
                                scale, precision
                            ),
                            range: primitive.range,
                        });
                    }
                    Ok(())
                }

                // Invalid combinations
                _ => {
                    // Provide helpful error message with required base type
                    let required_type = match logical_type.as_str() {
                        "date" | "time-millis" => "int",
                        "time-micros"
                        | "timestamp-millis"
                        | "timestamp-micros"
                        | "local-timestamp-millis"
                        | "local-timestamp-micros" => "long",
                        "uuid" => "string",
                        "decimal" => "bytes or fixed",
                        "duration" => "fixed",
                        _ => "unknown",
                    };

                    Err(SchemaError::Custom {
                        message: format!(
                            "Invalid logical type '{}' for primitive type '{:?}' - requires {}",
                            logical_type, primitive.primitive_type, required_type
                        ),
                        range: primitive.range,
                    })
                }
            }
        } else {
            // No logical type is valid
            Ok(())
        }
    }

    #[allow(dead_code)] // Internal helper, kept for backward compatibility
    fn validate_union(
        &self,
        types: &[AvroType],
        named_types: &HashMap<String, AvroType>,
    ) -> Result<()> {
        let resolver = LocalTypeResolver { named_types };
        self.validate_union_with_resolver(types, named_types, &resolver)
    }

    fn validate_union_with_resolver(
        &self,
        types: &[AvroType],
        named_types: &HashMap<String, AvroType>,
        resolver: &dyn TypeResolver,
    ) -> Result<()> {
        if types.is_empty() {
            return Err(SchemaError::Custom {
                message: "Union cannot be empty".to_string(),
                range: None,
            });
        }

        // Check for nested unions
        for t in types {
            if matches!(t, AvroType::Union(_)) {
                return Err(SchemaError::NestedUnion { range: None });
            }
        }

        // Check for duplicate types (except named types with different names)
        let mut type_signatures = HashSet::new();
        for t in types {
            let signature = self.type_signature(t);
            if !type_signatures.insert(signature.clone()) {
                return Err(SchemaError::DuplicateUnionType {
                    type_signature: signature,
                    range: None,
                });
            }
        }

        // Validate each type in the union
        for t in types {
            self.validate_type_with_resolver(t, named_types, resolver)?;
        }

        Ok(())
    }

    #[allow(dead_code)] // Internal helper, kept for backward compatibility
    fn validate_type_reference(
        &self,
        name: &str,
        named_types: &HashMap<String, AvroType>,
    ) -> Result<()> {
        let resolver = LocalTypeResolver { named_types };
        self.validate_type_reference_with_resolver(name, named_types, &resolver)
    }

    fn validate_type_reference_with_resolver(
        &self,
        name: &str,
        named_types: &HashMap<String, AvroType>,
        resolver: &dyn TypeResolver,
    ) -> Result<()> {
        // Check if it's a primitive type
        if PrimitiveType::parse(name).is_some() {
            return Ok(());
        }

        // Check if it's a defined named type (local or workspace)
        if named_types.contains_key(name) || resolver.type_exists(name) {
            return Ok(());
        }

        Err(SchemaError::UnknownTypeReference {
            type_name: name.to_string(),
            range: None,
        })
    }

    #[allow(dead_code)] // Used in tests
    fn validate_name(&self, name: &str) -> Result<()> {
        self.validate_name_with_range(name, None)
    }

    fn validate_name_with_range(
        &self,
        name: &str,
        range: Option<async_lsp::lsp_types::Range>,
    ) -> Result<()> {
        if !self.name_regex.is_match(name) {
            let suggested = fix_invalid_name(name);
            return Err(SchemaError::InvalidName {
                name: name.to_string(),
                range,
                suggested: Some(suggested),
            });
        }
        Ok(())
    }

    #[allow(dead_code)] // Used in tests
    fn validate_namespace(&self, namespace: &str) -> Result<()> {
        self.validate_namespace_with_range(namespace, None)
    }

    fn validate_namespace_with_range(
        &self,
        namespace: &str,
        range: Option<async_lsp::lsp_types::Range>,
    ) -> Result<()> {
        if namespace.is_empty() {
            return Ok(()); // Empty namespace is valid
        }

        for part in namespace.split('.') {
            if !self.name_regex.is_match(part) {
                let suggested = fix_invalid_namespace(namespace);
                return Err(SchemaError::InvalidNamespace {
                    namespace: namespace.to_string(),
                    range,
                    suggested: Some(suggested),
                });
            }
        }
        Ok(())
    }

    /// Validate that a default value matches the field's type
    fn validate_default_value(
        &self,
        default: &serde_json::Value,
        field_type: &AvroType,
        named_types: &HashMap<String, AvroType>,
        field_range: Option<async_lsp::lsp_types::Range>,
    ) -> Result<()> {
        use serde_json::Value;

        match field_type {
            AvroType::Primitive(prim) => match prim {
                PrimitiveType::Null => {
                    if !default.is_null() {
                        return Err(SchemaError::Custom {
                            message: "Default value for null type must be null".to_string(),
                            range: field_range,
                        });
                    }
                }
                PrimitiveType::Boolean => {
                    if !default.is_boolean() {
                        return Err(SchemaError::Custom {
                            message: "Default value for boolean type must be true or false"
                                .to_string(),
                            range: field_range,
                        });
                    }
                }
                PrimitiveType::Int | PrimitiveType::Long => {
                    if !default.is_number() {
                        return Err(SchemaError::Custom {
                            message: format!("Default value for {:?} type must be a number", prim),
                            range: field_range,
                        });
                    }
                }
                PrimitiveType::Float | PrimitiveType::Double => {
                    if !default.is_number() {
                        return Err(SchemaError::Custom {
                            message: format!("Default value for {:?} type must be a number", prim),
                            range: field_range,
                        });
                    }
                }
                PrimitiveType::String => {
                    if !default.is_string() {
                        return Err(SchemaError::Custom {
                            message: "Default value for string type must be a string".to_string(),
                            range: field_range,
                        });
                    }
                }
                PrimitiveType::Bytes => {
                    // Bytes default must be a string (Unicode code points 0-255 mapped to bytes)
                    if !default.is_string() {
                        return Err(SchemaError::Custom {
                            message: "Default value for bytes type must be a string".to_string(),
                            range: field_range,
                        });
                    }
                }
            },
            AvroType::PrimitiveObject(prim_obj) => {
                // PrimitiveObject with logical types validate the same as their base primitive type
                // The logical type doesn't change the default value requirements
                match prim_obj.primitive_type {
                    PrimitiveType::Null => {
                        if !default.is_null() {
                            return Err(SchemaError::Custom {
                                message: "Default value for null type must be null".to_string(),
                                range: field_range,
                            });
                        }
                    }
                    PrimitiveType::Boolean => {
                        if !default.is_boolean() {
                            return Err(SchemaError::Custom {
                                message: "Default value for boolean type must be true or false"
                                    .to_string(),
                                range: field_range,
                            });
                        }
                    }
                    PrimitiveType::Int | PrimitiveType::Long => {
                        if !default.is_number() {
                            return Err(SchemaError::Custom {
                                message: format!(
                                    "Default value for {:?} type must be a number",
                                    prim_obj.primitive_type
                                ),
                                range: field_range,
                            });
                        }
                    }
                    PrimitiveType::Float | PrimitiveType::Double => {
                        if !default.is_number() {
                            return Err(SchemaError::Custom {
                                message: format!(
                                    "Default value for {:?} type must be a number",
                                    prim_obj.primitive_type
                                ),
                                range: field_range,
                            });
                        }
                    }
                    PrimitiveType::String => {
                        if !default.is_string() {
                            return Err(SchemaError::Custom {
                                message: "Default value for string type must be a string"
                                    .to_string(),
                                range: field_range,
                            });
                        }
                    }
                    PrimitiveType::Bytes => {
                        if !default.is_string() {
                            return Err(SchemaError::Custom {
                                message: "Default value for bytes type must be a string"
                                    .to_string(),
                                range: field_range,
                            });
                        }
                    }
                }
            }
            AvroType::Record(_) => {
                if !default.is_object() {
                    return Err(SchemaError::Custom {
                        message: "Default value for record type must be an object".to_string(),
                        range: field_range,
                    });
                }
            }
            AvroType::Enum(enum_schema) => {
                if let Value::String(s) = default {
                    if !enum_schema.symbols.contains(s) {
                        return Err(SchemaError::Custom {
                            message: format!("Default value '{}' is not a valid enum symbol", s),
                            range: field_range,
                        });
                    }
                } else {
                    return Err(SchemaError::Custom {
                        message: "Default value for enum type must be a string".to_string(),
                        range: field_range,
                    });
                }
            }
            AvroType::Array(_) => {
                if !default.is_array() {
                    return Err(SchemaError::Custom {
                        message: "Default value for array type must be an array".to_string(),
                        range: field_range,
                    });
                }
            }
            AvroType::Map(_) => {
                if !default.is_object() {
                    return Err(SchemaError::Custom {
                        message: "Default value for map type must be an object".to_string(),
                        range: field_range,
                    });
                }
            }
            AvroType::Fixed(_) => {
                // Fixed default must be a string
                if !default.is_string() {
                    return Err(SchemaError::Custom {
                        message: "Default value for fixed type must be a string".to_string(),
                        range: field_range,
                    });
                }
            }
            AvroType::Union(types) => {
                // Default value must match the FIRST type in the union (per Avro spec)
                if let Some(first_type) = types.first() {
                    self.validate_default_value(default, first_type, named_types, field_range)?;
                } else {
                    return Err(SchemaError::Custom {
                        message: "Union must have at least one type".to_string(),
                        range: field_range,
                    });
                }
            }
            AvroType::TypeRef(type_ref) => {
                // Resolve the reference and validate against the actual type
                if let Some(resolved_type) = named_types.get(&type_ref.name) {
                    self.validate_default_value(default, resolved_type, named_types, field_range)?;
                }
                // If type not found, it will be caught by type reference validation
            }
            AvroType::Invalid(_) => {
                // Invalid types are already marked as errors during parsing
                // Skip default value validation as the type itself is invalid
            }
        }

        Ok(())
    }

    /// Generate a type signature for union duplicate detection
    fn type_signature(&self, avro_type: &AvroType) -> String {
        match avro_type {
            AvroType::Primitive(p) => format!("{:?}", p),
            AvroType::PrimitiveObject(p) => format!("{:?}", p.primitive_type),
            AvroType::Record(r) => format!("record:{}", r.name),
            AvroType::Enum(e) => format!("enum:{}", e.name),
            AvroType::Fixed(f) => format!("fixed:{}", f.name),
            AvroType::Array(_) => "array".to_string(),
            AvroType::Map(_) => "map".to_string(),
            AvroType::Union(_) => "union".to_string(),
            AvroType::TypeRef(type_ref) => format!("ref:{}", type_ref.name),
            AvroType::Invalid(invalid) => format!("invalid:{}", invalid.type_name),
        }
    }
}

/// Fix an invalid name by making it valid according to Avro naming rules
fn fix_invalid_name(name: &str) -> String {
    if name.is_empty() {
        return "field".to_string();
    }

    let mut result = String::new();

    for (i, ch) in name.chars().enumerate() {
        if i == 0 {
            // First character must be [A-Za-z_]
            if ch.is_ascii_alphabetic() || ch == '_' {
                result.push(ch);
            } else if ch.is_ascii_digit() {
                // If starts with digit, prepend underscore
                result.push('_');
                result.push(ch);
            } else {
                // Replace invalid character with underscore
                result.push('_');
            }
        } else {
            // Subsequent characters must be [A-Za-z0-9_]
            if ch.is_ascii_alphanumeric() || ch == '_' {
                result.push(ch);
            } else {
                // Replace invalid character with underscore
                result.push('_');
            }
        }
    }

    if result.is_empty() {
        result = "field".to_string();
    }

    result
}

/// Fix an invalid namespace by removing invalid segments
fn fix_invalid_namespace(namespace: &str) -> String {
    let name_regex = Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap();

    let valid_parts: Vec<&str> = namespace
        .split('.')
        .filter(|part| name_regex.is_match(part))
        .collect();

    if valid_parts.is_empty() {
        String::new()
    } else {
        valid_parts.join(".")
    }
}

/// Calculate Levenshtein distance between two strings
impl Default for AvroValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_name() {
        let validator = AvroValidator::new();
        assert!(validator.validate_name("ValidName").is_ok());
        assert!(validator.validate_name("_underscore").is_ok());
        assert!(validator.validate_name("name123").is_ok());
        assert!(validator.validate_name("123invalid").is_err());
        assert!(validator.validate_name("invalid-name").is_err());
    }

    #[test]
    fn test_validate_namespace() {
        let validator = AvroValidator::new();
        assert!(validator.validate_namespace("com.example").is_ok());
        assert!(validator.validate_namespace("").is_ok());
        assert!(validator.validate_namespace("a.b.c").is_ok());
        assert!(validator.validate_namespace("123.invalid").is_err());
    }

    #[test]
    fn test_validate_duplicate_symbols() {
        let validator = AvroValidator::new();
        let enum_schema = EnumSchema {
            type_name: "enum".to_string(),
            name: "Status".to_string(),
            namespace: None,
            doc: None,
            aliases: None,
            symbols: vec!["A".to_string(), "B".to_string(), "A".to_string()],
            default: None,
            range: None,
            name_range: None,
            namespace_range: None,
        };
        assert!(validator.validate_enum(&enum_schema).is_err());
    }

    #[test]
    fn test_validate_record_with_union() {
        use crate::schema::parser::AvroParser;

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

        let validator = AvroValidator::new();
        let result = validator.validate(&schema);

        match result {
            Ok(_) => {} // This is what we expect
            Err(e) => panic!("Validation should pass for valid union, got error: {:?}", e),
        }
    }

    #[test]
    fn test_validate_logical_type_date() {
        use crate::schema::parser::AvroParser;

        let mut parser = AvroParser::new();
        let json = r#"{"type": "int", "logicalType": "date"}"#;
        let schema = parser.parse(json).unwrap();

        let validator = AvroValidator::new();
        assert!(validator.validate(&schema).is_ok());
    }

    #[test]
    fn test_validate_logical_type_timestamp_millis() {
        use crate::schema::parser::AvroParser;

        let mut parser = AvroParser::new();
        let json = r#"{"type": "long", "logicalType": "timestamp-millis"}"#;
        let schema = parser.parse(json).unwrap();

        let validator = AvroValidator::new();
        assert!(validator.validate(&schema).is_ok());
    }

    #[test]
    fn test_validate_logical_type_uuid() {
        use crate::schema::parser::AvroParser;

        let mut parser = AvroParser::new();
        let json = r#"{"type": "string", "logicalType": "uuid"}"#;
        let schema = parser.parse(json).unwrap();

        let validator = AvroValidator::new();
        assert!(validator.validate(&schema).is_ok());
    }

    #[test]
    fn test_validate_decimal_bytes_with_precision() {
        use crate::schema::parser::AvroParser;

        let mut parser = AvroParser::new();
        let json = r#"{"type": "bytes", "logicalType": "decimal", "precision": 10, "scale": 2}"#;
        let schema = parser.parse(json).unwrap();

        let validator = AvroValidator::new();
        assert!(validator.validate(&schema).is_ok());
    }

    #[test]
    fn test_validate_decimal_bytes_no_precision() {
        use crate::schema::parser::AvroParser;

        let mut parser = AvroParser::new();
        let json = r#"{"type": "bytes", "logicalType": "decimal"}"#;
        let schema = parser.parse(json).unwrap();

        let validator = AvroValidator::new();
        let result = validator.validate(&schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("precision"));
    }

    #[test]
    fn test_validate_decimal_scale_exceeds_precision() {
        use crate::schema::parser::AvroParser;

        let mut parser = AvroParser::new();
        let json = r#"{"type": "bytes", "logicalType": "decimal", "precision": 5, "scale": 10}"#;
        let schema = parser.parse(json).unwrap();

        let validator = AvroValidator::new();
        let result = validator.validate(&schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("scale"));
    }

    #[test]
    fn test_validate_invalid_logical_type_combination() {
        use crate::schema::parser::AvroParser;

        let mut parser = AvroParser::new();
        // uuid should only be on string, not int
        let json = r#"{"type": "int", "logicalType": "uuid"}"#;
        let schema = parser.parse(json).unwrap();

        let validator = AvroValidator::new();
        let result = validator.validate(&schema);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid logical type")
        );
    }

    #[test]
    fn test_validate_all_int_logical_types() {
        use crate::schema::parser::AvroParser;

        let validator = AvroValidator::new();

        // date
        let mut parser = AvroParser::new();
        let schema = parser
            .parse(r#"{"type": "int", "logicalType": "date"}"#)
            .unwrap();
        assert!(validator.validate(&schema).is_ok());

        // time-millis
        let mut parser = AvroParser::new();
        let schema = parser
            .parse(r#"{"type": "int", "logicalType": "time-millis"}"#)
            .unwrap();
        assert!(validator.validate(&schema).is_ok());
    }

    #[test]
    fn test_validate_all_long_logical_types() {
        use crate::schema::parser::AvroParser;

        let validator = AvroValidator::new();

        let logical_types = vec![
            "time-micros",
            "timestamp-millis",
            "timestamp-micros",
            "local-timestamp-millis",
            "local-timestamp-micros",
        ];

        for logical_type in logical_types {
            let mut parser = AvroParser::new();
            let json = format!(r#"{{"type": "long", "logicalType": "{}"}}"#, logical_type);
            let schema = parser.parse(&json).unwrap();
            assert!(
                validator.validate(&schema).is_ok(),
                "Failed for logical type: {}",
                logical_type
            );
        }
    }
}
