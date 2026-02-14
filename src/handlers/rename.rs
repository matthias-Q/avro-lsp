use std::collections::HashMap;

use async_lsp::ResponseError;
use async_lsp::lsp_types::{
    Location, Position, PrepareRenameResponse, Range, TextEdit, Url, WorkspaceEdit,
};

use crate::schema::{AvroSchema, AvroType, Field, RecordSchema};
use crate::state::{AstNode, find_node_at_position, position_in_range};
use crate::workspace::Workspace;

/// Perform rename operation
#[allow(dead_code)] // Kept for backward compatibility
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

/// Perform rename operation with workspace support (cross-file rename)
pub fn rename_with_workspace(
    schema: &AvroSchema,
    text: &str,
    uri: &Url,
    position: Position,
    new_name: &str,
    workspace: Option<&Workspace>,
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

    let mut changes = HashMap::new();

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

                // Collect edits for current file
                let current_file_edits =
                    collect_type_rename_edits(schema, text, old_name, new_name);
                if !current_file_edits.is_empty() {
                    changes.insert(uri.clone(), current_file_edits);
                }

                // If workspace available, collect edits across all files
                if let Some(workspace) = workspace {
                    collect_cross_file_rename_edits(
                        workspace,
                        uri,
                        old_name,
                        new_name,
                        &mut changes,
                    );
                }
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

                let current_file_edits =
                    collect_type_rename_edits(schema, text, old_name, new_name);
                if !current_file_edits.is_empty() {
                    changes.insert(uri.clone(), current_file_edits);
                }

                if let Some(workspace) = workspace {
                    collect_cross_file_rename_edits(
                        workspace,
                        uri,
                        old_name,
                        new_name,
                        &mut changes,
                    );
                }
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

                // For fields, we only rename the field name itself (no cross-file impact)
                let edits = vec![TextEdit {
                    range: *name_range,
                    new_text: new_name.to_string(),
                }];
                changes.insert(uri.clone(), edits);
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

                let current_file_edits =
                    collect_type_rename_edits(schema, text, old_name, new_name);
                if !current_file_edits.is_empty() {
                    changes.insert(uri.clone(), current_file_edits);
                }

                if let Some(workspace) = workspace {
                    collect_cross_file_rename_edits(
                        workspace,
                        uri,
                        old_name,
                        new_name,
                        &mut changes,
                    );
                }
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

                let current_file_edits =
                    collect_type_rename_edits(schema, text, old_name, new_name);
                if !current_file_edits.is_empty() {
                    changes.insert(uri.clone(), current_file_edits);
                }

                if let Some(workspace) = workspace {
                    collect_cross_file_rename_edits(
                        workspace,
                        uri,
                        old_name,
                        new_name,
                        &mut changes,
                    );
                }
            }
        }
    }

    if changes.is_empty() {
        return Ok(None);
    }

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
#[allow(dead_code)] // Kept for backward compatibility
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

/// Find all references to a symbol with workspace support (cross-file search)
pub fn find_references_with_workspace(
    schema: &AvroSchema,
    uri: &Url,
    position: Position,
    include_declaration: bool,
    workspace: Option<&Workspace>,
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

    // Collect references in current file
    let references = collect_type_references(schema, type_name, include_declaration);
    let mut locations: Vec<Location> = references
        .into_iter()
        .map(|range| Location {
            uri: uri.clone(),
            range,
        })
        .collect();

    // If workspace is available, search across all files
    if let Some(workspace) = workspace {
        let workspace_refs = workspace.find_all_references(type_name);

        // Filter out references from the current file (already collected)
        for loc in workspace_refs {
            if loc.uri != *uri {
                locations.push(loc);
            }
        }
    }

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

/// Collect rename edits across all files in the workspace (including definition)
fn collect_cross_file_rename_edits(
    workspace: &Workspace,
    current_uri: &Url,
    old_name: &str,
    new_name: &str,
    changes: &mut HashMap<Url, Vec<TextEdit>>,
) {
    // First, find where the type is defined and add edit for the definition
    if let Some(type_info) = workspace.resolve_type(old_name, current_uri) {
        // If the definition is in a different file, add an edit for it
        if type_info.defined_in != *current_uri
            && let Some(def_range) = type_info.definition_range
        {
            let def_edit = TextEdit {
                range: def_range,
                new_text: format!("\"{}\"", new_name),
            };
            changes
                .entry(type_info.defined_in.clone())
                .or_default()
                .push(def_edit);
        }
    }

    // Find all references to the type across the workspace
    let references = workspace.find_all_references(old_name);

    // Group references by file URI
    let mut edits_by_file: HashMap<Url, Vec<Range>> = HashMap::new();
    for location in references {
        // Skip the current file (already handled)
        if location.uri == *current_uri {
            continue;
        }

        edits_by_file
            .entry(location.uri)
            .or_default()
            .push(location.range);
    }

    // Convert ranges to TextEdits for each file
    for (file_uri, ranges) in edits_by_file {
        let edits: Vec<TextEdit> = ranges
            .into_iter()
            .map(|range| TextEdit {
                range,
                new_text: format!("\"{}\"", new_name),
            })
            .collect();

        if !edits.is_empty() {
            // Append to existing edits for this file (in case we already added the definition)
            changes.entry(file_uri).or_default().extend(edits);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AvroParser;
    use crate::workspace::Workspace;

    #[test]
    fn test_rename_cross_file() {
        // Create workspace with multiple files
        let mut workspace = Workspace::new();

        // Define Address type
        let address_uri = Url::parse("file:///address.avsc").unwrap();
        let address_schema = r#"{
  "type": "record",
  "name": "Address",
  "namespace": "com.example",
  "fields": [{"name": "city", "type": "string"}]
}"#;
        workspace
            .update_file(address_uri.clone(), address_schema.to_string())
            .unwrap();

        // User references Address
        let user_uri = Url::parse("file:///user.avsc").unwrap();
        let user_schema = r#"{
  "type": "record",
  "name": "User",
  "namespace": "com.example",
  "fields": [{"name": "address", "type": "Address"}]
}"#;
        workspace
            .update_file(user_uri.clone(), user_schema.to_string())
            .unwrap();

        // Company also references Address
        let company_uri = Url::parse("file:///company.avsc").unwrap();
        let company_schema = r#"{
  "type": "record",
  "name": "Company",
  "namespace": "com.example",
  "fields": [
    {"name": "hqAddress", "type": "Address"},
    {"name": "ceo", "type": "User"}
  ]
}"#;
        workspace
            .update_file(company_uri.clone(), company_schema.to_string())
            .unwrap();

        // Parse the address schema
        let mut parser = AvroParser::new();
        let schema = parser.parse(address_schema).unwrap();

        // Position on the "Address" name declaration (line 2, after "name": ")
        let position = Position {
            line: 2,
            character: 10,
        };

        // Rename Address to Location
        let result = rename_with_workspace(
            &schema,
            address_schema,
            &address_uri,
            position,
            "Location",
            Some(&workspace),
        );

        assert!(result.is_ok(), "Rename should succeed");
        let edit = result.unwrap();
        assert!(edit.is_some(), "Should return workspace edit");

        let workspace_edit = edit.unwrap();
        let changes = workspace_edit.changes.unwrap();

        // Should have edits for all 3 files
        println!(
            "Files with changes: {:?}",
            changes.keys().collect::<Vec<_>>()
        );
        println!("address_uri edits: {:?}", changes.get(&address_uri));
        println!("user_uri edits: {:?}", changes.get(&user_uri));
        println!("company_uri edits: {:?}", changes.get(&company_uri));

        assert!(
            changes.contains_key(&address_uri),
            "Should edit address.avsc"
        );
        assert!(changes.contains_key(&user_uri), "Should edit user.avsc");
        assert!(
            changes.contains_key(&company_uri),
            "Should edit company.avsc"
        );

        // Verify user.avsc has the rename
        let user_edits = changes.get(&user_uri).unwrap();
        assert!(!user_edits.is_empty(), "User file should have edits");
        assert_eq!(user_edits[0].new_text, "\"Location\"");

        // Verify company.avsc has the rename
        let company_edits = changes.get(&company_uri).unwrap();
        assert!(!company_edits.is_empty(), "Company file should have edits");
        assert_eq!(company_edits[0].new_text, "\"Location\"");
    }

    #[test]
    fn test_rename_local_only() {
        // Single file, no workspace
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [{"name": "id", "type": "long"}]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).unwrap();
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on "User" name
        let position = Position {
            line: 2,
            character: 10,
        };

        let result = rename_with_workspace(&schema, schema_text, &uri, position, "Person", None);

        assert!(result.is_ok());
        let edit = result.unwrap();
        assert!(edit.is_some());

        let workspace_edit = edit.unwrap();
        let changes = workspace_edit.changes.unwrap();

        // Should only have edits for current file
        assert_eq!(changes.len(), 1);
        assert!(changes.contains_key(&uri));
    }

    #[test]
    fn test_rename_from_type_reference_in_different_file() {
        // This tests the scenario where you're in user.avsc and rename "Address"
        // The definition is in address.avsc, so it should rename across both files

        let mut workspace = Workspace::new();

        // Define Address type in address.avsc
        let address_uri = Url::parse("file:///address.avsc").unwrap();
        let address_schema = r#"{
  "type": "record",
  "name": "Address",
  "namespace": "com.example",
  "fields": [{"name": "city", "type": "string"}]
}"#;
        workspace
            .update_file(address_uri.clone(), address_schema.to_string())
            .unwrap();

        // User references Address in user.avsc
        let user_uri = Url::parse("file:///user.avsc").unwrap();
        let user_schema = r#"{
  "type": "record",
  "name": "User",
  "namespace": "com.example",
  "fields": [{"name": "address", "type": "Address"}]
}"#;
        workspace
            .update_file(user_uri.clone(), user_schema.to_string())
            .unwrap();

        // Parse user schema (we're working in user.avsc)
        let mut parser = AvroParser::new();
        let schema = parser.parse(user_schema).unwrap();

        // Position on "Address" in user.avsc - this is a TYPE REFERENCE, not definition
        // Line 4: {"name": "address", "type": "Address"}
        //                                      ^cursor here
        let position = Position {
            line: 4,
            character: 41,
        };

        // Rename Address to Location from the reference
        let result = rename_with_workspace(
            &schema,
            user_schema,
            &user_uri,
            position,
            "Location",
            Some(&workspace),
        );

        assert!(result.is_ok(), "Rename should succeed: {:?}", result);
        let edit = result.unwrap();
        assert!(edit.is_some(), "Should return workspace edit");

        let workspace_edit = edit.unwrap();
        let changes = workspace_edit.changes.unwrap();

        println!(
            "Files with changes: {:?}",
            changes.keys().collect::<Vec<_>>()
        );

        // Should have edits for BOTH files:
        // 1. address.avsc - the definition
        // 2. user.avsc - the reference
        assert!(
            changes.contains_key(&address_uri),
            "Should edit address.avsc (definition file), changes: {:?}",
            changes
        );
        assert!(
            changes.contains_key(&user_uri),
            "Should edit user.avsc (current file with reference), changes: {:?}",
            changes
        );

        // Verify address.avsc has the definition renamed
        let address_edits = changes.get(&address_uri).unwrap();
        assert!(!address_edits.is_empty(), "Address file should have edits");
        assert_eq!(
            address_edits[0].new_text, "\"Location\"",
            "Should rename definition to Location"
        );

        // Verify user.avsc has the reference renamed
        let user_edits = changes.get(&user_uri).unwrap();
        assert!(!user_edits.is_empty(), "User file should have edits");
        assert_eq!(
            user_edits[0].new_text, "\"Location\"",
            "Should rename reference to Location"
        );
    }
}
