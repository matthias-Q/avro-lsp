use super::*;
use crate::schema::error::SchemaError;
use crate::schema::parser::AvroParser;
use crate::schema::types::*;

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
fn test_validate_duplicate_field_names() {
    let validator = AvroValidator::new();
    let mut parser = AvroParser::new();
    let json = r#"{
        "type": "record",
        "name": "User",
        "fields": [
            {"name": "id", "type": "int"},
            {"name": "username", "type": "string"},
            {"name": "id", "type": "long"}
        ]
    }"#;
    let schema = parser.parse(json).unwrap();

    let result = validator.validate(&schema);
    assert!(result.is_err());

    if let Err(SchemaError::DuplicateFieldName { field, record, .. }) = result {
        assert_eq!(field, "id");
        assert_eq!(record, "User");
    } else {
        panic!("Expected DuplicateFieldName error, got: {:?}", result);
    }
}

#[test]
fn test_validate_record_with_union() {
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
        Ok(_) => {}
        Err(e) => panic!("Validation should pass for valid union, got error: {:?}", e),
    }
}

#[test]
fn test_validate_logical_type_date() {
    let mut parser = AvroParser::new();
    let json = r#"{"type": "int", "logicalType": "date"}"#;
    let schema = parser.parse(json).unwrap();

    let validator = AvroValidator::new();
    assert!(validator.validate(&schema).is_ok());
}

#[test]
fn test_validate_logical_type_timestamp_millis() {
    let mut parser = AvroParser::new();
    let json = r#"{"type": "long", "logicalType": "timestamp-millis"}"#;
    let schema = parser.parse(json).unwrap();

    let validator = AvroValidator::new();
    assert!(validator.validate(&schema).is_ok());
}

#[test]
fn test_validate_logical_type_uuid() {
    let mut parser = AvroParser::new();
    let json = r#"{"type": "string", "logicalType": "uuid"}"#;
    let schema = parser.parse(json).unwrap();

    let validator = AvroValidator::new();
    assert!(validator.validate(&schema).is_ok());
}

#[test]
fn test_validate_decimal_bytes_with_precision() {
    let mut parser = AvroParser::new();
    let json = r#"{"type": "bytes", "logicalType": "decimal", "precision": 10, "scale": 2}"#;
    let schema = parser.parse(json).unwrap();

    let validator = AvroValidator::new();
    assert!(validator.validate(&schema).is_ok());
}

#[test]
fn test_validate_decimal_bytes_no_precision() {
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
    let mut parser = AvroParser::new();
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
    let validator = AvroValidator::new();

    let mut parser = AvroParser::new();
    let schema = parser
        .parse(r#"{"type": "int", "logicalType": "date"}"#)
        .unwrap();
    assert!(validator.validate(&schema).is_ok());

    let mut parser = AvroParser::new();
    let schema = parser
        .parse(r#"{"type": "int", "logicalType": "time-millis"}"#)
        .unwrap();
    assert!(validator.validate(&schema).is_ok());
}

#[test]
fn test_validate_all_long_logical_types() {
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

#[test]
fn test_union_with_two_records_warns() {
    let validator = AvroValidator::new();
    let mut parser = AvroParser::new();
    let json = r#"{
        "type": "record",
        "name": "Container",
        "fields": [{
            "name": "content",
            "type": [
                {
                    "type": "record",
                    "name": "Person",
                    "fields": [{"name": "name", "type": "string"}]
                },
                {
                    "type": "record",
                    "name": "Company",
                    "fields": [{"name": "company_name", "type": "string"}]
                }
            ]
        }]
    }"#;
    let schema = parser.parse(json).unwrap();

    // Should validate successfully (not an error)
    assert!(validator.validate(&schema).is_ok());

    // But should produce a warning
    let warnings = validator.collect_warnings(&schema);
    assert_eq!(warnings.len(), 1);
    assert!(matches!(
        warnings[0],
        crate::schema::warning::SchemaWarning::UnionWithMultipleComplexTypes { .. }
    ));
}

#[test]
fn test_simple_nullable_no_warning() {
    let validator = AvroValidator::new();
    let mut parser = AvroParser::new();
    let json = r#"{
        "type": "record",
        "name": "Test",
        "fields": [{
            "name": "nullable_record",
            "type": ["null", {
                "type": "record",
                "name": "Person",
                "fields": [{"name": "name", "type": "string"}]
            }]
        }]
    }"#;
    let schema = parser.parse(json).unwrap();

    let warnings = validator.collect_warnings(&schema);
    assert!(
        warnings.is_empty(),
        "Simple nullable should not produce warnings"
    );
}

#[test]
fn test_union_with_array_and_record_warns() {
    let validator = AvroValidator::new();
    let mut parser = AvroParser::new();
    let json = r#"{
        "type": "record",
        "name": "Test",
        "fields": [{
            "name": "complex_union",
            "type": [
                {"type": "array", "items": "string"},
                {"type": "record", "name": "Person", "fields": [{"name": "name", "type": "string"}]}
            ]
        }]
    }"#;
    let schema = parser.parse(json).unwrap();

    let warnings = validator.collect_warnings(&schema);
    assert_eq!(warnings.len(), 1);
}

#[test]
fn test_union_of_primitives_no_warning() {
    let validator = AvroValidator::new();
    let mut parser = AvroParser::new();
    let json = r#"{
        "type": "record",
        "name": "Test",
        "fields": [{
            "name": "multi_primitive",
            "type": ["null", "int", "long", "string"]
        }]
    }"#;
    let schema = parser.parse(json).unwrap();

    let warnings = validator.collect_warnings(&schema);
    assert!(warnings.is_empty(), "Union of primitives should not warn");
}
