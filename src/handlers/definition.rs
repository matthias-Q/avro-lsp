use async_lsp::lsp_types::{Location, Url};

use crate::handlers::symbols;
use crate::schema::AvroSchema;
use crate::workspace::Workspace;

/// Find the definition of a symbol at the given position
#[allow(dead_code)] // Kept for backward compatibility
pub fn find_definition(schema: &AvroSchema, text: &str, word: &str, uri: &Url) -> Option<Location> {
    // Check if the word is a named type in the schema
    if schema.named_types.contains_key(word) {
        // Find where this type is defined (its name declaration)
        let range = symbols::find_name_range(text, word)?;

        return Some(Location {
            uri: uri.clone(),
            range,
        });
    }

    // Not a type reference we can navigate to
    None
}

/// Find the definition of a symbol with workspace support (cross-file navigation)
pub fn find_definition_with_workspace(
    schema: &AvroSchema,
    text: &str,
    word: &str,
    uri: &Url,
    workspace: Option<&Workspace>,
) -> Option<Location> {
    // First check local schema
    if schema.named_types.contains_key(word) {
        // Find where this type is defined (its name declaration)
        let range = symbols::find_name_range(text, word)?;

        return Some(Location {
            uri: uri.clone(),
            range,
        });
    }

    // If not found locally and workspace is available, search workspace
    if let Some(workspace) = workspace {
        // Extract namespace from the schema to use for resolution
        let namespace = get_schema_namespace(&schema.root);

        // Look up the type in the workspace with namespace context
        let type_info = if let Some(ns) = namespace {
            workspace.resolve_type_with_namespace(word, uri, Some(&ns))
        } else {
            workspace.resolve_type_with_namespace(word, uri, None)
        };

        if let Some(type_info) = type_info {
            // Type is defined in another file
            return Some(Location {
                uri: type_info.defined_in.clone(),
                range: type_info.definition_range?,
            });
        }
    }

    // Not a type reference we can navigate to
    None
}

/// Extract the namespace from a schema's root type
fn get_schema_namespace(root_type: &crate::schema::AvroType) -> Option<String> {
    use crate::schema::AvroType;
    match root_type {
        AvroType::Record(record) => record.namespace.clone(),
        AvroType::Enum(enum_schema) => enum_schema.namespace.clone(),
        AvroType::Fixed(fixed) => fixed.namespace.clone(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AvroParser;

    #[test]
    fn test_find_definition_local_type() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "address", "type": "Address"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).unwrap();
        let uri = Url::parse("file:///test.avsc").unwrap();

        // User is defined locally, should find it
        let location = find_definition_with_workspace(&schema, schema_text, "User", &uri, None);
        assert!(location.is_some());
        assert_eq!(location.unwrap().uri, uri);
    }

    #[test]
    fn test_find_definition_cross_file() {
        // Create workspace with Address definition
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

        // User schema references Address
        let user_uri = Url::parse("file:///user.avsc").unwrap();
        let user_schema = r#"{
  "type": "record",
  "name": "User",
  "namespace": "com.example",
  "fields": [{"name": "address", "type": "Address"}]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(user_schema).unwrap();

        // Looking up "Address" should point to address.avsc
        let location = find_definition_with_workspace(
            &schema,
            user_schema,
            "Address",
            &user_uri,
            Some(&workspace),
        );
        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.uri, address_uri);
    }

    #[test]
    fn test_find_definition_unknown_type() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": []
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).unwrap();
        let uri = Url::parse("file:///test.avsc").unwrap();

        // "UnknownType" doesn't exist
        let location =
            find_definition_with_workspace(&schema, schema_text, "UnknownType", &uri, None);
        assert!(location.is_none());
    }

    #[test]
    fn test_find_definition_fqn_cursor_on_last_segment() {
        use crate::handlers::hover::get_word_at_position;
        use async_lsp::lsp_types::Position;

        // Create workspace with Address definition
        let mut workspace = Workspace::new();
        let address_uri = Url::parse("file:///address.avsc").unwrap();
        let address_schema = r#"{
  "type": "record",
  "name": "Address",
  "namespace": "com.example.common",
  "fields": [{"name": "city", "type": "string"}]
}"#;
        workspace
            .update_file(address_uri.clone(), address_schema.to_string())
            .unwrap();

        // User schema references Address with FQN
        let user_uri = Url::parse("file:///user.avsc").unwrap();
        let user_schema = r#"{
  "type": "record",
  "name": "User",
  "namespace": "com.example.app",
  "fields": [
    {"name": "id", "type": "long"},
    {"name": "address", "type": "com.example.common.Address"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(user_schema).unwrap();

        // Extract word at position - cursor on 'A' in "Address" (last segment of FQN)
        // Line 6 (0-indexed): "    {"name": "address", "type": "com.example.common.Address"}"
        // Position of 'A' in Address: after "com.example.common."
        let line_text = r#"    {"name": "address", "type": "com.example.common.Address"}"#;
        let position_of_a =
            line_text.find("com.example.common.Address").unwrap() + "com.example.common.".len();
        let pos = Position::new(6, position_of_a as u32);

        let word = get_word_at_position(user_schema, pos);
        assert_eq!(word, Some("com.example.common.Address".to_string()));

        // Looking up the FQN should point to address.avsc
        let location = find_definition_with_workspace(
            &schema,
            user_schema,
            "com.example.common.Address",
            &user_uri,
            Some(&workspace),
        );
        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.uri, address_uri);
    }

    #[test]
    fn test_find_definition_fqn_cursor_on_middle_segment() {
        use crate::handlers::hover::get_word_at_position;
        use async_lsp::lsp_types::Position;

        // Create workspace with Address definition
        let mut workspace = Workspace::new();
        let address_uri = Url::parse("file:///address.avsc").unwrap();
        let address_schema = r#"{
  "type": "record",
  "name": "Address",
  "namespace": "com.example.common",
  "fields": [{"name": "city", "type": "string"}]
}"#;
        workspace
            .update_file(address_uri.clone(), address_schema.to_string())
            .unwrap();

        // User schema references Address with FQN
        let user_uri = Url::parse("file:///user.avsc").unwrap();
        let user_schema = r#"{
  "type": "record",
  "name": "User",
  "namespace": "com.example.app",
  "fields": [
    {"name": "id", "type": "long"},
    {"name": "address", "type": "com.example.common.Address"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(user_schema).unwrap();

        // Extract word at position - cursor on 'x' in "example" (middle segment of FQN)
        let line_text = r#"    {"name": "address", "type": "com.example.common.Address"}"#;
        let position_of_x = line_text.find("com.example.common.Address").unwrap() + "com.e".len();
        let pos = Position::new(6, position_of_x as u32);

        let word = get_word_at_position(user_schema, pos);
        assert_eq!(word, Some("com.example.common.Address".to_string()));

        // Looking up the FQN should point to address.avsc
        let location = find_definition_with_workspace(
            &schema,
            user_schema,
            "com.example.common.Address",
            &user_uri,
            Some(&workspace),
        );
        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.uri, address_uri);
    }
}
