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

    assert!(
        changes.contains_key(&address_uri),
        "Should edit address.avsc"
    );
    assert!(changes.contains_key(&user_uri), "Should edit user.avsc");
    assert!(
        changes.contains_key(&company_uri),
        "Should edit company.avsc"
    );

    // user.avsc references Address by simple name - replacement must be simple name
    let user_edits = changes.get(&user_uri).unwrap();
    assert!(!user_edits.is_empty(), "User file should have edits");
    assert_eq!(user_edits[0].new_text, "Location");

    // company.avsc references Address by simple name - only the Address edit should say Location
    let company_edits = changes.get(&company_uri).unwrap();
    assert!(
        company_edits.iter().all(|e| e.new_text == "Location"),
        "All company edits for Address reference should use simple name 'Location', got: {:?}",
        company_edits
    );
}

/// When a file in a *different* namespace references the type by FQN, rename must
/// preserve the namespace prefix and only replace the last segment.
#[test]
fn test_rename_cross_file_fqn_reference() {
    let mut workspace = Workspace::new();

    // Address defined in com.example.common
    let address_uri = Url::parse("file:///common_types.avsc").unwrap();
    let address_schema = r#"{
  "type": "record",
  "name": "Address",
  "namespace": "com.example.common",
  "fields": [{"name": "city", "type": "string"}]
}"#;
    workspace
        .update_file(address_uri.clone(), address_schema.to_string())
        .unwrap();

    // user.avsc in a different namespace — must reference Address by FQN
    let user_uri = Url::parse("file:///user.avsc").unwrap();
    let user_schema = r#"{
  "type": "record",
  "name": "User",
  "namespace": "com.example.app",
  "fields": [{"name": "address", "type": "com.example.common.Address"}]
}"#;
    workspace
        .update_file(user_uri.clone(), user_schema.to_string())
        .unwrap();

    let schema = AvroParser::new().parse(address_schema).unwrap();

    // Cursor on "Address" name in address.avsc (line 2, within the name value)
    let position = Position {
        line: 2,
        character: 10,
    };

    let result = rename_with_workspace(
        &schema,
        address_schema,
        &address_uri,
        position,
        "PostalAddress",
        Some(&workspace),
    );

    assert!(result.is_ok(), "Rename should succeed: {:?}", result);
    let changes = result.unwrap().unwrap().changes.unwrap();

    assert!(
        changes.contains_key(&address_uri),
        "Should edit definition file"
    );
    assert!(
        changes.contains_key(&user_uri),
        "Should edit file with FQN reference"
    );

    // Definition rename: the name token gets the new simple name
    let address_edits = changes.get(&address_uri).unwrap();
    assert!(
        address_edits
            .iter()
            .any(|e| e.new_text == "\"PostalAddress\""),
        "Definition edit should be '\"PostalAddress\"', got: {:?}",
        address_edits
    );

    // FQN reference: namespace prefix preserved, only last segment replaced
    let user_edits = changes.get(&user_uri).unwrap();
    assert_eq!(
        user_edits.len(),
        1,
        "Should have exactly 1 edit in user.avsc"
    );
    assert_eq!(
        user_edits[0].new_text, "com.example.common.PostalAddress",
        "FQN reference should have namespace prefix preserved"
    );
}

/// Mixed: one file uses simple name, another uses FQN — both must get correct replacements.
#[test]
fn test_rename_cross_file_mixed_simple_and_fqn() {
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

    // same-namespace file uses simple name
    let user_uri = Url::parse("file:///user.avsc").unwrap();
    let user_schema = r#"{
  "type": "record",
  "name": "User",
  "namespace": "com.example",
  "fields": [{"name": "addr", "type": "Address"}]
}"#;
    workspace
        .update_file(user_uri.clone(), user_schema.to_string())
        .unwrap();

    // different-namespace file uses FQN
    let order_uri = Url::parse("file:///order.avsc").unwrap();
    let order_schema = r#"{
  "type": "record",
  "name": "Order",
  "namespace": "com.other",
  "fields": [{"name": "shipTo", "type": "com.example.Address"}]
}"#;
    workspace
        .update_file(order_uri.clone(), order_schema.to_string())
        .unwrap();

    let schema = AvroParser::new().parse(address_schema).unwrap();
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

    assert!(result.is_ok(), "Rename should succeed: {:?}", result);
    let changes = result.unwrap().unwrap().changes.unwrap();

    // simple-name reference → simple name replacement
    let user_edits = changes.get(&user_uri).unwrap();
    assert_eq!(user_edits.len(), 1);
    assert_eq!(
        user_edits[0].new_text, "Location",
        "Simple-name reference should produce simple replacement"
    );

    // FQN reference → FQN replacement with namespace preserved
    let order_edits = changes.get(&order_uri).unwrap();
    assert_eq!(order_edits.len(), 1);
    assert_eq!(
        order_edits[0].new_text, "com.example.Location",
        "FQN reference should preserve namespace prefix"
    );
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
        user_edits[0].new_text, "Location",
        "Should rename reference to Location"
    );
}
