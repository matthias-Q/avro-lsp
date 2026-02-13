use async_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, DocumentSymbol, Hover,
    InsertTextFormat, Location, Position, Range, SemanticToken, SymbolKind, Url,
};
use async_lsp::ResponseError;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::schema::{
    AvroParser, AvroSchema, AvroType, AvroValidator, EnumSchema, Field, FixedSchema,
    PrimitiveType, RecordSchema, SchemaError,
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
}

/// Find the most specific AST node at the given position
pub fn find_node_at_position<'a>(schema: &'a AvroSchema, position: Position) -> Option<AstNode<'a>> {
    // Helper to check if position is inside a range
    fn position_in_range(pos: Position, range: &Range) -> bool {
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
                        && position_in_range(position, field_range) {
                            // Check if position is on field's type (MOST IMPORTANT for "make nullable")
                            if let Some(type_range) = &field.type_range
                                && position_in_range(position, type_range) {
                                    return Some(AstNode::FieldType(field));
                                }

                            // Recursively check the field's type for nested structures
                            if let Some(nested) =
                                find_node_in_type(&field.field_type, position, position_in_range)
                            {
                                return Some(nested);
                            }

                            // If no more specific match, return the field itself
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
                && position_in_range(position, enum_range) {
                    return Some(AstNode::EnumDefinition(enum_schema));
                }
            None
        }
        AvroType::Fixed(_fixed) => {
            // Fixed types don't have doc field, no code actions available
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
        let diagnostics = self.parse_and_validate(&text);

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
        let word = self.get_word_at_position(&document.text, position)?;

        // Try to find hover information for this word
        if let Some(schema) = &document.schema {
            self.generate_hover(schema, &document.text, &word, position)
        } else {
            None
        }
    }

    /// Parse and validate schema, returning diagnostics
    fn parse_and_validate(&self, text: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Try to parse
        let mut parser = AvroParser::new();
        let schema = match parser.parse(text) {
            Ok(schema) => schema,
            Err(e) => {
                // Parse error - try to find position from error context or serde_json error
                let error_msg = e.to_string();
                tracing::debug!("JSON parse error: {}", error_msg);
                
                // Check if error has position information embedded
                let (position_range, adjusted_msg) = match &e {
                    SchemaError::MissingFieldWithContext { range, field, context, .. } => {
                        if let Some(r) = range {
                            tracing::debug!("Using position from error context: {:?}", r);
                            let msg = format!("Missing required field '{}' in {}", field, context);
                            (*r, msg)
                        } else {
                            let (pos, was_adjusted) = self.extract_error_position_with_context(&error_msg, text);
                            let range = Range {
                                start: pos,
                                end: Position {
                                    line: pos.line,
                                    character: pos.character + 1,
                                },
                            };
                            let msg = self.improve_error_message(&error_msg, &pos, was_adjusted);
                            (range, msg)
                        }
                    }
                    _ => {
                        let (pos, was_adjusted) = self.extract_error_position_with_context(&error_msg, text);
                        let range = Range {
                            start: pos,
                            end: Position {
                                line: pos.line,
                                character: pos.character + 1,
                            },
                        };
                        let msg = self.improve_error_message(&error_msg, &pos, was_adjusted);
                        (range, msg)
                    }
                };
                
                tracing::debug!("Extracted position range: {:?}", position_range);
                diagnostics.push(Diagnostic {
                    range: position_range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: None,
                    code_description: None,
                    source: Some("avro-lsp".to_string()),
                    message: adjusted_msg,
                    related_information: None,
                    tags: None,
                    data: None,
                });
                return diagnostics;
            }
        };

        // Try to validate - now using AST-based error position finding
        let validator = AvroValidator::new();
        if let Err(e) = validator.validate(&schema) {
            // Try to find the position of the error using AST
            let position_range = self.find_error_position_in_ast(&e, &schema);

            diagnostics.push(Diagnostic {
                range: position_range,
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("avro-lsp".to_string()),
                message: format!("Validation error: {}", e),
                related_information: None,
                tags: None,
                data: None,
            });
        }

        diagnostics
    }

    /// Find the position of a validation error using AST
    fn find_error_position_in_ast(&self, error: &SchemaError, schema: &AvroSchema) -> Range {
        // Helper to search for error location in AST
        fn search_type(avro_type: &AvroType, error: &SchemaError) -> Option<Range> {
            match error {
                SchemaError::InvalidName(name) => {
                    tracing::debug!("Searching for InvalidName: {}", name);
                    // Search for a Record/Enum/Fixed with this name
                    match avro_type {
                        AvroType::Record(record) if record.name == *name => {
                            tracing::debug!("Found record with invalid name at {:?}", record.name_range);
                            record.name_range
                        }
                        AvroType::Enum(enum_schema) if enum_schema.name == *name => {
                            tracing::debug!("Found enum with invalid name at {:?}", enum_schema.name_range);
                            enum_schema.name_range
                        }
                        AvroType::Fixed(fixed) if fixed.name == *name => {
                            tracing::debug!("Found fixed with invalid name at {:?}", fixed.name_range);
                            fixed.name_range
                        }
                        AvroType::Record(record) => {
                            tracing::debug!("Searching in record: {}", record.name);
                            // Check fields
                            for field in &record.fields {
                                if field.name == *name {
                                    tracing::debug!("Found field with invalid name '{}' at {:?}", field.name, field.name_range);
                                    return field.name_range;
                                }
                                if let Some(range) = search_type(&field.field_type, error) {
                                    return Some(range);
                                }
                            }
                            None
                        }
                        AvroType::Array(array) => search_type(&array.items, error),
                        AvroType::Map(map) => search_type(&map.values, error),
                        AvroType::Union(types) => {
                            for t in types {
                                if let Some(range) = search_type(t, error) {
                                    return Some(range);
                                }
                            }
                            None
                        }
                        _ => None,
                    }
                }
                SchemaError::UnknownTypeReference(type_name) => {
                    tracing::debug!("Searching for UnknownTypeReference: {}", type_name);
                    // Search for TypeRef with this name
                    match avro_type {
                        AvroType::TypeRef(type_ref) if type_ref.name == *type_name => {
                            tracing::debug!("Found TypeRef with unknown type at {:?}", type_ref.range);
                            type_ref.range
                        }
                        AvroType::Record(record) => {
                            for field in &record.fields {
                                if let Some(range) = search_type(&field.field_type, error) {
                                    return Some(range);
                                }
                            }
                            None
                        }
                        AvroType::Array(array) => search_type(&array.items, error),
                        AvroType::Map(map) => search_type(&map.values, error),
                        AvroType::Union(types) => {
                            for t in types {
                                if let Some(range) = search_type(t, error) {
                                    return Some(range);
                                }
                            }
                            None
                        }
                        _ => None,
                    }
                }
                _ => {
                    tracing::debug!("Unsupported error type for position finding: {:?}", error);
                    None
                }
            }
        }

        // Search for the error in the AST
        if let Some(range) = search_type(&schema.root, error) {
            tracing::debug!("Found error position: {:?}", range);
            return range;
        }

        tracing::warn!("Could not find error position in AST, defaulting to (0,0)");
        // Default fallback
        Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 1,
            },
        }
    }

    /// Find the position of a validation error in the source text

    /// Extract position from error message and adjust for JSON syntax errors
    /// Returns (Position, was_adjusted)
    fn extract_error_position_with_context(&self, error_msg: &str, text: &str) -> (Position, bool) {
        // serde_json errors often contain "line X, column Y"
        if let Some(line_pos) = error_msg.find("line ")
            && let Some(col_pos) = error_msg.find("column ")
        {
            let line_str = &error_msg[line_pos + 5..];
            let line_end = line_str
                .find(|c: char| !c.is_numeric())
                .unwrap_or(line_str.len());
            let line_num: u32 = line_str[..line_end].parse().unwrap_or(1);

            let col_str = &error_msg[col_pos + 7..];
            let col_end = col_str
                .find(|c: char| !c.is_numeric())
                .unwrap_or(col_str.len());
            let col_num: u32 = col_str[..col_end].parse().unwrap_or(0);

            let mut position = Position {
                line: line_num.saturating_sub(1), // LSP is 0-indexed
                character: col_num.saturating_sub(1),
            };

            let mut was_adjusted = false;

            // For JSON parse errors at the start of a line (column near 0),
            // adjust to the end of the previous line (likely missing comma/brace)
            if position.character <= 2 && position.line > 0 {
                let lines: Vec<&str> = text.lines().collect();
                if let Some(prev_line) = lines.get(position.line as usize - 1) {
                    // Point to the end of the previous line
                    position = Position {
                        line: position.line - 1,
                        character: prev_line.trim_end().len() as u32,
                    };
                    was_adjusted = true;
                    tracing::debug!("Adjusted JSON parse error position to end of previous line: {:?}", position);
                }
            }

            return (position, was_adjusted);
        }

        (Position { line: 0, character: 0 }, false)
    }

    /// Improve error message with correct position and helpful hints
    fn improve_error_message(&self, original_msg: &str, pos: &Position, was_adjusted: bool) -> String {
        // Extract the base error message without position info
        let base_msg = if let Some(colon_pos) = original_msg.find(": ") {
            &original_msg[colon_pos + 2..]
        } else {
            original_msg
        };

        // Build the message with correct position (1-indexed for display)
        let location = format!("line {}, column {}", pos.line + 1, pos.character + 1);
        
        // Add helpful hints based on error type and adjustment
        if was_adjusted {
            format!("JSON syntax error at {}: Expected comma or closing brace", location)
        } else if base_msg.contains("Unexpected trailing content") {
            format!("JSON syntax error at {}: {}", location, base_msg)
        } else {
            format!("JSON parse error at {}", location)
        }
    }

    /// Convert byte offset to line/character position
    fn offset_to_position(&self, text: &str, offset: usize) -> Position {
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

    /// Get the word at a specific position in the text
    fn get_word_at_position(&self, text: &str, position: Position) -> Option<String> {
        let lines: Vec<&str> = text.lines().collect();
        let line = lines.get(position.line as usize)?;

        let chars: Vec<char> = line.chars().collect();
        let pos = position.character as usize;

        if pos >= chars.len() {
            return None;
        }

        // Check if we're on a quote or alphanumeric character
        let char_at_pos = chars[pos];
        if !char_at_pos.is_alphanumeric() && char_at_pos != '_' && char_at_pos != '"' {
            return None;
        }

        // Find the start of the word (or quoted string)
        let mut start = pos;
        let in_quotes = char_at_pos == '"' || (pos > 0 && chars[pos - 1] == '"');

        if in_quotes {
            // Find the opening quote
            while start > 0 && chars[start] != '"' {
                start -= 1;
            }
            // Find the closing quote
            let mut end = start + 1;
            while end < chars.len() && chars[end] != '"' {
                end += 1;
            }
            if end < chars.len() {
                return Some(chars[start + 1..end].iter().collect());
            }
        } else {
            // Regular word
            while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
                start -= 1;
            }
            let mut end = pos;
            while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            return Some(chars[start..end].iter().collect());
        }

        None
    }

    /// Generate hover information for a word in the schema
    fn generate_hover(
        &self,
        schema: &AvroSchema,
        text: &str,
        word: &str,
        _position: Position,
    ) -> Option<Hover> {
        use crate::schema::PrimitiveType;
        use async_lsp::lsp_types::{MarkupContent, MarkupKind};

        // Check if it's a primitive type
        if let Some(prim) = PrimitiveType::from_str(word) {
            let doc = self.get_primitive_documentation(&prim);
            return Some(Hover {
                contents: async_lsp::lsp_types::HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("**Primitive Type**: `{:?}`\n\n{}", prim, doc),
                }),
                range: None,
            });
        }

        // Check if it's a named type in the schema
        if let Some(named_type) = schema.named_types.get(word) {
            let type_info = self.format_type_info(named_type);
            return Some(Hover {
                contents: async_lsp::lsp_types::HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: type_info,
                }),
                range: None,
            });
        }

        // Check if it's a field name (search for it in the text)
        if let Some(field_info) = self.find_field_info(schema, word, text) {
            return Some(Hover {
                contents: async_lsp::lsp_types::HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: field_info,
                }),
                range: None,
            });
        }

        None
    }

    /// Get documentation for primitive types
    fn get_primitive_documentation(&self, prim: &PrimitiveType) -> &'static str {
        match prim {
            PrimitiveType::Null => "No value",
            PrimitiveType::Boolean => "A binary value (true or false)",
            PrimitiveType::Int => "32-bit signed integer",
            PrimitiveType::Long => "64-bit signed integer",
            PrimitiveType::Float => "Single precision (32-bit) IEEE 754 floating-point number",
            PrimitiveType::Double => "Double precision (64-bit) IEEE 754 floating-point number",
            PrimitiveType::Bytes => "Sequence of 8-bit unsigned bytes",
            PrimitiveType::String => "Unicode character sequence",
        }
    }

    /// Format type information for hover display
    fn format_type_info(&self, avro_type: &AvroType) -> String {
        match avro_type {
            AvroType::Record(record) => {
                let mut info = format!("**Record**: `{}`\n\n", record.name);
                if let Some(ns) = &record.namespace {
                    info.push_str(&format!("**Namespace**: `{}`\n\n", ns));
                }
                if let Some(doc) = &record.doc {
                    info.push_str(&format!("{}\n\n", doc));
                }
                info.push_str("**Fields**:\n");
                for field in &record.fields {
                    let type_str = self.format_type_name(&field.field_type);
                    info.push_str(&format!("- `{}`: {}\n", field.name, type_str));
                }
                info
            }
            AvroType::Enum(enum_schema) => {
                let mut info = format!("**Enum**: `{}`\n\n", enum_schema.name);
                if let Some(ns) = &enum_schema.namespace {
                    info.push_str(&format!("**Namespace**: `{}`\n\n", ns));
                }
                if let Some(doc) = &enum_schema.doc {
                    info.push_str(&format!("{}\n\n", doc));
                }
                info.push_str("**Symbols**: ");
                info.push_str(&enum_schema.symbols.join(", "));
                info
            }
            AvroType::Fixed(fixed) => {
                let mut info = format!("**Fixed**: `{}`\n\n", fixed.name);
                if let Some(ns) = &fixed.namespace {
                    info.push_str(&format!("**Namespace**: `{}`\n\n", ns));
                }
                info.push_str(&format!("**Size**: {} bytes", fixed.size));
                info
            }
            AvroType::Array(array) => {
                format!("**Array** of {}", self.format_type_name(&array.items))
            }
            AvroType::Map(map) => {
                format!(
                    "**Map** with values of type {}",
                    self.format_type_name(&map.values)
                )
            }
            AvroType::Union(types) => {
                let type_names: Vec<String> =
                    types.iter().map(|t| self.format_type_name(t)).collect();
                format!("**Union**: {}", type_names.join(" | "))
            }
            AvroType::Primitive(prim) => {
                format!("**Primitive**: `{:?}`", prim)
            }
            AvroType::TypeRef(type_ref) => {
                format!("**Type Reference**: `{}`", type_ref.name)
            }
        }
    }

    /// Format a type name for display
    fn format_type_name(&self, avro_type: &AvroType) -> String {
        match avro_type {
            AvroType::Primitive(prim) => format!("`{:?}`", prim).to_lowercase(),
            AvroType::Record(r) => format!("`{}`", r.name),
            AvroType::Enum(e) => format!("`{}`", e.name),
            AvroType::Fixed(f) => format!("`{}`", f.name),
            AvroType::Array(a) => format!("array<{}>", self.format_type_name(&a.items)),
            AvroType::Map(m) => format!("map<{}>", self.format_type_name(&m.values)),
            AvroType::Union(types) => {
                let names: Vec<String> = types.iter().map(|t| self.format_type_name(t)).collect();
                format!("[{}]", names.join(", "))
            }
            AvroType::TypeRef(type_ref) => format!("`{}`", type_ref.name),
        }
    }

    /// Find field information in the schema
    fn find_field_info(
        &self,
        schema: &AvroSchema,
        field_name: &str,
        _text: &str,
    ) -> Option<String> {
        // Search through all records for a field with this name
        for named_type in schema.named_types.values() {
            if let AvroType::Record(record) = named_type {
                for field in &record.fields {
                    if field.name == field_name {
                        let mut info = format!("**Field**: `{}`\n\n", field.name);
                        info.push_str(&format!(
                            "**Type**: {}\n\n",
                            self.format_type_name(&field.field_type)
                        ));
                        if let Some(doc) = &field.doc {
                            info.push_str(&format!("**Description**: {}\n\n", doc));
                        }
                        info.push_str(&format!("**In Record**: `{}`", record.name));
                        return Some(info);
                    }
                }
            }
        }
        None
    }

    /// Get document symbols for a URI
    pub async fn get_document_symbols(&self, uri: &Url) -> Option<Vec<DocumentSymbol>> {
        let state = self.inner.read().await;
        let doc = state.documents.get(uri)?;
        let schema = doc.schema.as_ref()?;
        let text = &doc.text;

        let mut symbols = Vec::new();

        // Add all named types as symbols
        for (name, avro_type) in &schema.named_types {
            if let Some(symbol) = self.create_symbol_from_type(name, avro_type, text) {
                symbols.push(symbol);
            }
        }

        Some(symbols)
    }

    /// Create a DocumentSymbol from an AvroType
    fn create_symbol_from_type(
        &self,
        name: &str,
        avro_type: &AvroType,
        text: &str,
    ) -> Option<DocumentSymbol> {
        match avro_type {
            AvroType::Record(record) => {
                let range = self.find_name_range(text, name)?;
                let mut children = Vec::new();

                // Add fields as children
                for field in &record.fields {
                    if let Some(field_range) = self.find_name_range(text, &field.name) {
                        #[allow(deprecated)]
                        children.push(DocumentSymbol {
                            name: field.name.clone(),
                            detail: Some(self.format_type_name(&field.field_type)),
                            kind: SymbolKind::FIELD,
                            tags: None,
                            deprecated: None,
                            range: field_range,
                            selection_range: field_range,
                            children: None,
                        });
                    }
                }

                #[allow(deprecated)]
                Some(DocumentSymbol {
                    name: record.name.clone(),
                    detail: record.namespace.clone(),
                    kind: SymbolKind::STRUCT,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: if children.is_empty() {
                        None
                    } else {
                        Some(children)
                    },
                })
            }
            AvroType::Enum(enum_type) => {
                let range = self.find_name_range(text, name)?;
                let mut children = Vec::new();

                // Add symbols as children
                for symbol in &enum_type.symbols {
                    if let Some(symbol_range) = self.find_name_range(text, symbol) {
                        #[allow(deprecated)]
                        children.push(DocumentSymbol {
                            name: symbol.clone(),
                            detail: None,
                            kind: SymbolKind::ENUM_MEMBER,
                            tags: None,
                            deprecated: None,
                            range: symbol_range,
                            selection_range: symbol_range,
                            children: None,
                        });
                    }
                }

                #[allow(deprecated)]
                Some(DocumentSymbol {
                    name: enum_type.name.clone(),
                    detail: enum_type.namespace.clone(),
                    kind: SymbolKind::ENUM,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: if children.is_empty() {
                        None
                    } else {
                        Some(children)
                    },
                })
            }
            AvroType::Fixed(fixed) => {
                let range = self.find_name_range(text, name)?;

                #[allow(deprecated)]
                Some(DocumentSymbol {
                    name: fixed.name.clone(),
                    detail: Some(format!("{} bytes", fixed.size)),
                    kind: SymbolKind::CONSTANT,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                })
            }
            _ => None,
        }
    }

    /// Find the range of a name in the text
    fn find_name_range(&self, text: &str, name: &str) -> Option<Range> {
        // Search for the name as a quoted string in the JSON
        let search_pattern = format!("\"{}\"", name);
        if let Some(offset) = text.find(&search_pattern) {
            let start_pos = self.offset_to_position(text, offset + 1); // +1 to skip opening quote
            let end_pos = self.offset_to_position(text, offset + 1 + name.len());
            return Some(Range {
                start: start_pos,
                end: end_pos,
            });
        }
        None
    }

    /// Get semantic tokens for a document
    pub async fn get_semantic_tokens(&self, uri: &Url) -> Option<Vec<SemanticToken>> {
        let state = self.inner.read().await;
        let doc = state.documents.get(uri)?;
        let schema = doc.schema.as_ref()?;
        let text = &doc.text;

        let mut builder = SemanticTokensBuilder::new(text.clone());
        builder.tokenize_schema(schema);
        let tokens = builder.build();

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

        let context = self.analyze_completion_context(text, position);

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
                items.extend(self.get_key_completions());
            }
            CompletionContext::TypeValue => {
                // Suggest type values (primitives, complex types, or references)
                items.extend(self.get_type_value_completions(schema));
            }
            CompletionContext::FieldAttribute => {
                // Suggest field attributes
                items.extend(self.get_field_attribute_completions());
            }
            CompletionContext::EnumAttribute => {
                // Suggest enum attributes
                items.extend(self.get_enum_attribute_completions());
            }
            CompletionContext::RecordAttribute => {
                // Suggest record attributes
                items.extend(self.get_record_attribute_completions());
            }
            CompletionContext::Unknown => {
                // Provide general suggestions
                items.extend(self.get_key_completions());
            }
        }

        Some(items)
    }

    /// Get definition location for a symbol at the given position
    pub async fn get_definition(&self, uri: &Url, position: Position) -> Option<Location> {
        let state = self.inner.read().await;
        let doc = state.documents.get(uri)?;
        let schema = doc.schema.as_ref()?;
        let text = &doc.text;

        // Get the word at the cursor position
        let word = self.get_word_at_position(text, position)?;

        tracing::debug!("Looking for definition of '{}'", word);

        // Check if the word is a named type in the schema
        if schema.named_types.contains_key(&word) {
            // Find where this type is defined (its name declaration)
            let range = self.find_name_range(text, &word)?;

            return Some(Location {
                uri: uri.clone(),
                range,
            });
        }

        // Not a type reference we can navigate to
        None
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

        // First, remove trailing commas before parsing
        let cleaned_text = self.remove_trailing_commas(&document.text);

        // Parse JSON to validate and normalize
        let json: serde_json::Value = serde_json::from_str(&cleaned_text).map_err(|e| {
            async_lsp::ResponseError::new(
                async_lsp::ErrorCode::PARSE_ERROR,
                format!("Invalid JSON, cannot format: {}", e),
            )
        })?;

        // Format with serde_json (uses 2-space indentation by default)
        let formatted = serde_json::to_string_pretty(&json).map_err(|e| {
            async_lsp::ResponseError::new(
                async_lsp::ErrorCode::INTERNAL_ERROR,
                format!("Formatting failed: {}", e),
            )
        })?;

        // Add final newline
        let formatted = format!("{}\n", formatted);

        // Calculate the end position of the document
        let line_count = document.text.lines().count() as u32;
        let last_line = document.text.lines().last().unwrap_or("");
        let last_line_length = last_line.len() as u32;

        // Create TextEdit for full document replacement
        Ok(Some(async_lsp::lsp_types::TextEdit {
            range: async_lsp::lsp_types::Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: line_count.saturating_sub(1),
                    character: last_line_length,
                },
            },
            new_text: formatted,
        }))
    }

    /// Remove trailing commas from JSON text
    /// This handles cases like {"foo": "bar",} which are invalid JSON
    fn remove_trailing_commas(&self, text: &str) -> String {
        // Strategy: Use regex to find commas followed by optional whitespace and then } or ]
        let re = regex::Regex::new(r",(\s*[}\]])").unwrap();
        re.replace_all(text, "$1").to_string()
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

        // Use AST traversal to find the node at cursor position
        let node = find_node_at_position(schema, range.start)?;

        let mut actions = Vec::new();

        match node {
            AstNode::RecordDefinition(record) => {
                // Offer "Add documentation" if record doesn't have doc
                if record.doc.is_none()
                    && let Some(action) = self.create_add_doc_action(uri, record) {
                        actions.push(action);
                    }
                // Offer "Add field to record"
                if let Some(action) = self.create_add_field_action(uri, record) {
                    actions.push(action);
                }
            }
            AstNode::Field(field) => {
                // Offer "Add field to record" (insert after this field)
                // We need to find the parent record
                if let Some(action) = self.find_parent_record_and_add_field(uri, schema, field) {
                    actions.push(action);
                }
            }
            AstNode::FieldType(field) => {
                // Check if type is not already a union
                if !self.is_union(&field.field_type) {
                    // Offer "Make field nullable"
                    if let Some(action) = self.create_make_nullable_action(uri, field) {
                        actions.push(action);
                    }
                }
            }
            AstNode::EnumDefinition(enum_schema) => {
                // Offer "Add documentation" if enum doesn't have doc
                if enum_schema.doc.is_none()
                    && let Some(action) = self.create_add_doc_action_enum(uri, enum_schema) {
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

    // ========================================================================
    // AST-based Code Action Creators (New Implementation)
    // ========================================================================

    /// Helper: Check if a type is already a union
    fn is_union(&self, avro_type: &AvroType) -> bool {
        matches!(avro_type, AvroType::Union(_))
    }

    /// Helper: Format an AvroType as JSON string
    fn format_avro_type(&self, avro_type: &AvroType) -> String {
        serde_json::to_string(avro_type).unwrap_or_else(|_| "\"string\"".to_string())
    }

    /// Create "Make field nullable" action using AST
    fn create_make_nullable_action(
        &self,
        uri: &Url,
        field: &Field,
    ) -> Option<async_lsp::lsp_types::CodeAction> {
        use async_lsp::lsp_types::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit};
        use std::collections::HashMap;

        let type_range = field.type_range.as_ref()?;
        let current_type = self.format_avro_type(&field.field_type);
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
        &self,
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
        &self,
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

    /// Create "Add field to record" action using AST
    fn create_add_field_action(
        &self,
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
        &self,
        uri: &Url,
        schema: &AvroSchema,
        _field: &Field,
    ) -> Option<async_lsp::lsp_types::CodeAction> {
        // For now, we'll find the root record if it's a record
        // In future, we could walk the tree to find the actual parent
        if let AvroType::Record(record) = &schema.root {
            self.create_add_field_action(uri, record)
        } else {
            None
        }
    }

    // ========================================================================
    // Rename Implementation
    // ========================================================================

    /// Helper to check if position is inside a range
    fn position_in_range(pos: Position, range: &Range) -> bool {
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

    /// Rename a symbol (record, enum, fixed, or field name)
    pub async fn rename(
        &self,
        uri: &Url,
        position: Position,
        new_name: &str,
    ) -> Result<Option<async_lsp::lsp_types::WorkspaceEdit>, ResponseError> {
        use async_lsp::lsp_types::{TextEdit, WorkspaceEdit};
        use std::collections::HashMap;

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

        // Find what symbol we're renaming
        let node = match find_node_at_position(schema, position) {
            Some(n) => n,
            None => return Ok(None),
        };

        // Collect all edits needed for the rename
        let mut edits = Vec::new();

        match node {
            AstNode::RecordDefinition(record) => {
                // Check if cursor is on the name specifically
                if let Some(name_range) = &record.name_range
                    && Self::position_in_range(position, name_range) {
                        let old_name = &record.name;
                        
                        // Check if new name conflicts with existing types (except itself)
                        if old_name != new_name && schema.named_types.contains_key(new_name) {
                            return Err(ResponseError::new(
                                async_lsp::ErrorCode::INVALID_PARAMS,
                                format!("A type named '{}' already exists", new_name),
                            ));
                        }
                        
                        // Rename the record: update declaration and all references
                        self.collect_type_rename_edits(schema, old_name, new_name, &mut edits);
                    }
            }
            AstNode::EnumDefinition(enum_schema) => {
                // Check if cursor is on the name specifically
                if let Some(name_range) = &enum_schema.name_range
                    && Self::position_in_range(position, name_range) {
                        let old_name = &enum_schema.name;
                        
                        // Check if new name conflicts with existing types (except itself)
                        if old_name != new_name && schema.named_types.contains_key(new_name) {
                            return Err(ResponseError::new(
                                async_lsp::ErrorCode::INVALID_PARAMS,
                                format!("A type named '{}' already exists", new_name),
                            ));
                        }
                        
                        // Rename the enum: update declaration and all references
                        self.collect_type_rename_edits(schema, old_name, new_name, &mut edits);
                    }
            }
            AstNode::Field(field) => {
                // Check if cursor is on the field name specifically
                if let Some(name_range) = &field.name_range
                    && Self::position_in_range(position, name_range) {
                        // For fields, we need to check if the new name conflicts with other fields in the same record
                        // Find the parent record to check for conflicts
                        let has_conflict = self.check_field_name_conflict(schema, field, new_name);
                        if has_conflict {
                            return Err(ResponseError::new(
                                async_lsp::ErrorCode::INVALID_PARAMS,
                                format!("A field named '{}' already exists in this record", new_name),
                            ));
                        }
                        
                        // Rename the field: only update this field's name
                        edits.push(TextEdit {
                            range: *name_range,
                            new_text: format!("\"{}\"", new_name),
                        });
                    }
            }
            AstNode::FieldType(field) => {
                // Check if the field's type is a TypeRef and cursor is on it
                if let AvroType::TypeRef(type_ref) = field.field_type.as_ref()
                    && let Some(type_range) = &type_ref.range
                    && Self::position_in_range(position, type_range) {
                        let old_name = &type_ref.name;
                        
                        // Check if new name conflicts with existing types (except itself)
                        if old_name != new_name && schema.named_types.contains_key(new_name) {
                            return Err(ResponseError::new(
                                async_lsp::ErrorCode::INVALID_PARAMS,
                                format!("A type named '{}' already exists", new_name),
                            ));
                        }
                        
                        // Rename the type: update declaration and all references
                        self.collect_type_rename_edits(schema, old_name, new_name, &mut edits);
                    } else {
                        // Cursor is on field type but not on a simple TypeRef
                        // Could be on a union, array, etc. - not supported yet
                        return Ok(None);
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

    /// Check if a field name would conflict with other fields in the same record
    fn check_field_name_conflict(&self, schema: &AvroSchema, target_field: &Field, new_name: &str) -> bool {
        // Helper to find the parent record containing this field
        fn find_parent_record<'a>(avro_type: &'a AvroType, target_field: &Field) -> Option<&'a RecordSchema> {
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

    /// Prepare for rename - validate that rename is possible at this position
    pub async fn prepare_rename(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<async_lsp::lsp_types::PrepareRenameResponse>, ResponseError> {
        use async_lsp::lsp_types::PrepareRenameResponse;

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

        // Find what symbol we're trying to rename
        let node = match find_node_at_position(schema, position) {
            Some(n) => n,
            None => return Ok(None),
        };

        // Check if the position is valid for renaming and return the range + placeholder
        match node {
            AstNode::RecordDefinition(record) => {
                if let Some(name_range) = &record.name_range
                    && Self::position_in_range(position, name_range) {
                        return Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
                            range: *name_range,
                            placeholder: record.name.clone(),
                        }));
                    }
            }
            AstNode::EnumDefinition(enum_schema) => {
                if let Some(name_range) = &enum_schema.name_range
                    && Self::position_in_range(position, name_range) {
                        return Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
                            range: *name_range,
                            placeholder: enum_schema.name.clone(),
                        }));
                    }
            }
            AstNode::Field(field) => {
                if let Some(name_range) = &field.name_range
                    && Self::position_in_range(position, name_range) {
                        return Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
                            range: *name_range,
                            placeholder: field.name.clone(),
                        }));
                    }
            }
            AstNode::FieldType(field) => {
                // Check if the field's type is a TypeRef and cursor is on it
                if let AvroType::TypeRef(type_ref) = field.field_type.as_ref()
                    && let Some(type_range) = &type_ref.range
                    && Self::position_in_range(position, type_range) {
                        return Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
                            range: *type_range,
                            placeholder: type_ref.name.clone(),
                        }));
                    }
            }
        }

        Ok(None)
    }

    /// Find all references to a symbol
    pub async fn find_references(
        &self,
        uri: &Url,
        position: Position,
        include_declaration: bool,
    ) -> Result<Option<Vec<async_lsp::lsp_types::Location>>, ResponseError> {
        use async_lsp::lsp_types::Location;

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

        // Find what symbol we're looking for references to
        let node = match find_node_at_position(schema, position) {
            Some(n) => n,
            None => return Ok(None),
        };

        let mut locations = Vec::new();

        match node {
            AstNode::RecordDefinition(record) => {
                // Check if cursor is on the name
                if let Some(name_range) = &record.name_range
                    && Self::position_in_range(position, name_range) {
                        let type_name = &record.name;
                        self.collect_type_references(schema, type_name, uri, include_declaration, &mut locations);
                    }
            }
            AstNode::EnumDefinition(enum_schema) => {
                // Check if cursor is on the name
                if let Some(name_range) = &enum_schema.name_range
                    && Self::position_in_range(position, name_range) {
                        let type_name = &enum_schema.name;
                        self.collect_type_references(schema, type_name, uri, include_declaration, &mut locations);
                    }
            }
            AstNode::Field(field) => {
                // For fields, we only have one location (the field name itself)
                if let Some(name_range) = &field.name_range
                    && Self::position_in_range(position, name_range) {
                        locations.push(Location {
                            uri: uri.clone(),
                            range: *name_range,
                        });
                    }
            }
            AstNode::FieldType(field) => {
                // Check if the field's type is a TypeRef and cursor is on it
                if let AvroType::TypeRef(type_ref) = field.field_type.as_ref()
                    && let Some(type_range) = &type_ref.range
                    && Self::position_in_range(position, type_range) {
                        // Find all references to this type
                        let type_name = &type_ref.name;
                        self.collect_type_references(schema, type_name, uri, include_declaration, &mut locations);
                    }
            }
        }

        if locations.is_empty() {
            return Ok(None);
        }

        Ok(Some(locations))
    }

    /// Collect all references to a type (record/enum/fixed)
    fn collect_type_references(
        &self,
        schema: &AvroSchema,
        type_name: &str,
        uri: &Url,
        include_declaration: bool,
        locations: &mut Vec<async_lsp::lsp_types::Location>,
    ) {
        use async_lsp::lsp_types::Location;

        // Helper to find references in an AvroType
        fn find_references_in_type(
            avro_type: &AvroType,
            type_name: &str,
            uri: &Url,
            include_declaration: bool,
            locations: &mut Vec<Location>,
        ) {
            match avro_type {
                AvroType::Record(record) => {
                    // Add the declaration if requested
                    if include_declaration && record.name == type_name {
                        if let Some(name_range) = &record.name_range {
                            locations.push(Location {
                                uri: uri.clone(),
                                range: *name_range,
                            });
                        }
                    }
                    // Check all fields for type references
                    for field in &record.fields {
                        find_references_in_type(&field.field_type, type_name, uri, include_declaration, locations);
                    }
                }
                AvroType::Enum(enum_schema) => {
                    // Add the declaration if requested
                    if include_declaration && enum_schema.name == type_name {
                        if let Some(name_range) = &enum_schema.name_range {
                            locations.push(Location {
                                uri: uri.clone(),
                                range: *name_range,
                            });
                        }
                    }
                }
                AvroType::Fixed(fixed) => {
                    // Add the declaration if requested
                    if include_declaration && fixed.name == type_name {
                        if let Some(name_range) = &fixed.name_range {
                            locations.push(Location {
                                uri: uri.clone(),
                                range: *name_range,
                            });
                        }
                    }
                }
                AvroType::TypeRef(type_ref) => {
                    // This is a reference to a named type
                    if type_ref.name == type_name {
                        if let Some(range) = &type_ref.range {
                            locations.push(Location {
                                uri: uri.clone(),
                                range: *range,
                            });
                        }
                    }
                }
                AvroType::Array(array) => {
                    find_references_in_type(&array.items, type_name, uri, include_declaration, locations);
                }
                AvroType::Map(map) => {
                    find_references_in_type(&map.values, type_name, uri, include_declaration, locations);
                }
                AvroType::Union(types) => {
                    for t in types {
                        find_references_in_type(t, type_name, uri, include_declaration, locations);
                    }
                }
                AvroType::Primitive(_) => {}
            }
        }

        // Search the entire schema for references
        find_references_in_type(&schema.root, type_name, uri, include_declaration, locations);
    }

    /// Collect all edits needed to rename a type (record/enum/fixed)
    fn collect_type_rename_edits(
        &self,
        schema: &AvroSchema,
        old_name: &str,
        new_name: &str,
        edits: &mut Vec<async_lsp::lsp_types::TextEdit>,
    ) {
        use async_lsp::lsp_types::TextEdit;

        // Helper to find references in an AvroType
        fn find_references_in_type(
            avro_type: &AvroType,
            old_name: &str,
            new_name: &str,
            edits: &mut Vec<TextEdit>,
        ) {
            match avro_type {
                AvroType::Record(record) => {
                    // Update the record's name declaration
                    if record.name == old_name
                        && let Some(name_range) = &record.name_range {
                            edits.push(TextEdit {
                                range: *name_range,
                                new_text: format!("\"{}\"", new_name),
                            });
                        }
                    // Check all fields for type references
                    for field in &record.fields {
                        find_references_in_type(&field.field_type, old_name, new_name, edits);
                    }
                }
                AvroType::Enum(enum_schema) => {
                    // Update the enum's name declaration
                    if enum_schema.name == old_name
                        && let Some(name_range) = &enum_schema.name_range {
                            edits.push(TextEdit {
                                range: *name_range,
                                new_text: format!("\"{}\"", new_name),
                            });
                        }
                }
                AvroType::Fixed(fixed) => {
                    // Update the fixed type's name declaration
                    if fixed.name == old_name
                        && let Some(name_range) = &fixed.name_range {
                            edits.push(TextEdit {
                                range: *name_range,
                                new_text: format!("\"{}\"", new_name),
                            });
                        }
                }
                AvroType::TypeRef(type_ref) => {
                    // This is a reference to a named type - rename it if it matches
                    if type_ref.name == old_name
                        && let Some(range) = &type_ref.range {
                            edits.push(TextEdit {
                                range: *range,
                                new_text: format!("\"{}\"", new_name),
                            });
                        }
                }
                AvroType::Array(array) => {
                    find_references_in_type(&array.items, old_name, new_name, edits);
                }
                AvroType::Map(map) => {
                    find_references_in_type(&map.values, old_name, new_name, edits);
                }
                AvroType::Union(types) => {
                    for t in types {
                        find_references_in_type(t, old_name, new_name, edits);
                    }
                }
                AvroType::Primitive(_) => {}
            }
        }

        // Search the entire schema for references
        find_references_in_type(&schema.root, old_name, new_name, edits);
    }

    /// Analyze the context at the cursor position to determine what kind of completion to provide
    fn analyze_completion_context(&self, text: &str, position: Position) -> CompletionContext {
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
    fn get_key_completions(&self) -> Vec<CompletionItem> {
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
    fn get_type_value_completions(&self, schema: Option<&AvroSchema>) -> Vec<CompletionItem> {
        let mut items = vec![
            // Complex types
            CompletionItem {
                label: "record".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("A record type with named fields".to_string()),
                insert_text: Some("\"record\"".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "enum".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("An enumeration type".to_string()),
                insert_text: Some("\"enum\"".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "array".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("An array type".to_string()),
                insert_text: Some("\"array\"".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "map".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("A map type with string keys".to_string()),
                insert_text: Some("\"map\"".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "fixed".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("A fixed-size byte array".to_string()),
                insert_text: Some("\"fixed\"".to_string()),
                ..Default::default()
            },
            // Primitive types
            CompletionItem {
                label: "null".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Null type".to_string()),
                insert_text: Some("\"null\"".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "boolean".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Boolean type".to_string()),
                insert_text: Some("\"boolean\"".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "int".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("32-bit signed integer".to_string()),
                insert_text: Some("\"int\"".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "long".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("64-bit signed integer".to_string()),
                insert_text: Some("\"long\"".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "float".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Single precision floating point".to_string()),
                insert_text: Some("\"float\"".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "double".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Double precision floating point".to_string()),
                insert_text: Some("\"double\"".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "bytes".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Sequence of bytes".to_string()),
                insert_text: Some("\"bytes\"".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "string".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Unicode string".to_string()),
                insert_text: Some("\"string\"".to_string()),
                ..Default::default()
            },
        ];

        // Add named types from the schema
        if let Some(schema) = schema {
            for name in schema.named_types.keys() {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::REFERENCE),
                    detail: Some(format!("Reference to type '{}'", name)),
                    insert_text: Some(format!("\"{}\"", name)),
                    ..Default::default()
                });
            }
        }

        items
    }

    /// Get completions for field attributes
    fn get_field_attribute_completions(&self) -> Vec<CompletionItem> {
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
                detail: Some("Sort order (ascending, descending, ignore)".to_string()),
                insert_text: Some("\"order\": \"${1|ascending,descending,ignore|}\"".to_string()),
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
    fn get_enum_attribute_completions(&self) -> Vec<CompletionItem> {
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
                detail: Some("Array of enum symbols".to_string()),
                insert_text: Some("\"symbols\": [$0]".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "default".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some("Default symbol value".to_string()),
                insert_text: Some("\"default\": \"$0\"".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
        ]
    }

    /// Get completions for record attributes
    fn get_record_attribute_completions(&self) -> Vec<CompletionItem> {
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
}

/// Token types (indices must match the legend order in server.rs)
const TOKEN_TYPE_KEYWORD: u32 = 0;
const TOKEN_TYPE_TYPE: u32 = 1;
const TOKEN_TYPE_ENUM: u32 = 2;
const TOKEN_TYPE_STRUCT: u32 = 3;
const TOKEN_TYPE_PROPERTY: u32 = 4;
const TOKEN_TYPE_ENUM_MEMBER: u32 = 5;
const TOKEN_TYPE_STRING: u32 = 6;
const TOKEN_TYPE_NUMBER: u32 = 7;

/// Token modifiers (bit flags)
const TOKEN_MODIFIER_DECLARATION: u32 = 0x01;
const TOKEN_MODIFIER_READONLY: u32 = 0x02;

/// Helper struct for building semantic tokens
struct SemanticTokensBuilder {
    text: String,
    tokens: Vec<Token>,
}

/// A token with absolute position
#[derive(Debug, Clone)]
struct Token {
    line: u32,
    character: u32,
    length: u32,
    token_type: u32,
    token_modifiers: u32,
}

impl SemanticTokensBuilder {
    fn new(text: String) -> Self {
        Self {
            text,
            tokens: Vec::new(),
        }
    }

    /// Main tokenization logic
    fn tokenize_schema(&mut self, schema: &AvroSchema) {
        use std::collections::HashSet;

        // Track which byte offsets we've already tokenized to avoid duplicates
        let mut tokenized_offsets: HashSet<usize> = HashSet::new();

        // PASS 1: Tokenize all JSON structural keywords (keys only, not values)
        // These are the keys like "type":, "name":, "fields":, etc.
        for key in &[
            "type",
            "name",
            "namespace",
            "doc",
            "fields",
            "symbols",
            "items",
            "values",
            "size",
            "default",
            "aliases",
            "order",
        ] {
            let pattern = format!("\"{}\":", key);
            let mut search_start = 0;

            while let Some(offset) = self.text[search_start..].find(&pattern) {
                let absolute_offset = search_start + offset + 1; // +1 for opening quote

                if tokenized_offsets.insert(absolute_offset) {
                    let pos = self.offset_to_position(absolute_offset);
                    self.add_token(
                        pos.line,
                        pos.character,
                        key.len() as u32,
                        TOKEN_TYPE_KEYWORD,
                        0,
                    );
                }

                search_start += offset + pattern.len();
            }
        }

        // PASS 2: Tokenize type keyword VALUES like "record", "enum", "array", "map", "fixed"
        for keyword in &["record", "enum", "array", "map", "fixed"] {
            let pattern = format!("\"type\": \"{}\"", keyword);
            let mut search_start = 0;

            while let Some(offset) = self.text[search_start..].find(&pattern) {
                let absolute_offset = search_start + offset;
                let value_offset = absolute_offset + "\"type\": \"".len();

                if tokenized_offsets.insert(value_offset) {
                    let pos = self.offset_to_position(value_offset);
                    self.add_token(
                        pos.line,
                        pos.character,
                        keyword.len() as u32,
                        TOKEN_TYPE_KEYWORD,
                        0,
                    );
                }

                search_start += offset + pattern.len();
            }
        }

        // PASS 3: Tokenize named type declarations from schema
        match &schema.root {
            AvroType::Record(record) => {
                self.tokenize_record_type(record, &mut tokenized_offsets);
            }
            AvroType::Enum(enum_type) => {
                self.tokenize_enum_type(enum_type, &mut tokenized_offsets);
            }
            AvroType::Fixed(fixed) => {
                self.tokenize_fixed_type(fixed, &mut tokenized_offsets);
            }
            _ => {}
        }

        for avro_type in schema.named_types.values() {
            match avro_type {
                AvroType::Record(record) => {
                    self.tokenize_record_type(record, &mut tokenized_offsets);
                }
                AvroType::Enum(enum_type) => {
                    self.tokenize_enum_type(enum_type, &mut tokenized_offsets);
                }
                AvroType::Fixed(fixed) => {
                    self.tokenize_fixed_type(fixed, &mut tokenized_offsets);
                }
                _ => {}
            }
        }
    }

    /// Tokenize a record type
    fn tokenize_record_type(
        &mut self,
        record: &RecordSchema,
        tokenized_offsets: &mut std::collections::HashSet<usize>,
    ) {
        // Find and tokenize the record name (appears after "name": at the top level)
        let name_pattern = format!("\"name\": \"{}\"", record.name);
        if let Some(offset) = self.text.find(&name_pattern) {
            let value_offset = offset + "\"name\": \"".len();
            if tokenized_offsets.insert(value_offset) {
                let pos = self.offset_to_position(value_offset);
                self.add_token(
                    pos.line,
                    pos.character,
                    record.name.len() as u32,
                    TOKEN_TYPE_STRUCT,
                    TOKEN_MODIFIER_DECLARATION,
                );
            }
        }

        // Tokenize fields
        for field in &record.fields {
            self.tokenize_field(field, tokenized_offsets);
        }
    }

    /// Tokenize an enum type
    fn tokenize_enum_type(
        &mut self,
        enum_type: &EnumSchema,
        tokenized_offsets: &mut std::collections::HashSet<usize>,
    ) {
        // Find and tokenize the enum name
        let name_pattern = format!("\"name\": \"{}\"", enum_type.name);
        if let Some(offset) = self.text.find(&name_pattern) {
            let value_offset = offset + "\"name\": \"".len();
            if tokenized_offsets.insert(value_offset) {
                let pos = self.offset_to_position(value_offset);
                self.add_token(
                    pos.line,
                    pos.character,
                    enum_type.name.len() as u32,
                    TOKEN_TYPE_ENUM,
                    TOKEN_MODIFIER_DECLARATION,
                );
            }
        }

        // Tokenize enum symbols (appear in the "symbols" array)
        if let Some(symbols_start) = self.text.find("\"symbols\":") {
            for symbol in &enum_type.symbols {
                let symbol_pattern = format!("\"{}\"", symbol);
                // Search only after the "symbols" key
                if let Some(offset) = self.text[symbols_start..].find(&symbol_pattern) {
                    let absolute_offset = symbols_start + offset + 1; // +1 for opening quote
                    if tokenized_offsets.insert(absolute_offset) {
                        let pos = self.offset_to_position(absolute_offset);
                        self.add_token(
                            pos.line,
                            pos.character,
                            symbol.len() as u32,
                            TOKEN_TYPE_ENUM_MEMBER,
                            0,
                        );
                    }
                }
            }
        }
    }

    /// Tokenize a fixed type
    fn tokenize_fixed_type(
        &mut self,
        fixed: &FixedSchema,
        tokenized_offsets: &mut std::collections::HashSet<usize>,
    ) {
        // Find and tokenize the fixed name
        let name_pattern = format!("\"name\": \"{}\"", fixed.name);
        if let Some(offset) = self.text.find(&name_pattern) {
            let value_offset = offset + "\"name\": \"".len();
            if tokenized_offsets.insert(value_offset) {
                let pos = self.offset_to_position(value_offset);
                self.add_token(
                    pos.line,
                    pos.character,
                    fixed.name.len() as u32,
                    TOKEN_TYPE_TYPE,
                    TOKEN_MODIFIER_DECLARATION,
                );
            }
        }
    }

    /// Tokenize a field
    fn tokenize_field(
        &mut self,
        field: &Field,
        tokenized_offsets: &mut std::collections::HashSet<usize>,
    ) {
        // Find field name - need to be careful since "name" can appear multiple times
        // Look for {"name": "field_name" pattern within the fields array
        let field_pattern = format!("\"name\": \"{}\"", field.name);
        let mut search_start = 0;

        // First find the "fields" array
        if let Some(fields_offset) = self.text.find("\"fields\":") {
            search_start = fields_offset;
        }

        // Now search for this specific field name pattern after the fields array
        if let Some(offset) = self.text[search_start..].find(&field_pattern) {
            let absolute_offset = search_start + offset;
            let value_offset = absolute_offset + "\"name\": \"".len();

            // Only tokenize if we haven't already (avoids duplicate in case of record name = field name)
            if tokenized_offsets.insert(value_offset) {
                let pos = self.offset_to_position(value_offset);
                self.add_token(
                    pos.line,
                    pos.character,
                    field.name.len() as u32,
                    TOKEN_TYPE_PROPERTY,
                    TOKEN_MODIFIER_DECLARATION,
                );
            }
        }

        // Tokenize the field's type
        self.tokenize_type(&field.field_type, tokenized_offsets);
    }

    /// Tokenize a type (primitive, reference, complex)
    fn tokenize_type(
        &mut self,
        avro_type: &AvroType,
        tokenized_offsets: &mut std::collections::HashSet<usize>,
    ) {
        match avro_type {
            AvroType::Primitive(prim) => {
                let type_str = match prim {
                    PrimitiveType::Null => "null",
                    PrimitiveType::Boolean => "boolean",
                    PrimitiveType::Int => "int",
                    PrimitiveType::Long => "long",
                    PrimitiveType::Float => "float",
                    PrimitiveType::Double => "double",
                    PrimitiveType::Bytes => "bytes",
                    PrimitiveType::String => "string",
                };

                // Find "type": "primitive" or in arrays ["null", "string"]
                let pattern = format!("\"{}\"", type_str);
                let mut search_start = 0;

                while let Some(offset) = self.text[search_start..].find(&pattern) {
                    let absolute_offset = search_start + offset + 1; // +1 for opening quote

                    // Check if already tokenized
                    if !tokenized_offsets.contains(&absolute_offset) {
                        // Check context - should be after "type": or in an array
                        let context_start = absolute_offset.saturating_sub(20);
                        let context = &self.text[context_start..absolute_offset];

                        if (context.contains("\"type\":") || context.contains("["))
                            && tokenized_offsets.insert(absolute_offset)
                        {
                            let pos = self.offset_to_position(absolute_offset);
                            let token_type = match type_str {
                                "string" => TOKEN_TYPE_STRING,
                                "int" | "long" | "float" | "double" => TOKEN_TYPE_NUMBER,
                                _ => TOKEN_TYPE_KEYWORD,
                            };
                            self.add_token(
                                pos.line,
                                pos.character,
                                type_str.len() as u32,
                                token_type,
                                TOKEN_MODIFIER_READONLY,
                            );
                        }
                    }

                    search_start += offset + pattern.len();
                }
            }
            AvroType::TypeRef(type_ref) => {
                // Reference to a named type
                let pattern = format!("\"type\": \"{}\"", type_ref.name);
                if let Some(offset) = self.text.find(&pattern) {
                    let value_offset = offset + "\"type\": \"".len();
                    if tokenized_offsets.insert(value_offset) {
                        let pos = self.offset_to_position(value_offset);
                        self.add_token(
                            pos.line,
                            pos.character,
                            type_ref.name.len() as u32,
                            TOKEN_TYPE_TYPE,
                            0,
                        );
                    }
                }
            }
            AvroType::Union(types) => {
                // Tokenize each type in the union
                for t in types {
                    self.tokenize_type(t, tokenized_offsets);
                }
            }
            AvroType::Array(array) => {
                self.tokenize_type(&array.items, tokenized_offsets);
            }
            AvroType::Map(map) => {
                self.tokenize_type(&map.values, tokenized_offsets);
            }
            _ => {}
        }
    }

    /// Add a token to the list
    fn add_token(
        &mut self,
        line: u32,
        character: u32,
        length: u32,
        token_type: u32,
        token_modifiers: u32,
    ) {
        self.tokens.push(Token {
            line,
            character,
            length,
            token_type,
            token_modifiers,
        });
    }

    /// Convert byte offset to position
    fn offset_to_position(&self, offset: usize) -> Position {
        let mut line = 0;
        let mut character = 0;

        for (i, ch) in self.text.char_indices() {
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

    /// Build the final token array with delta encoding
    fn build(mut self) -> Vec<SemanticToken> {
        // Sort tokens by position (line, then character)
        self.tokens.sort_by(|a, b| {
            if a.line != b.line {
                a.line.cmp(&b.line)
            } else {
                a.character.cmp(&b.character)
            }
        });

        let mut result = Vec::new();
        let mut prev_line = 0;
        let mut prev_character = 0;

        for token in self.tokens {
            let delta_line = token.line - prev_line;
            let delta_start = if delta_line == 0 {
                token.character - prev_character
            } else {
                token.character
            };

            result.push(SemanticToken {
                delta_line,
                delta_start,
                length: token.length,
                token_type: token.token_type,
                token_modifiers_bitset: token.token_modifiers,
            });

            prev_line = token.line;
            prev_character = token.character;
        }

        result
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

    #[tokio::test]
    async fn test_remove_trailing_commas() {
        let state = ServerState::new();

        // Test various trailing comma scenarios
        let test_cases = vec![
            (r#"{"foo":"bar",}"#, r#"{"foo":"bar"}"#),
            (r#"{"arr":[1,2,3,]}"#, r#"{"arr":[1,2,3]}"#),
            (r#"{"obj":{"a":1,},"b":2}"#, r#"{"obj":{"a":1},"b":2}"#),
            (r#"[1,2,3,]"#, r#"[1,2,3]"#),
        ];

        for (input, expected) in test_cases {
            let result = state.remove_trailing_commas(input);
            assert_eq!(result, expected, "Failed for input: {}", input);
        }
    }
}
