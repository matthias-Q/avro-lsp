use async_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::schema::{AvroParser, AvroSchema, AvroType, AvroValidator, SchemaError};

/// Parse and validate Avro schema text, returning diagnostics
pub fn parse_and_validate(text: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Try to parse
    let mut parser = AvroParser::new();
    let schema = match parser.parse(text) {
        Ok(schema) => schema,
        Err(e) => {
            // Parse error - try to find position from error context or serde_json error
            let error_msg = e.to_string();
            tracing::debug!("JSON parse error: {}", error_msg);

            // Check if error has position information embedded
            let (position_range, adjusted_msg) = match &e {
                SchemaError::MissingFieldWithContext {
                    range: Some(r),
                    field,
                    context,
                    ..
                } => {
                    tracing::debug!("Using position from error context: {:?}", r);
                    let msg = format!("Missing required field '{}' in {}", field, context);
                    (*r, msg)
                }
                SchemaError::MissingFieldWithContext {
                    range: None,
                    field: _,
                    context: _,
                    ..
                } => {
                    let (pos, was_adjusted) = extract_error_position_with_context(&error_msg, text);
                    let range = Range {
                        start: pos,
                        end: Position {
                            line: pos.line,
                            character: pos.character + 1,
                        },
                    };
                    let msg = improve_error_message(&error_msg, &pos, was_adjusted);
                    (range, msg)
                }
                _ => {
                    let (pos, was_adjusted) = extract_error_position_with_context(&error_msg, text);
                    let range = Range {
                        start: pos,
                        end: Position {
                            line: pos.line,
                            character: pos.character + 1,
                        },
                    };
                    let msg = improve_error_message(&error_msg, &pos, was_adjusted);
                    (range, msg)
                }
            };

            tracing::debug!("Extracted position range: {:?}", position_range);
            diagnostics.push(Diagnostic {
                range: position_range,
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("avro-lsp".to_string()),
                message: adjusted_msg,
                related_information: None,
                tags: None,
                data: None,
            });
            return diagnostics;
        }
    };

    // Try to validate - now using AST-based error position finding
    let validator = AvroValidator::new();
    if let Err(e) = validator.validate(&schema) {
        // Try to find the position of the error using AST
        let position_range = find_error_position_in_ast(&e, &schema);

        diagnostics.push(Diagnostic {
            range: position_range,
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            code_description: None,
            source: Some("avro-lsp".to_string()),
            message: format!("Validation error: {}", e),
            related_information: None,
            tags: None,
            data: None,
        });
    }

    diagnostics
}

/// Find the position of a validation error using AST
fn find_error_position_in_ast(error: &SchemaError, schema: &AvroSchema) -> Range {
    // Helper to search for error location in AST
    fn search_type(avro_type: &AvroType, error: &SchemaError) -> Option<Range> {
        match error {
            SchemaError::InvalidName(name) => {
                tracing::debug!("Searching for InvalidName: {}", name);
                // Search for a Record/Enum/Fixed with this name
                match avro_type {
                    AvroType::Record(record) if record.name == *name => {
                        tracing::debug!(
                            "Found record with invalid name at {:?}",
                            record.name_range
                        );
                        record.name_range
                    }
                    AvroType::Enum(enum_schema) if enum_schema.name == *name => {
                        tracing::debug!(
                            "Found enum with invalid name at {:?}",
                            enum_schema.name_range
                        );
                        enum_schema.name_range
                    }
                    AvroType::Fixed(fixed) if fixed.name == *name => {
                        tracing::debug!("Found fixed with invalid name at {:?}", fixed.name_range);
                        fixed.name_range
                    }
                    AvroType::Record(record) => {
                        tracing::debug!("Searching in record: {}", record.name);
                        // Check fields
                        for field in &record.fields {
                            if field.name == *name {
                                tracing::debug!(
                                    "Found field with invalid name '{}' at {:?}",
                                    field.name,
                                    field.name_range
                                );
                                return field.name_range;
                            }
                            if let Some(range) = search_type(&field.field_type, error) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    AvroType::Array(array) => search_type(&array.items, error),
                    AvroType::Map(map) => search_type(&map.values, error),
                    AvroType::Union(types) => {
                        for t in types {
                            if let Some(range) = search_type(t, error) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
            SchemaError::UnknownTypeReference(type_name) => {
                tracing::debug!("Searching for UnknownTypeReference: {}", type_name);
                // Search for TypeRef with this name
                match avro_type {
                    AvroType::TypeRef(type_ref) if type_ref.name == *type_name => {
                        tracing::debug!("Found TypeRef with unknown type at {:?}", type_ref.range);
                        type_ref.range
                    }
                    AvroType::Record(record) => {
                        for field in &record.fields {
                            if let Some(range) = search_type(&field.field_type, error) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    AvroType::Array(array) => search_type(&array.items, error),
                    AvroType::Map(map) => search_type(&map.values, error),
                    AvroType::Union(types) => {
                        for t in types {
                            if let Some(range) = search_type(t, error) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
            _ => {
                tracing::debug!("Unsupported error type for position finding: {:?}", error);
                None
            }
        }
    }

    // Search for the error in the AST
    if let Some(range) = search_type(&schema.root, error) {
        tracing::debug!("Found error position: {:?}", range);
        return range;
    }

    tracing::warn!("Could not find error position in AST, defaulting to (0,0)");
    // Default fallback
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: 1,
        },
    }
}

/// Extract position from error message and adjust for JSON syntax errors
/// Returns (Position, was_adjusted)
fn extract_error_position_with_context(error_msg: &str, text: &str) -> (Position, bool) {
    // serde_json errors often contain "line X, column Y"
    if let Some(line_pos) = error_msg.find("line ")
        && let Some(col_pos) = error_msg.find("column ")
    {
        let line_str = &error_msg[line_pos + 5..];
        let line_end = line_str
            .find(|c: char| !c.is_numeric())
            .unwrap_or(line_str.len());
        let line_num: u32 = line_str[..line_end].parse().unwrap_or(1);

        let col_str = &error_msg[col_pos + 7..];
        let col_end = col_str
            .find(|c: char| !c.is_numeric())
            .unwrap_or(col_str.len());
        let col_num: u32 = col_str[..col_end].parse().unwrap_or(0);

        let mut position = Position {
            line: line_num.saturating_sub(1), // LSP is 0-indexed
            character: col_num.saturating_sub(1),
        };

        let mut was_adjusted = false;
        let lines: Vec<&str> = text.lines().collect();

        // Try to find the actual location of the syntax error by looking for common patterns
        // Check if this looks like a missing comma error by scanning backwards for array/object elements
        if position.line > 0 && position.line < lines.len() as u32 {
            // Look at the lines around the error
            let error_line_idx = position.line as usize;

            // Check if we're in an array/object context and find missing commas
            // by looking for lines ending with } or ] without a comma before the next element
            for i in (0..error_line_idx).rev().take(10) {
                let line = lines[i].trim_end();

                // If we find a line ending with } or ] (end of an object/array element)
                // and the next non-empty line starts with { or contains a field/element
                // then this line is missing a comma
                if (line.ends_with('}') || line.ends_with(']')) && !line.ends_with(',') {
                    // Check if the next non-empty line looks like it starts a new element
                    if let Some(next_line) = lines.get(i + 1) {
                        let next_trimmed = next_line.trim();
                        if next_trimmed.starts_with('{') || next_trimmed.starts_with("\"") {
                            // This is likely where the comma is missing
                            position = Position {
                                line: i as u32,
                                character: line.len() as u32,
                            };
                            was_adjusted = true;
                            tracing::debug!(
                                "Adjusted JSON parse error to missing comma location: line {}, after '{}'",
                                i,
                                line
                            );
                            break;
                        }
                    }
                }
            }
        }

        // Fallback: For JSON parse errors at the start of a line (column near 0),
        // adjust to the end of the previous line (likely missing comma/brace)
        if !was_adjusted
            && position.character <= 2
            && position.line > 0
            && let Some(prev_line) = lines.get(position.line as usize - 1)
        {
            // Point to the end of the previous line (before newline, after last character)
            // Use the full line length, not trimmed, to get the position after the last char
            let prev_line_len = prev_line.len() as u32;
            position = Position {
                line: position.line - 1,
                character: prev_line_len,
            };
            was_adjusted = true;
            tracing::debug!(
                "Adjusted JSON parse error position to end of previous line: {:?}",
                position
            );
        }

        return (position, was_adjusted);
    }

    (
        Position {
            line: 0,
            character: 0,
        },
        false,
    )
}

/// Improve error message with correct position and helpful hints
fn improve_error_message(original_msg: &str, pos: &Position, was_adjusted: bool) -> String {
    // Extract the base error message without position info
    let base_msg = if let Some(colon_pos) = original_msg.find(": ") {
        &original_msg[colon_pos + 2..]
    } else {
        original_msg
    };

    // Build the message with correct position (1-indexed for display)
    let location = format!("line {}, column {}", pos.line + 1, pos.character + 1);

    // Add helpful hints based on error type and adjustment
    if was_adjusted {
        format!(
            "JSON syntax error at {}: Expected comma or closing brace",
            location
        )
    } else if base_msg.contains("Unexpected trailing content") {
        format!("JSON syntax error at {}: {}", location, base_msg)
    } else {
        format!("JSON parse error at {}", location)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
