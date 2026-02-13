use async_lsp::ResponseError;
use async_lsp::lsp_types::{
    Location, Position, PrepareRenameResponse, Range, TextEdit, Url, WorkspaceEdit,
};
use std::collections::HashMap;

use crate::schema::{AvroSchema, AvroType, Field, RecordSchema};
use crate::state::{AstNode, find_node_at_position, position_in_range};

/// Perform rename operation
pub fn rename(
    schema: &AvroSchema,
    text: &str,
    uri: &Url,
    position: Position,
    new_name: &str,
) -> Result<Option<WorkspaceEdit>, ResponseError> {
    // Validate the new name follows Avro naming rules
    let name_regex = regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap();
    if !name_regex.is_match(new_name) {
        return Err(ResponseError::new(
            async_lsp::ErrorCode::INVALID_PARAMS,
            format!(
                "Invalid name '{}'. Names must start with [A-Za-z_] and contain only [A-Za-z0-9_]",
                new_name
            ),
        ));
    }

    // Find the node at cursor position
    let node = find_node_at_position(schema, position).ok_or_else(|| {
        ResponseError::new(
            async_lsp::ErrorCode::INVALID_PARAMS,
            "No renameable symbol at cursor position",
        )
    })?;

    let mut edits = Vec::new();

    match node {
        AstNode::RecordDefinition(record) => {
            // Check if cursor is on the name specifically
            if let Some(name_range) = &record.name_range
                && position_in_range(position, name_range)
            {
                let old_name = &record.name;

                // Check if new name conflicts with existing types (except itself)
                if schema.named_types.contains_key(new_name) && new_name != old_name {
                    return Err(ResponseError::new(
                        async_lsp::ErrorCode::INVALID_PARAMS,
                        format!("Type '{}' already exists", new_name),
                    ));
                }

                // Collect all edits (renames of the type and its references)
                edits = collect_type_rename_edits(schema, text, old_name, new_name);
            }
        }
        AstNode::EnumDefinition(enum_schema) => {
            if let Some(name_range) = &enum_schema.name_range
                && position_in_range(position, name_range)
            {
                let old_name = &enum_schema.name;

                if schema.named_types.contains_key(new_name) && new_name != old_name {
                    return Err(ResponseError::new(
                        async_lsp::ErrorCode::INVALID_PARAMS,
                        format!("Type '{}' already exists", new_name),
                    ));
                }

                edits = collect_type_rename_edits(schema, text, old_name, new_name);
            }
        }
        AstNode::Field(field) => {
            if let Some(name_range) = &field.name_range
                && position_in_range(position, name_range)
            {
                // Renaming a field - check for conflicts in the same record
                if check_field_name_conflict(schema, field, new_name) {
                    return Err(ResponseError::new(
                        async_lsp::ErrorCode::INVALID_PARAMS,
                        format!("Field '{}' already exists in this record", new_name),
                    ));
                }

                // For fields, we only rename the field name itself
                edits.push(TextEdit {
                    range: *name_range,
                    new_text: new_name.to_string(),
                });
            }
        }
        AstNode::FieldType(field) => {
            // Cursor on the type reference - rename the referenced type
            if let AvroType::TypeRef(type_ref) = &*field.field_type
                && let Some(type_range) = &type_ref.range
                && position_in_range(position, type_range)
            {
                let old_name = &type_ref.name;

                if schema.named_types.contains_key(new_name) && new_name != old_name {
                    return Err(ResponseError::new(
                        async_lsp::ErrorCode::INVALID_PARAMS,
                        format!("Type '{}' already exists", new_name),
                    ));
                }

                edits = collect_type_rename_edits(schema, text, old_name, new_name);
            }
        }
        AstNode::FixedDefinition(fixed) => {
            if let Some(name_range) = &fixed.name_range
                && position_in_range(position, name_range)
            {
                let old_name = &fixed.name;

                if schema.named_types.contains_key(new_name) && new_name != old_name {
                    return Err(ResponseError::new(
                        async_lsp::ErrorCode::INVALID_PARAMS,
                        format!("Type '{}' already exists", new_name),
                    ));
                }

                edits = collect_type_rename_edits(schema, text, old_name, new_name);
            }
        }
    }

    if edits.is_empty() {
        return Ok(None);
    }

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);

    Ok(Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }))
}

/// Prepare rename - validate that rename is possible
pub fn prepare_rename(schema: &AvroSchema, position: Position) -> Option<PrepareRenameResponse> {
    let node = find_node_at_position(schema, position)?;

    match node {
        AstNode::RecordDefinition(record) => {
            if let Some(name_range) = &record.name_range
                && position_in_range(position, name_range)
            {
                return Some(PrepareRenameResponse::RangeWithPlaceholder {
                    range: *name_range,
                    placeholder: record.name.clone(),
                });
            }
        }
        AstNode::EnumDefinition(enum_schema) => {
            if let Some(name_range) = &enum_schema.name_range
                && position_in_range(position, name_range)
            {
                return Some(PrepareRenameResponse::RangeWithPlaceholder {
                    range: *name_range,
                    placeholder: enum_schema.name.clone(),
                });
            }
        }
        AstNode::Field(field) => {
            if let Some(name_range) = &field.name_range
                && position_in_range(position, name_range)
            {
                return Some(PrepareRenameResponse::RangeWithPlaceholder {
                    range: *name_range,
                    placeholder: field.name.clone(),
                });
            }
        }
        AstNode::FieldType(field) => {
            if let AvroType::TypeRef(type_ref) = &*field.field_type
                && let Some(type_range) = &type_ref.range
                && position_in_range(position, type_range)
            {
                return Some(PrepareRenameResponse::RangeWithPlaceholder {
                    range: *type_range,
                    placeholder: type_ref.name.clone(),
                });
            }
        }
        AstNode::FixedDefinition(fixed) => {
            if let Some(name_range) = &fixed.name_range
                && position_in_range(position, name_range)
            {
                return Some(PrepareRenameResponse::RangeWithPlaceholder {
                    range: *name_range,
                    placeholder: fixed.name.clone(),
                });
            }
        }
    }

    None
}

/// Find all references to a symbol
pub fn find_references(
    schema: &AvroSchema,
    uri: &Url,
    position: Position,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    let node = find_node_at_position(schema, position)?;

    let type_name = match node {
        AstNode::RecordDefinition(record) => {
            if let Some(name_range) = &record.name_range {
                if position_in_range(position, name_range) {
                    Some(&record.name)
                } else {
                    None
                }
            } else {
                None
            }
        }
        AstNode::EnumDefinition(enum_schema) => {
            if let Some(name_range) = &enum_schema.name_range {
                if position_in_range(position, name_range) {
                    Some(&enum_schema.name)
                } else {
                    None
                }
            } else {
                None
            }
        }
        AstNode::FieldType(field) => {
            if let AvroType::TypeRef(type_ref) = &*field.field_type {
                if let Some(type_range) = &type_ref.range {
                    if position_in_range(position, type_range) {
                        Some(&type_ref.name)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        AstNode::FixedDefinition(fixed) => {
            if let Some(name_range) = &fixed.name_range {
                if position_in_range(position, name_range) {
                    Some(&fixed.name)
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }?;

    let references = collect_type_references(schema, type_name, include_declaration);

    let locations: Vec<Location> = references
        .into_iter()
        .map(|range| Location {
            uri: uri.clone(),
            range,
        })
        .collect();

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

fn check_field_name_conflict(schema: &AvroSchema, target_field: &Field, new_name: &str) -> bool {
    // Helper to find the parent record containing this field
    fn find_parent_record<'a>(
        avro_type: &'a AvroType,
        target_field: &Field,
    ) -> Option<&'a RecordSchema> {
        match avro_type {
            AvroType::Record(record) => {
                // Check if this record contains the target field
                for field in &record.fields {
                    if std::ptr::eq(field, target_field) {
                        return Some(record);
                    }
                }
                // Check nested fields
                for field in &record.fields {
                    if let Some(parent) = find_parent_record(&field.field_type, target_field) {
                        return Some(parent);
                    }
                }
                None
            }
            AvroType::Array(array) => find_parent_record(&array.items, target_field),
            AvroType::Map(map) => find_parent_record(&map.values, target_field),
            AvroType::Union(types) => {
                for t in types {
                    if let Some(parent) = find_parent_record(t, target_field) {
                        return Some(parent);
                    }
                }
                None
            }
            _ => None,
        }
    }

    if let Some(parent_record) = find_parent_record(&schema.root, target_field) {
        // Check if any other field (not the target field) has the new name
        for field in &parent_record.fields {
            if !std::ptr::eq(field, target_field) && field.name == new_name {
                return true;
            }
        }
    }

    false
}

fn collect_type_rename_edits(
    schema: &AvroSchema,
    _text: &str,
    old_name: &str,
    new_name: &str,
) -> Vec<TextEdit> {
    let mut edits = Vec::new();

    // Collect all ranges where this type is referenced
    let ranges = collect_type_references(schema, old_name, true);

    for range in ranges {
        edits.push(TextEdit {
            range,
            new_text: format!("\"{}\"", new_name),
        });
    }

    edits
}

fn collect_type_references(
    schema: &AvroSchema,
    type_name: &str,
    include_declaration: bool,
) -> Vec<Range> {
    let mut ranges = Vec::new();

    // Add the declaration if requested
    if include_declaration
        && let Some(named_type) = schema.named_types.get(type_name)
        && let Some(range) = get_type_name_range(named_type)
    {
        ranges.push(range);
    }

    // Search for references in the AST
    collect_type_references_in_type(&schema.root, type_name, &mut ranges);

    ranges
}

fn get_type_name_range(avro_type: &AvroType) -> Option<Range> {
    match avro_type {
        AvroType::Record(record) => record.name_range,
        AvroType::Enum(enum_schema) => enum_schema.name_range,
        AvroType::Fixed(fixed) => fixed.name_range,
        _ => None,
    }
}

fn collect_type_references_in_type(avro_type: &AvroType, type_name: &str, ranges: &mut Vec<Range>) {
    match avro_type {
        AvroType::TypeRef(type_ref) if type_ref.name == type_name => {
            if let Some(range) = type_ref.range {
                ranges.push(range);
            }
        }
        AvroType::Record(record) => {
            for field in &record.fields {
                collect_type_references_in_type(&field.field_type, type_name, ranges);
            }
        }
        AvroType::Array(array) => {
            collect_type_references_in_type(&array.items, type_name, ranges);
        }
        AvroType::Map(map) => {
            collect_type_references_in_type(&map.values, type_name, ranges);
        }
        AvroType::Union(types) => {
            for t in types {
                collect_type_references_in_type(t, type_name, ranges);
            }
        }
        _ => {}
    }
}
