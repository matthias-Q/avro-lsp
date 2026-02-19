use std::collections::HashMap;

use async_lsp::lsp_types::{Location, Range, SymbolInformation, SymbolKind, Url};

use crate::schema::{AvroSchema, AvroType};

/// Collect all symbols from workspace schemas that match the query
pub fn collect_workspace_symbols(
    schemas: &HashMap<Url, AvroSchema>,
    query: &str,
) -> Vec<SymbolInformation> {
    let mut symbols = Vec::new();
    let query_lower = query.to_lowercase();

    for (uri, schema) in schemas {
        for (name, avro_type) in &schema.named_types {
            // Simple case-insensitive substring match
            // Empty query returns all symbols
            if (query.is_empty() || name.to_lowercase().contains(&query_lower))
                && let Some(symbol) = create_symbol_info(name, avro_type, uri)
            {
                symbols.push(symbol);
            }
        }
    }

    // Sort by name for consistent results
    symbols.sort_by(|a, b| a.name.cmp(&b.name));

    symbols
}

/// Create a SymbolInformation from an AvroType
fn create_symbol_info(_name: &str, avro_type: &AvroType, uri: &Url) -> Option<SymbolInformation> {
    match avro_type {
        AvroType::Record(record) =>
        {
            #[allow(deprecated)]
            Some(SymbolInformation {
                name: record.name.clone(),
                kind: SymbolKind::STRUCT,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: record.name_range.unwrap_or_else(default_range),
                },
                container_name: record.namespace.clone(),
            })
        }
        AvroType::Enum(enum_schema) =>
        {
            #[allow(deprecated)]
            Some(SymbolInformation {
                name: enum_schema.name.clone(),
                kind: SymbolKind::ENUM,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: enum_schema.name_range.unwrap_or_else(default_range),
                },
                container_name: enum_schema.namespace.clone(),
            })
        }
        AvroType::Fixed(fixed) =>
        {
            #[allow(deprecated)]
            Some(SymbolInformation {
                name: fixed.name.clone(),
                kind: SymbolKind::STRUCT,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: uri.clone(),
                    range: fixed.name_range.unwrap_or_else(default_range),
                },
                container_name: fixed.namespace.clone(),
            })
        }
        _ => None,
    }
}

/// Default range when no range is available
fn default_range() -> Range {
    Range {
        start: async_lsp::lsp_types::Position {
            line: 0,
            character: 0,
        },
        end: async_lsp::lsp_types::Position {
            line: 0,
            character: 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AvroParser;

    #[test]
    fn test_collect_workspace_symbols_empty_query() {
        let mut schemas = HashMap::new();
        let uri = Url::parse("file:///test.avsc").unwrap();

        let mut parser = AvroParser::new();
        let schema_text = r#"{
            "type": "record",
            "name": "User",
            "namespace": "com.example",
            "fields": [{"name": "id", "type": "long"}]
        }"#;
        let schema = parser.parse(schema_text).unwrap();
        schemas.insert(uri.clone(), schema);

        // Empty query should return all symbols
        let symbols = collect_workspace_symbols(&schemas, "");

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].kind, SymbolKind::STRUCT);
        assert_eq!(symbols[0].container_name, Some("com.example".to_string()));
    }

    #[test]
    fn test_collect_workspace_symbols_exact_match() {
        let mut schemas = HashMap::new();
        let uri = Url::parse("file:///test.avsc").unwrap();

        let mut parser = AvroParser::new();
        let schema_text = r#"{
            "type": "record",
            "name": "Address",
            "fields": [{"name": "city", "type": "string"}]
        }"#;
        let schema = parser.parse(schema_text).unwrap();
        schemas.insert(uri.clone(), schema);

        let symbols = collect_workspace_symbols(&schemas, "Address");

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Address");
    }

    #[test]
    fn test_collect_workspace_symbols_partial_match() {
        let mut schemas = HashMap::new();
        let uri = Url::parse("file:///test.avsc").unwrap();

        let mut parser = AvroParser::new();
        let schema_text = r#"{
            "type": "record",
            "name": "UserAddress",
            "fields": [{"name": "city", "type": "string"}]
        }"#;
        let schema = parser.parse(schema_text).unwrap();
        schemas.insert(uri.clone(), schema);

        // Partial match should work
        let symbols = collect_workspace_symbols(&schemas, "Address");

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "UserAddress");
    }

    #[test]
    fn test_collect_workspace_symbols_case_insensitive() {
        let mut schemas = HashMap::new();
        let uri = Url::parse("file:///test.avsc").unwrap();

        let mut parser = AvroParser::new();
        let schema_text = r#"{
            "type": "record",
            "name": "Address",
            "fields": [{"name": "city", "type": "string"}]
        }"#;
        let schema = parser.parse(schema_text).unwrap();
        schemas.insert(uri.clone(), schema);

        // Case insensitive search
        let symbols = collect_workspace_symbols(&schemas, "address");

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Address");
    }

    #[test]
    fn test_collect_workspace_symbols_multiple_files() {
        let mut schemas = HashMap::new();

        // First file
        let uri1 = Url::parse("file:///user.avsc").unwrap();
        let mut parser1 = AvroParser::new();
        let schema1 = parser1
            .parse(
                r#"{
            "type": "record",
            "name": "User",
            "namespace": "com.example",
            "fields": [{"name": "id", "type": "long"}]
        }"#,
            )
            .unwrap();
        schemas.insert(uri1, schema1);

        // Second file
        let uri2 = Url::parse("file:///address.avsc").unwrap();
        let mut parser2 = AvroParser::new();
        let schema2 = parser2
            .parse(
                r#"{
            "type": "record",
            "name": "Address",
            "namespace": "com.example",
            "fields": [{"name": "city", "type": "string"}]
        }"#,
            )
            .unwrap();
        schemas.insert(uri2, schema2);

        let symbols = collect_workspace_symbols(&schemas, "");

        assert_eq!(symbols.len(), 2);
        // Results should be sorted by name
        assert_eq!(symbols[0].name, "Address");
        assert_eq!(symbols[1].name, "User");
    }

    #[test]
    fn test_collect_workspace_symbols_different_types() {
        let mut schemas = HashMap::new();

        // Record
        let uri1 = Url::parse("file:///record.avsc").unwrap();
        let mut parser1 = AvroParser::new();
        let schema1 = parser1
            .parse(
                r#"{
            "type": "record",
            "name": "TestRecord",
            "fields": [{"name": "id", "type": "long"}]
        }"#,
            )
            .unwrap();
        schemas.insert(uri1, schema1);

        // Enum
        let uri2 = Url::parse("file:///enum.avsc").unwrap();
        let mut parser2 = AvroParser::new();
        let schema2 = parser2
            .parse(
                r#"{
            "type": "enum",
            "name": "TestEnum",
            "symbols": ["A", "B", "C"]
        }"#,
            )
            .unwrap();
        schemas.insert(uri2, schema2);

        // Fixed
        let uri3 = Url::parse("file:///fixed.avsc").unwrap();
        let mut parser3 = AvroParser::new();
        let schema3 = parser3
            .parse(
                r#"{
            "type": "fixed",
            "name": "TestFixed",
            "size": 16
        }"#,
            )
            .unwrap();
        schemas.insert(uri3, schema3);

        let symbols = collect_workspace_symbols(&schemas, "Test");

        assert_eq!(symbols.len(), 3);
        // Check that different kinds are represented
        assert!(symbols.iter().any(|s| s.kind == SymbolKind::STRUCT)); // Record and Fixed
        assert!(symbols.iter().any(|s| s.kind == SymbolKind::ENUM));
    }

    #[test]
    fn test_collect_workspace_symbols_no_match() {
        let mut schemas = HashMap::new();
        let uri = Url::parse("file:///test.avsc").unwrap();

        let mut parser = AvroParser::new();
        let schema_text = r#"{
            "type": "record",
            "name": "User",
            "fields": [{"name": "id", "type": "long"}]
        }"#;
        let schema = parser.parse(schema_text).unwrap();
        schemas.insert(uri.clone(), schema);

        let symbols = collect_workspace_symbols(&schemas, "NonExistent");

        assert_eq!(symbols.len(), 0);
    }

    #[test]
    fn test_collect_workspace_symbols_with_namespace() {
        let mut schemas = HashMap::new();
        let uri = Url::parse("file:///test.avsc").unwrap();

        let mut parser = AvroParser::new();
        let schema_text = r#"{
            "type": "record",
            "name": "User",
            "namespace": "com.example.model",
            "fields": [{"name": "id", "type": "long"}]
        }"#;
        let schema = parser.parse(schema_text).unwrap();
        schemas.insert(uri.clone(), schema);

        let symbols = collect_workspace_symbols(&schemas, "User");

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(
            symbols[0].container_name,
            Some("com.example.model".to_string())
        );
    }
}
