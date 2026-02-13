use async_lsp::lsp_types::{CodeAction, Diagnostic, Position, Range, Url};
use regex::Regex;

use crate::schema::{AvroSchema, AvroType, Field};
use crate::schema::{EnumSchema, RecordSchema};
use crate::state::{AstNode, find_node_at_position};

/// Get code actions available at the given range
pub fn get_code_actions(schema: &AvroSchema, uri: &Url, range: Range) -> Vec<CodeAction> {
    // Use AST traversal to find the node at cursor position
    let node = match find_node_at_position(schema, range.start) {
        Some(n) => n,
        None => return Vec::new(),
    };

    let mut actions = Vec::new();

    match node {
        AstNode::RecordDefinition(record) => {
            // Offer "Add documentation" if record doesn't have doc
            if record.doc.is_none()
                && let Some(action) = create_add_doc_action(uri, record)
            {
                actions.push(action);
            }
            // Offer "Add field to record"
            if let Some(action) = create_add_field_action(uri, record) {
                actions.push(action);
            }
            // Offer "Sort fields alphabetically" if record has multiple fields
            if record.fields.len() > 1
                && let Some(action) = create_sort_fields_action(uri, record)
            {
                actions.push(action);
            }
        }
        AstNode::Field(field) => {
            // Offer "Add documentation" if field doesn't have doc
            if field.doc.is_none()
                && let Some(action) = create_add_doc_action_field(uri, field)
            {
                actions.push(action);
            }

            // Offer "Add field to record" (insert after this field)
            // We need to find the parent record
            if let Some(action) = find_parent_record_and_add_field(uri, schema, field) {
                actions.push(action);
            }

            // Offer "Make nullable" if field type is not already a union with null
            if !is_union(&field.field_type)
                && let Some(action) = create_make_nullable_action(uri, field)
            {
                actions.push(action);
            }

            // Offer "Add default value" if field doesn't have a default
            if field.default.is_none()
                && let Some(action) = create_add_default_value_action(uri, field)
            {
                actions.push(action);
            }
        }
        AstNode::FieldType(field) => {
            // When cursor is on the type value, offer "Make nullable"
            if !is_union(&field.field_type)
                && let Some(action) = create_make_nullable_action(uri, field)
            {
                actions.push(action);
            }
        }
        AstNode::EnumDefinition(enum_schema) => {
            // Offer "Add documentation" if enum doesn't have doc
            if enum_schema.doc.is_none()
                && let Some(action) = create_add_doc_action_enum(uri, enum_schema)
            {
                actions.push(action);
            }
        }
        AstNode::FixedDefinition(fixed_schema) => {
            // Offer "Add documentation" if fixed doesn't have doc
            if fixed_schema.doc.is_none()
                && let Some(action) = create_add_doc_action_fixed(uri, fixed_schema)
            {
                actions.push(action);
            }
        }
    }

    actions
}

fn is_union(avro_type: &AvroType) -> bool {
    matches!(avro_type, AvroType::Union(_))
}

fn format_avro_type_as_json(avro_type: &AvroType) -> String {
    match avro_type {
        AvroType::Primitive(prim) => {
            format!("\"{}\"", format!("{:?}", prim).to_lowercase())
        }
        AvroType::TypeRef(type_ref) => format!("\"{}\"", type_ref.name),
        // For all other types (including Record, Enum, Fixed, Array, Map, Union),
        // use serde_json serialization to preserve the full structure
        _ => serde_json::to_string(avro_type).unwrap_or_else(|_| "\"string\"".to_string()),
    }
}

fn create_make_nullable_action(
    uri: &Url,
    field: &Field,
) -> Option<async_lsp::lsp_types::CodeAction> {
    use async_lsp::lsp_types::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    let type_range = field.type_range.as_ref()?;
    let current_type = format_avro_type_as_json(&field.field_type);
    let new_type = format!("[\"null\", {}]", current_type);

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: *type_range,
            new_text: new_type,
        }],
    );

    Some(CodeAction {
        title: format!("Make field '{}' nullable", field.name),
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

/// Create "Add documentation" action for record using AST
fn create_add_doc_action(
    uri: &Url,
    record: &RecordSchema,
) -> Option<async_lsp::lsp_types::CodeAction> {
    use async_lsp::lsp_types::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    let name_range = record.name_range.as_ref()?;

    // Insert doc field after the name line
    let insert_position = Position {
        line: name_range.end.line,
        character: name_range.end.character,
    };
    let insert_text = format!(",\n  \"doc\": \"Description for {}\"", record.name);

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range {
                start: insert_position,
                end: insert_position,
            },
            new_text: insert_text,
        }],
    );

    Some(CodeAction {
        title: format!("Add documentation for '{}'", record.name),
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

/// Create "Add documentation" action for enum using AST
fn create_add_doc_action_enum(
    uri: &Url,
    enum_schema: &EnumSchema,
) -> Option<async_lsp::lsp_types::CodeAction> {
    use async_lsp::lsp_types::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    let name_range = enum_schema.name_range.as_ref()?;

    let insert_position = Position {
        line: name_range.end.line,
        character: name_range.end.character,
    };
    let insert_text = format!(",\n  \"doc\": \"Description for {}\"", enum_schema.name);

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range {
                start: insert_position,
                end: insert_position,
            },
            new_text: insert_text,
        }],
    );

    Some(CodeAction {
        title: format!("Add documentation for '{}'", enum_schema.name),
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

/// Create "Add documentation" action for fixed using AST
fn create_add_doc_action_fixed(
    uri: &Url,
    fixed_schema: &crate::schema::FixedSchema,
) -> Option<async_lsp::lsp_types::CodeAction> {
    use async_lsp::lsp_types::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    let name_range = fixed_schema.name_range.as_ref()?;

    let insert_position = Position {
        line: name_range.end.line,
        character: name_range.end.character,
    };
    let insert_text = format!(",\n  \"doc\": \"Description for {}\"", fixed_schema.name);

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range {
                start: insert_position,
                end: insert_position,
            },
            new_text: insert_text,
        }],
    );

    Some(CodeAction {
        title: format!("Add documentation for '{}'", fixed_schema.name),
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

/// Create "Add documentation" action for field
fn create_add_doc_action_field(
    uri: &Url,
    field: &Field,
) -> Option<async_lsp::lsp_types::CodeAction> {
    use async_lsp::lsp_types::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    let name_range = field.name_range.as_ref()?;

    // Insert doc field after the field name
    let insert_position = Position {
        line: name_range.end.line,
        character: name_range.end.character,
    };
    let insert_text = format!(",\n    \"doc\": \"Description for {}\"", field.name);

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range {
                start: insert_position,
                end: insert_position,
            },
            new_text: insert_text,
        }],
    );

    Some(CodeAction {
        title: format!("Add documentation for field '{}'", field.name),
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

/// Create "Add field to record" action using AST
fn create_add_field_action(
    uri: &Url,
    record: &RecordSchema,
) -> Option<async_lsp::lsp_types::CodeAction> {
    use async_lsp::lsp_types::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    // Insert at the end of the fields array
    // We need to find the last field's range and insert after it
    let last_field = record.fields.last()?;
    let last_field_range = last_field.range.as_ref()?;

    let insert_position = Position {
        line: last_field_range.end.line,
        character: last_field_range.end.character,
    };

    let new_field = r#"{"name": "new_field", "type": "string"}"#;
    let insert_text = format!(",\n    {}", new_field);

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range {
                start: insert_position,
                end: insert_position,
            },
            new_text: insert_text,
        }],
    );

    Some(CodeAction {
        title: "Add field to record".to_string(),
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

/// Helper to find parent record and create add field action
fn find_parent_record_and_add_field(
    uri: &Url,
    schema: &AvroSchema,
    _field: &Field,
) -> Option<async_lsp::lsp_types::CodeAction> {
    // For now, we'll find the root record if it's a record
    // In future, we could walk the tree to find the actual parent
    if let AvroType::Record(record) = &schema.root {
        create_add_field_action(uri, record)
    } else {
        None
    }
}

/// Create "Sort fields alphabetically" action for records
fn create_sort_fields_action(
    uri: &Url,
    record: &RecordSchema,
) -> Option<async_lsp::lsp_types::CodeAction> {
    use async_lsp::lsp_types::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    // Check if fields are already sorted
    let field_names: Vec<&str> = record.fields.iter().map(|f| f.name.as_str()).collect();
    let mut sorted_names = field_names.clone();
    sorted_names.sort();

    if field_names == sorted_names {
        // Already sorted, no action needed
        return None;
    }

    // We need to find the range covering all fields and replace with sorted version
    let first_field = record.fields.first()?;
    let last_field = record.fields.last()?;

    let first_range = first_field.range.as_ref()?;
    let last_range = last_field.range.as_ref()?;

    let fields_range = Range {
        start: first_range.start,
        end: last_range.end,
    };

    // Sort fields by name
    let mut sorted_fields = record.fields.clone();
    sorted_fields.sort_by(|a, b| a.name.cmp(&b.name));

    // Serialize sorted fields as JSON
    let mut sorted_json = Vec::new();
    for (i, field) in sorted_fields.iter().enumerate() {
        let field_json = serde_json::json!({
            "name": field.name,
            "type": &*field.field_type,
            "doc": field.doc,
            "default": field.default,
            "order": field.order,
            "aliases": field.aliases,
        });

        // Remove null fields for cleaner output
        let mut field_map = field_json.as_object()?.clone();
        field_map.retain(|_, v| !v.is_null());

        let field_str = serde_json::to_string_pretty(&field_map).ok()?;

        if i > 0 {
            sorted_json.push(",\n    ".to_string());
        } else {
            sorted_json.push("".to_string());
        }
        sorted_json.push(field_str);
    }

    let new_text = sorted_json.concat();

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: fields_range,
            new_text,
        }],
    );

    Some(CodeAction {
        title: "Sort fields alphabetically".to_string(),
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

/// Create "Add default value" action for fields without defaults
fn create_add_default_value_action(
    uri: &Url,
    field: &Field,
) -> Option<async_lsp::lsp_types::CodeAction> {
    use async_lsp::lsp_types::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    // Determine appropriate default value based on type
    let default_value = get_default_for_type(&field.field_type)?;

    let type_range = field.type_range.as_ref()?;

    // Insert after the type field
    let insert_position = Position {
        line: type_range.end.line,
        character: type_range.end.character,
    };

    let insert_text = format!(", \"default\": {}", default_value);

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: Range {
                start: insert_position,
                end: insert_position,
            },
            new_text: insert_text,
        }],
    );

    Some(CodeAction {
        title: format!("Add default value for '{}'", field.name),
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

/// Get a sensible default value for an Avro type
fn get_default_for_type(avro_type: &AvroType) -> Option<String> {
    match avro_type {
        AvroType::Primitive(prim) => match prim {
            crate::schema::PrimitiveType::Null => Some("null".to_string()),
            crate::schema::PrimitiveType::Boolean => Some("false".to_string()),
            crate::schema::PrimitiveType::Int => Some("0".to_string()),
            crate::schema::PrimitiveType::Long => Some("0".to_string()),
            crate::schema::PrimitiveType::Float => Some("0.0".to_string()),
            crate::schema::PrimitiveType::Double => Some("0.0".to_string()),
            crate::schema::PrimitiveType::Bytes => Some("\"\"".to_string()),
            crate::schema::PrimitiveType::String => Some("\"\"".to_string()),
        },
        AvroType::Array(_) => Some("[]".to_string()),
        AvroType::Map(_) => Some("{}".to_string()),
        AvroType::Union(types) => {
            // For unions, use the first type's default
            types.first().and_then(get_default_for_type)
        }
        // For complex types (Record, Enum, Fixed) and TypeRefs, don't provide defaults
        // as they require more context
        _ => None,
    }
}

/// Get quick fix code actions from diagnostics
pub fn get_quick_fixes_from_diagnostics(
    schema: &AvroSchema,
    text: &str,
    uri: &Url,
    diagnostics: &[Diagnostic],
) -> Vec<CodeAction> {
    tracing::debug!(
        "get_quick_fixes_from_diagnostics called with {} diagnostics",
        diagnostics.len()
    );
    let mut actions = Vec::new();

    for diagnostic in diagnostics {
        tracing::debug!("Processing diagnostic: {}", diagnostic.message);

        // Strip "Validation error: " prefix if present
        let msg = diagnostic
            .message
            .strip_prefix("Validation error: ")
            .unwrap_or(&diagnostic.message);

        tracing::debug!("Stripped message: {}", msg);

        // Try to generate fixes based on the error message
        if let Some(remainder) = msg.strip_prefix("Invalid name '") {
            if let Some(name_end) = remainder.find('\'') {
                let invalid_name = &remainder[..name_end];
                tracing::debug!("Found invalid name: {}", invalid_name);
                if let Some(fix) = create_fix_invalid_name(uri, schema, diagnostic, invalid_name) {
                    actions.push(fix);
                }
            }
        } else if let Some(remainder) = msg.strip_prefix("Invalid namespace '") {
            if let Some(ns_end) = remainder.find('\'') {
                let invalid_namespace = &remainder[..ns_end];
                tracing::debug!("Found invalid namespace: {}", invalid_namespace);
                if let Some(fix) =
                    create_fix_invalid_namespace(uri, schema, diagnostic, invalid_namespace)
                {
                    actions.push(fix);
                }
            }
        } else if msg.contains("logical type") && msg.contains("requires") {
            // e.g., "Invalid logical type 'uuid' for type int - requires string"
            tracing::debug!("Found logical type error");
            if let Some(fix) = create_fix_logical_type(uri, schema, text, diagnostic) {
                actions.push(fix);
            }
        } else if let Some(remainder) = msg.strip_prefix("Duplicate symbol '")
            && let Some(symbol_end) = remainder.find('\'')
        {
            let duplicate_symbol = &remainder[..symbol_end];
            tracing::debug!("Found duplicate symbol: {}", duplicate_symbol);
            if let Some(fix) = create_fix_duplicate_symbol(uri, text, diagnostic, duplicate_symbol)
            {
                actions.push(fix);
            }
        } else {
            tracing::debug!("No matching pattern for diagnostic: {}", msg);
        }
    }

    actions
}

/// Create a quick fix for invalid name errors
fn create_fix_invalid_name(
    uri: &Url,
    schema: &AvroSchema,
    diagnostic: &Diagnostic,
    invalid_name: &str,
) -> Option<CodeAction> {
    use async_lsp::lsp_types::{CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    // Generate a valid name by:
    // 1. If starts with digit, prepend underscore
    // 2. Replace invalid characters with underscores
    let fixed_name = fix_invalid_name(invalid_name);

    // Find the name in the schema to get the exact position
    let name_range = find_name_range_in_schema(schema, invalid_name, diagnostic.range)?;

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: name_range,
            new_text: format!("\"{}\"", fixed_name),
        }],
    );

    Some(CodeAction {
        title: format!("Fix invalid name: '{}' → '{}'", invalid_name, fixed_name),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
}

/// Create a quick fix for invalid namespace errors
fn create_fix_invalid_namespace(
    uri: &Url,
    schema: &AvroSchema,
    diagnostic: &Diagnostic,
    invalid_namespace: &str,
) -> Option<CodeAction> {
    use async_lsp::lsp_types::{CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    // Fix namespace by filtering out invalid segments
    let fixed_namespace = fix_invalid_namespace(invalid_namespace);

    if fixed_namespace.is_empty() {
        // Offer to remove the namespace field entirely
        return create_remove_namespace_action(uri, schema, diagnostic);
    }

    // Find the namespace value in the schema
    let namespace_range = find_namespace_range_in_schema(schema, diagnostic.range)?;

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: namespace_range,
            new_text: format!("\"{}\"", fixed_namespace),
        }],
    );

    Some(CodeAction {
        title: format!(
            "Fix invalid namespace: '{}' → '{}'",
            invalid_namespace, fixed_namespace
        ),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
}

/// Create action to remove invalid namespace field
fn create_remove_namespace_action(
    uri: &Url,
    _schema: &AvroSchema,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    use async_lsp::lsp_types::{CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    // For now, just suggest to fix the namespace manually
    // A more sophisticated implementation would find and remove the entire field
    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: diagnostic.range,
            new_text: "\"valid_namespace\"".to_string(),
        }],
    );

    Some(CodeAction {
        title: "Replace with valid namespace placeholder".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

/// Create a quick fix for logical type errors
fn create_fix_logical_type(
    uri: &Url,
    _schema: &AvroSchema,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    use async_lsp::lsp_types::{CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    // Parse the error message to extract the required type
    // e.g., "Invalid logical type 'uuid' for type int - requires string"
    let msg = &diagnostic.message;
    let required_type = if msg.contains("requires string") {
        "string"
    } else if msg.contains("requires int") {
        "int"
    } else if msg.contains("requires long") {
        "long"
    } else if msg.contains("requires bytes") {
        "bytes"
    } else if msg.contains("requires fixed") {
        return None; // Fixed types are more complex
    } else {
        return None;
    };

    // Find the type field in the object
    let type_range = find_primitive_type_range(text, diagnostic.range)?;

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: type_range,
            new_text: format!("\"{}\"", required_type),
        }],
    );

    Some(CodeAction {
        title: format!("Change base type to '{}'", required_type),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
}

/// Create a quick fix for duplicate symbols
fn create_fix_duplicate_symbol(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
    duplicate_symbol: &str,
) -> Option<CodeAction> {
    use async_lsp::lsp_types::{CodeActionKind, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    // Find the duplicate symbol in the symbols array and remove it
    let (_first_pos, second_pos) = find_duplicate_symbol_positions(text, duplicate_symbol)?;

    // Remove the second occurrence (including comma)
    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: second_pos,
            new_text: String::new(),
        }],
    );

    Some(CodeAction {
        title: format!("Remove duplicate symbol '{}'", duplicate_symbol),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
}

/// Fix an invalid name according to Avro rules
fn fix_invalid_name(name: &str) -> String {
    let regex = Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap();

    // If already valid, return as-is
    if regex.is_match(name) {
        return name.to_string();
    }

    let mut fixed = String::new();
    let mut has_valid_char = false;

    for (i, ch) in name.chars().enumerate() {
        if i == 0 {
            // First character must be letter or underscore
            if ch.is_ascii_alphabetic() || ch == '_' {
                fixed.push(ch);
                has_valid_char = true;
            } else if ch.is_ascii_digit() {
                // Prepend underscore if starts with digit
                fixed.push('_');
                fixed.push(ch);
                has_valid_char = true;
            } else {
                // Skip invalid chars at start, we'll add underscore if needed
            }
        } else {
            // Subsequent characters can be letter, digit, or underscore
            if ch.is_ascii_alphanumeric() || ch == '_' {
                fixed.push(ch);
                has_valid_char = true;
            } else {
                // Replace invalid char with underscore (but avoid consecutive underscores)
                if !fixed.ends_with('_') && has_valid_char {
                    fixed.push('_');
                }
            }
        }
    }

    // Ensure we start with valid character
    if fixed.is_empty() || !regex.is_match(&fixed) {
        if fixed.is_empty() {
            fixed = "_".to_string();
        } else if !fixed.chars().next().unwrap().is_ascii_alphabetic() && !fixed.starts_with('_') {
            fixed = format!("_{}", fixed);
        }
    }

    fixed
}

/// Fix an invalid namespace by removing or fixing invalid segments
fn fix_invalid_namespace(namespace: &str) -> String {
    let segments: Vec<&str> = namespace.split('.').collect();
    let regex = Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap();

    let valid_segments: Vec<String> = segments
        .iter()
        .filter_map(|seg| {
            if regex.is_match(seg) {
                // Already valid
                Some(seg.to_string())
            } else {
                // Check if segment has any valid characters
                let has_letter = seg.chars().any(|c| c.is_ascii_alphabetic());
                if has_letter {
                    // Try to fix it if it has letters
                    let fixed = fix_invalid_name(seg);
                    if regex.is_match(&fixed) {
                        Some(fixed)
                    } else {
                        None
                    }
                } else {
                    // Skip segments with no letters (pure numbers, symbols)
                    None
                }
            }
        })
        .collect();

    valid_segments.join(".")
}

/// Find the range of a name value in the schema
fn find_name_range_in_schema(
    _schema: &AvroSchema,
    _name: &str,
    diagnostic_range: Range,
) -> Option<Range> {
    // Use diagnostic range as a hint - the name should be near there
    // For simplicity, we'll use the diagnostic range itself
    // A more sophisticated implementation would parse the JSON to find exact positions
    Some(diagnostic_range)
}

/// Find the range of a namespace value in the schema
fn find_namespace_range_in_schema(_schema: &AvroSchema, diagnostic_range: Range) -> Option<Range> {
    // Use diagnostic range directly
    Some(diagnostic_range)
}

/// Find the range of the "type" value in a primitive object
fn find_primitive_type_range(text: &str, diagnostic_range: Range) -> Option<Range> {
    // Search within the diagnostic range area for "type": "..."
    let lines: Vec<&str> = text.lines().collect();

    let start_line = diagnostic_range.start.line as usize;
    let end_line = (diagnostic_range.end.line as usize).min(lines.len());

    for line_num in start_line..=end_line {
        if line_num >= lines.len() {
            break;
        }

        let line = lines[line_num];

        // Look for "type": "int" or similar
        if let Some(type_pos) = line.find("\"type\"") {
            // Find the value after the colon
            if let Some(colon_pos) = line[type_pos..].find(':') {
                let after_colon = &line[type_pos + colon_pos + 1..];
                if let Some(quote_start) = after_colon.find('"')
                    && let Some(quote_end) = after_colon[quote_start + 1..].find('"')
                {
                    let value_start = type_pos + colon_pos + 1 + quote_start;
                    let value_end = value_start + quote_end + 2; // Include both quotes

                    return Some(Range {
                        start: Position {
                            line: line_num as u32,
                            character: value_start as u32,
                        },
                        end: Position {
                            line: line_num as u32,
                            character: value_end as u32,
                        },
                    });
                }
            }
        }
    }

    None
}

/// Find positions of duplicate symbols in the symbols array
fn find_duplicate_symbol_positions(text: &str, symbol: &str) -> Option<(Range, Range)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut positions = Vec::new();

    let search_pattern = format!("\"{}\"", symbol);

    for (line_num, line) in lines.iter().enumerate() {
        let mut search_start = 0;
        while let Some(pos) = line[search_start..].find(&search_pattern) {
            let absolute_pos = search_start + pos;
            let mut start_pos = Position {
                line: line_num as u32,
                character: absolute_pos as u32,
            };
            let mut end_pos = Position {
                line: line_num as u32,
                character: (absolute_pos + search_pattern.len()) as u32,
            };

            // Check if we need to include the comma
            if let Some(comma_pos) = line[absolute_pos + search_pattern.len()..].find(',') {
                // Include comma and any trailing spaces
                end_pos.character += comma_pos as u32 + 1;

                // Skip trailing spaces
                let after_comma = &line[(absolute_pos + search_pattern.len() + comma_pos + 1)..];
                let spaces = after_comma
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .count();
                end_pos.character += spaces as u32;
            } else {
                // Check for preceding comma
                let before_match = &line[..absolute_pos];
                if let Some(comma_pos) = before_match.rfind(',') {
                    // Include preceding comma
                    start_pos = Position {
                        line: line_num as u32,
                        character: comma_pos as u32,
                    };
                }
            }

            positions.push(Range {
                start: start_pos,
                end: end_pos,
            });

            search_start = absolute_pos + search_pattern.len();
        }
    }

    if positions.len() >= 2 {
        Some((positions[0], positions[1]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AvroParser;

    #[test]
    fn test_make_nullable_primitive_type() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "name", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the "string" type
        let position = Position {
            line: 4,
            character: 30,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");

        // Find the "Make nullable" action
        let make_nullable = actions
            .iter()
            .find(|a| a.title.contains("Make") && a.title.contains("nullable"));

        assert!(
            make_nullable.is_some(),
            "Should have 'Make nullable' action"
        );
        let action = make_nullable.unwrap();

        // Check the edit
        let edit = action.edit.as_ref().expect("Should have edit");
        let changes = edit.changes.as_ref().expect("Should have changes");
        let file_edits = changes.get(&uri).expect("Should have edits for file");

        assert_eq!(file_edits.len(), 1, "Should have one edit");
        let text_edit = &file_edits[0];

        // The new text should be ["null", "string"], NOT ["null", `string`]
        assert_eq!(text_edit.new_text, r#"["null", "string"]"#);
        assert!(
            !text_edit.new_text.contains('`'),
            "Should not contain backticks"
        );
    }

    #[test]
    fn test_make_nullable_complex_type() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "address", "type": {"type": "record", "name": "Address", "fields": []}}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the address field type
        let position = Position {
            line: 4,
            character: 40,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");
    }

    #[test]
    fn test_make_nullable_type_reference() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "int"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on "int"
        let position = Position {
            line: 4,
            character: 26,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");

        let make_nullable = actions
            .iter()
            .find(|a| a.title.contains("Make") && a.title.contains("nullable"));

        assert!(
            make_nullable.is_some(),
            "Should have 'Make nullable' action"
        );
        let action = make_nullable.unwrap();

        let edit = action.edit.as_ref().expect("Should have edit");
        let changes = edit.changes.as_ref().expect("Should have changes");
        let file_edits = changes.get(&uri).expect("Should have edits for file");

        let text_edit = &file_edits[0];
        assert_eq!(text_edit.new_text, r#"["null", "int"]"#);
    }

    #[test]
    fn test_no_make_nullable_on_union() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "email", "type": ["null", "string"]}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the union type
        let position = Position {
            line: 4,
            character: 35,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        // Should not offer "Make nullable" for fields that are already unions
        if !actions.is_empty() {
            let make_nullable = actions
                .iter()
                .find(|a| a.title.contains("Make") && a.title.contains("nullable"));

            assert!(
                make_nullable.is_none(),
                "Should not offer 'Make nullable' for union types"
            );
        }
    }

    #[test]
    fn test_sort_fields_alphabetically() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "zipcode", "type": "string"},
    {"name": "age", "type": "int"},
    {"name": "name", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the record definition
        let position = Position {
            line: 2,
            character: 12,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");

        let sort_action = actions
            .iter()
            .find(|a| a.title == "Sort fields alphabetically");

        assert!(
            sort_action.is_some(),
            "Should have 'Sort fields alphabetically' action"
        );
    }

    #[test]
    fn test_no_sort_action_when_already_sorted() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "age", "type": "int"},
    {"name": "name", "type": "string"},
    {"name": "zipcode", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        let position = Position {
            line: 2,
            character: 12,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        if !actions.is_empty() {
            let sort_action = actions
                .iter()
                .find(|a| a.title == "Sort fields alphabetically");

            assert!(
                sort_action.is_none(),
                "Should not have sort action when fields are already sorted"
            );
        }
    }

    #[test]
    fn test_add_default_value_to_field() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "age", "type": "int"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the field (on "age")
        let position = Position {
            line: 4,
            character: 15,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");

        let add_default = actions
            .iter()
            .find(|a| a.title.contains("Add default value"));

        assert!(
            add_default.is_some(),
            "Should have 'Add default value' action"
        );
    }

    #[test]
    fn test_no_add_default_when_default_exists() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "age", "type": "int", "default": 0}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        let position = Position {
            line: 4,
            character: 15,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        if !actions.is_empty() {
            let add_default = actions
                .iter()
                .find(|a| a.title.contains("Add default value"));

            assert!(
                add_default.is_none(),
                "Should not have 'Add default value' when default already exists"
            );
        }
    }

    #[test]
    fn test_add_documentation_to_record() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": []
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the record name
        let position = Position {
            line: 2,
            character: 12,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");

        let add_doc = actions
            .iter()
            .find(|a| a.title.contains("Add documentation"));

        assert!(add_doc.is_some(), "Should have 'Add documentation' action");
    }

    #[test]
    fn test_add_field_to_record() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "int"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the record definition
        let position = Position {
            line: 2,
            character: 12,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");

        let add_field = actions.iter().find(|a| a.title.contains("Add field"));

        assert!(add_field.is_some(), "Should have 'Add field' action");
        let action = add_field.unwrap();

        // Verify the edit inserts valid JSON
        let edit = action.edit.as_ref().expect("Should have edit");
        let changes = edit.changes.as_ref().expect("Should have changes");
        let file_edits = changes.get(&uri).expect("Should have edits for file");

        assert_eq!(file_edits.len(), 1, "Should have one edit");
        let text_edit = &file_edits[0];

        // Should insert a valid field JSON object
        assert!(text_edit.new_text.contains("new_field"));
        assert!(text_edit.new_text.contains("\"name\""));
        assert!(text_edit.new_text.contains("\"type\""));
    }

    #[test]
    fn test_add_doc_to_field() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "int"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the "id" field name
        let position = Position {
            line: 4,
            character: 15, // On "id"
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions for field");

        println!(
            "Available actions: {:?}",
            actions.iter().map(|a| &a.title).collect::<Vec<_>>()
        );

        // Find the "Add documentation" action
        let add_doc = actions
            .iter()
            .find(|a| a.title.contains("Add documentation") && a.title.contains("field"));

        assert!(
            add_doc.is_some(),
            "Should have 'Add documentation for field' action. Available: {:?}",
            actions.iter().map(|a| &a.title).collect::<Vec<_>>()
        );

        let action = add_doc.unwrap();

        // Check the edit
        let edit = action.edit.as_ref().expect("Should have edit");
        let changes = edit.changes.as_ref().expect("Should have changes");
        let file_edits = changes.get(&uri).expect("Should have edits for file");

        assert_eq!(file_edits.len(), 1, "Should have one edit");
        let text_edit = &file_edits[0];

        // Should insert doc field after field name
        assert!(
            text_edit.new_text.contains("\"doc\""),
            "Should contain doc field"
        );
        assert!(
            text_edit.new_text.contains("Description for id"),
            "Should contain description"
        );
    }

    #[test]
    fn test_add_doc_to_field_multiple_positions() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "int"},
    {"name": "name", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Test at different positions within a simple field

        // Position 1: On the field name
        let position1 = Position {
            line: 4,
            character: 15, // On "id"
        };

        let actions1 = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position1,
                end: position1,
            },
        );
        assert!(!actions1.is_empty());
        assert!(
            actions1
                .iter()
                .any(|a| a.title.contains("Add documentation for field"))
        );

        // Position 2: On the type value
        let position2 = Position {
            line: 4,
            character: 30, // On "int"
        };

        let actions2 = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position2,
                end: position2,
            },
        );
        assert!(!actions2.is_empty());
        // On type value, we get FieldType actions
        let actions2_vec = actions2;
        let action_titles: Vec<_> = actions2_vec.iter().map(|a| a.title.as_str()).collect();
        println!("Actions at type value: {:?}", action_titles);
        // At type position, we prioritize FieldType for "Make nullable",
        // so Field doc might not be there - that's OK
    }

    // === Quick Fix Tests ===

    #[test]
    fn test_quick_fix_invalid_name() {
        let schema_text = r#"{
  "type": "record",
  "name": "123Invalid",
  "fields": [
    {"name": "value", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Create a diagnostic for invalid name
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 2,
                    character: 11,
                },
                end: Position {
                    line: 2,
                    character: 22,
                },
            },
            severity: Some(async_lsp::lsp_types::DiagnosticSeverity::ERROR),
            message: "Invalid name '123Invalid': must match [A-Za-z_][A-Za-z0-9_]*".to_string(),
            source: Some("avro-lsp".to_string()),
            ..Default::default()
        };

        let quick_fixes =
            get_quick_fixes_from_diagnostics(&schema, schema_text, &uri, &[diagnostic]);

        assert!(!quick_fixes.is_empty(), "Should have quick fixes");

        // Find the fix invalid name action
        let fix = quick_fixes
            .iter()
            .find(|a| a.title.contains("Fix invalid name"));

        assert!(fix.is_some(), "Should have 'Fix invalid name' action");
        let action = fix.unwrap();

        // Verify it's a QUICKFIX
        assert_eq!(
            action.kind,
            Some(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
        );

        // Verify the fix suggests a valid name
        assert!(
            action.title.contains("_123Invalid"),
            "Should suggest '_123Invalid', got: {}",
            action.title
        );
    }

    #[test]
    fn test_quick_fix_invalid_namespace() {
        let schema_text = r#"{
  "type": "record",
  "name": "Test",
  "namespace": "123.invalid",
  "fields": [
    {"name": "value", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Create a diagnostic for invalid namespace
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 3,
                    character: 16,
                },
                end: Position {
                    line: 3,
                    character: 29,
                },
            },
            severity: Some(async_lsp::lsp_types::DiagnosticSeverity::ERROR),
            message: "Invalid namespace '123.invalid': must be dot-separated names".to_string(),
            source: Some("avro-lsp".to_string()),
            ..Default::default()
        };

        let quick_fixes =
            get_quick_fixes_from_diagnostics(&schema, schema_text, &uri, &[diagnostic]);

        assert!(!quick_fixes.is_empty(), "Should have quick fixes");

        // Should have at least one fix for the namespace
        let fix = quick_fixes.iter().find(|a| a.title.contains("namespace"));

        assert!(fix.is_some(), "Should have namespace fix action");
    }

    #[test]
    fn test_quick_fix_logical_type() {
        let schema_text = r#"{
  "type": "record",
  "name": "InvalidLogicalType",
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

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Create a diagnostic for logical type error
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 7,
                    character: 8,
                },
                end: Position {
                    line: 10,
                    character: 7,
                },
            },
            severity: Some(async_lsp::lsp_types::DiagnosticSeverity::ERROR),
            message: "Invalid logical type 'uuid' for type int - requires string".to_string(),
            source: Some("avro-lsp".to_string()),
            ..Default::default()
        };

        let quick_fixes =
            get_quick_fixes_from_diagnostics(&schema, schema_text, &uri, &[diagnostic]);

        assert!(!quick_fixes.is_empty(), "Should have quick fixes");

        // Find the fix logical type action
        let fix = quick_fixes
            .iter()
            .find(|a| a.title.contains("Change base type"));

        assert!(fix.is_some(), "Should have 'Change base type' action");
        let action = fix.unwrap();

        // Verify it suggests changing to string
        assert!(
            action.title.contains("string"),
            "Should suggest changing to 'string', got: {}",
            action.title
        );
    }

    #[test]
    fn test_quick_fix_duplicate_symbol() {
        let schema_text = r#"{
  "type": "enum",
  "name": "Colors",
  "symbols": ["RED", "GREEN", "RED"]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Create a diagnostic for duplicate symbol
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 3,
                    character: 15,
                },
                end: Position {
                    line: 3,
                    character: 38,
                },
            },
            severity: Some(async_lsp::lsp_types::DiagnosticSeverity::ERROR),
            message: "Duplicate symbol 'RED' in enum".to_string(),
            source: Some("avro-lsp".to_string()),
            ..Default::default()
        };

        let quick_fixes =
            get_quick_fixes_from_diagnostics(&schema, schema_text, &uri, &[diagnostic]);

        assert!(!quick_fixes.is_empty(), "Should have quick fixes");

        // Find the remove duplicate action
        let fix = quick_fixes
            .iter()
            .find(|a| a.title.contains("Remove duplicate"));

        assert!(fix.is_some(), "Should have 'Remove duplicate' action");
        let action = fix.unwrap();

        // Verify it mentions the symbol name
        assert!(
            action.title.contains("RED"),
            "Should mention symbol 'RED', got: {}",
            action.title
        );
    }

    #[test]
    fn test_fix_invalid_name_function() {
        assert_eq!(fix_invalid_name("123abc"), "_123abc");
        assert_eq!(fix_invalid_name("valid_name"), "valid_name");
        assert_eq!(fix_invalid_name("with-dash"), "with_dash");
        assert_eq!(fix_invalid_name("with space"), "with_space");
        assert_eq!(fix_invalid_name("!!!"), "_");
    }

    #[test]
    fn test_end_to_end_invalid_name_flow() {
        // This test simulates the EXACT flow that happens when editor triggers code action
        let text = r#"{
  "type": "record",
  "name": "123Invalid",
  "fields": [
    {"name": "value", "type": "string"}
  ]
}"#;

        // Step 1: Parse (what server does on didOpen)
        let mut parser = AvroParser::new();
        let schema = parser.parse(text).expect("Should parse");

        // Step 2: Get diagnostics (what server does after parsing)
        let diagnostics = crate::handlers::diagnostics::parse_and_validate(text);

        eprintln!("\n=== DIAGNOSTICS (what server sends to editor) ===");
        for (i, diag) in diagnostics.iter().enumerate() {
            eprintln!("Diagnostic {}: '{}'", i, diag.message);
            eprintln!("  Range: {:?}", diag.range);
        }

        assert!(!diagnostics.is_empty(), "Should have diagnostics");

        // Step 3: Get code actions (what server does when editor requests code actions)
        let uri = Url::parse("file:///test.avsc").unwrap();
        let quick_fixes = get_quick_fixes_from_diagnostics(&schema, text, &uri, &diagnostics);

        eprintln!("\n=== QUICK FIXES (what server should return) ===");
        for (i, fix) in quick_fixes.iter().enumerate() {
            eprintln!("Fix {}: '{}'", i, fix.title);
        }

        // THIS IS THE KEY TEST - if this fails, code actions won't work in editor
        assert!(
            !quick_fixes.is_empty(),
            "Should generate quick fixes! If this fails, the string parsing is broken."
        );

        // Verify the fix is correct
        let fix = &quick_fixes[0];
        assert!(
            fix.title.contains("123Invalid") && fix.title.contains("_123Invalid"),
            "Fix should suggest renaming to _123Invalid, got: {}",
            fix.title
        );
    }

    #[test]
    fn test_fix_invalid_namespace_function() {
        // Valid namespace stays the same
        assert_eq!(
            fix_invalid_namespace("com.example.test"),
            "com.example.test"
        );
        // Pure number segment is removed
        assert_eq!(fix_invalid_namespace("123.invalid"), "invalid");
        // Segment starting with number but containing letters is fixed
        assert_eq!(
            fix_invalid_namespace("valid.123invalid"),
            "valid._123invalid"
        );
        // Dashes are replaced with underscores
        assert_eq!(
            fix_invalid_namespace("com.test-dash.app"),
            "com.test_dash.app"
        );
    }
}
