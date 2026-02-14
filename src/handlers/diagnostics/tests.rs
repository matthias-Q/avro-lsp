#[cfg(test)]
mod tests {
    use crate::handlers::diagnostics::parse_and_validate;

    #[test]
    fn test_missing_comma_between_fields() {
        let text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "name", "type": "string"}
    {"name": "age", "type": "int"}
  ]
}"#;

        let diagnostics = parse_and_validate(text);

        // Should detect the invalid JSON
        assert!(
            !diagnostics.is_empty(),
            "Should have diagnostics for missing comma"
        );
        assert_eq!(diagnostics.len(), 1, "Should have exactly one diagnostic");

        let diag = &diagnostics[0];

        // The diagnostic should indicate a parse/syntax error
        assert!(
            diag.message.contains("parse")
                || diag.message.contains("Parse")
                || diag.message.contains("syntax")
                || diag.message.contains("JSON"),
            "Message should indicate a JSON parse error, got: '{}'",
            diag.message
        );

        // The diagnostic should be somewhere in the schema (not at 0,0)
        assert!(
            diag.range.start.line < 8,
            "Diagnostic should be within the schema bounds"
        );
    }

    #[test]
    fn test_invalid_json_syntax() {
        let text = r#"{
  "type": "record",
  "name": "User"
  "fields": []
}"#;

        let diagnostics = parse_and_validate(text);

        assert!(
            !diagnostics.is_empty(),
            "Should have diagnostics for invalid JSON"
        );
        assert_eq!(diagnostics.len(), 1, "Should have exactly one diagnostic");
    }

    #[test]
    fn test_missing_required_field() {
        let text = r#"{
  "type": "record",
  "name": "User"
}"#;

        let diagnostics = parse_and_validate(text);

        // Should have at least one diagnostic for the incomplete/invalid schema
        assert!(
            !diagnostics.is_empty(),
            "Should have diagnostics for incomplete record schema"
        );

        // The error might be a parse error or validation error
        // Just verify that we detect there's something wrong with this schema
    }

    #[test]
    fn test_valid_schema_no_diagnostics() {
        let text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "name", "type": "string"},
    {"name": "age", "type": "int"}
  ]
}"#;

        let diagnostics = parse_and_validate(text);

        assert!(
            diagnostics.is_empty(),
            "Valid schema should have no diagnostics"
        );
    }

    #[test]
    fn test_invalid_type_reference() {
        let text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "address", "type": "NonExistentType"}
  ]
}"#;

        let diagnostics = parse_and_validate(text);

        assert!(
            !diagnostics.is_empty(),
            "Should have diagnostics for invalid type reference"
        );

        let has_type_error = diagnostics
            .iter()
            .any(|d| d.message.contains("NonExistentType") || d.message.contains("Unknown"));
        assert!(has_type_error, "Should report unknown type reference");
    }

    #[test]
    fn test_duplicate_enum_symbols() {
        let text = r#"{
  "type": "enum",
  "name": "Status",
  "symbols": ["ACTIVE", "INACTIVE", "ACTIVE"]
}"#;

        let diagnostics = parse_and_validate(text);

        assert!(
            !diagnostics.is_empty(),
            "Should have diagnostics for duplicate symbols"
        );

        let has_duplicate_error = diagnostics
            .iter()
            .any(|d| d.message.contains("duplicate") || d.message.contains("Duplicate"));
        assert!(has_duplicate_error, "Should report duplicate enum symbols");
    }

    #[test]
    fn test_invalid_name() {
        let text = r#"{
  "type": "record",
  "name": "123Invalid",
  "fields": []
}"#;

        let diagnostics = parse_and_validate(text);

        assert!(
            !diagnostics.is_empty(),
            "Should have diagnostics for invalid name"
        );

        let has_name_error = diagnostics
            .iter()
            .any(|d| d.message.contains("name") || d.message.contains("Invalid"));
        assert!(has_name_error, "Should report invalid name format");
    }

    #[test]
    fn test_invalid_namespace_positioning() {
        let text = r#"{
  "type": "record",
  "name": "Test",
  "namespace": "123.invalid",
  "fields": [
    {"name": "value", "type": "string"}
  ]
}"#;

        let diagnostics = parse_and_validate(text);

        assert!(
            !diagnostics.is_empty(),
            "Should have diagnostics for invalid namespace"
        );

        eprintln!("Diagnostics: {}", diagnostics.len());
        for (i, d) in diagnostics.iter().enumerate() {
            eprintln!("  {}: {} at {:?}", i, d.message, d.range);
        }

        let diag = &diagnostics[0];

        // The error should be positioned in the record, not at (0,0)
        assert!(
            diag.range.start.line > 0 || diag.range.start.character > 0,
            "Error should not be at position (0,0), got: {:?}",
            diag.range.start
        );

        assert!(
            diag.message.contains("namespace") || diag.message.contains("Namespace"),
            "Message should mention namespace, got: '{}'",
            diag.message
        );
    }

    #[test]
    fn test_logical_type_error_positioning() {
        let text = r#"{
  "type": "record",
  "name": "Test",
  "fields": [
    {
      "name": "bad_uuid",
      "type": {
        "type": "int",
        "logicalType": "uuid"
      }
    }
  ]
}"#;

        let diagnostics = parse_and_validate(text);

        assert!(
            !diagnostics.is_empty(),
            "Should have diagnostics for invalid logical type"
        );

        let diag = &diagnostics[0];

        // The error should be positioned at the primitive object with logical type (line 7-10)
        assert!(
            diag.range.start.line >= 6 && diag.range.start.line <= 10,
            "Error should be positioned at the logical type definition (lines 7-10), got line: {}",
            diag.range.start.line
        );

        assert!(
            diag.message.contains("logical type") || diag.message.contains("uuid"),
            "Message should mention logical type error, got: '{}'",
            diag.message
        );
    }

    #[test]
    fn test_decimal_missing_precision_positioning() {
        let text = r#"{
  "type": "record",
  "name": "Test",
  "fields": [
    {
      "name": "price",
      "type": {
        "type": "bytes",
        "logicalType": "decimal"
      }
    }
  ]
}"#;

        let diagnostics = parse_and_validate(text);

        assert!(
            !diagnostics.is_empty(),
            "Should have diagnostics for missing precision"
        );

        let diag = &diagnostics[0];

        // The error should be positioned at the decimal definition
        assert!(
            diag.range.start.line >= 6 && diag.range.start.line <= 10,
            "Error should be positioned at the decimal definition, got line: {}",
            diag.range.start.line
        );

        assert!(
            diag.message.contains("precision"),
            "Message should mention precision, got: '{}'",
            diag.message
        );
    }

    #[test]
    fn test_duplicate_symbols_positioning() {
        let text = r#"{
  "type": "enum",
  "name": "Status",
  "symbols": ["ACTIVE", "INACTIVE", "ACTIVE"]
}"#;

        let diagnostics = parse_and_validate(text);

        assert!(
            !diagnostics.is_empty(),
            "Should have diagnostics for duplicate symbols"
        );

        let diag = &diagnostics[0];

        // The error should be positioned at the enum (line 1-4), not at (0,0)
        assert!(
            diag.range.start.line <= 4,
            "Error should be positioned at the enum definition, got line: {}",
            diag.range.start.line
        );
    }

    #[test]
    fn test_all_positioning_not_default() {
        // Test that various error types don't default to (0,0)
        let test_cases = vec![
            (
                r#"{"type": "record", "name": "123Bad", "fields": []}"#,
                "invalid name",
            ),
            (
                r#"{"type": "record", "name": "Test", "namespace": "123.bad", "fields": []}"#,
                "invalid namespace",
            ),
            (
                r#"{"type": "enum", "name": "E", "symbols": ["A", "A"]}"#,
                "duplicate symbols",
            ),
        ];

        for (text, description) in test_cases {
            let diagnostics = parse_and_validate(text);

            assert!(
                !diagnostics.is_empty(),
                "Should have diagnostics for {}",
                description
            );

            let diag = &diagnostics[0];

            // None of these should be at the default (0,0) position
            // They should at least have a non-zero line or character
            let is_meaningful_position = diag.range.start.line > 0
                || diag.range.start.character > 0
                || diag.range.end.line > 0
                || diag.range.end.character > 1; // End character > 1 means it's not the default

            assert!(
                is_meaningful_position,
                "Error for '{}' should not be at default position (0,0)-(0,1), got: {:?}",
                description, diag.range
            );
        }
    }

    #[test]
    fn test_nested_union_diagnostic_range() {
        // Test that nested union errors have proper ranges (not 0,0)
        let text = r#"{
  "type": "record",
  "name": "Test",
  "fields": [
    {
      "name": "nested_union_field",
      "type": [["null", "string"]]
    }
  ]
}"#;

        let diagnostics = parse_and_validate(text);

        assert!(
            !diagnostics.is_empty(),
            "Should have diagnostic for nested union"
        );

        let nested_union_diag = diagnostics
            .iter()
            .find(|d| d.message.contains("Nested union"));

        assert!(
            nested_union_diag.is_some(),
            "Should have nested union error, diagnostics: {:?}",
            diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );

        let diag = nested_union_diag.unwrap();

        // The diagnostic should NOT be at the default (0,0) position
        let is_not_default = diag.range.start.line > 0
            || diag.range.start.character > 0
            || diag.range.end.character > 1;

        assert!(
            is_not_default,
            "Nested union diagnostic should not be at default position (0,0)-(0,1), got: {:?}. \
             This means the quick fix won't be offered in the editor!",
            diag.range
        );

        // Ideally should be around line 6 (the field with the nested union)
        assert!(
            diag.range.start.line >= 4 && diag.range.start.line <= 7,
            "Expected nested union error around line 5-6, got line {}",
            diag.range.start.line
        );
    }

    #[test]
    fn test_duplicate_union_type_diagnostic_range() {
        // Test that duplicate union type errors have proper ranges (not 0,0)
        let text = r#"{
  "type": "record",
  "name": "Test",
  "fields": [
    {
      "name": "duplicate_field",
      "type": ["null", "string", "null"]
    }
  ]
}"#;

        let diagnostics = parse_and_validate(text);

        assert!(
            !diagnostics.is_empty(),
            "Should have diagnostic for duplicate union types"
        );

        let duplicate_diag = diagnostics
            .iter()
            .find(|d| d.message.contains("Duplicate type") || d.message.contains("duplicate"));

        assert!(
            duplicate_diag.is_some(),
            "Should have duplicate union type error"
        );

        let diag = duplicate_diag.unwrap();

        // The diagnostic should NOT be at the default (0,0) position
        let is_not_default = diag.range.start.line > 0
            || diag.range.start.character > 0
            || diag.range.end.character > 1;

        assert!(
            is_not_default,
            "Duplicate union type diagnostic should not be at default position (0,0)-(0,1), got: {:?}. \
             This means the quick fix won't be offered in the editor!",
            diag.range
        );

        // Ideally should be around line 6 (the field with the duplicate union)
        assert!(
            diag.range.start.line >= 4 && diag.range.start.line <= 7,
            "Expected duplicate union type error around line 5-6, got line {}",
            diag.range.start.line
        );
    }
}
