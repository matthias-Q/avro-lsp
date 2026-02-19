use async_lsp::lsp_types::{DocumentHighlight, DocumentHighlightKind, Position, Range};

use crate::schema::AvroSchema;

/// Find all occurrences of a symbol in the document for highlighting
pub fn find_document_highlights(
    schema: &AvroSchema,
    text: &str,
    word: &str,
) -> Vec<DocumentHighlight> {
    let mut highlights = Vec::new();

    // Check if it's a named type (Record, Enum, Fixed)
    if schema.named_types.contains_key(word) {
        // Find all occurrences of this type name in the document
        highlights.extend(find_all_occurrences(text, word));
    } else {
        // Could be a field name or enum symbol - find all occurrences
        highlights.extend(find_all_occurrences(text, word));
    }

    highlights
}

/// Find all occurrences of a word in the text as quoted JSON strings
fn find_all_occurrences(text: &str, word: &str) -> Vec<DocumentHighlight> {
    let mut highlights = Vec::new();
    let search_pattern = format!("\"{}\"", word);

    let mut search_offset = 0;
    while let Some(relative_offset) = text[search_offset..].find(&search_pattern) {
        let absolute_offset = search_offset + relative_offset;

        // Check if this is a JSON key (followed by colon)
        // Skip it if it is - we only want to highlight values
        let after_quote = absolute_offset + search_pattern.len();
        let is_json_key = text[after_quote..].chars().find(|c| !c.is_whitespace()) == Some(':');

        if !is_json_key {
            // Calculate position (inside quotes)
            let start_pos = offset_to_position(text, absolute_offset + 1); // +1 to skip opening quote
            let end_pos = offset_to_position(text, absolute_offset + 1 + word.len());

            highlights.push(DocumentHighlight {
                range: Range {
                    start: start_pos,
                    end: end_pos,
                },
                kind: Some(DocumentHighlightKind::TEXT),
            });
        }

        // Move past this occurrence
        search_offset = absolute_offset + search_pattern.len();
    }

    highlights
}

/// Convert byte offset to LSP Position
fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line = 0;
    let mut character = 0;

    for (i, ch) in text.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }

    Position { line, character }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AvroParser;

    #[test]
    fn test_highlight_type_name() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "long"},
    {"name": "address", "type": "Address"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).unwrap();

        // Highlighting "User" should find it once (the name declaration)
        let highlights = find_document_highlights(&schema, schema_text, "User");
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].kind, Some(DocumentHighlightKind::TEXT));
    }

    #[test]
    fn test_highlight_field_name() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "long"},
    {"name": "email", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).unwrap();

        // Highlighting "id" should find it once
        let highlights = find_document_highlights(&schema, schema_text, "id");
        assert_eq!(highlights.len(), 1);
    }

    #[test]
    fn test_highlight_type_reference() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "address", "type": "Address"},
    {"name": "billing", "type": "Address"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).unwrap();

        // Highlighting "Address" should find it twice (two type references)
        let highlights = find_document_highlights(&schema, schema_text, "Address");
        assert_eq!(highlights.len(), 2);
    }

    #[test]
    fn test_highlight_enum_symbol() {
        let schema_text = r#"{
  "type": "enum",
  "name": "Status",
  "symbols": ["ACTIVE", "INACTIVE", "PENDING"]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).unwrap();

        // Highlighting "ACTIVE" should find it once
        let highlights = find_document_highlights(&schema, schema_text, "ACTIVE");
        assert_eq!(highlights.len(), 1);
    }

    #[test]
    fn test_highlight_field_same_as_key() {
        // This test verifies that when a field name matches a JSON property name,
        // we only highlight the VALUE, not the JSON keys
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "name", "type": "string"},
    {"name": "age", "type": "int"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).unwrap();

        // When searching for "name", we should find it where it appears as a VALUE:
        // Line 4: {"name": "name", ...} - the field whose name VALUE is "name"
        //
        // We do NOT highlight JSON keys like "name": on lines 2, 4, 5
        let highlights = find_document_highlights(&schema, schema_text, "name");

        // Should find exactly 1 occurrence: the field name value "name" on line 4
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].range.start.line, 4);
    }

    #[test]
    fn test_highlight_multiple_field_occurrences() {
        // This test verifies highlighting a type name that appears multiple times as VALUES
        let schema_text = r#"{
  "type": "record",
  "name": "Person",
  "fields": [
    {"name": "home", "type": "Address"},
    {"name": "work", "type": "Address"},
    {"name": "billing", "type": "Address"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).unwrap();

        // Highlighting "Address" should find it three times (three type references)
        let highlights = find_document_highlights(&schema, schema_text, "Address");
        assert_eq!(highlights.len(), 3);
    }

    #[test]
    fn test_highlight_no_matches() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "long"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).unwrap();

        // Highlighting "NonExistent" should find nothing
        let highlights = find_document_highlights(&schema, schema_text, "NonExistent");
        assert_eq!(highlights.len(), 0);
    }

    #[test]
    fn test_offset_to_position() {
        let text = "line 0\nline 1\nline 2";

        // Position at start
        let pos = offset_to_position(text, 0);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);

        // Position in middle of line 0
        let pos = offset_to_position(text, 3);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 3);

        // Position at start of line 1
        let pos = offset_to_position(text, 7); // After "line 0\n"
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);

        // Position in middle of line 1
        let pos = offset_to_position(text, 10);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 3);
    }
}
