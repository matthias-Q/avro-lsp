use async_lsp::lsp_types::{DocumentSymbol, Position, Range, SymbolKind};

use crate::schema::{AvroSchema, AvroType};

/// Generate document symbols from an Avro schema
pub fn create_document_symbols(schema: &AvroSchema, text: &str) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();

    // Add all named types as symbols
    for (name, avro_type) in &schema.named_types {
        if let Some(symbol) = create_symbol_from_type(name, avro_type, text) {
            symbols.push(symbol);
        }
    }

    symbols
}

/// Create a DocumentSymbol from an AvroType
fn create_symbol_from_type(name: &str, avro_type: &AvroType, text: &str) -> Option<DocumentSymbol> {
    match avro_type {
        AvroType::Record(record) => {
            // Use pre-computed name_range from the AST; fall back to text scan only if missing
            let range = record.name_range.or_else(|| find_name_range(text, name))?;
            let mut children = Vec::new();

            // Add fields as children using pre-computed field name ranges
            for field in &record.fields {
                let field_range = field
                    .name_range
                    .or_else(|| find_name_range(text, &field.name));
                if let Some(field_range) = field_range {
                    #[allow(deprecated)]
                    children.push(DocumentSymbol {
                        name: field.name.clone(),
                        detail: Some(crate::handlers::hover::format_type_name(&field.field_type)),
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
            // Use pre-computed name_range from the AST; fall back to text scan only if missing
            let range = enum_type
                .name_range
                .or_else(|| find_name_range(text, name))?;
            let mut children = Vec::new();

            // Enum symbols have no stored range in the AST; use text scan
            for symbol in &enum_type.symbols {
                if let Some(symbol_range) = find_name_range(text, symbol) {
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
            // Use pre-computed name_range from the AST; fall back to text scan only if missing
            let range = fixed.name_range.or_else(|| find_name_range(text, name))?;

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

/// Find the range of a name in the text by scanning for it as a quoted string.
/// This is only used as a fallback when the AST does not carry a pre-computed range,
/// and for enum symbols which have no stored range.
pub fn find_name_range(text: &str, name: &str) -> Option<Range> {
    // Helper to convert byte offset to Position
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

    // Search for the name as a quoted string in the JSON
    let search_pattern = format!("\"{}\"", name);
    if let Some(offset) = text.find(&search_pattern) {
        let start_pos = offset_to_position(text, offset + 1); // +1 to skip opening quote
        let end_pos = offset_to_position(text, offset + 1 + name.len());
        return Some(Range {
            start: start_pos,
            end: end_pos,
        });
    }
    None
}
