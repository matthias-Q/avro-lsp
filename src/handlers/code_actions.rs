use async_lsp::lsp_types::{CodeAction, Position, Range, Url};

use crate::schema::{AvroSchema, AvroType, Field};
use crate::schema::{EnumSchema,  RecordSchema};
use crate::state::{AstNode, find_node_at_position};

/// Get code actions available at the given range
pub fn get_code_actions(schema: &AvroSchema, uri: &Url, range: Range) -> Option<Vec<CodeAction>> {
    // Use AST traversal to find the node at cursor position
    let node = find_node_at_position(schema, range.start)?;

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

    if actions.is_empty() {
        None
    } else {
        Some(actions)
    }
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

        assert!(actions.is_some(), "Should have code actions");
        let actions = actions.unwrap();

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

        assert!(actions.is_some(), "Should have code actions");
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

        assert!(actions.is_some(), "Should have code actions");
        let actions = actions.unwrap();

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
        if let Some(actions) = actions {
            let make_nullable = actions
                .iter()
                .find(|a| a.title.contains("Make") && a.title.contains("nullable"));
            assert!(
                make_nullable.is_none(),
                "Should not have 'Make nullable' for union types"
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

        assert!(actions.is_some(), "Should have code actions");
        let actions = actions.unwrap();

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

        assert!(actions.is_some(), "Should have code actions");
        let actions = actions.unwrap();

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

        assert!(actions.is_some(), "Should have code actions for field");
        let actions = actions.unwrap();

        println!("Available actions: {:?}", actions.iter().map(|a| &a.title).collect::<Vec<_>>());

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
        assert!(text_edit.new_text.contains("\"doc\""), "Should contain doc field");
        assert!(text_edit.new_text.contains("Description for id"), "Should contain description");
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

        let actions1 = get_code_actions(&schema, &uri, Range { start: position1, end: position1 });
        assert!(actions1.is_some());
        assert!(actions1.unwrap().iter().any(|a| a.title.contains("Add documentation for field")));

        // Position 2: On the type value
        let position2 = Position {
            line: 4,
            character: 30, // On "int"
        };

        let actions2 = get_code_actions(&schema, &uri, Range { start: position2, end: position2 });
        assert!(actions2.is_some());
        // On type value, we get FieldType actions
        let actions2_vec = actions2.unwrap();
        let action_titles: Vec<_> = actions2_vec.iter().map(|a| a.title.as_str()).collect();
        println!("Actions at type value: {:?}", action_titles);
        // At type position, we prioritize FieldType for "Make nullable",
        // so Field doc might not be there - that's OK
    }
}
