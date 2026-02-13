use async_lsp::ResponseError;
use async_lsp::lsp_types::{
    CompletionItem, Diagnostic, DocumentSymbol, Hover, Location, Position, PrepareRenameResponse,
    Range, SemanticToken, Url,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::schema::{
    AvroParser, AvroSchema, AvroType, EnumSchema, Field, FixedSchema, RecordSchema,
};

#[derive(Clone)]
pub struct ServerState {
    inner: Arc<RwLock<ServerStateInner>>,
}

struct ServerStateInner {
    documents: HashMap<Url, Document>,
}

struct Document {
    #[allow(dead_code)]
    text: String,
    #[allow(dead_code)]
    version: i32,
    #[allow(dead_code)]
    schema: Option<AvroSchema>,
    #[allow(dead_code)]
    diagnostics: Vec<Diagnostic>,
}

/// Represents a specific node in the AST at a given position
#[derive(Debug, Clone)]
pub enum AstNode<'a> {
    /// Cursor is somewhere in the record definition
    RecordDefinition(&'a RecordSchema),
    /// Cursor is on a field object
    Field(&'a Field),
    /// Cursor is on a field's type value (key for "make nullable" action)
    FieldType(&'a Field),
    /// Cursor is somewhere in the enum definition
    EnumDefinition(&'a EnumSchema),
    /// Cursor is somewhere in the fixed definition
    FixedDefinition(&'a FixedSchema),
}

/// Helper to check if position is inside a range
pub fn position_in_range(pos: Position, range: &Range) -> bool {
    if pos.line < range.start.line || pos.line > range.end.line {
        return false;
    }
    if pos.line == range.start.line && pos.character < range.start.character {
        return false;
    }
    if pos.line == range.end.line && pos.character > range.end.character {
        return false;
    }
    true
}

/// Find the most specific AST node at the given position
pub fn find_node_at_position<'a>(
    schema: &'a AvroSchema,
    position: Position,
) -> Option<AstNode<'a>> {
    // Walk the root type
    find_node_in_type(&schema.root, position, &position_in_range)
}

/// Recursively find node in an AvroType
fn find_node_in_type<'a>(
    avro_type: &'a AvroType,
    position: Position,
    position_in_range: &impl Fn(Position, &Range) -> bool,
) -> Option<AstNode<'a>> {
    match avro_type {
        AvroType::Record(record) => {
            // Check if position is in this record's range
            if let Some(record_range) = &record.range {
                if !position_in_range(position, record_range) {
                    return None;
                }

                // Check each field for more specific matches
                for field in &record.fields {
                    if let Some(field_range) = &field.range
                        && position_in_range(position, field_range)
                    {
                        // Check if position is specifically on field's name
                        if let Some(name_range) = &field.name_range
                            && position_in_range(position, name_range)
                        {
                            // On field name - always return Field for field-level actions
                            return Some(AstNode::Field(field));
                        }

                        // Check if position is on field's type value (for "make nullable")
                        if let Some(type_range) = &field.type_range
                            && position_in_range(position, type_range)
                        {
                            return Some(AstNode::FieldType(field));
                        }

                        // For any other position within the field (but not on nested type definitions),
                        // return Field to provide field-level actions
                        // This makes "Add documentation" available anywhere in the field
                        return Some(AstNode::Field(field));
                    }
                }

                // Position is in record but not in any specific sub-element
                return Some(AstNode::RecordDefinition(record));
            }
            None
        }
        AvroType::Enum(enum_schema) => {
            if let Some(enum_range) = &enum_schema.range
                && position_in_range(position, enum_range)
            {
                return Some(AstNode::EnumDefinition(enum_schema));
            }
            None
        }
        AvroType::Fixed(fixed) => {
            // Check if position is in this fixed's range
            if let Some(fixed_range) = &fixed.range
                && position_in_range(position, fixed_range)
            {
                return Some(AstNode::FixedDefinition(fixed));
            }
            None
        }
        AvroType::Array(array) => {
            // Recursively check the array's items type
            find_node_in_type(&array.items, position, position_in_range)
        }
        AvroType::Map(map) => {
            // Recursively check the map's values type
            find_node_in_type(&map.values, position, position_in_range)
        }
        AvroType::Union(types) => {
            // Check each type in the union
            for avro_type in types {
                if let Some(node) = find_node_in_type(avro_type, position, position_in_range) {
                    return Some(node);
                }
            }
            None
        }
        // Primitives and TypeRefs don't have position info
        AvroType::Primitive(_) | AvroType::TypeRef(_) => None,
    }
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(ServerStateInner {
                documents: HashMap::new(),
            })),
        }
    }

    /// Open a document and parse/validate it
    pub async fn did_open(&self, uri: Url, text: String, version: i32) -> Vec<Diagnostic> {
        let mut state = self.inner.write().await;
        let diagnostics = crate::handlers::diagnostics::parse_and_validate(&text);

        let schema = if diagnostics.is_empty() {
            let mut parser = AvroParser::new();
            parser.parse(&text).ok()
        } else {
            None
        };

        state.documents.insert(
            uri,
            Document {
                text,
                version,
                schema,
                diagnostics: diagnostics.clone(),
            },
        );

        diagnostics
    }

    /// Update a document and reparse/revalidate
    pub async fn did_change(&self, uri: Url, text: String, version: i32) -> Vec<Diagnostic> {
        self.did_open(uri, text, version).await
    }

    /// Close a document and clean up state
    pub async fn did_close(&self, uri: &Url) {
        let mut state = self.inner.write().await;
        state.documents.remove(uri);
    }

    /// Get hover information for a position in the document
    pub async fn get_hover(&self, uri: &Url, position: Position) -> Option<Hover> {
        let state = self.inner.read().await;
        let document = state.documents.get(uri)?;

        // Get the word at the cursor position
        let word = crate::handlers::hover::get_word_at_position(&document.text, position)?;

        // Try to find hover information for this word
        if let Some(schema) = &document.schema {
            crate::handlers::hover::generate_hover(schema, &document.text, &word)
        } else {
            None
        }
    }

    /// Get document symbols for a URI
    pub async fn get_document_symbols(&self, uri: &Url) -> Option<Vec<DocumentSymbol>> {
        let state = self.inner.read().await;
        let doc = state.documents.get(uri)?;
        let schema = doc.schema.as_ref()?;
        let text = &doc.text;

        let symbols = crate::handlers::symbols::create_document_symbols(schema, text);

        Some(symbols)
    }

    /// Get semantic tokens for a document
    pub async fn get_semantic_tokens(&self, uri: &Url) -> Option<Vec<SemanticToken>> {
        let state = self.inner.read().await;
        let doc = state.documents.get(uri)?;
        let schema = doc.schema.as_ref()?;
        let text = &doc.text;

        let tokens = crate::handlers::semantic_tokens::build_semantic_tokens(schema, text.clone());

        tracing::debug!("Generated {} semantic tokens for {}", tokens.len(), uri);

        Some(tokens)
    }

    /// Get completions for a position in the document
    pub async fn get_completions(
        &self,
        uri: &Url,
        position: Position,
    ) -> Option<Vec<CompletionItem>> {
        let state = self.inner.read().await;
        let doc = state.documents.get(uri)?;
        let text = &doc.text;
        let schema = doc.schema.as_ref();

        let items = crate::handlers::completion::get_completions(text, position, schema);

        Some(items)
    }

    /// Get definition location for a symbol at the given position
    pub async fn get_definition(&self, uri: &Url, position: Position) -> Option<Location> {
        let state = self.inner.read().await;
        let doc = state.documents.get(uri)?;
        let schema = doc.schema.as_ref()?;
        let text = &doc.text;

        // Get the word at the cursor position
        let word = crate::handlers::hover::get_word_at_position(text, position)?;

        tracing::debug!("Looking for definition of '{}'", word);

        crate::handlers::definition::find_definition(schema, text, &word, uri)
    }

    /// Format the document with proper JSON formatting
    /// Removes trailing commas and formats with 2-space indentation
    pub async fn format_document(
        &self,
        uri: &Url,
    ) -> Result<Option<async_lsp::lsp_types::TextEdit>, async_lsp::ResponseError> {
        let state = self.inner.read().await;
        let document = state.documents.get(uri).ok_or_else(|| {
            async_lsp::ResponseError::new(
                async_lsp::ErrorCode::INVALID_REQUEST,
                "Document not found",
            )
        })?;

        let edit = crate::handlers::formatting::format_document(&document.text)?;
        Ok(Some(edit))
    }

    /// Get code actions available at the given range
    pub async fn get_code_actions(
        &self,
        uri: &Url,
        range: Range,
        _diagnostics: Vec<Diagnostic>,
    ) -> Option<Vec<async_lsp::lsp_types::CodeAction>> {
        let state = self.inner.read().await;
        let doc = state.documents.get(uri)?;
        let schema = doc.schema.as_ref()?;

        crate::handlers::code_actions::get_code_actions(schema, uri, range)
    }

    /// Rename a symbol (record, enum, fixed, or field name)
    pub async fn rename(
        &self,
        uri: &Url,
        position: Position,
        new_name: &str,
    ) -> Result<Option<async_lsp::lsp_types::WorkspaceEdit>, ResponseError> {
        let state = self.inner.read().await;
        let doc = state.documents.get(uri);

        let doc = match doc {
            Some(d) => d,
            None => return Ok(None),
        };

        let schema = match &doc.schema {
            Some(s) => s,
            None => return Ok(None),
        };

        crate::handlers::rename::rename(schema, &doc.text, uri, position, new_name)
    }

    /// Prepare for rename - validate that rename is possible at this position
    pub async fn prepare_rename(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<PrepareRenameResponse>, ResponseError> {
        let state = self.inner.read().await;
        let doc = state.documents.get(uri);

        let doc = match doc {
            Some(d) => d,
            None => return Ok(None),
        };

        let schema = match &doc.schema {
            Some(s) => s,
            None => return Ok(None),
        };

        Ok(crate::handlers::rename::prepare_rename(schema, position))
    }

    /// Find all references to a symbol
    pub async fn find_references(
        &self,
        uri: &Url,
        position: Position,
        include_declaration: bool,
    ) -> Result<Option<Vec<Location>>, ResponseError> {
        let state = self.inner.read().await;
        let doc = state.documents.get(uri);

        let doc = match doc {
            Some(d) => d,
            None => return Ok(None),
        };

        let schema = match &doc.schema {
            Some(s) => s,
            None => return Ok(None),
        };

        Ok(crate::handlers::rename::find_references(
            schema,
            uri,
            position,
            include_declaration,
        ))
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_format_simple_record() {
        let state = ServerState::new();
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Unformatted JSON with no spaces
        let unformatted =
            r#"{"type":"record","name":"User","fields":[{"name":"id","type":"long"}]}"#;

        state
            .did_open(uri.clone(), unformatted.to_string(), 1)
            .await;

        let result = state.format_document(&uri).await;
        assert!(result.is_ok(), "Formatting should succeed");

        let edit = result.unwrap().unwrap();
        let formatted = edit.new_text;

        // Check that it's properly formatted with 2-space indentation
        assert!(formatted.contains("  \"type\": \"record\""));
        assert!(formatted.contains("  \"name\": \"User\""));
        assert!(formatted.contains("  \"fields\": ["));
        assert!(formatted.ends_with("}\n"), "Should end with newline");
    }

    #[tokio::test]
    async fn test_format_removes_trailing_commas() {
        let state = ServerState::new();
        let uri = Url::parse("file:///test.avsc").unwrap();

        // JSON with trailing commas (invalid JSON but common mistake)
        let with_trailing =
            r#"{"type":"record","name":"User","fields":[{"name":"id","type":"long",}]}"#;

        state
            .did_open(uri.clone(), with_trailing.to_string(), 1)
            .await;

        let result = state.format_document(&uri).await;
        assert!(
            result.is_ok(),
            "Formatting should succeed and remove trailing commas"
        );

        let edit = result.unwrap().unwrap();
        let formatted = edit.new_text;

        // Verify trailing commas are removed
        assert!(
            !formatted.contains(",}"),
            "Should not contain trailing comma before }}"
        );
        assert!(
            !formatted.contains(",]"),
            "Should not contain trailing comma before ]"
        );

        // Verify it's valid JSON that can be parsed
        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(parsed["type"], "record");
    }

    #[tokio::test]
    async fn test_format_nested_record() {
        let state = ServerState::new();
        let uri = Url::parse("file:///test.avsc").unwrap();

        let unformatted = r#"{"type":"record","name":"Person","fields":[{"name":"address","type":{"type":"record","name":"Address","fields":[{"name":"city","type":"string"}]}}]}"#;

        state
            .did_open(uri.clone(), unformatted.to_string(), 1)
            .await;

        let result = state.format_document(&uri).await;
        assert!(result.is_ok());

        let edit = result.unwrap().unwrap();
        let formatted = edit.new_text;

        // Check nested structure is properly indented
        assert!(
            formatted.contains("    \"type\": \"record\""),
            "Nested record should be indented 4 spaces"
        );
        assert!(
            formatted.contains("      \"name\": \"Address\""),
            "Nested fields should be indented 6 spaces"
        );
    }

    #[tokio::test]
    async fn test_format_invalid_json() {
        let state = ServerState::new();
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Invalid JSON - missing closing brace
        let invalid = r#"{"type": "record""#;

        state.did_open(uri.clone(), invalid.to_string(), 1).await;

        let result = state.format_document(&uri).await;
        assert!(result.is_err(), "Should return error for invalid JSON");

        let err = result.unwrap_err();
        assert_eq!(err.code, async_lsp::ErrorCode::PARSE_ERROR);
        assert!(err.message.contains("Invalid JSON"));
    }

    #[tokio::test]
    async fn test_format_idempotent() {
        let state = ServerState::new();
        let uri = Url::parse("file:///test.avsc").unwrap();

        let unformatted =
            r#"{"type":"record","name":"User","fields":[{"name":"id","type":"long"}]}"#;

        state
            .did_open(uri.clone(), unformatted.to_string(), 1)
            .await;

        // Format once
        let result1 = state.format_document(&uri).await.unwrap().unwrap();
        let formatted1 = result1.new_text;

        // Update document with formatted text
        state.did_change(uri.clone(), formatted1.clone(), 2).await;

        // Format again
        let result2 = state.format_document(&uri).await.unwrap().unwrap();
        let formatted2 = result2.new_text;

        // Should be identical
        assert_eq!(formatted1, formatted2, "Formatting should be idempotent");
    }

    #[tokio::test]
    async fn test_format_enum_with_symbols() {
        let state = ServerState::new();
        let uri = Url::parse("file:///test.avsc").unwrap();

        let unformatted = r#"{"type":"enum","name":"Color","symbols":["RED","GREEN","BLUE"]}"#;

        state
            .did_open(uri.clone(), unformatted.to_string(), 1)
            .await;

        let result = state.format_document(&uri).await;
        assert!(result.is_ok());

        let edit = result.unwrap().unwrap();
        let formatted = edit.new_text;

        // Check that symbols array is formatted
        assert!(formatted.contains("  \"symbols\": ["));
        assert!(formatted.contains("\"RED\""));
    }
}
