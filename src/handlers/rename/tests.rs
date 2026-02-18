use async_lsp::lsp_types::{Position, Url};

use super::*;
use crate::schema::AvroParser;
use crate::workspace::Workspace;

#[test]
fn test_rename_cross_file() {
    let mut workspace = Workspace::new();

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

    let mut parser = AvroParser::new();
    let schema = parser.parse(address_schema).unwrap();

    let position = Position {
        line: 2,
        character: 10,
    };

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

    let user_edits = changes.get(&user_uri).unwrap();
    assert!(!user_edits.is_empty(), "User file should have edits");
    assert_eq!(user_edits[0].new_text, "\"Location\"");

    let company_edits = changes.get(&company_uri).unwrap();
    assert!(!company_edits.is_empty(), "Company file should have edits");
    assert_eq!(company_edits[0].new_text, "\"Location\"");
}

#[test]
fn test_rename_local_only() {
    let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [{"name": "id", "type": "long"}]
}"#;

    let mut parser = AvroParser::new();
    let schema = parser.parse(schema_text).unwrap();
    let uri = Url::parse("file:///test.avsc").unwrap();

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

    assert_eq!(changes.len(), 1);
    assert!(changes.contains_key(&uri));
}

#[test]
fn test_rename_from_type_reference_in_different_file() {
    let mut workspace = Workspace::new();

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

    let mut parser = AvroParser::new();
    let schema = parser.parse(user_schema).unwrap();

    let position = Position {
        line: 4,
        character: 42,
    };

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

    let address_edits = changes.get(&address_uri).unwrap();
    assert!(!address_edits.is_empty(), "Address file should have edits");
    assert_eq!(
        address_edits[0].new_text, "\"Location\"",
        "Should rename definition to Location"
    );

    let user_edits = changes.get(&user_uri).unwrap();
    assert!(!user_edits.is_empty(), "User file should have edits");
    assert_eq!(
        user_edits[0].new_text, "\"Location\"",
        "Should rename reference to Location"
    );
}
