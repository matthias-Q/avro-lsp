use async_lsp::lsp_types::{CompletionItem, CompletionItemKind, InsertTextFormat, Position};

use crate::schema::AvroSchema;

/// Context for determining what kind of completion to provide
#[derive(Debug, Clone, PartialEq)]
enum CompletionContext {
    JsonKey,         // Suggesting a JSON key (e.g., after { or ,)
    TypeValue,       // Suggesting a type value (e.g., after "type":)
    FieldAttribute,  // Suggesting field attributes (inside fields array)
    EnumAttribute,   // Suggesting enum attributes
    RecordAttribute, // Suggesting record attributes
    Unknown,         // Unknown context
}

pub fn get_completions(
    text: &str,
    position: Position,
    schema: Option<&AvroSchema>,
) -> Vec<CompletionItem> {
    let context = analyze_completion_context(text, position);

    tracing::debug!(
        "Completion context at {}:{} - {:?}",
        position.line,
        position.character,
        context
    );

    let mut items = Vec::new();

    match context {
        CompletionContext::JsonKey => {
            // Suggest common Avro schema keys
            items.extend(get_key_completions());
        }
        CompletionContext::TypeValue => {
            // Suggest type values (primitives, complex types, or references)
            items.extend(get_type_value_completions(schema));
        }
        CompletionContext::FieldAttribute => {
            // Suggest field attributes
            items.extend(get_field_attribute_completions());
        }
        CompletionContext::EnumAttribute => {
            // Suggest enum attributes
            items.extend(get_enum_attribute_completions());
        }
        CompletionContext::RecordAttribute => {
            // Suggest record attributes
            items.extend(get_record_attribute_completions());
        }
        CompletionContext::Unknown => {
            // Provide general suggestions
            items.extend(get_key_completions());
        }
    }

    items
}

fn analyze_completion_context(text: &str, position: Position) -> CompletionContext {
    let lines: Vec<&str> = text.lines().collect();
    if position.line as usize >= lines.len() {
        return CompletionContext::Unknown;
    }

    let line = lines[position.line as usize];
    let char_pos = position.character as usize;

    // Get text before cursor on this line
    let before_cursor = if char_pos <= line.len() {
        &line[..char_pos]
    } else {
        line
    };

    // Check if we're after a colon (suggesting a value)
    if before_cursor.trim_end().ends_with(':') {
        // Determine what key we're providing a value for
        if before_cursor.contains("\"type\"") {
            return CompletionContext::TypeValue;
        }
        return CompletionContext::Unknown;
    }

    // Check if we're in a "fields" array (suggesting field attributes)
    let context_start = position.line.saturating_sub(10) as usize;
    let context_lines = &lines[context_start..=position.line as usize];
    let context_text = context_lines.join("\n");

    if context_text.contains("\"fields\"")
        && !context_text[context_text.rfind("\"fields\"").unwrap()..].contains(']')
    {
        // We're inside a fields array
        if before_cursor.trim_end().ends_with('{') || before_cursor.trim_end().ends_with(',') {
            return CompletionContext::FieldAttribute;
        }
    }

    // Check if we're in an enum definition
    if context_text.contains("\"type\": \"enum\"")
        && (before_cursor.trim_end().ends_with('{') || before_cursor.trim_end().ends_with(','))
    {
        return CompletionContext::EnumAttribute;
    }

    // Check if we're in a record definition
    if context_text.contains("\"type\": \"record\"")
        && (before_cursor.trim_end().ends_with('{') || before_cursor.trim_end().ends_with(','))
    {
        return CompletionContext::RecordAttribute;
    }

    // Default: suggest JSON keys
    if before_cursor.trim_end().ends_with('{') || before_cursor.trim_end().ends_with(',') {
        return CompletionContext::JsonKey;
    }

    CompletionContext::Unknown
}

/// Get completions for JSON keys
fn get_key_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "type".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("The type of the schema".to_string()),
            insert_text: Some("\"type\": $0".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "name".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("The name of the type".to_string()),
            insert_text: Some("\"name\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "namespace".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("The namespace for the type".to_string()),
            insert_text: Some("\"namespace\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "doc".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Documentation for the type".to_string()),
            insert_text: Some("\"doc\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "fields".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Array of fields (for record types)".to_string()),
            insert_text: Some("\"fields\": [$0]".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "symbols".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Array of symbols (for enum types)".to_string()),
            insert_text: Some("\"symbols\": [$0]".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
    ]
}

/// Get completions for type values
fn get_type_value_completions(schema: Option<&AvroSchema>) -> Vec<CompletionItem> {
    let mut items = vec![
        // Primitive types
        CompletionItem {
            label: "string".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Primitive type: Unicode character sequence".to_string()),
            insert_text: Some("\"string\"".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "int".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Primitive type: 32-bit signed integer".to_string()),
            insert_text: Some("\"int\"".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "long".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Primitive type: 64-bit signed integer".to_string()),
            insert_text: Some("\"long\"".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "float".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Primitive type: Single precision floating-point".to_string()),
            insert_text: Some("\"float\"".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "double".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Primitive type: Double precision floating-point".to_string()),
            insert_text: Some("\"double\"".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "boolean".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Primitive type: Binary value (true or false)".to_string()),
            insert_text: Some("\"boolean\"".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "null".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Primitive type: No value".to_string()),
            insert_text: Some("\"null\"".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "bytes".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Primitive type: Sequence of 8-bit bytes".to_string()),
            insert_text: Some("\"bytes\"".to_string()),
            ..Default::default()
        },
        // Complex types
        CompletionItem {
            label: "record".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Complex type: Named collection of fields".to_string()),
            insert_text: Some("\"record\"".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "enum".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Complex type: Enumeration of symbols".to_string()),
            insert_text: Some("\"enum\"".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "array".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Complex type: List of items of same type".to_string()),
            insert_text: Some("\"array\"".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "map".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Complex type: Key-value pairs".to_string()),
            insert_text: Some("\"map\"".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "fixed".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Complex type: Fixed-size byte array".to_string()),
            insert_text: Some("\"fixed\"".to_string()),
            ..Default::default()
        },
        // Union type
        CompletionItem {
            label: "[...]".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Union type: Array of types".to_string()),
            insert_text: Some("[$0]".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
    ];

    // Add named types from schema if available
    if let Some(schema) = schema {
        for name in schema.named_types.keys() {
            items.push(CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::REFERENCE),
                detail: Some(format!("Reference to named type '{}'", name)),
                insert_text: Some(format!("\"{}\"", name)),
                ..Default::default()
            });
        }
    }

    items
}

/// Get completions for field attributes
fn get_field_attribute_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "name".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("The name of the field".to_string()),
            insert_text: Some("\"name\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "type".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("The type of the field".to_string()),
            insert_text: Some("\"type\": $0".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "doc".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Documentation for the field".to_string()),
            insert_text: Some("\"doc\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "default".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Default value for the field".to_string()),
            insert_text: Some("\"default\": $0".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "order".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Sorting order (ascending, descending, ignore)".to_string()),
            insert_text: Some("\"order\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "aliases".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Alternative names for the field".to_string()),
            insert_text: Some("\"aliases\": [$0]".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
    ]
}

/// Get completions for enum attributes
fn get_enum_attribute_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "type".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Must be \"enum\"".to_string()),
            insert_text: Some("\"type\": \"enum\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "name".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("The name of the enum".to_string()),
            insert_text: Some("\"name\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "namespace".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("The namespace for the enum".to_string()),
            insert_text: Some("\"namespace\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "doc".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Documentation for the enum".to_string()),
            insert_text: Some("\"doc\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "symbols".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Array of symbol strings".to_string()),
            insert_text: Some("\"symbols\": [$0]".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "aliases".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Alternative names for the enum".to_string()),
            insert_text: Some("\"aliases\": [$0]".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "default".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Default symbol for the enum".to_string()),
            insert_text: Some("\"default\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
    ]
}

/// Get completions for record attributes
fn get_record_attribute_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "type".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Must be \"record\"".to_string()),
            insert_text: Some("\"type\": \"record\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "name".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("The name of the record".to_string()),
            insert_text: Some("\"name\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "namespace".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("The namespace for the record".to_string()),
            insert_text: Some("\"namespace\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "doc".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Documentation for the record".to_string()),
            insert_text: Some("\"doc\": \"$0\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "fields".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Array of record fields".to_string()),
            insert_text: Some("\"fields\": [$0]".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "aliases".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Alternative names for the record".to_string()),
            insert_text: Some("\"aliases\": [$0]".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AvroParser;

    #[test]
    fn test_completion_after_opening_brace() {
        let text = r#"{
  "type": "record",
  "name": "User",
  "
}"#;

        // Position after the opening quote on line 3
        let position = Position {
            line: 3,
            character: 3,
        };
        let items = get_completions(text, position, None);

        assert!(!items.is_empty(), "Should have completions");

        // Should suggest keys like "type", "name", etc.
        let has_type = items.iter().any(|i| i.label == "type");
        let has_name = items.iter().any(|i| i.label == "name");

        assert!(has_type, "Should suggest 'type' key");
        assert!(has_name, "Should suggest 'name' key");
    }

    #[test]
    fn test_completion_for_type_value() {
        let text = r#"{
  "type": 
}"#;

        // Position after "type":
        let position = Position {
            line: 1,
            character: 10,
        };
        let items = get_completions(text, position, None);

        assert!(!items.is_empty(), "Should have completions");

        // Should suggest primitive types
        let has_string = items.iter().any(|i| i.label == "string");
        let has_int = items.iter().any(|i| i.label == "int");
        let has_record = items.iter().any(|i| i.label == "record");

        assert!(has_string, "Should suggest 'string' type");
        assert!(has_int, "Should suggest 'int' type");
        assert!(has_record, "Should suggest 'record' type");
    }

    #[test]
    fn test_completion_includes_named_types() {
        let schema_text = r#"{
  "type": "record",
  "name": "Address",
  "fields": []
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");

        let text = r#"{
  "type": 
}"#;

        let position = Position {
            line: 1,
            character: 10,
        };
        let items = get_completions(text, position, Some(&schema));

        // Should include the named type "Address" as a reference
        let has_address = items.iter().any(|i| i.label == "Address");
        assert!(has_address, "Should suggest 'Address' as a type reference");

        // Check it's marked as a reference
        let address_item = items.iter().find(|i| i.label == "Address").unwrap();
        assert_eq!(address_item.kind, Some(CompletionItemKind::REFERENCE));
    }

    #[test]
    fn test_completion_in_fields_array() {
        let text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {
      "
    }
  ]
}"#;

        // Position inside a field object
        let position = Position {
            line: 5,
            character: 7,
        };
        let items = get_completions(text, position, None);

        assert!(!items.is_empty(), "Should have completions");

        // Should suggest field attributes or at least common keys
        let has_name = items.iter().any(|i| i.label == "name");
        let has_type = items.iter().any(|i| i.label == "type");

        assert!(has_name, "Should suggest 'name' attribute");
        assert!(has_type, "Should suggest 'type' attribute");

        // If we detect field context, we should have field-specific completions
        // But if we fall back to general completions, that's also acceptable
    }

    #[test]
    fn test_completion_in_enum_definition() {
        let text = r#"{
  "type": "enum",
  "
}"#;

        // Position inside an enum object
        let position = Position {
            line: 2,
            character: 3,
        };
        let items = get_completions(text, position, None);

        assert!(!items.is_empty(), "Should have completions");

        // Should suggest enum attributes
        let has_symbols = items.iter().any(|i| i.label == "symbols");
        assert!(has_symbols, "Should suggest 'symbols' for enum");
    }

    #[test]
    fn test_completion_snippets_have_placeholders() {
        let text = r#"{
  "
}"#;

        let position = Position {
            line: 1,
            character: 3,
        };
        let items = get_completions(text, position, None);

        // Find the "name" completion
        let name_item = items.iter().find(|i| i.label == "name");
        assert!(name_item.is_some(), "Should have 'name' completion");

        let name_item = name_item.unwrap();

        // Check it has snippet format
        assert_eq!(
            name_item.insert_text_format,
            Some(InsertTextFormat::SNIPPET)
        );

        // Check it has a placeholder ($0)
        let insert_text = name_item.insert_text.as_ref().unwrap();
        assert!(
            insert_text.contains("$0"),
            "Should have snippet placeholder"
        );
    }
}
