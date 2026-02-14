use async_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::schema::{AvroParser, AvroSchema, AvroType, AvroValidator, SchemaError};
use crate::workspace::Workspace;

/// Parse and validate Avro schema text, returning diagnostics
/// If workspace is provided, cross-file type references will be validated
#[allow(dead_code)] // Used for backward compatibility
pub fn parse_and_validate(text: &str) -> Vec<Diagnostic> {
    parse_and_validate_with_workspace(text, None)
}

/// Parse and validate with optional workspace for cross-file type checking
pub fn parse_and_validate_with_workspace(
    text: &str,
    workspace: Option<&Workspace>,
) -> Vec<Diagnostic> {
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
                SchemaError::InvalidPrimitiveType {
                    type_name,
                    range: Some(r),
                    suggested,
                } => {
                    tracing::debug!("Using position from InvalidPrimitiveType: {:?}", r);
                    let msg = if let Some(suggestion) = suggested {
                        format!(
                            "Invalid primitive type '{}'. Did you mean '{}'?",
                            type_name, suggestion
                        )
                    } else {
                        format!("Invalid primitive type '{}'", type_name)
                    };
                    (*r, msg)
                }
                SchemaError::InvalidPrimitiveType {
                    type_name,
                    range: None,
                    suggested,
                } => {
                    let (pos, _) = extract_error_position_with_context(&error_msg, text);
                    let range = Range {
                        start: pos,
                        end: Position {
                            line: pos.line,
                            character: pos.character + 1,
                        },
                    };
                    let msg = if let Some(suggestion) = suggested {
                        format!(
                            "Invalid primitive type '{}'. Did you mean '{}'?",
                            type_name, suggestion
                        )
                    } else {
                        format!("Invalid primitive type '{}'", type_name)
                    };
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

            // Serialize the SchemaError to JSON for the data field (for code actions)
            let error_data = serde_json::to_value(&e).ok();

            diagnostics.push(Diagnostic {
                range: position_range,
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("avro-lsp".to_string()),
                message: adjusted_msg,
                related_information: None,
                tags: None,
                data: error_data,
            });
            return diagnostics;
        }
    };

    // Check for parse errors that were collected during error recovery
    for parse_error in &schema.parse_errors {
        let position_range = match parse_error {
            SchemaError::InvalidPrimitiveType { range: Some(r), .. } => *r,
            _ => Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 1,
                },
            },
        };

        let error_msg = match parse_error {
            SchemaError::InvalidPrimitiveType {
                type_name,
                suggested,
                ..
            } => {
                if let Some(suggestion) = suggested {
                    format!(
                        "Invalid primitive type '{}'. Did you mean '{}'?",
                        type_name, suggestion
                    )
                } else {
                    format!("Invalid primitive type '{}'", type_name)
                }
            }
            _ => parse_error.to_string(),
        };

        // Serialize the SchemaError to JSON for the data field (for code actions)
        let error_data = serde_json::to_value(parse_error).ok();

        diagnostics.push(Diagnostic {
            range: position_range,
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            code_description: None,
            source: Some("avro-lsp".to_string()),
            message: error_msg,
            related_information: None,
            tags: None,
            data: error_data,
        });
    }

    // Try to validate - use workspace if provided for cross-file type checking
    let validator = AvroValidator::new();
    let validation_result = if let Some(ws) = workspace {
        validator.validate_with_resolver(&schema, ws)
    } else {
        validator.validate(&schema)
    };

    if let Err(e) = validation_result {
        // Try to find the position of the error using AST
        let position_range = find_error_position_in_ast(&e, &schema);

        // Serialize the SchemaError to JSON for the data field
        // This allows code actions to access structured error data
        let error_data = serde_json::to_value(&e).ok();

        diagnostics.push(Diagnostic {
            range: position_range,
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            code_description: None,
            source: Some("avro-lsp".to_string()),
            message: format!("Validation error: {}", e),
            related_information: None,
            tags: None,
            data: error_data,
        });
    }

    diagnostics
}

/// Find the position of a validation error using AST
fn find_error_position_in_ast(error: &SchemaError, schema: &AvroSchema) -> Range {
    // Helper to search for error location in AST
    fn search_type(avro_type: &AvroType, error: &SchemaError) -> Option<Range> {
        match error {
            SchemaError::InvalidName { name, range, .. } => {
                // If error already has a range, use it
                if range.is_some() {
                    return *range;
                }
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
            SchemaError::InvalidNamespace {
                namespace, range, ..
            } => {
                // If error already has a range, use it
                if range.is_some() {
                    return *range;
                }
                tracing::debug!("Searching for InvalidNamespace: {}", namespace);
                // Search for a type with this namespace
                match avro_type {
                    AvroType::Record(record) => {
                        if record.namespace.as_ref() == Some(namespace) {
                            tracing::debug!(
                                "Found record with invalid namespace, namespace_range: {:?}",
                                record.namespace_range
                            );
                            // Use namespace_range if available, otherwise fall back to name_range or record range
                            return record
                                .namespace_range
                                .or(record.name_range)
                                .or(record.range);
                        }
                        // Recurse into fields
                        for field in &record.fields {
                            if let Some(range) = search_type(&field.field_type, error) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    AvroType::Enum(enum_schema) => {
                        if enum_schema.namespace.as_ref() == Some(namespace) {
                            tracing::debug!(
                                "Found enum with invalid namespace, namespace_range: {:?}",
                                enum_schema.namespace_range
                            );
                            return enum_schema
                                .namespace_range
                                .or(enum_schema.name_range)
                                .or(enum_schema.range);
                        }
                        None
                    }
                    AvroType::Fixed(fixed) => {
                        if fixed.namespace.as_ref() == Some(namespace) {
                            tracing::debug!(
                                "Found fixed with invalid namespace, namespace_range: {:?}",
                                fixed.namespace_range
                            );
                            return fixed.namespace_range.or(fixed.name_range).or(fixed.range);
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
            SchemaError::DuplicateSymbol {
                symbol,
                duplicate_occurrence,
                ..
            } => {
                // If error already has a range for the duplicate, use it
                if duplicate_occurrence.is_some() {
                    return *duplicate_occurrence;
                }
                tracing::debug!("Searching for DuplicateSymbol: {}", symbol);
                // Find the enum with this symbol
                match avro_type {
                    AvroType::Enum(enum_schema) => {
                        if enum_schema.symbols.contains(symbol) {
                            tracing::debug!(
                                "Found enum with duplicate symbol at {:?}",
                                enum_schema.range
                            );
                            return enum_schema.range;
                        }
                        None
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
            SchemaError::Custom {
                message: msg,
                range,
            } => {
                // If error already has a range, use it
                if range.is_some() {
                    return *range;
                }
                tracing::debug!("Searching for Custom error: {}", msg);
                // Try to extract relevant info from custom messages
                // Handle common patterns like field validation errors, decimal errors, etc.

                // For "Record must have at least one field"
                if msg.contains("Record must have at least one field")
                    && let AvroType::Record(record) = avro_type
                {
                    return record.range;
                }

                // For "Enum must have at least one symbol"
                if msg.contains("Enum must have at least one symbol")
                    && let AvroType::Enum(enum_schema) = avro_type
                {
                    return enum_schema.range;
                }

                // For "Fixed size must be greater than 0"
                if msg.contains("Fixed size must be greater than 0")
                    && let AvroType::Fixed(fixed) = avro_type
                {
                    return fixed.range;
                }

                // For decimal/duration logical type errors
                if msg.contains("Decimal") || msg.contains("precision") || msg.contains("scale") {
                    match avro_type {
                        AvroType::Fixed(fixed) if fixed.logical_type.is_some() => {
                            return fixed.range;
                        }
                        AvroType::PrimitiveObject(prim) if prim.logical_type.is_some() => {
                            return prim.range;
                        }
                        _ => {}
                    }
                }

                if msg.contains("Duration")
                    && let AvroType::Fixed(fixed) = avro_type
                    && fixed.logical_type == Some("duration".to_string())
                {
                    return fixed.range;
                }

                // For "Invalid logical type" errors
                if msg.contains("Invalid logical type")
                    && let AvroType::PrimitiveObject(prim) = avro_type
                {
                    return prim.range;
                }

                // Recurse for nested structures
                match avro_type {
                    AvroType::Record(record) => {
                        // Check if error is about this record
                        if let Some(range) = record.range {
                            // Could be about this record
                            if msg.contains("field") && record.fields.is_empty() {
                                return Some(range);
                            }
                        }
                        // Recurse into fields
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
            SchemaError::UnknownTypeReference { type_name, range } => {
                // If error already has a range, use it
                if range.is_some() {
                    return *range;
                }
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
            SchemaError::NestedUnion { range } => {
                // If error already has a range, use it
                if range.is_some() {
                    return *range;
                }
                tracing::debug!("Searching for NestedUnion");
                // Search for a union containing another union
                match avro_type {
                    AvroType::Union(types) => {
                        // Check if any element is a union (nested union detected)
                        for t in types {
                            if matches!(t, AvroType::Union(_)) {
                                // Found nested union, but Union doesn't store its own range
                                // Return None to let parent context (field/array/map) provide range
                                return None;
                            }
                        }
                        // Recurse into types
                        for t in types {
                            if let Some(range) = search_type(t, error) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    AvroType::Record(record) => {
                        // Check fields for unions with nested unions
                        for field in &record.fields {
                            if let AvroType::Union(types) = &*field.field_type {
                                // Check if this union contains another union
                                if types.iter().any(|t| matches!(t, AvroType::Union(_))) {
                                    // Found it - return field range as proxy for union range
                                    tracing::debug!(
                                        "Found nested union in field, returning field range: {:?}",
                                        field.range
                                    );
                                    return field.range;
                                }
                            }
                            // Recurse into field type
                            if let Some(range) = search_type(&field.field_type, error) {
                                // If recursion found a nested union but couldn't get range,
                                // use this field's range as proxy
                                return field.range.or(Some(range));
                            }
                        }
                        None
                    }
                    AvroType::Array(array) => {
                        // Check if array items is a union with nested unions
                        if let AvroType::Union(types) = &*array.items
                            && types.iter().any(|t| matches!(t, AvroType::Union(_)))
                        {
                            // Array items is a nested union, but we don't have array range
                            // Return None to let parent provide context
                            return None;
                        }
                        search_type(&array.items, error)
                    }
                    AvroType::Map(map) => {
                        // Check if map values is a union with nested unions
                        if let AvroType::Union(types) = &*map.values
                            && types.iter().any(|t| matches!(t, AvroType::Union(_)))
                        {
                            // Map values is a nested union, but we don't have map range
                            // Return None to let parent provide context
                            return None;
                        }
                        search_type(&map.values, error)
                    }
                    _ => None,
                }
            }
            SchemaError::DuplicateUnionType {
                range,
                type_signature,
            } => {
                // If error already has a range, use it
                if range.is_some() {
                    return *range;
                }
                tracing::debug!("Searching for DuplicateUnionType: {}", type_signature);

                // Helper to check if a union has duplicate type signatures
                let has_duplicates = |types: &[AvroType]| -> bool {
                    use std::collections::HashSet;
                    let mut signatures = HashSet::new();
                    for t in types {
                        let sig = match t {
                            AvroType::Primitive(p) => format!("{:?}", p),
                            AvroType::PrimitiveObject(p) => format!("{:?}", p.primitive_type),
                            AvroType::Record(r) => format!("record:{}", r.name),
                            AvroType::Enum(e) => format!("enum:{}", e.name),
                            AvroType::Fixed(f) => format!("fixed:{}", f.name),
                            AvroType::Array(_) => "array".to_string(),
                            AvroType::Map(_) => "map".to_string(),
                            AvroType::Union(_) => "union".to_string(),
                            AvroType::TypeRef(type_ref) => format!("ref:{}", type_ref.name),
                            AvroType::Invalid(inv) => format!("invalid:{}", inv.type_name),
                        };
                        if !signatures.insert(sig) {
                            return true; // Found duplicate
                        }
                    }
                    false
                };

                // Return the range of the union that has duplicates
                match avro_type {
                    AvroType::Union(types) => {
                        // Check if this union has duplicates
                        if has_duplicates(types) {
                            // Found it, but Union doesn't store its own range
                            // Return None to let parent context provide range
                            return None;
                        }
                        // Recurse into types
                        for t in types {
                            if let Some(range) = search_type(t, error) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    AvroType::Record(record) => {
                        // Check fields for unions with duplicates
                        for field in &record.fields {
                            if let AvroType::Union(types) = &*field.field_type
                                && has_duplicates(types)
                            {
                                // Found it - return field range as proxy for union range
                                tracing::debug!(
                                    "Found duplicate union type in field, returning field range: {:?}",
                                    field.range
                                );
                                return field.range;
                            }
                            // Recurse into field type
                            if let Some(range) = search_type(&field.field_type, error) {
                                // If recursion found a duplicate union but couldn't get range,
                                // use this field's range as proxy
                                return field.range.or(Some(range));
                            }
                        }
                        None
                    }
                    AvroType::Array(array) => {
                        // Check if array items is a union with duplicates
                        if let AvroType::Union(types) = &*array.items
                            && has_duplicates(types)
                        {
                            // Array items has duplicate union, but we don't have array range
                            // Return None to let parent provide context
                            return None;
                        }
                        search_type(&array.items, error)
                    }
                    AvroType::Map(map) => {
                        // Check if map values is a union with duplicates
                        if let AvroType::Union(types) = &*map.values
                            && has_duplicates(types)
                        {
                            // Map values has duplicate union, but we don't have map range
                            // Return None to let parent provide context
                            return None;
                        }
                        search_type(&map.values, error)
                    }
                    _ => None,
                }
            }
            SchemaError::MissingField { field } => {
                tracing::debug!("Searching for MissingField: {}", field);
                // For missing "fields" in a record
                if field == "fields" {
                    match avro_type {
                        AvroType::Record(record) => {
                            tracing::debug!("Found record missing fields at {:?}", record.range);
                            record.range
                        }
                        _ => None,
                    }
                } else {
                    None
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
