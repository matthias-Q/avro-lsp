use regex::Regex;
use std::collections::{HashMap, HashSet};

use super::error::{Result, SchemaError};
use super::types::*;

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

    fn validate_type(
        &self,
        avro_type: &AvroType,
        named_types: &HashMap<String, AvroType>,
    ) -> Result<()> {
        match avro_type {
            AvroType::Primitive(_) => Ok(()),
            AvroType::Record(record) => self.validate_record(record, named_types),
            AvroType::Enum(enum_schema) => self.validate_enum(enum_schema),
            AvroType::Array(array) => self.validate_type(&array.items, named_types),
            AvroType::Map(map) => self.validate_type(&map.values, named_types),
            AvroType::Union(types) => self.validate_union(types, named_types),
            AvroType::Fixed(fixed) => self.validate_fixed(fixed),
            AvroType::TypeRef(name) => self.validate_type_reference(name, named_types),
        }
    }

    fn validate_record(
        &self,
        record: &RecordSchema,
        named_types: &HashMap<String, AvroType>,
    ) -> Result<()> {
        // Validate name
        self.validate_name(&record.name)?;

        // Validate namespace if present
        if let Some(namespace) = &record.namespace {
            self.validate_namespace(namespace)?;
        }

        // Validate fields
        if record.fields.is_empty() {
            return Err(SchemaError::Custom(
                "Record must have at least one field".to_string(),
            ));
        }

        for field in &record.fields {
            self.validate_name(&field.name)?;
            self.validate_type(&field.field_type, named_types)?;
        }

        Ok(())
    }

    fn validate_enum(&self, enum_schema: &EnumSchema) -> Result<()> {
        // Validate name
        self.validate_name(&enum_schema.name)?;

        // Validate namespace if present
        if let Some(namespace) = &enum_schema.namespace {
            self.validate_namespace(namespace)?;
        }

        // Validate symbols
        if enum_schema.symbols.is_empty() {
            return Err(SchemaError::Custom(
                "Enum must have at least one symbol".to_string(),
            ));
        }

        // Check for duplicate symbols
        let mut seen = HashSet::new();
        for symbol in &enum_schema.symbols {
            self.validate_name(symbol)?;
            if !seen.insert(symbol) {
                return Err(SchemaError::DuplicateSymbol(symbol.clone()));
            }
        }

        // Validate default if present
        if let Some(default) = &enum_schema.default
            && !enum_schema.symbols.contains(default)
        {
            return Err(SchemaError::Custom(format!(
                "Default value '{}' is not in symbols list",
                default
            )));
        }

        Ok(())
    }

    fn validate_fixed(&self, fixed: &FixedSchema) -> Result<()> {
        // Validate name
        self.validate_name(&fixed.name)?;

        // Validate namespace if present
        if let Some(namespace) = &fixed.namespace {
            self.validate_namespace(namespace)?;
        }

        // Size must be positive
        if fixed.size == 0 {
            return Err(SchemaError::Custom(
                "Fixed size must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_union(
        &self,
        types: &[AvroType],
        named_types: &HashMap<String, AvroType>,
    ) -> Result<()> {
        if types.is_empty() {
            return Err(SchemaError::Custom("Union cannot be empty".to_string()));
        }

        // Check for nested unions
        for t in types {
            if matches!(t, AvroType::Union(_)) {
                return Err(SchemaError::NestedUnion);
            }
        }

        // Check for duplicate types (except named types with different names)
        let mut type_signatures = HashSet::new();
        for t in types {
            let signature = self.type_signature(t);
            if !type_signatures.insert(signature.clone()) {
                return Err(SchemaError::DuplicateUnionType(signature));
            }
        }

        // Validate each type in the union
        for t in types {
            self.validate_type(t, named_types)?;
        }

        Ok(())
    }

    fn validate_type_reference(
        &self,
        name: &str,
        named_types: &HashMap<String, AvroType>,
    ) -> Result<()> {
        // Check if it's a primitive type
        if PrimitiveType::from_str(name).is_some() {
            return Ok(());
        }

        // Check if it's a defined named type
        if named_types.contains_key(name) {
            return Ok(());
        }

        Err(SchemaError::UnknownTypeReference(name.to_string()))
    }

    fn validate_name(&self, name: &str) -> Result<()> {
        if !self.name_regex.is_match(name) {
            return Err(SchemaError::InvalidName(name.to_string()));
        }
        Ok(())
    }

    fn validate_namespace(&self, namespace: &str) -> Result<()> {
        if namespace.is_empty() {
            return Ok(()); // Empty namespace is valid
        }

        for part in namespace.split('.') {
            if !self.name_regex.is_match(part) {
                return Err(SchemaError::InvalidNamespace(namespace.to_string()));
            }
        }
        Ok(())
    }

    /// Generate a type signature for union duplicate detection
    fn type_signature(&self, avro_type: &AvroType) -> String {
        match avro_type {
            AvroType::Primitive(p) => format!("{:?}", p),
            AvroType::Record(r) => format!("record:{}", r.name),
            AvroType::Enum(e) => format!("enum:{}", e.name),
            AvroType::Fixed(f) => format!("fixed:{}", f.name),
            AvroType::Array(_) => "array".to_string(),
            AvroType::Map(_) => "map".to_string(),
            AvroType::Union(_) => "union".to_string(),
            AvroType::TypeRef(name) => format!("ref:{}", name),
        }
    }
}

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
}
