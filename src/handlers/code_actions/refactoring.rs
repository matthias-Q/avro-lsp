use async_lsp::lsp_types::{CodeAction, CodeActionKind, Position, Range, Url};

use super::builder::CodeActionBuilder;
use super::helpers::{format_avro_type_as_json, get_default_for_type, is_union};
use crate::schema::{AvroSchema, AvroType, EnumSchema, Field, FixedSchema, RecordSchema};

/// Create "Make field nullable" action
pub(super) fn create_make_nullable_action(uri: &Url, field: &Field) -> Option<CodeAction> {
    let type_range = field.type_range.as_ref()?;
    let current_type = format_avro_type_as_json(&field.field_type);
    let new_type = format!("[\"null\", {}]", current_type);

    Some(
        CodeActionBuilder::new(uri.clone(), format!("Make field '{}' nullable", field.name))
            .with_kind(CodeActionKind::REFACTOR)
            .add_edit(*type_range, new_type)
            .build(),
    )
}

/// Create "Add documentation" action for record using AST
pub(super) fn create_add_doc_action(uri: &Url, record: &RecordSchema) -> Option<CodeAction> {
    let name_range = record.name_range.as_ref()?;

    let insert_position = Position {
        line: name_range.end.line,
        character: name_range.end.character,
    };
    let insert_text = format!(",\n  \"doc\": \"Description for {}\"", record.name);

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Add documentation for '{}'", record.name),
        )
        .with_kind(CodeActionKind::REFACTOR)
        .add_insert(insert_position, insert_text)
        .build(),
    )
}

/// Create "Add documentation" action for enum using AST
pub(super) fn create_add_doc_action_enum(
    uri: &Url,
    enum_schema: &EnumSchema,
) -> Option<CodeAction> {
    let name_range = enum_schema.name_range.as_ref()?;

    let insert_position = Position {
        line: name_range.end.line,
        character: name_range.end.character,
    };
    let insert_text = format!(",\n  \"doc\": \"Description for {}\"", enum_schema.name);

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Add documentation for '{}'", enum_schema.name),
        )
        .with_kind(CodeActionKind::REFACTOR)
        .add_insert(insert_position, insert_text)
        .build(),
    )
}

/// Create "Add documentation" action for fixed using AST
pub(super) fn create_add_doc_action_fixed(
    uri: &Url,
    fixed_schema: &FixedSchema,
) -> Option<CodeAction> {
    let name_range = fixed_schema.name_range.as_ref()?;

    let insert_position = Position {
        line: name_range.end.line,
        character: name_range.end.character,
    };
    let insert_text = format!(",\n  \"doc\": \"Description for {}\"", fixed_schema.name);

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Add documentation for '{}'", fixed_schema.name),
        )
        .with_kind(CodeActionKind::REFACTOR)
        .add_insert(insert_position, insert_text)
        .build(),
    )
}

/// Create "Add documentation" action for field
pub(super) fn create_add_doc_action_field(uri: &Url, field: &Field) -> Option<CodeAction> {
    let name_range = field.name_range.as_ref()?;

    let insert_position = Position {
        line: name_range.end.line,
        character: name_range.end.character,
    };
    let insert_text = format!(",\n    \"doc\": \"Description for {}\"", field.name);

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Add documentation for field '{}'", field.name),
        )
        .with_kind(CodeActionKind::REFACTOR)
        .add_insert(insert_position, insert_text)
        .build(),
    )
}

/// Create "Add field to record" action using AST
pub(super) fn create_add_field_action(uri: &Url, record: &RecordSchema) -> Option<CodeAction> {
    let last_field = record.fields.last()?;
    let last_field_range = last_field.range.as_ref()?;

    let insert_position = Position {
        line: last_field_range.end.line,
        character: last_field_range.end.character,
    };

    let new_field = r#"{"name": "new_field", "type": "string"}"#;
    let insert_text = format!(",\n    {}", new_field);

    Some(
        CodeActionBuilder::new(uri.clone(), "Add field to record".to_string())
            .with_kind(CodeActionKind::REFACTOR)
            .add_insert(insert_position, insert_text)
            .build(),
    )
}

/// Helper to find parent record and create add field action
pub(super) fn find_parent_record_and_add_field(
    uri: &Url,
    schema: &AvroSchema,
    _field: &Field,
) -> Option<CodeAction> {
    // For now, we'll find the root record if it's a record
    // In future, we could walk the tree to find the actual parent
    if let AvroType::Record(record) = &schema.root {
        create_add_field_action(uri, record)
    } else {
        None
    }
}

/// Create "Sort fields alphabetically" action for records
pub(super) fn create_sort_fields_action(uri: &Url, record: &RecordSchema) -> Option<CodeAction> {
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

    Some(
        CodeActionBuilder::new(uri.clone(), "Sort fields alphabetically".to_string())
            .with_kind(CodeActionKind::REFACTOR)
            .add_edit(fields_range, new_text)
            .build(),
    )
}

/// Create "Add default value" action for fields without defaults
pub(super) fn create_add_default_value_action(uri: &Url, field: &Field) -> Option<CodeAction> {
    // Determine appropriate default value based on type
    let default_value = get_default_for_type(&field.field_type)?;

    let type_range = field.type_range.as_ref()?;

    // Insert after the type field
    let insert_position = Position {
        line: type_range.end.line,
        character: type_range.end.character,
    };

    let insert_text = format!(", \"default\": {}", default_value);

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Add default value for '{}'", field.name),
        )
        .with_kind(CodeActionKind::REFACTOR)
        .add_insert(insert_position, insert_text)
        .build(),
    )
}

/// Check if field type is already nullable (helper function for potential future use)
#[allow(dead_code)]
pub(super) fn is_field_already_nullable(field: &Field) -> bool {
    is_union(&field.field_type)
}
